//! RPC adapter for durable agent sessions and provider turns.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use retcon_agents::{AgentEvent, ClaudeCodeProvider, StartTurnRequest, normalize_claude_stream_event};
use retcon_storage::{NewSession, NewTurn, Session, Turn};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::secrets_rpc::scan_prompt;
use crate::session_engine::{
    InvalidTransition, SessionMachine, SessionState, TurnMachine, TurnState, parse_session_state,
    parse_turn_state,
};
use crate::state::CoreState;

const DEFAULT_PROVIDER_ID: &str = ClaudeCodeProvider::ID;

/// In-memory provider turns and working directories keyed by session id.
#[derive(Default)]
pub struct SessionRegistry {
    sessions: Mutex<HashMap<Uuid, ActiveSession>>,
}

struct ActiveSession {
    cwd: PathBuf,
    provider_id: String,
    native_session_id: Option<String>,
    active_turn: Option<ActiveTurn>,
}

struct ActiveTurn {
    turn_id: Uuid,
    handle: Arc<tokio::sync::Mutex<retcon_agents::AgentTurn>>,
}

impl SessionRegistry {
    /// Cancel every in-flight provider turn owned by the core.
    pub async fn shutdown(&self) {
        let turns = self
            .sessions
            .lock()
            .map(|mut map| {
                map.values_mut()
                    .filter_map(|session| session.active_turn.take())
                    .map(|turn| turn.handle)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for turn in turns {
            turn.lock().await.cancel().await;
        }
    }

    fn upsert(&self, session_id: Uuid, cwd: PathBuf, provider_id: String) {
        if let Ok(mut map) = self.sessions.lock() {
            map.entry(session_id)
                .and_modify(|entry| {
                    entry.cwd = cwd.clone();
                    entry.provider_id = provider_id.clone();
                })
                .or_insert(ActiveSession {
                    cwd,
                    provider_id,
                    native_session_id: None,
                    active_turn: None,
                });
        }
    }

    fn set_native_session_id(&self, session_id: Uuid, native_session_id: Option<String>) {
        if let Ok(mut map) = self.sessions.lock()
            && let Some(entry) = map.get_mut(&session_id)
        {
            entry.native_session_id = native_session_id;
        }
    }

    fn active_turn(&self, session_id: Uuid) -> Option<ActiveTurn> {
        let guard = self.sessions.lock().ok()?;
        let entry = guard.get(&session_id)?;
        entry.active_turn.as_ref().map(|turn| ActiveTurn {
            turn_id: turn.turn_id,
            handle: Arc::clone(&turn.handle),
        })
    }

    fn set_active_turn(&self, session_id: Uuid, turn: Option<ActiveTurn>) {
        if let Ok(mut map) = self.sessions.lock()
            && let Some(entry) = map.get_mut(&session_id)
        {
            entry.active_turn = turn;
        }
    }

    fn cwd(&self, session_id: Uuid) -> Option<PathBuf> {
        self.sessions
            .lock()
            .ok()
            .and_then(|map| map.get(&session_id).map(|entry| entry.cwd.clone()))
    }

    fn provider_id(&self, session_id: Uuid) -> Option<String> {
        self.sessions
            .lock()
            .ok()
            .and_then(|map| map.get(&session_id).map(|entry| entry.provider_id.clone()))
    }

    fn native_session_id(&self, session_id: Uuid) -> Option<String> {
        self.sessions.lock().ok().and_then(|map| {
            map.get(&session_id)
                .and_then(|entry| entry.native_session_id.clone())
        })
    }
}

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn invalid(id: u64, user_message: &str, technical_message: impl Into<String>) -> Response {
    failed(
        id,
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            user_message,
            technical_message,
        ),
    )
}

fn parse_uuid(id: u64, field: &str, raw: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(raw).map_err(|error| {
        invalid(
            id,
            &format!("The {field} is invalid."),
            format!("invalid {field}: {error}"),
        )
    })
}

fn session_json(session: &Session) -> Value {
    json!({
        "sessionId": session.id,
        "projectId": session.project_id,
        "title": session.title,
        "status": session.status,
        "createdAt": session.created_at,
        "updatedAt": session.updated_at,
    })
}

fn turn_json(turn: &Turn) -> Value {
    json!({
        "turnId": turn.id,
        "sessionId": turn.session_id,
        "sequence": turn.sequence,
        "status": turn.status,
        "startedAt": turn.started_at,
        "completedAt": turn.completed_at,
    })
}

fn transition_error(id: u64, transition: InvalidTransition<SessionState>) -> Response {
    failed(
        id,
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "That session cannot move to the requested state.",
            format!("invalid session transition {:?} -> {:?}", transition.from, transition.to),
        ),
    )
}

