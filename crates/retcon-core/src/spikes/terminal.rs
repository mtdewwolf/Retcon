//! Terminal spike handlers: `terminal.*` methods over the PTY crate.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use retcon_storage::{NewCommand, NewTerminalSession};
use retcon_terminal::{CommandLineTracker, PtySession};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{fail, param_str, param_u64};
use crate::error::ErrorCode;
use crate::rpc::{Request, Response};
use crate::state::CoreState;

const OUTPUT_COALESCE_MS: u64 = 50;
const OUTPUT_COALESCE_BYTES: usize = 4_096;
const SCROLLBACK_FLUSH_BYTES: usize = 16_384;
const OUTPUT_QUEUE_CAPACITY: usize = 256;
const MAX_SCROLLBACK_BYTES: usize = 256 * 1024;

struct LiveTerminal {
    pty: Arc<PtySession>,
    session_id: Uuid,
    shell: String,
    cwd: String,
    scrollback: Mutex<String>,
    tracker: Mutex<CommandLineTracker>,
}

/// Live PTY sessions owned by the core.
#[derive(Default)]
pub struct TerminalRegistry {
    next: AtomicU64,
    map: Mutex<HashMap<u64, LiveTerminal>>,
    shells: Mutex<Option<Vec<Value>>>,
}

impl TerminalRegistry {
    /// Kill all terminal process trees owned by the spike.
    pub fn shutdown(&self) {
        let sessions = self
            .map
            .lock()
            .ok()
            .map(|mut sessions| sessions.drain().collect::<Vec<_>>())
            .unwrap_or_default();
        for (_, session) in sessions {
            session.pty.kill();
        }
    }
}

/// Handle a `terminal.*` request.
pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "terminal.detectShells" => {
            if let Ok(cache) = state.terminals().shells.lock()
                && let Some(shells) = cache.as_ref()
            {
                return Response::ok(id, json!({"shells": shells}));
            }
            let shells = detect_shells().await;
            if let Ok(mut cache) = state.terminals().shells.lock() {
                *cache = Some(shells.clone());
            }
            Response::ok(id, json!({"shells": shells}))
        }
        "terminal.start" => start(state, id, &params).await,
        "terminal.input" => input_terminal(&state, id, &params),
        "terminal.resize" => with_session(&state, id, &params, |terminal| {
            let cols = param_u64(&params, "cols").unwrap_or(80) as u16;
            let rows = param_u64(&params, "rows").unwrap_or(24) as u16;
            terminal.pty.resize(cols, rows)
        }),
        "terminal.kill" => kill_terminal(&state, id, &params).await,
        "terminal.list" => list_sessions(&state, id, &params),
        "terminal.scrollback" => scrollback(&state, id, &params),
        other => fail(
            id,
            ErrorCode::NotFound,
            "The requested terminal operation is not available.",
            format!("unknown RPC method: {other}"),
        ),
    }
}

async fn detect_shells() -> Vec<Value> {
    let mut available = Vec::new();
    for (shell_id, executable) in [("powershell", "powershell.exe"), ("cmd", "cmd.exe")] {
        if let Ok(output) = tokio::process::Command::new("where.exe")
            .arg(executable)
            .output()
            .await
            && output.status.success()
            && let Some(path) = String::from_utf8_lossy(&output.stdout).lines().next()
        {
            available.push(json!({"id": shell_id, "path": path}));
        }
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        let path = std::path::PathBuf::from(program_files)
            .join("Git")
            .join("bin")
            .join("bash.exe");
        if path.exists() {
            available.push(json!({"id":"git-bash", "path": path}));
        }
    }
    available
}

