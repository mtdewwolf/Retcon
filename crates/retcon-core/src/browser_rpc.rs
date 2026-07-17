//! Durable RPC orchestration for the managed browser service.

use std::path::{Component, Path, PathBuf};

use retcon_browser::{
    BROWSER_SERVICE_PROTOCOL, BrowserCallResult, BrowserLaunchRequest, BrowserServiceArtifact,
    MAX_SERVICE_ARTIFACT_BYTES,
};
use retcon_storage::{
    BrowserObservation, BrowserTab, DurableBrowserSession, NewDurableBrowserSession, StorageError,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;

const ACTOR: &str = "local_user";
const MAX_SCRIPT_BYTES: usize = 64 * 1024;
const MAX_PATHS: usize = 64;
const MAX_SERVICE_ARTIFACTS: usize = 16;
const MAX_SERVICE_ARTIFACT_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_SERVICE_METADATA_BYTES: usize = 256 * 1024;
const MAX_LOG_ENTRIES: usize = 1_000;
const MAX_LOG_ENTRY_BYTES: usize = 64 * 1024;

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "browser.startService" | "browser.stopService" | "browser.call" => {
            crate::spikes::browser::handle(state, Request { id, method, params }).await
        }
        "browser.session.start" => start_session(&state, id, &params).await,
        "browser.session.stop" => stop_session(&state, id, &params).await,
        "browser.session.list" => list_sessions(&state, id, &params),
        "browser.session.status" => status(&state, id, &params),
        "browser.session.history" => history(&state, id, &params),
        "browser.service.diagnostics" => diagnostics(&state, id),
        "browser.tab.list" => list_tabs(&state, id, &params),
        "browser.tab.open"
        | "browser.tab.close"
        | "browser.tab.activate"
        | "browser.navigate"
        | "browser.back"
        | "browser.forward"
        | "browser.reload"
        | "browser.observation.screenshot"
        | "browser.observation.snapshot"
        | "browser.observation.logs"
        | "browser.observation.trace"
        | "browser.automation.action"
        | "browser.automation.script"
        | "browser.automation.upload"
        | "browser.automation.download" => call_operation(&state, id, &method, params).await,
        "browser.observation.list" => observations(&state, id, &params),
        "browser.takeover.start" => takeover_start(&state, id, &params).await,
        "browser.takeover.stop" => takeover_stop(&state, id, &params).await,
        "browser.takeover.status" => takeover_status(&state, id, &params),
        _ => missing(id, "browser method"),
    }
}

