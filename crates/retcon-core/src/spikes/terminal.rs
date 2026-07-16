//! Terminal spike handlers: `terminal.*` methods over the PTY crate.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use retcon_terminal::PtySession;
use serde_json::{Value, json};

use crate::rpc::Response;
use crate::state::CoreState;

/// Live PTY sessions owned by the core.
#[derive(Default)]
pub struct TerminalRegistry {
    next: AtomicU64,
    map: Mutex<HashMap<u64, Arc<PtySession>>>,
}

fn param_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn param_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(Value::as_u64)
}

/// Handle a `terminal.*` request.
pub async fn handle(state: CoreState, id: u64, method: &str, params: Value) -> Response {
    match method {
        "terminal.start" => start(state, id, &params),
        "terminal.input" => with_session(&state, id, &params, |s| {
            let data = param_str(&params, "data").unwrap_or_default();
            s.write(data.as_bytes())
        }),
        "terminal.resize" => with_session(&state, id, &params, |s| {
            let cols = param_u64(&params, "cols").unwrap_or(80) as u16;
            let rows = param_u64(&params, "rows").unwrap_or(24) as u16;
            s.resize(cols, rows)
        }),
        "terminal.kill" => with_session(&state, id, &params, |s| {
            s.kill();
            Ok(())
        }),
        _ => Response::err(id, "not_found", format!("unknown method {method}")),
    }
}

fn start(state: CoreState, id: u64, params: &Value) -> Response {
    let shell = param_str(params, "shell").unwrap_or("powershell.exe").to_owned();
    let cwd = param_str(params, "cwd").map(std::path::PathBuf::from);
    let cols = param_u64(params, "cols").unwrap_or(120) as u16;
    let rows = param_u64(params, "rows").unwrap_or(30) as u16;

    let terminal_id = state.terminals().next.fetch_add(1, Ordering::Relaxed) + 1;
    let output_state = state.clone();
    let session = match PtySession::spawn(&shell, cwd.as_deref(), cols, rows, move |chunk| {
        output_state.emit(
            "terminal.output",
            json!({ "id": terminal_id, "data": String::from_utf8_lossy(chunk) }),
        );
    }) {
        Ok(s) => Arc::new(s),
        Err(e) => return Response::err(id, "terminal_spawn_failed", e),
    };

    if let Ok(mut map) = state.terminals().map.lock() {
        map.insert(terminal_id, Arc::clone(&session));
    }

    // Exit watcher: poll until the shell terminates, then emit and clean up.
    let watch_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Some(code) = session.try_exit_code() {
                watch_state
                    .emit("terminal.exit", json!({ "id": terminal_id, "exitCode": code }));
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

fn with_session(
    state: &CoreState,
    id: u64,
    params: &Value,
    f: impl FnOnce(&PtySession) -> Result<(), String>,
) -> Response {
    let Some(terminal_id) = param_u64(params, "id") else {
        return Response::err(id, "bad_params", "missing terminal id");
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
            Err(e) => Response::err(id, "terminal_io_failed", e),
        },
        None => Response::err(id, "not_found", format!("no terminal {terminal_id}")),
    }
}
