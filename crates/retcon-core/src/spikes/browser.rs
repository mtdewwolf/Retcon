//! Browser spike handlers: `browser.*` methods proxied to the Bun/Playwright
//! browser service over its stdio (newline-delimited JSON).

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};

use super::{fail, param_str};
use crate::error::ErrorCode;
use crate::frame::read_capped_line;
use crate::rpc::{Request, Response};
use crate::state::CoreState;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

struct BrowserWriter {
    stdin: ChildStdin,
}

struct BrowserProc {
    child: Child,
    writer: Arc<Mutex<BrowserWriter>>,
    pending: Pending,
    next: AtomicU64,
}

/// Handle to the (at most one) running browser-service process.
#[derive(Default)]
pub struct BrowserHandle {
    proc: Mutex<Option<BrowserProc>>,
}

impl BrowserHandle {
    /// Stop the browser-service child if it is running.
    pub async fn shutdown(&self) {
        stop_service(self).await;
    }
}

/// Handle a `browser.*` request.
pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "browser.startService" => start_service(state, id, &params).await,
        "browser.stopService" => {
            stop_service(state.browser()).await;
            Response::ok(id, json!({}))
        }
        "browser.call" => call(state, id, &params).await,
        other => fail(
            id,
            ErrorCode::NotFound,
            "The requested browser operation is not available.",
            format!("unknown RPC method: {other}"),
        ),
    }
}

async fn stop_service(handle: &BrowserHandle) {
    if let Some(mut proc) = handle.proc.lock().await.take() {
        let _ = write_rpc(
            &proc.writer,
            json!({ "id": u64::MAX, "method": "browser.close", "params": {} }),
        )
        .await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), proc.child.wait()).await;
        let _ = proc.child.kill().await;
        drain_pending(
            &proc.pending,
            json!({"error":{"message":"browser service stopped"}}),
        )
        .await;
    }
}

async fn start_service(state: CoreState, id: u64, params: &Value) -> Response {
    let Some(dir) = param_str(params, "dir") else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "Starting the browser service needs its directory.",
            "missing 'dir' parameter (path to apps/browser-service)",
        );
    };

    let mut guard = state.browser().proc.lock().await;
    if guard.is_some() {
        return Response::ok(id, json!({ "alreadyRunning": true }));
    }

    let mut command = Command::new("bun");
    command
        .args(["run", "src/main.ts", "--stdio"])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return fail(
                id,
                ErrorCode::Internal,
                "Retcon could not start the browser service.",
                format!("failed to spawn bun: {e}"),
            );
        }
    };

    let Some(stdin) = child.stdin.take() else {
        return fail(
            id,
            ErrorCode::Internal,
            "The browser service has no input.",
            "no stdin",
        );
    };
    let Some(stdout) = child.stdout.take() else {
        return fail(
            id,
            ErrorCode::Internal,
            "The browser service has no output.",
            "no stdout",
        );
    };
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            while let Ok(Some(line)) = read_capped_line(&mut reader).await {
                let line = retcon_secrets::redact_text(&line);
                tracing::debug!(target: "retcon_browser_service", "{line}");
            }
        });
    }

    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let reader_pending = Arc::clone(&pending);
    let event_state = state.clone();
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        while let Ok(Some(line)) = read_capped_line(&mut reader).await {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let value = retcon_secrets::scrub_json(value);
            if let Some(reply_id) = value.get("id").and_then(Value::as_u64) {
                if let Some(sender) = reader_pending.lock().await.remove(&reply_id) {
                    let _ = sender.send(value);
                }
            } else {
                event_state.emit("browser.event", value);
            }
        }
        drain_pending(
            &reader_pending,
            json!({"error":{"message":"browser service exited"}}),
        )
        .await;
        cleanup_state.browser().proc.lock().await.take();
        event_state.emit("browser.serviceExited", json!({}));
    });

    *guard = Some(BrowserProc {
        child,
        writer: Arc::new(Mutex::new(BrowserWriter { stdin })),
        pending,
        next: AtomicU64::new(0),
    });
    tracing::info!(dir, "browser service started");
    Response::ok(id, json!({}))
}

async fn call(state: CoreState, id: u64, params: &Value) -> Response {
    let Some(method) = param_str(params, "method") else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "The browser call is missing its method.",
            "missing 'method' parameter",
        );
    };
    let inner_params = params.get("params").cloned().unwrap_or_else(|| json!({}));

    let (writer, call_id, rx) = {
        let mut guard = state.browser().proc.lock().await;
        let Some(proc) = guard.as_mut() else {
            return fail(
                id,
                ErrorCode::NotFound,
                "The browser service is not running.",
                "call browser.startService first",
            );
        };
        let call_id = proc.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        proc.pending.lock().await.insert(call_id, tx);
        (Arc::clone(&proc.writer), call_id, rx)
    };

    let request = json!({ "id": call_id, "method": method, "params": inner_params });
    if write_rpc(&writer, request).await.is_err() {
        if let Some(proc) = state.browser().proc.lock().await.as_mut() {
            proc.pending.lock().await.remove(&call_id);
        }
        return fail(
            id,
            ErrorCode::Io,
            "The browser service did not accept the request.",
            "stdin write failed; the service may have crashed",
        );
    }

    match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
        Ok(Ok(value)) => {
            if let Some(error) = value.get("error") {
                fail(
                    id,
                    ErrorCode::Internal,
                    "The browser action failed.",
                    error.to_string(),
                )
            } else {
                Response::ok(
                    id,
                    value.get("result").cloned().unwrap_or_else(|| json!({})),
                )
            }
        }
        Ok(Err(_)) => {
            if let Some(proc) = state.browser().proc.lock().await.as_mut() {
                proc.pending.lock().await.remove(&call_id);
            }
            fail(
                id,
                ErrorCode::Internal,
                "The browser service stopped before replying.",
                "response channel closed",
            )
        }
        Err(_) => {
            if let Some(proc) = state.browser().proc.lock().await.as_mut() {
                proc.pending.lock().await.remove(&call_id);
            }
            fail(
                id,
                ErrorCode::Io,
                "The browser action timed out.",
                format!("no reply to '{method}' within 60s"),
            )
        }
    }
}

async fn write_rpc(writer: &Arc<Mutex<BrowserWriter>>, payload: Value) -> Result<(), ()> {
    let mut guard = writer.lock().await;
    let line = payload.to_string();
    guard
        .stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|_| ())?;
    guard.stdin.write_all(b"\n").await.map_err(|_| ())
}

async fn drain_pending(pending: &Pending, response: Value) {
    let mut waiters = pending.lock().await;
    for (_, sender) in waiters.drain() {
        let _ = sender.send(response.clone());
    }
}
