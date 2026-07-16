//! Agent spike handlers: `agent.*` methods driving the Claude Code CLI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use retcon_agents::AgentTurn;
use serde_json::{Value, json};

use super::{fail, param_str, param_u64};
use crate::error::ErrorCode;
use crate::rpc::{Request, Response};
use crate::state::CoreState;

/// Running provider turns owned by the core.
#[derive(Default)]
pub struct AgentRegistry {
    next: AtomicU64,
    map: Mutex<HashMap<u64, Arc<tokio::sync::Mutex<AgentTurn>>>>,
}

impl AgentRegistry {
    /// Stop all provider turns owned by the spike.
    pub async fn shutdown(&self) {
        let turns = self
            .map
            .lock()
            .map(|mut map| map.drain().map(|(_, turn)| turn).collect::<Vec<_>>())
            .unwrap_or_default();
        for turn in turns {
            turn.lock().await.cancel().await;
        }
    }
}

/// Handle an `agent.*` request.
pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "agent.detect" => match retcon_agents::detect_claude().await {
            Ok(info) => Response::ok(id, json!(info)),
            Err(e) => fail(
                id,
                ErrorCode::NotFound,
                "No supported coding agent was found on this machine.",
                e,
            ),
        },
        "agent.start" => start(state, id, &params),
        "agent.cancel" => {
            let Some(turn_id) = param_u64(&params, "id") else {
                return fail(
                    id,
                    ErrorCode::InvalidRequest,
                    "The cancel request is missing the turn id.",
                    "missing 'id' parameter",
                );
            };
            let turn = state
                .agents()
                .map
                .lock()
                .ok()
                .and_then(|m| m.get(&turn_id).cloned());
            match turn {
                Some(turn) => {
                    turn.lock().await.cancel().await;
                    Response::ok(id, json!({}))
                }
                None => fail(
                    id,
                    ErrorCode::NotFound,
                    "That agent turn is no longer running.",
                    format!("no agent turn {turn_id}"),
                ),
            }
        }
        other => fail(
            id,
            ErrorCode::NotFound,
            "The requested agent operation is not available.",
            format!("unknown RPC method: {other}"),
        ),
    }
}

fn start(state: CoreState, id: u64, params: &Value) -> Response {
    let Some(cwd) = param_str(params, "cwd").map(PathBuf::from) else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "Starting an agent needs a working directory.",
            "missing 'cwd' parameter",
        );
    };
    let Some(prompt) = param_str(params, "prompt") else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "Starting an agent needs a prompt.",
            "missing 'prompt' parameter",
        );
    };

    let turn_id = state.agents().next.fetch_add(1, Ordering::Relaxed) + 1;
    let line_state = state.clone();
    let exit_state = state.clone();
    let resume = param_str(params, "sessionId");
    let turn = AgentTurn::start_with_session(&cwd, prompt, resume, move |line| {
        // Forward parsed provider JSON when possible, raw text otherwise.
        let payload =
            serde_json::from_str::<Value>(&line).unwrap_or_else(|_| Value::String(line.clone()));
        line_state.emit("agent.line", json!({ "id": turn_id, "message": payload }));
    });

    match turn {
        Ok(turn) => {
            let turn = Arc::new(tokio::sync::Mutex::new(turn));
            if let Ok(mut map) = state.agents().map.lock() {
                map.insert(turn_id, Arc::clone(&turn));
            }
            let watch_state = state.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    if let Some(code) = turn.lock().await.try_exit_code() {
                        exit_state.emit("agent.exit", json!({ "id": turn_id, "exitCode": code }));
                        if let Ok(mut map) = watch_state.agents().map.lock() {
                            map.remove(&turn_id);
                        }
                        break;
                    }
                }
            });
            tracing::info!(turn_id, "agent turn started");
            Response::ok(id, json!({ "turnId": turn_id }))
        }
        Err(e) => fail(
            id,
            ErrorCode::Internal,
            "Retcon could not start the coding agent.",
            e,
        ),
    }
}