async fn start(state: CoreState, id: u64, params: &Value) -> Response {
    let shell = param_str(params, "shell")
        .unwrap_or("powershell.exe")
        .to_owned();
    let cwd = param_str(params, "cwd")
        .map(str::to_owned)
        .unwrap_or_else(|| ".".to_owned());
    let cwd_path = param_str(params, "cwd").map(std::path::PathBuf::from);
    let cols = param_u64(params, "cols").unwrap_or(120) as u16;
    let rows = param_u64(params, "rows").unwrap_or(30) as u16;

    let mut record = NewTerminalSession::new(&shell, &cwd);
    record.status = "running".into();
    let session_uuid = record.id;
    let repos = state.storage().database();
    if let Err(error) = repos.terminal_sessions().create(&record) {
        return fail(
            id,
            ErrorCode::Internal,
            "Retcon could not record the terminal session.",
            error.to_string(),
        );
    }

    let terminal_id = state.terminals().next.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, mut rx) = mpsc::channel::<String>(OUTPUT_QUEUE_CAPACITY);
    let output_state = state.clone();
    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut ticker = tokio::time::interval(Duration::from_millis(OUTPUT_COALESCE_MS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                chunk = rx.recv() => {
                    let Some(chunk) = chunk else {
                        if !buffer.is_empty() {
                            output_state.emit("terminal.output", json!({ "id": terminal_id, "data": buffer }));
                        }
                        break;
                    };
                    append_scrollback(&output_state, terminal_id, &chunk);
                    buffer.push_str(&chunk);
                    if buffer.len() >= OUTPUT_COALESCE_BYTES {
                        output_state.emit("terminal.output", json!({ "id": terminal_id, "data": buffer.clone() }));
                        buffer.clear();
                    }
                }
                _ = ticker.tick() => {
                    if !buffer.is_empty() {
                        output_state.emit("terminal.output", json!({ "id": terminal_id, "data": buffer.clone() }));
                        buffer.clear();
                    }
                }
            }
        }
    });

    let session = match PtySession::spawn(&shell, cwd_path.as_deref(), cols, rows, move |chunk| {
        let _ = tx.try_send(String::from_utf8_lossy(chunk).into_owned());
    }) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            let _ = state
                .storage()
                .database()
                .terminal_sessions()
                .finish(session_uuid, "failed");
            return fail(
                id,
                ErrorCode::Internal,
                "Retcon could not start the shell.",
                e,
            );
        }
    };

    let live = LiveTerminal {
        pty: Arc::clone(&session),
        session_id: session_uuid,
        shell: shell.clone(),
        cwd: cwd.clone(),
        scrollback: Mutex::new(String::new()),
        tracker: Mutex::new(CommandLineTracker::new()),
    };
    if let Ok(mut map) = state.terminals().map.lock() {
        map.insert(terminal_id, live);
    }

    let watch_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Some(code) = session.try_exit_code() {
                watch_state.emit(
                    "terminal.exit",
                    json!({ "id": terminal_id, "exitCode": code, "sessionId": session_uuid.to_string() }),
                );
                if let Ok(mut map) = watch_state.terminals().map.lock()
                    && let Some(terminal) = map.remove(&terminal_id)
                {
                    flush_scrollback(&watch_state, &terminal);
                    let _ = watch_state
                        .storage()
                        .database()
                        .terminal_sessions()
                        .finish(terminal.session_id, "ended");
                }
                break;
            }
        }
    });

    tracing::info!(terminal_id, %session_uuid, shell, "terminal started");
    Response::ok(
        id,
        json!({
            "terminalId": terminal_id,
            "sessionId": session_uuid.to_string(),
            "shell": shell
        }),
    )
}

fn input_terminal(state: &CoreState, id: u64, params: &Value) -> Response {
    let data = param_str(params, "data").unwrap_or_default();
    with_session(state, id, params, |terminal| {
        if let Ok(mut tracker) = terminal.tracker.lock() {
            let commands = tracker.push_input(data);
            let repos = state.storage().database();
            for command in commands {
                let record = NewCommand::new(terminal.session_id, command, &terminal.cwd);
                if let Err(error) = repos.commands().create(&record) {
                    tracing::warn!(error = %error, "failed to record terminal command");
                }
            }
        }
        terminal.pty.write(data.as_bytes())
    })
}

async fn kill_terminal(state: &CoreState, id: u64, params: &Value) -> Response {
    let Some(terminal_id) = param_u64(params, "id") else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "The terminal request is missing its id.",
            "missing 'id' parameter",
        );
    };
    let terminal = state
        .terminals()
        .map
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&terminal_id));
    match terminal {
        Some(terminal) => {
            flush_scrollback(state, &terminal);
            let _ = state
                .storage()
                .database()
                .terminal_sessions()
                .finish(terminal.session_id, "ended");
            terminal.pty.kill();
            Response::ok(id, json!({}))
        }
        None => fail(
            id,
            ErrorCode::NotFound,
            "That terminal is no longer running.",
            format!("no terminal {terminal_id}"),
        ),
    }
}

fn list_sessions(state: &CoreState, id: u64, params: &Value) -> Response {
    let limit = param_u64(params, "limit").unwrap_or(20) as usize;
    let repos = state.storage().database();
    match repos.terminal_sessions().list_recent(limit) {
        Ok(sessions) => Response::ok(
            id,
            json!({
                "sessions": sessions.into_iter().map(session_json).collect::<Vec<_>>()
            }),
        ),
        Err(error) => fail(
            id,
            ErrorCode::Internal,
            "Retcon could not list terminal sessions.",
            error.to_string(),
        ),
    }
}

