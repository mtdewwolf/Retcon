//! Loopback-only RPC server for the core lifecycle endpoints.

#![allow(missing_docs)] // Phase 2 API; public documentation lands with the generated protocol.

use std::net::SocketAddr;

use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::frame::read_capped_line;
use crate::rpc::{Request, Response};
use crate::state::CoreState;
use retcon_protocol::AuthLine;

const MAX_RPC_CONNECTIONS: usize = 64;

pub struct Server {
    listener: TcpListener,
    token: String,
    state: CoreState,
}

impl Server {
    pub async fn bind(state: CoreState, token: String) -> Result<Self, CoreError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| CoreError::io("bind core RPC listener", error))?;
        Ok(Self {
            listener,
            token,
            state,
        })
    }

    pub fn address(&self) -> Result<SocketAddr, CoreError> {
        self.listener
            .local_addr()
            .map_err(|error| CoreError::io("read RPC address", error))
    }

    pub async fn run(self) {
        let mut shutdown = self.state.shutdown_receiver();
        let mut connections = JoinSet::new();
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            let at_capacity = connections.len() >= MAX_RPC_CONNECTIONS;
            tokio::select! {
                _ = heartbeat.tick() => {
                    let _ = self.state.events().emit_volatile(
                        "system.heartbeat",
                        json!({"uptime_ms": self.state.uptime().as_millis()}),
                    );
                },
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
                result = self.listener.accept(), if !at_capacity => match result {
                    Ok((stream, _)) => {
                        let token = self.token.clone();
                        let state = self.state.clone();
                        connections.spawn(async move {
                            if let Err(error) = serve_connection(stream, &token, state).await {
                                tracing::warn!(error_id = %error.id, error = %error.technical_message, "RPC connection ended");
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
    stream: TcpStream,
    expected_token: &str,
    state: CoreState,
) -> Result<(), CoreError> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let auth_line = read_capped_line(&mut reader)
        .await?
        .ok_or_else(|| invalid_request("Authentication is required."))?;
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

    let mut events = state.events().subscribe();

    loop {
        let message = tokio::select! {
            line = read_capped_line(&mut reader) => {
                let Some(line) = line? else { break };
                let response = match serde_json::from_str::<Request>(&line) {
                    Ok(request) => dispatch(request, &state).await,
                    Err(error) => Response::error(0, &invalid_request(error.to_string())),
                };
                serde_json::to_value(response)
            }
            event = events.recv() => match event {
                Ok(event) => Ok(json!({"event": &*event})),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => Ok(json!({"event": {"kind":"system.eventsLagged", "payload":{"skipped":skipped}}})),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }.map_err(|error| CoreError::new(ErrorCode::Internal, ErrorSource::Rpc, "Retcon could not encode a response.", error.to_string()))?;
        let mut encoded = serde_json::to_vec(&message).map_err(|error| {
            CoreError::new(
                ErrorCode::Internal,
                ErrorSource::Rpc,
                "Retcon could not encode a response.",
                error.to_string(),
            )
        })?;
        encoded.push(b'\n');
        if encoded.len() > retcon_protocol::MAX_FRAME_BYTES + 1 {
            return Err(CoreError::new(
                ErrorCode::Internal,
                ErrorSource::Rpc,
                "Retcon could not send an oversized local response.",
                format!(
                    "outbound frame exceeds {} bytes",
                    retcon_protocol::MAX_FRAME_BYTES
                ),
            ));
        }
        writer
            .write_all(&encoded)
            .await
            .map_err(|error| CoreError::io("write RPC response", error))?;
    }
    Ok(())
}

async fn dispatch(request: Request, state: &CoreState) -> Response {
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
            Some("terminal") => crate::spikes::terminal::handle(state.clone(), request).await,
            Some("git") => crate::spikes::git::handle(request).await,
            Some("agent") => crate::spikes::agent::handle(state.clone(), request).await,
            Some("browser") => crate::spikes::browser::handle(state.clone(), request).await,
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