fn persist_session_status(state: &CoreState, session_id: Uuid, status: SessionState) -> Result<(), CoreError> {
    state
        .storage()
        .database()
        .sessions()
        .set_status(session_id, status.as_str())
        .map(|_| ())
        .map_err(CoreError::from)
}

fn persist_turn_status_internal(
    state: &CoreState,
    turn_id: Uuid,
    status: TurnState,
) -> Result<(), CoreError> {
    state
        .storage()
        .database()
        .turns()
        .set_status(turn_id, status.as_str(), status.is_terminal())
        .map(|_| ())
        .map_err(CoreError::from)
}

fn transition_turn_internal(
    state: &CoreState,
    turn_id: Uuid,
    machine: &mut TurnMachine,
    to: TurnState,
) -> Result<TurnState, CoreError> {
    machine.transition(to).map_err(|error| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "That turn cannot move to the requested state.",
            format!("invalid turn transition {:?} -> {:?}", error.from, error.to),
        )
    })?;
    persist_turn_status_internal(state, turn_id, to)?;
    Ok(to)
}

fn transition_session(
    state: &CoreState,
    id: u64,
    session: &Session,
    machine: &mut SessionMachine,
    to: SessionState,
) -> Result<SessionState, Response> {
    machine.transition(to).map_err(|error| transition_error(id, error))?;
    persist_session_status(state, session.id, to).map_err(|error| failed(id, error))?;
    Ok(to)
}

fn transition_turn(
    state: &CoreState,
    id: u64,
    turn_id: Uuid,
    machine: &mut TurnMachine,
    to: TurnState,
) -> Result<TurnState, Response> {
    transition_turn_internal(state, turn_id, machine, to).map_err(|error| failed(id, error))
}

fn emit_agent_event(state: &CoreState, session_id: Uuid, turn_id: Uuid, event: &AgentEvent) {
    state.emit(
        "session.agentEvent",
        json!({
            "sessionId": session_id,
            "turnId": turn_id,
            "providerId": event.provider_id,
            "nativeSessionId": event.native_session_id,
            "kind": event.kind,
            "data": event.data,
        }),
    );
}

fn emit_session_state(state: &CoreState, session_id: Uuid, status: SessionState) {
    state.emit(
        "session.stateChanged",
        json!({
            "sessionId": session_id,
            "status": status.as_str(),
        }),
    );
}

fn emit_turn_state(state: &CoreState, session_id: Uuid, turn_id: Uuid, status: TurnState) {
    state.emit(
        "session.turnStateChanged",
        json!({
            "sessionId": session_id,
            "turnId": turn_id,
            "status": status.as_str(),
        }),
    );
}

/// Handle `session.*` and `turn.*` requests.
pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "session.create" => create_session(&state, id, &params),
        "session.list" => list_sessions(&state, id, &params),
        "session.start" => start_session(state, id, &params).await,
        "session.cancel" => cancel_session(state, id, &params).await,
        "session.pause" => pause_session(state, id, &params).await,
        "session.resume" => resume_session(state, id, &params).await,
        "turn.send" => send_turn(state, id, &params).await,
        "turn.cancel" => cancel_turn(state, id, &params).await,
        _ => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested session operation is not available.",
                format!("unknown RPC method: {method}"),
            ),
        ),
    }
}