async fn start_session(state: &CoreState, id: u64, params: &Value) -> Response {
    let project_id = match uuid_param(params, "projectId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let task_id = match optional_uuid(params, "taskId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let dev_server_instance_id = match optional_uuid(params, "devServerInstanceId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let network_policy = params
        .get("networkPolicy")
        .and_then(Value::as_str)
        .unwrap_or("loopback");
    if !matches!(network_policy, "loopback" | "network") {
        return invalid(id, "networkPolicy must be 'loopback' or 'network'");
    }
    let diagnostics = match state.browser_service().diagnostics() {
        Ok(value) if value.compatible() => value,
        Ok(value) => {
            return Response::error(
                id,
                &CoreError::new(
                    ErrorCode::Internal,
                    ErrorSource::System,
                    "The browser service is incompatible.",
                    format!(
                        "browser service version '{}' protocol {} is not compatible with protocol {}",
                        value.service_version, value.protocol_version, BROWSER_SERVICE_PROTOCOL
                    ),
                ),
            );
        }
        Err(error) => return service_error(id, "inspect browser service", error.to_string()),
    };
    let session_id = Uuid::new_v4();
    let profile_id = Uuid::new_v4();
    let profile_path = state
        .storage()
        .data_dir()
        .join("browser-profiles")
        .join(profile_id.to_string());
    let input = NewDurableBrowserSession {
        id: session_id,
        project_id,
        task_id,
        dev_server_instance_id,
        profile_id,
        profile_path: profile_path.to_string_lossy().into_owned(),
        persistent_profile: params
            .get("persistentProfile")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        network_policy: network_policy.into(),
    };
    let repository = state.storage().database().durable_browsers();
    let prepared = match repository.prepare_session(&input, ACTOR) {
        Ok(value) => value,
        Err(error) => return storage_error(id, error),
    };
    let launch = BrowserLaunchRequest {
        session_id,
        profile_path: input.profile_path,
        network_policy: input.network_policy,
    };
    match state.browser_service().launch(&launch).await {
        Ok(started) if !started.service_session_id.trim().is_empty() => {
            let session = match repository.mark_running(
                session_id,
                &started.service_session_id,
                &diagnostics.service_version,
                i64::from(diagnostics.protocol_version),
                ACTOR,
            ) {
                Ok(Some(value)) => value,
                Ok(None) => return missing(id, "browser session"),
                Err(error) => {
                    let _ = state.browser_service().close(session_id).await;
                    return storage_error(id, error);
                }
            };
            let initial_tab = match started.initial_tab.as_ref() {
                Some(tab) => {
                    match persist_tab(&repository, session_id, tab, &session.network_policy) {
                        Ok(tab) => Some(tab),
                        Err(error) => {
                            let failure = scrub(&format!(
                                "browser service returned an invalid initial tab: {error}"
                            ));
                            fail_closed_service_session(state, &repository, &session, &failure)
                                .await;
                            return service_error(id, "launch browser session", failure);
                        }
                    }
                }
                None => None,
            };
            emit(
                state,
                "browser.session_started",
                &session,
                json!({"serviceVersion":diagnostics.service_version}),
            );
            Response::ok(id, json!({"session":session,"initialTab":initial_tab}))
        }
        Ok(_) => {
            let failure = "browser service returned an empty session id";
            let _ = repository.mark_failed(prepared.id, failure, "service");
            service_error(id, "launch browser session", failure)
        }
        Err(error) => {
            let failure = scrub(&error.to_string());
            let _ = repository.mark_failed(prepared.id, &failure, "service");
            service_error(id, "launch browser session", failure)
        }
    }
}

async fn stop_session(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let repository = state.storage().database().durable_browsers();
    let session = match repository.begin_stop(session_id, ACTOR) {
        Ok(Some(value)) => value,
        Ok(None) => return missing(id, "browser session"),
        Err(error) => return storage_error(id, error),
    };
    match state.browser_service().close(session_id).await {
        Ok(()) => match repository.mark_stopped(session_id, ACTOR) {
            Ok(Some(stopped)) => {
                emit(state, "browser.session_stopped", &stopped, json!({}));
                Response::ok(id, json!({"session":stopped}))
            }
            Ok(None) => missing(id, "browser session"),
            Err(error) => storage_error(id, error),
        },
        Err(error) => {
            let failure = scrub(&error.to_string());
            let _ = repository.mark_orphaned(session.id, &failure, "service");
            service_error(id, "stop browser session", failure)
        }
    }
}

fn list_sessions(state: &CoreState, id: u64, params: &Value) -> Response {
    let project_id = match uuid_param(params, "projectId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .durable_browsers()
        .list_sessions(project_id)
    {
        Ok(sessions) => Response::ok(id, json!({"sessions":sessions})),
        Err(error) => storage_error(id, error),
    }
}

fn status(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let repository = state.storage().database().durable_browsers();
    match (
        repository.session(session_id),
        repository.list_tabs(session_id),
        repository.active_takeover(session_id),
    ) {
        (Ok(Some(session)), Ok(tabs), Ok(takeover)) => Response::ok(
            id,
            json!({"session":session,"tabs":tabs,"takeover":takeover}),
        ),
        (Ok(None), _, _) => missing(id, "browser session"),
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => storage_error(id, error),
    }
}

fn history(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .durable_browsers()
        .history(session_id)
    {
        Ok(events) => Response::ok(id, json!({"events":events})),
        Err(error) => storage_error(id, error),
    }
}

fn diagnostics(state: &CoreState, id: u64) -> Response {
    match state.browser_service().diagnostics() {
        Ok(value) => Response::ok(
            id,
            json!({"service":value,"compatible":value.compatible(),"requiredProtocol":BROWSER_SERVICE_PROTOCOL}),
        ),
        Err(error) => service_error(id, "inspect browser service", error.to_string()),
    }
}

fn list_tabs(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .durable_browsers()
        .list_tabs(session_id)
    {
        Ok(tabs) => Response::ok(id, json!({"tabs":tabs})),
        Err(error) => storage_error(id, error),
    }
}

async fn call_operation(
    state: &CoreState,
    id: u64,
    rpc_method: &str,
    mut params: Value,
) -> Response {
    let session_id = match uuid_param(&params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let repository = state.storage().database().durable_browsers();
    let Some(session) = (match repository.session(session_id) {
        Ok(value) => value,
        Err(error) => return storage_error(id, error),
    }) else {
        return missing(id, "browser session");
    };
    if session.status != "running" {
        return invalid(id, "browser session is not running");
    }
    let mut tab = None;
    if let Some(tab_id) = match optional_uuid(&params, "tabId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    } {
        tab = match repository.tab(tab_id) {
            Ok(Some(value)) if value.browser_session_id == session_id && value.status == "open" => {
                Some(value)
            }
            Ok(Some(_)) => return invalid(id, "browser tab is not open in this session"),
            Ok(None) => return missing(id, "browser tab"),
            Err(error) => return storage_error(id, error),
        };
    }
    let observation = rpc_method.starts_with("browser.observation.");
    if !observation {
        match repository.active_takeover(session_id) {
            Ok(Some(_)) => return invalid(id, "browser automation is paused during takeover"),
            Ok(None) => {}
            Err(error) => return storage_error(id, error),
        }
    }
    if let Err(error) = validate_operation(state, &session, rpc_method, &mut params) {
        return invalid(id, error);
    }
    if let Some(tab) = &tab {
        params["serviceTabId"] = Value::String(tab.service_tab_id.clone());
    }
    let service_method = rpc_method;
    let result = match state
        .browser_service()
        .call(session_id, service_method, params.clone())
        .await
    {
        Ok(value) => value,
        Err(error) => {
            let failure = scrub(&error.to_string());
            if error.is_fatal() {
                let _ = repository.mark_orphaned(session.id, &failure, "service");
                emit(
                    state,
                    "browser.session_orphaned",
                    &session,
                    json!({"failure":failure}),
                );
            }
            return service_error(id, service_method, failure);
        }
    };
    if let Err(error) = validate_service_result(&session, rpc_method, &result) {
        let failure = scrub(&format!(
            "browser service violated its response contract: {error}"
        ));
        fail_closed_service_session(state, &repository, &session, &failure).await;
        return service_error(id, service_method, failure);
    }
    match persist_call_result(state, &session, tab.as_ref(), rpc_method, result) {
        Ok((value, artifacts, stored_tab)) => {
            let _ = repository.record_event(
                session_id,
                rpc_method,
                ACTOR,
                &json!({"tabId":tab.as_ref().map(|value|value.id),"artifacts":artifacts.iter().map(|value|&value.artifact_hash).collect::<Vec<_>>()}),
            );
            emit(
                state,
                rpc_method,
                &session,
                json!({"tabId":tab.as_ref().map(|value|value.id)}),
            );
            Response::ok(
                id,
                json!({"result":value,"artifacts":artifacts,"tab":stored_tab}),
            )
        }
        Err(error) => storage_error(id, error),
    }
}

fn validate_operation(
    state: &CoreState,
    session: &DurableBrowserSession,
    method: &str,
    params: &mut Value,
) -> Result<(), String> {
    if matches!(method, "browser.navigate" | "browser.tab.open") {
        let url = params.get("url").and_then(Value::as_str);
        if method == "browser.navigate" && url.is_none() {
            return Err("browser.navigate requires url".to_owned());
        }
        if let Some(url) = url
            && url != "about:blank"
        {
            validate_url(url, &session.network_policy)?;
        }
    }
    if method == "browser.automation.script" {
        let script = params
            .get("script")
            .and_then(Value::as_str)
            .ok_or_else(|| "browser.automation.script requires script".to_owned())?;
        if script.is_empty() || script.len() > MAX_SCRIPT_BYTES || script.contains('\0') {
            return Err("browser script must be non-empty, NUL-free, and at most 64 KiB".into());
        }
    }
    if method == "browser.automation.action" {
        let action = params
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(action, "click" | "fill" | "press" | "text" | "select") {
            return Err("unsupported browser automation action".into());
        }
    }
    if matches!(
        method,
        "browser.automation.upload" | "browser.automation.download"
    ) {
        let root = project_root(state, session)?;
        let field = if method.ends_with("upload") {
            "paths"
        } else {
            "destination"
        };
        if field == "paths" {
            let paths = params
                .get(field)
                .and_then(Value::as_array)
                .ok_or_else(|| "browser upload requires paths".to_owned())?;
            if paths.is_empty() || paths.len() > MAX_PATHS {
                return Err("browser upload requires between 1 and 64 paths".into());
            }
            let mut resolved = Vec::with_capacity(paths.len());
            for path in paths {
                let raw = path
                    .as_str()
                    .ok_or_else(|| "browser upload paths must be strings".to_owned())?;
                let path = contained_project_path(&root, safe_project_path(&root, raw)?, true)?;
                resolved.push(Value::String(path.to_string_lossy().into_owned()));
            }
            params[field] = Value::Array(resolved);
        } else {
            let raw = params
                .get(field)
                .and_then(Value::as_str)
                .ok_or_else(|| "browser download requires destination".to_owned())?;
            let path = contained_project_path(&root, safe_project_path(&root, raw)?, false)?;
            params[field] = Value::String(path.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn validate_service_result(
    session: &DurableBrowserSession,
    method: &str,
    result: &BrowserCallResult,
) -> Result<(), String> {
    if matches!(
        method,
        "browser.tab.open"
            | "browser.navigate"
            | "browser.back"
            | "browser.forward"
            | "browser.reload"
    ) {
        let url = result.value.get("url").and_then(Value::as_str);
        if matches!(method, "browser.tab.open" | "browser.navigate") && url.is_none() {
            return Err(format!("{method} response omitted url"));
        }
        if let Some(url) = url
            && url != "about:blank"
        {
            validate_url(url, &session.network_policy)?;
        }
    }
    Ok(())
}

async fn fail_closed_service_session(
    state: &CoreState,
    repository: &retcon_storage::BrowserRepository<'_>,
    session: &DurableBrowserSession,
    failure: &str,
) {
    let terminal_status = if state.browser_service().close(session.id).await.is_ok() {
        let _ = repository.mark_failed(session.id, failure, "service");
        "failed"
    } else {
        let _ = repository.mark_orphaned(session.id, failure, "service");
        "orphaned"
    };
    emit(
        state,
        "browser.session_failed_closed",
        session,
        json!({"failure":failure,"terminalStatus":terminal_status}),
    );
}

fn validate_log_entries(entries: &[Value], kind: &str) -> retcon_storage::Result<()> {
    if entries.len() > MAX_LOG_ENTRIES {
        return Err(StorageError::Validation(format!(
            "browser {kind} log response exceeds {MAX_LOG_ENTRIES} entries"
        )));
    }
    for entry in entries {
        let size = serde_json::to_vec(entry)
            .map_err(|error| {
                StorageError::Validation(format!("could not encode browser {kind} log: {error}"))
            })?
            .len();
        if size > MAX_LOG_ENTRY_BYTES {
            return Err(StorageError::Validation(format!(
                "browser {kind} log entry exceeds 64 KiB"
            )));
        }
    }
    Ok(())
}

fn validate_service_artifacts(artifacts: &[BrowserServiceArtifact]) -> retcon_storage::Result<()> {
    if artifacts.len() > MAX_SERVICE_ARTIFACTS {
        return Err(StorageError::Validation(format!(
            "browser service returned more than {MAX_SERVICE_ARTIFACTS} artifacts"
        )));
    }
    let mut total = 0_usize;
    for artifact in artifacts {
        total = total.checked_add(artifact.bytes.len()).ok_or_else(|| {
            StorageError::Validation("browser artifact byte count overflowed".into())
        })?;
        if artifact.bytes.len() > MAX_SERVICE_ARTIFACT_BYTES
            || total > MAX_SERVICE_ARTIFACT_TOTAL_BYTES
        {
            return Err(StorageError::Validation(
                "browser service artifacts exceed configured byte limits".into(),
            ));
        }
        if artifact.kind.is_empty()
            || artifact.kind.len() > 64
            || !artifact
                .kind
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            || artifact.mime_type.is_empty()
            || artifact.mime_type.len() > 128
            || artifact
                .mime_type
                .bytes()
                .any(|byte| byte.is_ascii_control())
        {
            return Err(StorageError::Validation(
                "browser artifact kind or MIME type is invalid".into(),
            ));
        }
        let metadata_size = serde_json::to_vec(&artifact.metadata)
            .map_err(|error| {
                StorageError::Validation(format!(
                    "could not encode browser artifact metadata: {error}"
                ))
            })?
            .len();
        if metadata_size > MAX_SERVICE_METADATA_BYTES {
            return Err(StorageError::Validation(
                "browser artifact metadata exceeds 256 KiB".into(),
            ));
        }
    }
    Ok(())
}

fn persist_call_result(
    state: &CoreState,
    session: &DurableBrowserSession,
    tab: Option<&BrowserTab>,
    method: &str,
    mut result: BrowserCallResult,
) -> retcon_storage::Result<(Value, Vec<BrowserObservation>, Option<BrowserTab>)> {
    result.value = retcon_secrets::scrub_json(result.value);
    if method == "browser.observation.logs" {
        let console = result
            .value
            .get("console")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let network = result
            .value
            .get("network")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        validate_log_entries(&console, "console")?;
        validate_log_entries(&network, "network")?;
        let bytes = serde_json::to_vec(&result.value).map_err(|error| {
            StorageError::Validation(format!("could not encode browser log artifact: {error}"))
        })?;
        if bytes.len() > MAX_SERVICE_ARTIFACT_BYTES {
            return Err(StorageError::Validation(
                "browser log artifact exceeds 16 MiB".into(),
            ));
        }
        let repository = state.storage().database().durable_browsers();
        repository.append_console(session.id, &console)?;
        repository.append_network(session.id, &network)?;
        result.artifacts.push(BrowserServiceArtifact {
            kind: "logs".into(),
            mime_type: "application/json".into(),
            bytes,
            metadata: json!({"bounded":true}),
        });
    }
    validate_service_artifacts(&result.artifacts)?;
    let repository = state.storage().database().durable_browsers();
    let mut observations = Vec::new();
    for service_artifact in result.artifacts {
        if service_artifact.bytes.len() > MAX_SERVICE_ARTIFACT_BYTES {
            return Err(StorageError::Validation(format!(
                "browser artifact '{}' exceeds 16 MiB",
                service_artifact.kind
            )));
        }
        if service_artifact.kind.trim().is_empty() || service_artifact.mime_type.trim().is_empty() {
            return Err(StorageError::Validation(
                "browser artifact kind and MIME type are required".into(),
            ));
        }
        let artifact = state
            .storage()
            .artifacts()
            .store_bytes(&service_artifact.bytes)?;
        let verified = state.storage().artifacts().verify(&artifact.hash)?;
        if verified.size != artifact.size {
            return Err(StorageError::Validation(
                "browser artifact size changed while storing".into(),
            ));
        }
        observations.push(repository.record_observation(
            session.id,
            tab.map(|value| value.id),
            &service_artifact.kind,
            &artifact.hash,
            &service_artifact.mime_type,
            i64::try_from(artifact.size).unwrap_or(i64::MAX),
            &retcon_secrets::scrub_json(service_artifact.metadata),
            "service",
        )?);
    }
    let stored_tab = if method == "browser.tab.open" {
        Some(persist_tab(
            &repository,
            session.id,
            &result.value,
            &session.network_policy,
        )?)
    } else if method == "browser.tab.close" {
        if let Some(tab) = tab {
            repository.close_tab(session.id, tab.id, ACTOR)?;
        }
        None
    } else if matches!(
        method,
        "browser.navigate" | "browser.back" | "browser.forward" | "browser.reload"
    ) {
        if let Some(tab) = tab {
            let reported_url = result.value.get("url").and_then(Value::as_str);
            if let Some(url) = reported_url {
                validate_url(url, &session.network_policy).map_err(StorageError::Validation)?;
            }
            Some(repository.upsert_tab(
                session.id,
                &tab.service_tab_id,
                reported_url,
                result.value.get("title").and_then(Value::as_str),
                "service",
            )?)
        } else {
            None
        }
    } else {
        None
    };
    Ok((result.value, observations, stored_tab))
}

fn persist_tab(
    repository: &retcon_storage::BrowserRepository<'_>,
    session_id: Uuid,
    value: &Value,
    network_policy: &str,
) -> retcon_storage::Result<BrowserTab> {
    let service_tab_id = value
        .get("serviceTabId")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| StorageError::Validation("browser service omitted tab id".into()))?;
    let url = value.get("url").and_then(Value::as_str);
    if let Some(url) = url
        && url != "about:blank"
    {
        validate_url(url, network_policy).map_err(StorageError::Validation)?;
    }
    repository.upsert_tab(
        session_id,
        service_tab_id,
        url,
        value.get("title").and_then(Value::as_str),
        "service",
    )
}

fn observations(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let repository = state.storage().database().durable_browsers();
    match (
        repository.list_observations(session_id),
        repository.console(session_id, 1_000),
        repository.network(session_id, 1_000),
    ) {
        (Ok(observations), Ok(console), Ok(network)) => Response::ok(
            id,
            json!({"observations":observations,"console":console,"network":network}),
        ),
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => storage_error(id, error),
    }
}

async fn takeover_start(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let reason = params.get("reason").and_then(Value::as_str).map(scrub);
    let reason = reason.as_deref();
    let repository = state.storage().database().durable_browsers();
    let takeover = match repository.start_takeover(session_id, ACTOR, reason) {
        Ok(value) => value,
        Err(error) => return storage_error(id, error),
    };
    match state
        .browser_service()
        .call(
            session_id,
            "browser.takeover.start",
            json!({"reason":reason}),
        )
        .await
    {
        Ok(_) => {
            state.emit(
                "browser.takeover_started",
                json!({"sessionId":session_id,"takeoverId":takeover.id}),
            );
            Response::ok(id, json!({"takeover":takeover}))
        }
        Err(error) => {
            let _ = repository.stop_takeover(session_id, "service", "service_error");
            let failure = scrub(&error.to_string());
            if error.is_fatal() {
                let _ = repository.mark_orphaned(session_id, &failure, "service");
            }
            service_error(id, "start browser takeover", failure)
        }
    }
}

async fn takeover_stop(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    if let Err(error) = state
        .browser_service()
        .call(session_id, "browser.takeover.stop", json!({}))
        .await
    {
        let failure = scrub(&error.to_string());
        if error.is_fatal() {
            let _ = state
                .storage()
                .database()
                .durable_browsers()
                .mark_orphaned(session_id, &failure, "service");
        }
        return service_error(id, "stop browser takeover", failure);
    }
    match state
        .storage()
        .database()
        .durable_browsers()
        .stop_takeover(session_id, ACTOR, "released")
    {
        Ok(value) => Response::ok(id, json!({"takeover":value})),
        Err(error) => storage_error(id, error),
    }
}

fn takeover_status(state: &CoreState, id: u64, params: &Value) -> Response {
    let session_id = match uuid_param(params, "sessionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .durable_browsers()
        .active_takeover(session_id)
    {
        Ok(value) => Response::ok(id, json!({"takeover":value})),
        Err(error) => storage_error(id, error),
    }
}

fn project_root(state: &CoreState, session: &DurableBrowserSession) -> Result<PathBuf, String> {
    if let Some(worktree_id) = session.worktree_id {
        return state
            .storage()
            .database()
            .git_worktrees()
            .get(worktree_id)
            .map_err(|error| error.to_string())?
            .map(|worktree| PathBuf::from(worktree.path))
            .ok_or_else(|| "browser worktree does not exist".into());
    }
    state
        .storage()
        .database()
        .projects()
        .primary_location_path(session.project_id)
        .map_err(|error| error.to_string())?
        .map(PathBuf::from)
        .ok_or_else(|| "browser project has no repository location".into())
}

fn safe_project_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if raw.is_empty()
        || raw.contains('\0')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err("browser file paths must be safe project-relative paths".into());
    }
    Ok(root.join(path))
}

fn contained_project_path(root: &Path, path: PathBuf, must_exist: bool) -> Result<PathBuf, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "browser project root does not exist".to_owned())?;
    if must_exist {
        let canonical = path
            .canonicalize()
            .map_err(|_| "browser upload file does not exist".to_owned())?;
        if !canonical.is_file() || !canonical.starts_with(&canonical_root) {
            return Err("browser upload path escapes the project root".into());
        }
        return Ok(canonical);
    }
    if let Ok(metadata) = path.symlink_metadata() {
        if metadata.file_type().is_symlink() {
            return Err("browser download destination cannot be a symbolic link".into());
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| "browser download destination could not be resolved".to_owned())?;
        if !metadata.is_file() || !canonical.starts_with(&canonical_root) {
            return Err("browser download destination escapes the project root".into());
        }
        return Ok(canonical);
    }
    let parent = path
        .parent()
        .ok_or_else(|| "browser download destination has no parent".to_owned())?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| "browser download parent directory does not exist".to_owned())?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err("browser download destination escapes the project root".into());
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| "browser download destination needs a filename".to_owned())?;
    Ok(canonical_parent.join(file_name))
}

fn validate_url(url: &str, network_policy: &str) -> Result<(), String> {
    let remainder = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| "browser navigation only supports HTTP(S) URLs".to_owned())?;
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty()
        || authority.contains('@')
        || url.chars().any(char::is_control)
        || url.len() > 8_192
    {
        return Err("browser navigation URL is invalid".into());
    }
    if network_policy == "loopback" {
        let host = if authority.starts_with('[') {
            authority
                .split(']')
                .next()
                .map(|value| format!("{value}]"))
                .unwrap_or_default()
        } else {
            authority.split(':').next().unwrap_or_default().to_owned()
        };
        if !matches!(
            host.to_ascii_lowercase().as_str(),
            "localhost" | "127.0.0.1" | "[::1]"
        ) {
            return Err("browser session is restricted to loopback URLs".into());
        }
    }
    Ok(())
}

fn emit(state: &CoreState, kind: &str, session: &DurableBrowserSession, details: Value) {
    state.emit(
        kind,
        json!({"sessionId":session.id,"projectId":session.project_id,"taskId":session.task_id,"details":details}),
    );
}

fn uuid_param(params: &Value, name: &str) -> Result<Uuid, String> {
    params
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing '{name}' UUID"))
        .and_then(|raw| Uuid::parse_str(raw).map_err(|_| format!("invalid '{name}' UUID")))
}

fn optional_uuid(params: &Value, name: &str) -> Result<Option<Uuid>, String> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => Uuid::parse_str(raw)
            .map(Some)
            .map_err(|_| format!("invalid '{name}' UUID")),
        _ => Err(format!("'{name}' must be a UUID string or null")),
    }
}

