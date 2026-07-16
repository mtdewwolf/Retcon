//! Terminal spike handlers: `terminal.*` methods over the PTY crate.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use retcon_terminal::PtySession;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{fail, param_str, param_u64};
use crate::error::ErrorCode;
use crate::rpc::{Request, Response};
use crate::state::CoreState;

const OUTPUT_COALESCE_MS: u64 = 50;
const OUTPUT_COALESCE_BYTES: usize = 4_096;
const OUTPUT_CHANNEL_CAPACITY: usize = 256;

/// Live PTY sessions owned by the core.
#[derive(Default)]
pub struct TerminalRegistry {
    next: AtomicU64,
    map: Mutex<HashMap<u64, Arc<PtySession>>>,
    shells: Mutex<Option<Vec<Value>>>,
}

impl TerminalRegistry {
    /// Kill all terminal process trees owned by the spike.
    pub fn shutdown(&self) {
        let sessions = self
            .map
            .lock()
            .ok()
            .map(|mut sessions| {
                sessions
                    .drain()
                    .map(|(_, session)| session)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for session in sessions {
            session.kill();
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
        "terminal.input" => with_session(&state, id, &params, |s| {
            let data = param_str(&params, "data").unwrap_or_default();
            s.write(data.as_bytes())
        }),
        "terminal.resize" => with_session(&state, id, &params, |s| {
            let cols = param_u64(&params, "cols").unwrap_or(80) as u16;
            let rows = param_u64(&params, "rows").unwrap_or(24) as u16;
            s.resize(cols, rows)
        }),
        "terminal.kill" => kill_terminal(&state, id, &params).await,
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
    let cwd = param_str(params, "cwd").map(std::path::PathBuf::from);
    let cols = param_u64(params, "cols").unwrap_or(120) as u16;
    let rows = param_u64(params, "rows").unwrap_or(30) as u16;

    let terminal_id = state.terminals().next.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, mut rx) = mpsc::channel::<String>(OUTPUT_CHANNEL_CAPACITY);
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

    let session = match PtySession::spawn(&shell, cwd.as_deref(), cols, rows, move |chunk| {
        // Drop chunks when the coalescer falls behind rather than growing unbounded.
        let _ = tx.try_send(String::from_utf8_lossy(chunk).into_owned());
    }) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            return fail(
                id,
                ErrorCode::Internal,
                "Retcon could not start the shell.",
                e,
            );
        }
    };

    if let Ok(mut map) = state.terminals().map.lock() {
        map.insert(terminal_id, Arc::clone(&session));
    }

    let watch_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Some(code) = session.try_exit_code() {
                watch_state.emit(
                    "terminal.exit",
                    json!({ "id": terminal_id, "exitCode": code }),
                );
                if let Ok(mut map) = watch_state.terminals().map.lock() {
                    map.remove(&terminal_id);
                }
                break;
            }
        }
    });

    tracing::info!(terminal_id, shell, "terminal started");
    Response::ok(id, json!({ "terminalId": terminal_id, "shell": shell }))
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
    let session = state
        .terminals()
        .map
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&terminal_id));
    match session {
        Some(session) => {
            session.kill();
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

fn with_session(
    state: &CoreState,
    id: u64,
    params: &Value,
    f: impl FnOnce(&PtySession) -> Result<(), String>,
) -> Response {
    let Some(terminal_id) = param_u64(params, "id") else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "The terminal request is missing its id.",
            "missing 'id' parameter",
        );
    };
    let session = state
        .terminals()
        .map
        .lock()
        .ok()
        .and_then(|m| m.get(&terminal_id).cloned());
    match session {
        Some(s) => match f(&s) {
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