fn create_session(state: &CoreState, id: u64, params: &Value) -> Response {
    let Some(project_raw) = params.get("projectId").and_then(Value::as_str) else {
        return invalid(id, "Creating a session requires a project ID.", "missing 'projectId'");
    };
    let project_id = match parse_uuid(id, "project ID", project_raw) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match state.storage().database().projects().get(project_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return failed(
                id,
                CoreError::new(
                    ErrorCode::NotFound,
                    ErrorSource::Rpc,
                    "That project was not found.",
                    format!("unknown project id: {project_id}"),
                ),
            );
        }
        Err(error) => return failed(id, CoreError::from(error)),
    }
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("New session");
    let provider_id = params
        .get("providerId")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROVIDER_ID);
    if provider_id != DEFAULT_PROVIDER_ID {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That provider is not available in this Retcon build.",
                format!("unsupported provider id: {provider_id}"),
            ),
        );
    }
    let session = match state
        .storage()
        .database()
        .sessions()
        .create(&NewSession::new(project_id, title))
    {
        Ok(session) => session,
        Err(error) => return failed(id, CoreError::from(error)),
    };
    state.emit(
        "session.created",
        json!({
            "sessionId": session.id,
            "projectId": session.project_id,
            "providerId": provider_id,
        }),
    );
    Response::ok(id, session_json(&session))
}

fn list_sessions(state: &CoreState, id: u64, params: &Value) -> Response {
    let Some(project_raw) = params.get("projectId").and_then(Value::as_str) else {
        return invalid(id, "Listing sessions requires a project ID.", "missing 'projectId'");
    };
    let project_id = match parse_uuid(id, "project ID", project_raw) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let sessions = match state
        .storage()
        .database()
        .sessions()
        .list_for_project(project_id)
    {
        Ok(sessions) => sessions,
        Err(error) => return failed(id, CoreError::from(error)),
    };
    Response::ok(
        id,
        json!({
            "sessions": sessions.iter().map(session_json).collect::<Vec<_>>(),
        }),
    )
}

async fn start_session(state: CoreState, id: u64, params: &Value) -> Response {
    let Some(session_raw) = params.get("sessionId").and_then(Value::as_str) else {
        return invalid(id, "Starting a session requires a session ID.", "missing 'sessionId'");
    };
    let session_id = match parse_uuid(id, "session ID", session_raw) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let session = match state.storage().database().sessions().get(session_id) {
        Ok(Some(session)) => session,
        Ok(None) => {
            return failed(
                id,
                CoreError::new(
                    ErrorCode::NotFound,
                    ErrorSource::Rpc,
                    "That session was not found.",
                    format!("unknown session id: {session_id}"),
                ),
            );
        }
        Err(error) => return failed(id, CoreError::from(error)),
    };
    let Some(cwd_raw) = params.get("cwd").and_then(Value::as_str) else {
        return invalid(
            id,
            "Starting a session requires a working directory.",
            "missing 'cwd'",
        );
    };
    let cwd = PathBuf::from(cwd_raw);
    let provider_id = params
        .get("providerId")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROVIDER_ID)
        .to_owned();
    if provider_id != DEFAULT_PROVIDER_ID {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That provider is not available in this Retcon build.",
                format!("unsupported provider id: {provider_id}"),
            ),
        );
    }

    let current = parse_session_state(&session.status).unwrap_or(SessionState::Created);
    if current.is_terminal() {
        return failed(
            id,
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "That session has already ended.",
                format!("session {session_id} is terminal ({current:?})"),
            ),
        );
    }
    if matches!(
        current,
        SessionState::Running | SessionState::WaitingForApproval | SessionState::WaitingForUser
    ) {
        state.sessions().upsert(session_id, cwd, provider_id);
        return Response::ok(id, session_json(&session));
    }

    let mut machine = SessionMachine::from_state(current);
    let steps = match current {
        SessionState::Created | SessionState::Paused => {
            vec![SessionState::Preparing, SessionState::Starting, SessionState::Running]
        }
        SessionState::Disconnected => vec![SessionState::Recovering, SessionState::Running],
        SessionState::Recovering => vec![SessionState::Running],
        SessionState::Preparing => vec![SessionState::Starting, SessionState::Running],
        SessionState::Starting => vec![SessionState::Running],
        other => {
            return failed(
                id,
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    ErrorSource::Rpc,
                    "That session cannot be started from its current state.",
                    format!("session {session_id} is in state {other:?}"),
                ),
            );
        }
    };

    for step in steps {
        if let Err(response) = transition_session(&state, id, &session, &mut machine, step) {
            return response;
        }
        emit_session_state(&state, session_id, step);
    }

    state.sessions().upsert(session_id, cwd, provider_id);
    let updated = match state.storage().database().sessions().get(session_id) {
        Ok(Some(session)) => session,
        Ok(None) => session,
        Err(error) => return failed(id, CoreError::from(error)),
    };
    Response::ok(id, session_json(&updated))
}