fn invalid(id: u64, message: impl Into<String>) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The browser request is invalid.",
            message,
        ),
    )
}

fn missing(id: u64, kind: &str) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "The requested browser information was not found.",
            format!("{kind} not found"),
        ),
    )
}

fn storage_error(id: u64, error: StorageError) -> Response {
    if let StorageError::Validation(message) = error {
        invalid(id, message)
    } else {
        Response::error(id, &CoreError::from(error))
    }
}

fn service_error(id: u64, operation: &str, error: impl Into<String>) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::Internal,
            ErrorSource::System,
            "The browser service could not complete the request.",
            format!("{operation}: {}", scrub(&error.into())),
        ),
    )
}

fn scrub(message: &str) -> String {
    retcon_secrets::redact_text(message)
        .chars()
        .take(4_096)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use retcon_browser::{
        BrowserFuture, BrowserLaunchResult, BrowserService, BrowserServiceDiagnostics,
        BrowserServiceError,
    };
    use retcon_storage::NewProject;

    use super::*;

    #[derive(Default)]
    struct FakeService {
        calls: Mutex<Vec<String>>,
    }

    impl BrowserService for FakeService {
        fn diagnostics(&self) -> Result<BrowserServiceDiagnostics, BrowserServiceError> {
            Ok(BrowserServiceDiagnostics {
                service_version: "0.1.0".into(),
                protocol_version: BROWSER_SERVICE_PROTOCOL,
                features: vec!["tabs".into(), "trace".into()],
                healthy: true,
            })
        }

        fn launch<'a>(
            &'a self,
            request: &'a BrowserLaunchRequest,
        ) -> BrowserFuture<'a, BrowserLaunchResult> {
            self.calls.lock().unwrap().push("launch".into());
            Box::pin(async move {
                Ok(BrowserLaunchResult {
                    service_session_id: format!("service-{}", request.session_id),
                    initial_tab: Some(
                        json!({"serviceTabId":"tab-1","url":"about:blank","title":""}),
                    ),
                })
            })
        }

        fn close<'a>(&'a self, _session_id: Uuid) -> BrowserFuture<'a, ()> {
            self.calls.lock().unwrap().push("close".into());
            Box::pin(async { Ok(()) })
        }

        fn call<'a>(
            &'a self,
            _session_id: Uuid,
            method: &'a str,
            params: Value,
        ) -> BrowserFuture<'a, BrowserCallResult> {
            self.calls.lock().unwrap().push(method.into());
            Box::pin(async move {
                let value = match method {
                    "browser.tab.open" => {
                        json!({"serviceTabId":"tab-2","url":"about:blank","title":""})
                    }
                    "browser.navigate" => json!({"url":params["url"],"title":"Preview"}),
                    "browser.observation.logs" => {
                        json!({"console":[{"type":"log","text":"ok"}],"network":[{"url":"http://localhost:3000"}]})
                    }
                    _ => json!({"completed":true}),
                };
                let artifacts = if method == "browser.observation.screenshot" {
                    vec![BrowserServiceArtifact {
                        kind: "screenshot".into(),
                        mime_type: "image/png".into(),
                        bytes: b"png".to_vec(),
                        metadata: json!({"width":1280,"height":720}),
                    }]
                } else {
                    Vec::new()
                };
                Ok(BrowserCallResult { value, artifacts })
            })
        }
    }

    #[tokio::test]
    async fn durable_lifecycle_navigation_observation_and_takeover() {
        let directory = tempfile::tempdir().unwrap();
        let service = Arc::new(FakeService::default());
        let state = CoreState::new_with_browser_service(directory.path(), service.clone()).unwrap();
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Web"))
            .unwrap();
        let started = handle(
            state.clone(),
            Request {
                id: 1,
                method: "browser.session.start".into(),
                params: json!({"projectId":project.id}),
            },
        )
        .await;
        let session_id = started.result.as_ref().unwrap()["session"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let initial_tab = started.result.as_ref().unwrap()["initialTab"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let rejected_open = handle(
            state.clone(),
            Request {
                id: 9,
                method: "browser.tab.open".into(),
                params: json!({"sessionId":session_id,"url":"https://example.com"}),
            },
        )
        .await;
        assert!(rejected_open.error.is_some());
        assert!(
            !service
                .calls
                .lock()
                .unwrap()
                .contains(&"browser.tab.open".into())
        );
        let navigated = handle(
            state.clone(),
            Request {
                id: 2,
                method: "browser.navigate".into(),
                params: json!({"sessionId":session_id,"tabId":initial_tab,"url":"http://localhost:3000"}),
            },
        )
        .await;
        assert_eq!(navigated.result.unwrap()["tab"]["title"], "Preview");
        let screenshot = handle(
            state.clone(),
            Request {
                id: 3,
                method: "browser.observation.screenshot".into(),
                params: json!({"sessionId":session_id,"tabId":initial_tab}),
            },
        )
        .await;
        assert_eq!(
            screenshot.result.unwrap()["artifacts"][0]["kind"],
            "screenshot"
        );
        let logs = handle(
            state.clone(),
            Request {
                id: 4,
                method: "browser.observation.logs".into(),
                params: json!({"sessionId":session_id}),
            },
        )
        .await;
        assert_eq!(logs.result.unwrap()["artifacts"][0]["kind"], "logs");
        let takeover = handle(
            state.clone(),
            Request {
                id: 5,
                method: "browser.takeover.start".into(),
                params: json!({"sessionId":session_id,"reason":"inspect"}),
            },
        )
        .await;
        assert!(takeover.result.is_some());
        let blocked = handle(
            state.clone(),
            Request {
                id: 6,
                method: "browser.reload".into(),
                params: json!({"sessionId":session_id,"tabId":initial_tab}),
            },
        )
        .await;
        assert!(blocked.error.is_some());
        handle(
            state.clone(),
            Request {
                id: 7,
                method: "browser.takeover.stop".into(),
                params: json!({"sessionId":session_id}),
            },
        )
        .await;
        let stopped = handle(
            state,
            Request {
                id: 8,
                method: "browser.session.stop".into(),
                params: json!({"sessionId":session_id}),
            },
        )
        .await;
        assert_eq!(stopped.result.unwrap()["session"]["status"], "stopped");
    }

    #[test]
    fn rejects_network_and_path_escape_policy() {
        assert!(validate_url("http://localhost:3000", "loopback").is_ok());
        assert!(validate_url("https://example.com", "loopback").is_err());
        assert!(validate_url("https://example.com", "network").is_ok());
        assert!(safe_project_path(Path::new("C:/repo"), "../secret").is_err());
        assert!(safe_project_path(Path::new("C:/repo"), "assets/file.txt").is_ok());
    }

    #[test]
    fn rejects_existing_symlink_as_download_destination() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("outside.txt");
        std::fs::write(&outside_file, b"outside").unwrap();
        let link = root.path().join("download.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside_file, &link).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&outside_file, &link).is_err() {
            return;
        }
        assert!(contained_project_path(root.path(), link, false).is_err());
    }

    #[derive(Default)]
    struct InvalidInitialTabService {
        closed: Mutex<bool>,
    }

    impl BrowserService for InvalidInitialTabService {
        fn diagnostics(&self) -> Result<BrowserServiceDiagnostics, BrowserServiceError> {
            Ok(BrowserServiceDiagnostics {
                service_version: "0.1.0".into(),
                protocol_version: BROWSER_SERVICE_PROTOCOL,
                features: vec![],
                healthy: true,
            })
        }

        fn launch<'a>(
            &'a self,
            request: &'a BrowserLaunchRequest,
        ) -> BrowserFuture<'a, BrowserLaunchResult> {
            Box::pin(async move {
                Ok(BrowserLaunchResult {
                    service_session_id: format!("service-{}", request.session_id),
                    initial_tab: Some(json!({
                        "serviceTabId":"tab-1",
                        "url":"https://example.com"
                    })),
                })
            })
        }

        fn close<'a>(&'a self, _session_id: Uuid) -> BrowserFuture<'a, ()> {
            *self.closed.lock().unwrap() = true;
            Box::pin(async { Ok(()) })
        }

        fn call<'a>(
            &'a self,
            _session_id: Uuid,
            _method: &'a str,
            _params: Value,
        ) -> BrowserFuture<'a, BrowserCallResult> {
            Box::pin(async { Ok(BrowserCallResult::value(json!({}))) })
        }
    }

    #[tokio::test]
    async fn invalid_initial_tab_fails_closed_and_releases_profile() {
        let directory = tempfile::tempdir().unwrap();
        let service = Arc::new(InvalidInitialTabService::default());
        let state = CoreState::new_with_browser_service(directory.path(), service.clone()).unwrap();
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Web"))
            .unwrap();
        let response = handle(
            state.clone(),
            Request {
                id: 1,
                method: "browser.session.start".into(),
                params: json!({"projectId":project.id}),
            },
        )
        .await;
        assert!(response.error.is_some());
        assert!(*service.closed.lock().unwrap());
        let sessions = state
            .storage()
            .database()
            .durable_browsers()
            .list_sessions(project.id)
            .unwrap();
        assert_eq!(sessions[0].status, "failed");
        let profile = state
            .storage()
            .database()
            .durable_browsers()
            .profile(sessions[0].profile_id)
            .unwrap()
            .unwrap();
        assert_eq!(profile.status, "released");
    }
}