fn scrollback(state: &CoreState, id: u64, params: &Value) -> Response {
    let Some(session_id) = param_str(params, "sessionId") else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "The terminal scrollback request is missing its session id.",
            "missing 'sessionId' parameter",
        );
    };
    let Ok(uuid) = Uuid::parse_str(session_id) else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "The terminal session id is invalid.",
            format!("invalid session id: {session_id}"),
        );
    };

    if let Ok(map) = state.terminals().map.lock() {
        for terminal in map.values() {
            if terminal.session_id == uuid {
                let text = terminal
                    .scrollback
                    .lock()
                    .map(|buffer| buffer.clone())
                    .unwrap_or_default();
                return Response::ok(id, json!({ "text": text, "live": true }));
            }
        }
    }

    let repos = state.storage().database();
    let Ok(Some(record)) = repos.terminal_sessions().get(uuid) else {
        return fail(
            id,
            ErrorCode::NotFound,
            "That terminal session was not found.",
            format!("no terminal session {session_id}"),
        );
    };
    let Some(hash) = record.log_artifact_hash else {
        return Response::ok(id, json!({ "text": "", "live": false }));
    };
    match read_artifact_text(state, &hash) {
        Ok(text) => Response::ok(id, json!({ "text": text, "live": false })),
        Err(error) => fail(
            id,
            ErrorCode::Internal,
            "Retcon could not read terminal scrollback.",
            error,
        ),
    }
}

fn with_session(
    state: &CoreState,
    id: u64,
    params: &Value,
    f: impl FnOnce(&LiveTerminal) -> Result<(), String>,
) -> Response {
    let Some(terminal_id) = param_u64(params, "id") else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "The terminal request is missing its id.",
            "missing 'id' parameter",
        );
    };
    let terminal = state
        .terminals()
        .map
        .lock()
        .ok()
        .and_then(|m| m.get(&terminal_id).cloned());
    match terminal {
        Some(terminal) => match f(&terminal) {
            Ok(()) => Response::ok(id, json!({})),
            Err(e) => fail(
                id,
                ErrorCode::Io,
                "The terminal did not accept the operation.",
                e,
            ),
        },
        None => fail(
            id,
            ErrorCode::NotFound,
            "That terminal is no longer running.",
            format!("no terminal {terminal_id}"),
        ),
    }
}

fn append_scrollback(state: &CoreState, terminal_id: u64, chunk: &str) {
    let Some(terminal) = state
        .terminals()
        .map
        .lock()
        .ok()
        .and_then(|map| map.get(&terminal_id).cloned())
    else {
        return;
    };
    let mut should_flush = false;
    if let Ok(mut buffer) = terminal.scrollback.lock() {
        buffer.push_str(chunk);
        if buffer.len() > MAX_SCROLLBACK_BYTES {
            let excess = buffer.len() - MAX_SCROLLBACK_BYTES;
            buffer.drain(..excess);
        }
        should_flush = buffer.len() >= SCROLLBACK_FLUSH_BYTES;
    }
    if should_flush {
        flush_scrollback(state, &terminal);
    }
}

fn flush_scrollback(state: &CoreState, terminal: &LiveTerminal) {
    let text = if let Ok(mut buffer) = terminal.scrollback.lock() {
        if buffer.is_empty() {
            return;
        }
        std::mem::take(&mut *buffer)
    } else {
        return;
    };
    let store = state.storage().artifacts();
    let Ok(artifact) = store.store_bytes(text.as_bytes()) else {
        return;
    };
    let _ = state
        .storage()
        .database()
        .terminal_sessions()
        .set_log_artifact(terminal.session_id, &artifact.hash);
}

fn read_artifact_text(state: &CoreState, hash: &str) -> Result<String, String> {
    let mut file = state
        .storage()
        .artifacts()
        .get(hash)
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn session_json(session: retcon_storage::TerminalSession) -> Value {
    json!({
        "sessionId": session.id.to_string(),
        "status": session.status,
        "shell": session.shell,
        "cwd": session.cwd,
        "startedAt": session.started_at,
        "endedAt": session.ended_at,
        "logArtifactHash": session.log_artifact_hash,
    })
}

impl Clone for LiveTerminal {
    fn clone(&self) -> Self {
        Self {
            pty: Arc::clone(&self.pty),
            session_id: self.session_id,
            shell: self.shell.clone(),
            cwd: self.cwd.clone(),
            scrollback: Mutex::new(
                self.scrollback
                    .lock()
                    .map(|buffer| buffer.clone())
                    .unwrap_or_default(),
            ),
            tracker: Mutex::new(
                self.tracker
                    .lock()
                    .map(|tracker| tracker.clone())
                    .unwrap_or_default(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_json_includes_session_id() {
        let session = retcon_storage::TerminalSession {
            id: Uuid::new_v4(),
            session_id: None,
            status: "ended".into(),
            shell: "pwsh".into(),
            cwd: ".".into(),
            started_at: 1,
            ended_at: Some(2),
            log_artifact_hash: None,
        };
        let value = session_json(session);
        assert_eq!(value["status"], "ended");
        assert!(value["sessionId"].is_string());
    }
}