async fn cancel_session(state: CoreState, id: u64, params: &Value) -> Response {
    let session_id = match session_id_param(id, params) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(session) = load_session(&state, id, session_id) else {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That session was not found.",
                format!("unknown session id: {session_id}"),
            ),
        );
    };
    if let Some(active) = state.sessions().active_turn(session_id) {
        active.handle.lock().await.cancel().await;
        state.sessions().set_active_turn(session_id, None);
    }
    let current = parse_session_state(&session.status).unwrap_or(SessionState::Created);
    let mut machine = SessionMachine::from_state(current);
    if !current.is_terminal()
        && let Err(response) =
            transition_session(&state, id, &session, &mut machine, SessionState::Cancelled)
    {
        return response;
    }
    emit_session_state(&state, session_id, SessionState::Cancelled);
    Response::ok(id, json!({"sessionId": session_id, "status": "cancelled"}))
}

async fn pause_session(state: CoreState, id: u64, params: &Value) -> Response {
    let session_id = match session_id_param(id, params) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(session) = load_session(&state, id, session_id) else {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That session was not found.",
                format!("unknown session id: {session_id}"),
            ),
        );
    };
    if let Some(active) = state.sessions().active_turn(session_id) {
        active.handle.lock().await.cancel().await;
        state.sessions().set_active_turn(session_id, None);
    }
    let current = parse_session_state(&session.status).unwrap_or(SessionState::Running);
    let mut machine = SessionMachine::from_state(current);
    if let Err(response) = transition_session(&state, id, &session, &mut machine, SessionState::Paused) {
        return response;
    }
    emit_session_state(&state, session_id, SessionState::Paused);
    Response::ok(id, json!({"sessionId": session_id, "status": "paused"}))
}

async fn resume_session(state: CoreState, id: u64, params: &Value) -> Response {
    let session_id = match session_id_param(id, params) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(session) = load_session(&state, id, session_id) else {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That session was not found.",
                format!("unknown session id: {session_id}"),
            ),
        );
    };
    let current = parse_session_state(&session.status).unwrap_or(SessionState::Paused);
    let mut machine = SessionMachine::from_state(current);
    if let Err(response) =
        transition_session(&state, id, &session, &mut machine, SessionState::Preparing)
    {
        return response;
    }
    emit_session_state(&state, session_id, SessionState::Preparing);
    if let Err(response) =
        transition_session(&state, id, &session, &mut machine, SessionState::Running)
    {
        return response;
    }
    emit_session_state(&state, session_id, SessionState::Running);
    Response::ok(id, json!({"sessionId": session_id, "status": "running"}))
}

