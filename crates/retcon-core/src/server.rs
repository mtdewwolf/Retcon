//! Local RPC server using the cross-platform transport abstraction.

#![allow(missing_docs)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use retcon_platform::{LocalEndpoint, TransportListener, TransportStream};
use retcon_protocol::{
    AuthLine, ClientHello, ClientMessage, SERVER_FEATURES, ServerHello, ServerHelloKind, VERSION,
    redact_line_for_trace,
};
use serde_json::json;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::sync::Notify;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::frame::read_capped_line;
use crate::rpc::{Request, Response};
use crate::state::CoreState;

pub struct Server {
    listener: TransportListener,
    token: String,
    state: CoreState,
}

impl Server {
    pub async fn bind(state: CoreState, token: String, data_dir: &std::path::Path) -> Result<Self, CoreError> {
        let listener = TransportListener::bind(data_dir)
            .await
            .map_err(|error| CoreError::io("bind core RPC listener", std::io::Error::other(error.to_string())))?;
        Ok(Self {
            listener,
            token,
            state,
        })
    }

    pub fn endpoint(&self) -> &LocalEndpoint {
        self.listener.endpoint()
    }

    pub async fn run(mut self) {
        let mut shutdown = self.state.shutdown_receiver();
        let mut connections = JoinSet::new();
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    let _ = self.state.events().emit_volatile(
                        "system.heartbeat",
                        json!({"uptime_ms": self.state.uptime().as_millis()}),
                    );
                },
                result = self.listener.accept() => match result {
                    Ok(stream) => {
                        let token = self.token.clone();
                        let state = self.state.clone();
                        connections.spawn(async move {
                            if let Err(error) = serve_connection(stream, &token, state).await {
                                tracing::warn!(
                                    error_id = %error.id,
                                    error = %error.technical_message,
                                    "RPC connection ended"
                                );
                            }
                        });
                    }
                    Err(error) => tracing::warn!(%error, "failed to accept RPC connection"),
                },
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }

        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
}

async fn serve_connection(
    stream: TransportStream,
    expected_token: &str,
    state: CoreState,
) -> Result<(), CoreError> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let auth_line = read_capped_line(&mut reader)
        .await?
        .ok_or_else(|| invalid_request("Authentication is required."))?;
    tracing::trace!(line = %redact_line_for_trace(&auth_line), "RPC auth frame");
    let auth: AuthLine = serde_json::from_str(&auth_line)
        .map_err(|_| invalid_request("The authentication message is malformed."))?;
    if auth.auth != expected_token {
        return Err(CoreError::new(
            ErrorCode::AuthenticationFailed,
            ErrorSource::Rpc,
            "Retcon could not authenticate this local client.",
            "RPC authentication token mismatch",
        ));
    }

    let hello_line = read_capped_line(&mut reader)
        .await?
        .ok_or_else(|| invalid_request("client.hello is required."))?;
    tracing::trace!(line = %redact_line_for_trace(&hello_line), "RPC hello frame");
    let client_hello: ClientHello = serde_json::from_str(&hello_line)
        .map_err(|_| invalid_request("The client hello message is malformed."))?;
    if client_hello.protocol_version != VERSION {
        return Err(invalid_request(format!(
            "unsupported protocol version {}",
            client_hello.protocol_version
        )));
    }

    let server_features: Vec<String> = SERVER_FEATURES.iter().map(|feature| (*feature).to_owned()).collect();
    let negotiated = retcon_protocol::negotiate_features(&client_hello.features, &server_features);
    let server_hello = ServerHello {
        kind: ServerHelloKind::Hello,
        protocol_version: VERSION,
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
        features: negotiated,
    };
    write_json_line(&mut writer, &server_hello).await?;

    let cancellations: Arc<Mutex<HashMap<u64, Arc<Notify>>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut events = state.events().subscribe();

    loop {
        let outgoing = tokio::select! {
            line = read_capped_line(&mut reader) => {
                let Some(line) = line? else { break };
                tracing::trace!(line = %redact_line_for_trace(&line), "RPC inbound frame");
                match ClientMessage::parse(&line) {
                    Ok(ClientMessage::Ping) => Some(json!({"kind": "pong"})),
                    Ok(ClientMessage::Cancel(cancel)) => {
                        if let Ok(mut map) = cancellations.lock()
                            && let Some(notify) = map.remove(&cancel.id)
                        {
                            notify.notify_waiters();
                        }
                        None
                    }
                    Ok(ClientMessage::Request(request)) => {
                        let request_id = request.id;
                        let notify = Arc::new(Notify::new());
                        if let Ok(mut map) = cancellations.lock() {
                            map.insert(request_id, notify.clone());
                        }
                        let response = tokio::select! {
                            _ = notify.notified() => Response::error(
                                request_id,
                                &CoreError::new(
                                    ErrorCode::InvalidRequest,
                                    ErrorSource::Rpc,
                                    "The request was cancelled.",
                                    format!("request {request_id} cancelled"),
                                ),
                            ),
                            response = dispatch(request, &state) => response,
                        };
                        if let Ok(mut map) = cancellations.lock() {
                            map.remove(&request_id);
                        }
                        Some(serde_json::to_value(response).map_err(|error| CoreError::new(
                            ErrorCode::Internal,
                            ErrorSource::Rpc,
                            "Retcon could not encode a response.",
                            error.to_string(),
                        ))?)
                    }
                    Err(parse_error) => Some(serde_json::to_value(Response::error(
                        0,
                        &invalid_request(parse_error.technical_message),
                    )).map_err(|error| CoreError::new(
                        ErrorCode::Internal,
                        ErrorSource::Rpc,
                        "Retcon could not encode a response.",
                        error.to_string(),
                    ))?),
                }
            }
            event = events.recv() => match event {
                Ok(event) => Some(json!({"event": &*event})),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    Some(json!({"event": {"kind":"system.eventsLagged", "payload":{"skipped":skipped}}}))
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        };

        if let Some(message) = outgoing {
            write_json_value(&mut writer, message).await?;
        }
    }
    Ok(())
}