async fn send_turn(state: CoreState, id: u64, params: &Value) -> Response {
    let session_id = match session_id_param(id, params) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if load_session(&state, id, session_id).is_none() {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That session was not found.",
                format!("unknown session id: {session_id}"),
            ),
        );
    };
    let Some(prompt) = params.get("prompt").and_then(Value::as_str) else {
        return invalid(id, "Sending a turn requires a prompt.", "missing 'prompt'");
    };
    let secret_scan = scan_prompt(prompt);
    if !secret_scan.is_clean() {
        return failed(
            id,
            CoreError::new(
                ErrorCode::PermissionDenied,
                ErrorSource::Rpc,
                "Retcon blocked this turn because the prompt may contain secrets.",
                format!("secret scan blocked turn.send: {}", secret_scan.summary()),
            )
            .suggested_fix(
                "Remove API keys, tokens, passwords, and private keys from the prompt before sending.",
            ),
        );
    }
    if state.sessions().active_turn(session_id).is_some() {
        return failed(
            id,
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "That session already has a turn in progress.",
                format!("session {session_id} already has an active turn"),
            ),
        );
    }

    let cwd = params
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| state.sessions().cwd(session_id))
        .unwrap_or_else(|| PathBuf::from("."));
    let provider_id = state
        .sessions()
        .provider_id(session_id)
        .unwrap_or_else(|| DEFAULT_PROVIDER_ID.to_owned());
    state
        .sessions()
        .upsert(session_id, cwd.clone(), provider_id.clone());

    let sequence = match state
        .storage()
        .database()
        .turns()
        .next_sequence(session_id)
    {
        Ok(sequence) => sequence,
        Err(error) => return failed(id, CoreError::from(error)),
    };
    let turn = match state
        .storage()
        .database()
        .turns()
        .create(&NewTurn::new(session_id, sequence))
    {
        Ok(turn) => turn,
        Err(error) => return failed(id, CoreError::from(error)),
    };
    if let Ok(checkpoint) = retcon_checkpoints::CheckpointService::new(
        state.storage().database().clone(),
        state.storage().artifacts().clone(),
    )
    .create_turn_start(&cwd, turn.id)
    .await
    {
        state.emit(
            "checkpoint.created",
            json!({
                "checkpointId": checkpoint.id.to_string(),
                "kind": checkpoint.kind,
                "turnId": turn.id.to_string(),
            }),
        );
    }
    let mut turn_machine = TurnMachine::from_state(TurnState::Queued);
    if let Err(response) = transition_turn(&state, id, turn.id, &mut turn_machine, TurnState::Sending) {
        return response;
    }
    emit_turn_state(&state, session_id, turn.id, TurnState::Sending);

    let resume_session = state.sessions().native_session_id(session_id);
    let request = StartTurnRequest {
        cwd,
        prompt: prompt.to_owned(),
        resume_session,
    };
    let turn_id = turn.id;
    let event_state = state.clone();
    let provider = ClaudeCodeProvider;
    let spawned = provider.start_turn(
        &request,
        move |line| {
            let event = normalize_claude_stream_event(&provider_id, &line);
            if let Some(native) = event.native_session_id.clone() {
                event_state
                    .sessions()
                    .set_native_session_id(session_id, Some(native));
            }
            emit_agent_event(&event_state, session_id, turn_id, &event);
        },
    );

    let agent_turn = match spawned {
        Ok(turn) => turn,
        Err(error) => {
            let _ = transition_turn(&state, id, turn.id, &mut turn_machine, TurnState::Failed);
            emit_turn_state(&state, session_id, turn.id, TurnState::Failed);
            return failed(
                id,
                CoreError::new(
                    ErrorCode::Internal,
                    ErrorSource::Rpc,
                    "Retcon could not start the provider turn.",
                    error.to_string(),
                ),
            );
        }
    };

    if let Err(response) = transition_turn(&state, id, turn.id, &mut turn_machine, TurnState::Running) {
        return response;
    }
    emit_turn_state(&state, session_id, turn.id, TurnState::Running);

    let handle = Arc::new(tokio::sync::Mutex::new(agent_turn));
    state.sessions().set_active_turn(
        session_id,
        Some(ActiveTurn {
            turn_id: turn.id,
            handle: Arc::clone(&handle),
        }),
    );

    let watch_state = state.clone();
    tokio::spawn(async move {
        let exit_code = {
            let mut guard = handle.lock().await;
            guard.wait().await
        };
        let mut machine = TurnMachine::from_state(TurnState::Running);
        let final_state = if exit_code == Some(0) {
            let _ = transition_turn_internal(
                &watch_state,
                turn_id,
                &mut machine,
                TurnState::Completing,
            );
            emit_turn_state(&watch_state, session_id, turn_id, TurnState::Completing);
            let _ = transition_turn_internal(
                &watch_state,
                turn_id,
                &mut machine,
                TurnState::Completed,
            );
            TurnState::Completed
        } else {
            let _ =
                transition_turn_internal(&watch_state, turn_id, &mut machine, TurnState::Failed);
            TurnState::Failed
        };
        emit_turn_state(&watch_state, session_id, turn_id, final_state);
        watch_state.emit(
            "session.turnCompleted",
            json!({
                "sessionId": session_id,
                "turnId": turn_id,
                "status": final_state.as_str(),
                "exitCode": exit_code,
            }),
        );
        watch_state.sessions().set_active_turn(session_id, None);
    });

    Response::ok(id, turn_json(&turn))
}

async fn cancel_turn(state: CoreState, id: u64, params: &Value) -> Response {
    let turn_id = if let Some(raw) = params.get("turnId").and_then(Value::as_str) {
        match parse_uuid(id, "turn ID", raw) {
            Ok(value) => Some(value),
            Err(response) => return response,
        }
    } else {
        None
    };
    let session_id = if let Some(turn_id) = turn_id {
        match state.storage().database().turns().get(turn_id) {
            Ok(Some(turn)) => Some(turn.session_id),
            Ok(None) => {
                return failed(
                    id,
                    CoreError::new(
                        ErrorCode::NotFound,
                        ErrorSource::Rpc,
                        "That turn was not found.",
                        format!("unknown turn id: {turn_id}"),
                    ),
                );
            }
            Err(error) => return failed(id, CoreError::from(error)),
        }
    } else if let Some(raw) = params.get("sessionId").and_then(Value::as_str) {
        match parse_uuid(id, "session ID", raw) {
            Ok(session_id) => Some(session_id),
            Err(response) => return response,
        }
    } else {
        return invalid(
            id,
            "Cancelling a turn requires a turn ID or session ID.",
            "missing 'turnId' or 'sessionId'",
        );
    };
    let Some(session_id) = session_id else {
        return invalid(
            id,
            "Cancelling a turn requires a turn ID or session ID.",
            "missing 'turnId' or 'sessionId'",
        );
    };
    let active = state.sessions().active_turn(session_id);
    let Some(active) = active else {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That turn is no longer running.",
                format!("no active turn for session {session_id}"),
            ),
        );
    };
    if let Some(turn_id) = turn_id
        && active.turn_id != turn_id
    {
        return failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That turn is no longer running.",
                format!("active turn {} does not match {turn_id}", active.turn_id),
            ),
        );
    }
    active.handle.lock().await.cancel().await;
    state.sessions().set_active_turn(session_id, None);
    let stored = state
        .storage()
        .database()
        .turns()
        .get(active.turn_id)
        .ok()
        .flatten();
    let current = stored
        .as_ref()
        .and_then(|turn| parse_turn_state(&turn.status))
        .unwrap_or(TurnState::Running);
    let mut machine = TurnMachine::from_state(current);
    if !current.is_terminal() {
        let _ = transition_turn(&state, id, active.turn_id, &mut machine, TurnState::Cancelled);
    }
    emit_turn_state(&state, session_id, active.turn_id, TurnState::Cancelled);
    Response::ok(
        id,
        json!({
            "sessionId": session_id,
            "turnId": active.turn_id,
            "status": "cancelled",
        }),
    )
}