async fn write_json_line(
    writer: &mut retcon_platform::TransportWriteHalf,
    value: &impl serde::Serialize,
) -> Result<(), CoreError> {
    write_json_value(writer, serde_json::to_value(value).map_err(|error| {
        CoreError::new(
            ErrorCode::Internal,
            ErrorSource::Rpc,
            "Retcon could not encode a response.",
            error.to_string(),
        )
    })?).await
}

async fn write_json_value(
    writer: &mut retcon_platform::TransportWriteHalf,
    message: serde_json::Value,
) -> Result<(), CoreError> {
    let mut encoded = serde_json::to_vec(&message).map_err(|error| {
        CoreError::new(
            ErrorCode::Internal,
            ErrorSource::Rpc,
            "Retcon could not encode a response.",
            error.to_string(),
        )
    })?;
    encoded.push(b'\n');
    writer
        .write_all(&encoded)
        .await
        .map_err(|error| CoreError::io("write RPC response", error))?;
    Ok(())
}

async fn dispatch(request: Request, state: &CoreState) -> Response {
    if request.method == "turn.send"
        && let Some(prompt) = request.params.get("prompt").and_then(|value| value.as_str())
    {
        let scan = retcon_secrets::scan_text(prompt);
        if !scan.is_clean() {
            return Response::error(
                request.id,
                &CoreError::new(
                    ErrorCode::PermissionDenied,
                    ErrorSource::Rpc,
                    "Retcon blocked this turn because the prompt may contain secrets.",
                    format!("secret scan blocked turn.send: {}", scan.summary()),
                )
                .suggested_fix(
                    "Remove API keys, tokens, passwords, and private keys from the prompt before sending.",
                ),
            );
        }
    }

    if !matches!(request.method.as_str(), "approval.list" | "approval.decide" | "permission.rules.list" | "permission.rules.create" | "permission.rules.delete" | "secrets.scan") {
        let project_id = request
            .params
            .get("projectId")
            .and_then(|value| value.as_str())
            .and_then(|raw| Uuid::parse_str(raw).ok());
        let check = state
            .permissions()
            .check_rpc(&request.method, &request.params, project_id);
        for record in &check.audit {
            state.emit(&record.kind, record.payload.clone());
        }
        if let retcon_permissions::RpcPermission::Denied {
            user_message,
            technical_message,
        } = check.permission
        {
            let approval_id = check
                .audit
                .first()
                .and_then(|record| record.payload.get("id").and_then(|value| value.as_str()))
                .map(str::to_owned);
            let mut error = CoreError::new(
                ErrorCode::PermissionDenied,
                ErrorSource::Rpc,
                user_message,
                technical_message,
            )
            .suggested_fix(
                "Approve this action in the Retcon approval center, then retry the request with approvalId.",
            )
            .retryable(true);
            if let Some(approval_id) = approval_id {
                error = error.diagnostic(json!({"approvalId": approval_id}));
            }
            return Response::error(request.id, &error);
        }
    }

    let result = match request.method.as_str() {
        "core.health" => Some(state.health()),
        "core.version" => Some(json!({"version": env!("CARGO_PKG_VERSION"), "protocolVersion": 1})),
        "core.capabilities" => Some(state.capabilities()),
        "core.diagnostics" => Some(state.diagnostics().await),
        "core.shutdown" => {
            state.request_shutdown();
            Some(json!({"accepted": true}))
        }
        "events.replay" => {
            let after = request
                .params
                .get("afterSequence")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let limit = request
                .params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(100) as usize;
            Some(
                json!({"events": state.events().replay(after, limit), "latestSequence": state.events().latest_sequence()}),
            )
        }
        "events.emit" => {
            let kind = request
                .params
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("system.test");
            let payload = request
                .params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match state.events().emit(kind, payload) {
                Ok(event) => Some(json!(event)),
                Err(error) => return Response::error(request.id, &error),
            }
        }
        "jobs.list" => {
            let offset = request
                .params
                .get("offset")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let limit = request
                .params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(100) as usize;
            Some(json!({"jobs": state.jobs().list(offset, limit)}))
        }
        "jobs.get" | "jobs.cancel" | "jobs.forceStop" => {
            let Some(raw) = request.params.get("id").and_then(|v| v.as_str()) else {
                return Response::error(request.id, &invalid_request("missing job id"));
            };
            let Ok(id) = uuid::Uuid::parse_str(raw) else {
                return Response::error(request.id, &invalid_request("invalid job id"));
            };
            match request.method.as_str() {
                "jobs.get" => Some(json!({"job": state.jobs().get(id)})),
                "jobs.cancel" => Some(json!({"accepted": state.jobs().cancel(id)})),
                _ => Some(json!({"accepted": state.jobs().force_stop(id)})),
            }
        }
        _ => None,
    };

    match result {
        Some(result) => Response::ok(request.id, result),
        None => match request.method.split('.').next() {
            Some("project") => crate::projects_rpc::handle(state.clone(), request).await,
            Some("provider") => crate::providers_rpc::handle(request).await,
            Some("session") | Some("turn") => crate::session_rpc::handle(state.clone(), request).await,
            Some("terminal") => crate::spikes::terminal::handle(state.clone(), request).await,
            Some("git") => crate::git_rpc::handle(state.clone(), request).await,
            Some("agent") => crate::spikes::agent::handle(state.clone(), request).await,
            Some("browser") => crate::spikes::browser::handle(state.clone(), request).await,
            Some("storage") => crate::storage_rpc::handle(state.clone(), request).await,
            Some("file") => crate::file_rpc::handle(state.clone(), request).await,
            Some("checkpoint") => crate::checkpoints_rpc::handle(state.clone(), request).await,
            Some("task") => crate::tasks_rpc::handle(state.clone(), request).await,
            Some("verification") => crate::verification_rpc::handle(state.clone(), request).await,
            Some("approval") | Some("permission") => {
                crate::permissions_rpc::handle(state.clone(), request).await
            }
            Some("secrets") => crate::secrets_rpc::handle(state.clone(), request).await,
            _ => Response::error(
                request.id,
                &CoreError::new(
                    ErrorCode::NotFound,
                    ErrorSource::Rpc,
                    "The requested core operation is not available.",
                    format!("unknown RPC method: {}", request.method),
                ),
            ),
        },
    }
}

fn invalid_request(technical_message: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        ErrorSource::Rpc,
        "Retcon received an invalid local request.",
        technical_message,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_mut)]
mod tests {
    use super::*;
    use retcon_platform::TransportStream;
    use retcon_protocol::{ClientHelloKind, Ping, PingKind};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    async fn read_rpc_response(
        lines: &mut tokio::io::Lines<BufReader<retcon_platform::TransportReadHalf>>,
    ) -> serde_json::Value {
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(&line).expect("json line");
            if value.get("id").is_some() {
                return value;
            }
        }
        panic!("transport closed before RPC response");
    }

    async fn connect_client(
        endpoint: &retcon_platform::LocalEndpoint,
        token: &str,
    ) -> (
        BufReader<retcon_platform::TransportReadHalf>,
        retcon_platform::TransportWriteHalf,
    ) {
        let stream = TransportStream::connect(endpoint).await.unwrap();
        let (reader, mut writer) = stream.into_split();
        write_json_line(
            &mut writer,
            &AuthLine {
                auth: token.to_owned(),
            },
        )
        .await
        .unwrap();
        write_json_line(
            &mut writer,
            &ClientHello {
                kind: ClientHelloKind::Hello,
                protocol_version: VERSION,
                client_version: "test".into(),
                features: vec!["ping".into(), "events.replay".into()],
            },
        )
        .await
        .unwrap();
        let mut reader = BufReader::new(reader);
        let mut hello_line = String::new();
        reader.read_line(&mut hello_line).await.unwrap();
        assert!(hello_line.contains("server.hello"));
        (reader, writer)
    }

    #[tokio::test]
    async fn ping_pong_and_malformed_messages_do_not_panic() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let token = uuid::Uuid::new_v4().to_string();
        let server = Server::bind(state.clone(), token.clone(), directory.path())
            .await
            .unwrap();
        let endpoint = server.endpoint().clone();
        let server_task = tokio::spawn(server.run());

        let (_reader, mut writer) = connect_client(&endpoint, &token).await;
        write_json_line(&mut writer, &Ping { kind: PingKind::Ping })
            .await
            .unwrap();

        let directory2 = directory.path().to_owned();
        let token2 = token.clone();
        let endpoint2 = endpoint.clone();
        tokio::spawn(async move {
            let stream = TransportStream::connect(&endpoint2).await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            write_json_line(
                &mut writer,
                &AuthLine {
                    auth: token2.clone(),
                },
            )
            .await
            .unwrap();
            write_json_line(
                &mut writer,
                &ClientHello {
                    kind: ClientHelloKind::Hello,
                    protocol_version: VERSION,
                    client_version: "test".into(),
                    features: vec![],
                },
            )
            .await
            .unwrap();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            writer
                .write_all(b"{\"id\":1,\"method\":\"core.health\",\"params\":{},\"extra\":true}\n")
                .await
                .unwrap();
            let mut response = String::new();
            reader.read_line(&mut response).await.unwrap();
            assert!(response.contains("error"));
        });

        state.request_shutdown();
        server_task.await.unwrap();
        let _ = directory2;
    }

    #[tokio::test]
    async fn blocked_rpc_methods_return_permission_denied() {
        // Ensure dev bypass is off even when the outer test harness sets it.
        unsafe {
            std::env::remove_var("RETCON_PERMISSIONS_BYPASS");
        }
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let token = uuid::Uuid::new_v4().to_string();
        let server = Server::bind(state.clone(), token.clone(), directory.path())
            .await
            .unwrap();
        let endpoint = server.endpoint().clone();
        let server_task = tokio::spawn(server.run());

        let (reader, mut writer) = connect_client(&endpoint, &token).await;
        write_json_line(
            &mut writer,
            &retcon_protocol::Request {
                id: 42,
                method: "agent.start".into(),
                params: json!({"cwd": directory.path()}),
            },
        )
        .await
        .unwrap();

        let mut lines = reader.lines();
        let response = read_rpc_response(&mut lines).await;
        let encoded = response.to_string();
        assert!(
            encoded.contains("permission_denied"),
            "expected permission_denied, got: {encoded}"
        );
        assert!(encoded.contains("explicit approval"));

        state.request_shutdown();
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn client_can_reconnect_after_disconnect() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let token = uuid::Uuid::new_v4().to_string();
        let server = Server::bind(state.clone(), token.clone(), directory.path())
            .await
            .unwrap();
        let endpoint = server.endpoint().clone();
        let server_task = tokio::spawn(server.run());

        for _ in 0..2 {
            let (reader, mut writer) = connect_client(&endpoint, &token).await;
            write_json_line(
                &mut writer,
                &retcon_protocol::Request {
                    id: 1,
                    method: "core.health".into(),
                    params: json!({}),
                },
            )
            .await
            .unwrap();
            let mut lines = reader.lines();
            let response = read_rpc_response(&mut lines).await;
            assert!(response.to_string().contains("healthy"));
            drop(writer);
            drop(lines);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        state.request_shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        server_task.await.unwrap();
    }
}