fn session_id_param(id: u64, params: &Value) -> Result<Uuid, Response> {
    let raw = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(id, "The session request is missing a session ID.", "missing 'sessionId'"))?;
    parse_uuid(id, "session ID", raw)
}

fn load_session(state: &CoreState, id: u64, session_id: Uuid) -> Option<Session> {
    state
        .storage()
        .database()
        .sessions()
        .get(session_id)
        .map_err(|error| {
            failed(id, CoreError::from(error));
        })
        .ok()
        .flatten()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use retcon_storage::NewProject;

    fn test_state() -> CoreState {
        let directory = tempfile::tempdir().unwrap();
        CoreState::new(directory.path()).unwrap()
    }

    #[test]
    fn session_json_uses_camel_case_fields() {
        let session = Session {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test".into(),
            status: "created".into(),
            created_at: 1,
            updated_at: 2,
        };
        let value = session_json(&session);
        assert_eq!(value["sessionId"], session.id.to_string());
        assert_eq!(value["projectId"], session.project_id.to_string());
    }

    #[tokio::test]
    async fn create_and_list_sessions_round_trip() {
        let state = test_state();
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Retcon"))
            .unwrap();
        let created = handle(
            state.clone(),
            Request {
                id: 1,
                method: "session.create".into(),
                params: json!({"projectId": project.id, "title": "Build feature"}),
            },
        )
        .await;
        assert!(created.result.is_some());
        let session_id = created.result.unwrap()["sessionId"]
            .as_str()
            .unwrap()
            .to_owned();
        let listed = handle(
            state,
            Request {
                id: 2,
                method: "session.list".into(),
                params: json!({"projectId": project.id}),
            },
        )
        .await;
        let result = listed.result.unwrap();
        let sessions = result["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["sessionId"], session_id);
    }

    #[tokio::test]
    async fn disconnected_session_recovers_on_start() {
        let state = test_state();
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Retcon"))
            .unwrap();
        let mut new_session = NewSession::new(project.id, "Recover me");
        new_session.status = "disconnected".into();
        let session = state
            .storage()
            .database()
            .sessions()
            .create(&new_session)
            .unwrap();
        let response = handle(
            state.clone(),
            Request {
                id: 1,
                method: "session.start".into(),
                params: json!({"sessionId": session.id, "cwd": "."}),
            },
        )
        .await;
        assert!(response.result.is_some(), "{response:?}");
        let updated = state
            .storage()
            .database()
            .sessions()
            .get(session.id)
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, "running");
    }

    #[tokio::test]
    async fn send_turn_blocks_prompts_with_secrets() {
        let state = test_state();
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Retcon"))
            .unwrap();
        let created = handle(
            state.clone(),
            Request {
                id: 1,
                method: "session.create".into(),
                params: json!({"projectId": project.id, "title": "Secrets"}),
            },
        )
        .await;
        let session_id = created.result.unwrap()["sessionId"]
            .as_str()
            .unwrap()
            .to_owned();
        let _ = handle(
            state.clone(),
            Request {
                id: 2,
                method: "session.start".into(),
                params: json!({"sessionId": session_id, "cwd": "."}),
            },
        )
        .await;
        let blocked = handle(
            state,
            Request {
                id: 3,
                method: "turn.send".into(),
                params: json!({"sessionId": session_id, "prompt": "password=hunter2"}),
            },
        )
        .await;
        assert!(blocked.error.is_some(), "{blocked:?}");
        assert_eq!(
            blocked.error.unwrap()["code"].as_str().unwrap(),
            "permission_denied"
        );
    }
}
