//! Authenticated, bounded stdio transport for the Node Playwright service.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as SyncMutex, RwLock};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

use crate::{
    BROWSER_SERVICE_PROTOCOL, BrowserCallResult, BrowserFuture, BrowserLaunchRequest,
    BrowserLaunchResult, BrowserService, BrowserServiceArtifact, BrowserServiceDiagnostics,
    BrowserServiceError, BrowserTransport, MAX_SERVICE_ARTIFACT_BYTES,
};

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

type PendingSender = oneshot::Sender<Result<Value, BrowserServiceError>>;

#[derive(Clone, Debug)]
pub struct NodeBrowserServiceConfig {
    pub node_path: PathBuf,
    pub service_dir: PathBuf,
    pub entrypoint: PathBuf,
    pub artifact_root: PathBuf,
    pub profile_root: PathBuf,
    pub input_roots: Vec<PathBuf>,
    pub request_timeout: Duration,
    pub protocol_version: u32,
}

impl NodeBrowserServiceConfig {
    #[must_use]
    pub fn discover(data_dir: &Path) -> Self {
        let service_dir = std::env::var_os("RETCON_BROWSER_SERVICE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("apps/browser-service")
            });
        let node_path = std::env::var_os("RETCON_NODE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("node"));
        Self {
            node_path,
            entrypoint: service_dir.join("src/main.ts"),
            service_dir,
            artifact_root: data_dir.join("browser-artifacts"),
            profile_root: data_dir.join("browser-profiles"),
            input_roots: Vec::new(),
            request_timeout: DEFAULT_TIMEOUT,
            protocol_version: BROWSER_SERVICE_PROTOCOL,
        }
    }
}

struct ProcessState {
    child: Child,
    writer: Arc<Mutex<ChildStdin>>,
    generation: u64,
}

struct TransportInner {
    config: NodeBrowserServiceConfig,
    token: String,
    process: Mutex<Option<ProcessState>>,
    start_lock: Mutex<()>,
    pending: Mutex<HashMap<u64, PendingSender>>,
    next_id: AtomicU64,
    next_generation: AtomicU64,
    diagnostics: RwLock<BrowserServiceDiagnostics>,
}

#[derive(Clone)]
pub struct NodeStdioTransport {
    inner: Arc<TransportInner>,
}

impl NodeStdioTransport {
    #[must_use]
    pub fn new(config: NodeBrowserServiceConfig) -> Self {
        let healthy = config.entrypoint.is_file() && config.service_dir.is_dir();
        Self {
            inner: Arc::new(TransportInner {
                config,
                token: Uuid::new_v4().to_string(),
                process: Mutex::new(None),
                start_lock: Mutex::new(()),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(0),
                next_generation: AtomicU64::new(0),
                diagnostics: RwLock::new(BrowserServiceDiagnostics {
                    service_version: "0.1.0".into(),
                    protocol_version: BROWSER_SERVICE_PROTOCOL,
                    features: Vec::new(),
                    healthy,
                }),
            }),
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> BrowserServiceDiagnostics {
        self.inner
            .diagnostics
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn ensure_started(&self) -> Result<(), BrowserServiceError> {
        let _start = self.inner.start_lock.lock().await;
        {
            let mut process = self.inner.process.lock().await;
            if let Some(running) = process.as_mut() {
                match running.child.try_wait() {
                    Ok(None) => return Ok(()),
                    Ok(Some(status)) => {
                        tracing::warn!(%status, "browser service exited before request");
                    }
                    Err(error) => tracing::warn!(%error, "could not inspect browser service"),
                }
                process.take();
            }
        }
        let config = &self.inner.config;
        if !config.entrypoint.is_file() {
            return Err(BrowserServiceError::new(
                "unavailable",
                format!(
                    "browser service entrypoint is missing: {}",
                    config.entrypoint.display()
                ),
            ));
        }
        tokio::fs::create_dir_all(&config.artifact_root)
            .await
            .map_err(io_error("create browser artifact root"))?;
        tokio::fs::create_dir_all(&config.profile_root)
            .await
            .map_err(io_error("create browser profile root"))?;
        let mut command = Command::new(&config.node_path);
        command
            .arg(&config.entrypoint)
            .arg("--stdio")
            .current_dir(&config.service_dir)
            .env("RETCON_BROWSER_AUTH_TOKEN", &self.inner.token)
            .env("RETCON_BROWSER_ARTIFACT_ROOT", &config.artifact_root)
            .env("RETCON_BROWSER_PROFILE_ROOT", &config.profile_root)
            .env(
                "RETCON_BROWSER_INPUT_ROOTS",
                std::env::join_paths(&config.input_roots).map_err(|error| {
                    BrowserServiceError::new("config", format!("invalid input roots: {error}"))
                })?,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            BrowserServiceError::new("spawn", format!("could not start browser service: {error}"))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BrowserServiceError::new("spawn", "browser service stdin is missing"))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            BrowserServiceError::new("spawn", "browser service stdout is missing")
        })?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                while let Ok(Some(line)) = read_bounded_line(&mut reader, 64 * 1024).await {
                    tracing::debug!(target: "retcon_browser_service", "{line}");
                }
            });
        }
        let generation = self.inner.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let reader_inner = Arc::clone(&self.inner);
        tokio::spawn(async move { read_loop(reader_inner, stdout, generation).await });
        *self.inner.process.lock().await = Some(ProcessState {
            child,
            writer: Arc::new(Mutex::new(stdin)),
            generation,
        });
        let hello = self
            .request_running(
                "service.hello",
                json!({
                    "token": self.inner.token,
                    "protocolVersion": self.inner.config.protocol_version,
                    "clientVersion": env!("CARGO_PKG_VERSION"),
                }),
            )
            .await;
        match hello {
            Ok(value) => {
                let diagnostics: BrowserServiceDiagnostics = serde_json::from_value(value)
                    .map_err(|error| BrowserServiceError::new("protocol", error.to_string()))?;
                if !diagnostics.compatible() {
                    self.kill_current().await;
                    return Err(BrowserServiceError::new(
                        "protocol_mismatch",
                        "browser service handshake returned incompatible diagnostics",
                    ));
                }
                *self
                    .inner
                    .diagnostics
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = diagnostics;
                Ok(())
            }
            Err(error) => {
                self.kill_current().await;
                Err(error)
            }
        }
    }

    async fn request_running(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, BrowserServiceError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let frame = serde_json::to_vec(&json!({"id":id,"method":method,"params":params}))
            .map_err(|error| BrowserServiceError::new("encode", error.to_string()))?;
        if frame.len() > MAX_REQUEST_BYTES {
            return Err(BrowserServiceError::new(
                "frame_too_large",
                "browser request exceeds 1 MiB",
            ));
        }
        let writer = self
            .inner
            .process
            .lock()
            .await
            .as_ref()
            .map(|process| Arc::clone(&process.writer))
            .ok_or_else(|| {
                BrowserServiceError::new("process_exited", "browser service is not running")
            })?;
        let (sender, receiver) = oneshot::channel();
        self.inner.pending.lock().await.insert(id, sender);
        {
            let mut writer = writer.lock().await;
            if let Err(error) = writer.write_all(&frame).await {
                self.inner.pending.lock().await.remove(&id);
                return Err(io_error("write browser request")(error));
            }
            if let Err(error) = writer.write_all(b"\n").await {
                self.inner.pending.lock().await.remove(&id);
                return Err(io_error("finish browser request")(error));
            }
        }
        match tokio::time::timeout(self.inner.config.request_timeout, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(BrowserServiceError::new(
                "process_exited",
                "browser service exited before replying",
            )),
            Err(_) => {
                self.inner.pending.lock().await.remove(&id);
                let _ = self.send_cancel(id).await;
                Err(BrowserServiceError::new(
                    "timeout",
                    format!("browser request '{method}' timed out"),
                ))
            }
        }
    }

    async fn send_cancel(&self, request_id: u64) -> Result<(), BrowserServiceError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let frame = serde_json::to_vec(&json!({
            "id":id,
            "method":"browser.cancel",
            "params":{"requestId":request_id}
        }))
        .map_err(|error| BrowserServiceError::new("encode", error.to_string()))?;
        let writer = self
            .inner
            .process
            .lock()
            .await
            .as_ref()
            .map(|process| Arc::clone(&process.writer))
            .ok_or_else(|| {
                BrowserServiceError::new("process_exited", "browser service is not running")
            })?;
        let mut writer = writer.lock().await;
        writer
            .write_all(&frame)
            .await
            .map_err(io_error("cancel browser request"))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(io_error("finish browser cancellation"))
    }

    pub async fn shutdown_process(&self) -> Result<(), BrowserServiceError> {
        if self.inner.process.lock().await.is_none() {
            return Ok(());
        }
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            self.request_running("service.shutdown", json!({})),
        )
        .await;
        self.kill_current().await;
        Ok(())
    }

    async fn kill_current(&self) {
        if let Some(mut process) = self.inner.process.lock().await.take() {
            let _ = process.child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(3), process.child.wait()).await;
        }
        drain_pending(
            &self.inner.pending,
            BrowserServiceError::new("process_exited", "browser service stopped"),
        )
        .await;
    }

    #[cfg(test)]
    pub(crate) async fn terminate_for_test(&self) {
        self.kill_current().await;
    }
}

impl BrowserTransport for NodeStdioTransport {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> BrowserFuture<'a, Value> {
        Box::pin(async move {
            self.ensure_started().await?;
            self.request_running(method, params).await
        })
    }
}

pub struct NodeBrowserService {
    transport: Arc<NodeStdioTransport>,
    sessions: Arc<SyncMutex<HashMap<Uuid, BrowserLaunchRequest>>>,
    headed_takeovers: Arc<SyncMutex<HashMap<Uuid, bool>>>,
    artifact_root: PathBuf,
}

impl NodeBrowserService {
    #[must_use]
    pub fn new(config: NodeBrowserServiceConfig) -> Self {
        let artifact_root = config.artifact_root.clone();
        Self {
            transport: Arc::new(NodeStdioTransport::new(config)),
            sessions: Arc::new(SyncMutex::new(HashMap::new())),
            headed_takeovers: Arc::new(SyncMutex::new(HashMap::new())),
            artifact_root,
        }
    }

    #[must_use]
    pub fn discover(data_dir: &Path) -> Self {
        Self::new(NodeBrowserServiceConfig::discover(data_dir))
    }

    async fn launch_wire(
        &self,
        request: &BrowserLaunchRequest,
    ) -> Result<BrowserLaunchResult, BrowserServiceError> {
        if !request.input_roots.is_empty() {
            self.transport
                .request("service.roots.add", json!({"roots":request.input_roots}))
                .await?;
        }
        let profile_name = Path::new(&request.profile_path)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                BrowserServiceError::new("validation", "browser profile has no safe name")
            })?;
        let value = self
            .transport
            .request(
                "browser.launch",
                json!({
                    "sessionId":request.session_id,
                    "profileMode":if request.persistent_profile {"persistent"} else {"temporary"},
                    "profileName":profile_name,
                    "headless":true,
                }),
            )
            .await?;
        let service_session_id = value
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let initial_tab = value.get("pageId").and_then(Value::as_str).map(|page_id| {
            json!({
                "serviceTabId":page_id,
                "url":value.get("url").cloned().unwrap_or_else(|| Value::String("about:blank".into())),
                "title":"",
            })
        });
        Ok(BrowserLaunchResult {
            service_session_id,
            initial_tab,
        })
    }

    async fn call_wire(
        &self,
        session_id: Uuid,
        method: &str,
        params: Value,
    ) -> Result<BrowserCallResult, BrowserServiceError> {
        let (wire_method, wire_params) =
            map_call(session_id, method, params, &self.headed_takeovers)?;
        let mut value = self
            .transport
            .request(&wire_method, wire_params.clone())
            .await;
        let needs_recovery = value.as_ref().err().is_some_and(|error| {
            matches!(error.code.as_str(), "process_exited" | "spawn")
                || (error.code == "service_error" && error.message.contains("is not running"))
        });
        if needs_recovery {
            let launch = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&session_id)
                .cloned();
            if let Some(launch) = launch {
                self.launch_wire(&launch).await?;
                if method.starts_with("browser.observation.") {
                    value = self
                        .transport
                        .request(&wire_method, wire_params.clone())
                        .await;
                } else {
                    return Err(BrowserServiceError::new(
                        "recovered_retry_required",
                        "browser service restarted; retry the operation to avoid duplicate automation",
                    ));
                }
            }
        }
        let mut value = value?;
        if method == "browser.observation.snapshot" {
            let performance = self
                .transport
                .request("browser.performance", json!({"sessionId":session_id}))
                .await?;
            if let Some(object) = value.as_object_mut() {
                object.insert("performance".into(), performance);
            }
        }
        normalize_result(method, &mut value);
        let artifacts = extract_artifacts(method, &value, &self.artifact_root).await?;
        Ok(BrowserCallResult { value, artifacts })
    }

    #[cfg(test)]
    pub(crate) async fn terminate_for_test(&self) {
        self.transport.terminate_for_test().await;
    }
}

impl BrowserService for NodeBrowserService {
    fn diagnostics(&self) -> Result<BrowserServiceDiagnostics, BrowserServiceError> {
        Ok(self.transport.diagnostics())
    }

    fn launch<'a>(
        &'a self,
        request: &'a BrowserLaunchRequest,
    ) -> BrowserFuture<'a, BrowserLaunchResult> {
        Box::pin(async move {
            let result = self.launch_wire(request).await?;
            self.sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(request.session_id, request.clone());
            Ok(result)
        })
    }

    fn close<'a>(&'a self, session_id: Uuid) -> BrowserFuture<'a, ()> {
        Box::pin(async move {
            self.transport
                .request("browser.close", json!({"sessionId":session_id}))
                .await?;
            self.sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&session_id);
            Ok(())
        })
    }

    fn call<'a>(
        &'a self,
        session_id: Uuid,
        method: &'a str,
        params: Value,
    ) -> BrowserFuture<'a, BrowserCallResult> {
        Box::pin(async move { self.call_wire(session_id, method, params).await })
    }

    fn shutdown(&self) -> BrowserFuture<'_, ()> {
        Box::pin(async move { self.transport.shutdown_process().await })
    }
}

fn map_call(
    session_id: Uuid,
    method: &str,
    params: Value,
    headed_takeovers: &SyncMutex<HashMap<Uuid, bool>>,
) -> Result<(String, Value), BrowserServiceError> {
    let mut object = params.as_object().cloned().ok_or_else(|| {
        BrowserServiceError::new("validation", "browser params must be an object")
    })?;
    object.insert("sessionId".into(), Value::String(session_id.to_string()));
    if let Some(service_tab_id) = object.remove("serviceTabId") {
        object.insert("pageId".into(), service_tab_id);
    }
    let wire_method = match method {
        "browser.tab.open" => "browser.tab.new",
        "browser.tab.close" => "browser.tab.close",
        "browser.tab.activate" => "browser.tab.select",
        "browser.navigate" | "browser.back" | "browser.forward" | "browser.reload" => method,
        "browser.observation.screenshot" => "browser.screenshot",
        "browser.observation.snapshot" => "browser.snapshot",
        "browser.observation.logs" => "browser.logs",
        "browser.observation.trace" => {
            object.remove("path");
            if object.get("action").and_then(Value::as_str) == Some("start") {
                "browser.trace.start"
            } else {
                "browser.trace.stop"
            }
        }
        "browser.automation.action" => "browser.action",
        "browser.automation.script" => {
            object.insert("action".into(), Value::String("script".into()));
            "browser.action"
        }
        "browser.automation.upload" => {
            object.insert("action".into(), Value::String("upload".into()));
            "browser.action"
        }
        "browser.automation.download" => {
            object.insert("action".into(), Value::String("download".into()));
            object.remove("destination");
            object.remove("path");
            "browser.action"
        }
        "browser.takeover.start" => {
            let headed = object
                .get("headed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            headed_takeovers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(session_id, headed);
            if headed {
                "browser.takeover.open"
            } else {
                "browser.pause"
            }
        }
        "browser.takeover.stop" => {
            let headed = headed_takeovers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&session_id)
                .unwrap_or(false);
            if headed {
                "browser.takeover.resume"
            } else {
                "browser.resume"
            }
        }
        _ => {
            return Err(BrowserServiceError::new(
                "unsupported",
                format!("unsupported browser service method: {method}"),
            ));
        }
    };
    Ok((wire_method.into(), Value::Object(object)))
}

fn normalize_result(method: &str, value: &mut Value) {
    if matches!(method, "browser.tab.open" | "browser.tab.activate")
        && let Some(object) = value.as_object_mut()
        && let Some(page_id) = object.remove("pageId")
    {
        object.insert("serviceTabId".into(), page_id);
    }
}

async fn extract_artifacts(
    method: &str,
    value: &Value,
    artifact_root: &Path,
) -> Result<Vec<BrowserServiceArtifact>, BrowserServiceError> {
    let Some((kind, mime_type)) = (match method {
        "browser.observation.screenshot" => Some(("screenshot", "image/png")),
        "browser.observation.trace" if value.get("path").is_some() => {
            Some(("trace", "application/zip"))
        }
        "browser.automation.download" => Some(("download", "application/octet-stream")),
        _ => None,
    }) else {
        return Ok(Vec::new());
    };
    let path = value.get("path").and_then(Value::as_str).ok_or_else(|| {
        BrowserServiceError::new("artifact", "browser service omitted artifact path")
    })?;
    let root = tokio::fs::canonicalize(artifact_root)
        .await
        .map_err(io_error("resolve browser artifact root"))?;
    let path = tokio::fs::canonicalize(path)
        .await
        .map_err(io_error("resolve browser artifact"))?;
    if !path.starts_with(&root) {
        return Err(BrowserServiceError::new(
            "artifact_boundary",
            "browser artifact escaped the managed artifact root",
        ));
    }
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(io_error("inspect browser artifact"))?;
    if metadata.len() > MAX_SERVICE_ARTIFACT_BYTES as u64 {
        return Err(BrowserServiceError::new(
            "artifact_too_large",
            "browser artifact exceeds 16 MiB",
        ));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(io_error("read browser artifact"))?;
    Ok(vec![BrowserServiceArtifact {
        kind: kind.into(),
        mime_type: mime_type.into(),
        bytes,
        metadata: json!({"servicePath":path,"serviceMetadata":value}),
    }])
}

async fn read_loop(
    inner: Arc<TransportInner>,
    stdout: tokio::process::ChildStdout,
    generation: u64,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_bounded_line(&mut reader, MAX_RESPONSE_BYTES).await {
            Ok(Some(line)) => {
                let value: Value = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(%error, "browser service emitted invalid JSON");
                        continue;
                    }
                };
                let Some(id) = value.get("id").and_then(Value::as_u64) else {
                    continue;
                };
                let Some(sender) = inner.pending.lock().await.remove(&id) else {
                    continue;
                };
                if let Some(error) = value.get("error") {
                    let code = error
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or("service_error");
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("browser service request failed");
                    let _ = sender.send(Err(BrowserServiceError::new(code, message)));
                } else {
                    let _ = sender.send(Ok(value
                        .get("result")
                        .cloned()
                        .unwrap_or_else(|| json!({}))));
                }
            }
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "browser service frame reader stopped");
                break;
            }
        }
    }
    let mut process = inner.process.lock().await;
    if process
        .as_ref()
        .is_some_and(|value| value.generation == generation)
    {
        process.take();
    }
    drop(process);
    drain_pending(
        &inner.pending,
        BrowserServiceError::new("process_exited", "browser service process exited"),
    )
    .await;
}

async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    maximum: usize,
) -> Result<Option<String>, std::io::Error> {
    let mut bytes = Vec::new();
    let read = reader.read_until(b'\n', &mut bytes).await?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.len() > maximum {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "browser service frame exceeds configured bound",
        ));
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

async fn drain_pending(pending: &Mutex<HashMap<u64, PendingSender>>, error: BrowserServiceError) {
    for (_, sender) in pending.lock().await.drain() {
        let _ = sender.send(Err(error.clone()));
    }
}

fn io_error(operation: &'static str) -> impl FnOnce(std::io::Error) -> BrowserServiceError {
    move |error| BrowserServiceError::new("io", format!("{operation}: {error}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    use super::*;

    fn service_config(data_dir: &Path) -> NodeBrowserServiceConfig {
        let service_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/browser-service");
        NodeBrowserServiceConfig {
            node_path: PathBuf::from("node"),
            entrypoint: service_dir.join("src/main.ts"),
            service_dir,
            artifact_root: data_dir.join("artifacts"),
            profile_root: data_dir.join("profiles"),
            input_roots: vec![data_dir.to_path_buf()],
            request_timeout: Duration::from_secs(30),
            protocol_version: BROWSER_SERVICE_PROTOCOL,
        }
    }

    async fn fixture() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut reader = BufReader::new(reader);
                    let mut first = String::new();
                    if reader.read_line(&mut first).await.is_err() {
                        return;
                    }
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).await.is_err()
                            || matches!(header.as_str(), "\r\n" | "\n" | "")
                        {
                            break;
                        }
                    }
                    let next = first.contains("GET /next ");
                    let body = if next {
                        "<title>Next</title><h1>next</h1>"
                    } else {
                        r#"<!doctype html><title>Rust fixture</title>
                        <input id="name"><button id="proof" onclick="this.textContent='clicked';console.log('clicked')">proof</button>"#
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = writer.write_all(response.as_bytes()).await;
                });
            }
        });
        (format!("http://{address}"), task)
    }

    #[tokio::test]
    async fn rust_node_chromium_lifecycle_artifacts_takeover_and_recovery() {
        if std::env::var("RETCON_BROWSER_E2E").as_deref() != Ok("1") {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let (url, fixture) = fixture().await;
        let config = service_config(directory.path());
        let profile_root = config.profile_root.clone();
        let service = NodeBrowserService::new(config);
        let session_id = Uuid::new_v4();
        let profile_name = Uuid::new_v4().to_string();
        let launch = BrowserLaunchRequest {
            session_id,
            profile_path: profile_root
                .join(&profile_name)
                .to_string_lossy()
                .into_owned(),
            persistent_profile: true,
            network_policy: "loopback".into(),
            input_roots: vec![directory.path().to_string_lossy().into_owned()],
        };
        let started = service.launch(&launch).await.unwrap();
        assert_eq!(started.service_session_id, session_id.to_string());
        let tab_id = started.initial_tab.unwrap()["serviceTabId"]
            .as_str()
            .unwrap()
            .to_owned();
        let navigated = service
            .call(
                session_id,
                "browser.navigate",
                json!({"serviceTabId":tab_id,"url":url}),
            )
            .await
            .unwrap();
        assert_eq!(navigated.value["status"], 200);
        service
            .call(
                session_id,
                "browser.automation.action",
                json!({"serviceTabId":tab_id,"action":"fill","selector":"#name","value":"Retcon"}),
            )
            .await
            .unwrap();
        service
            .call(
                session_id,
                "browser.automation.action",
                json!({"serviceTabId":tab_id,"action":"click","selector":"#proof"}),
            )
            .await
            .unwrap();
        let text = service
            .call(
                session_id,
                "browser.automation.action",
                json!({"serviceTabId":tab_id,"action":"text","selector":"#proof"}),
            )
            .await
            .unwrap();
        assert_eq!(text.value["text"], "clicked");
        let snapshot = service
            .call(
                session_id,
                "browser.observation.snapshot",
                json!({"serviceTabId":tab_id}),
            )
            .await
            .unwrap();
        assert!(
            snapshot.value["accessibility"]
                .as_str()
                .unwrap()
                .contains("clicked")
        );
        assert!(snapshot.value["performance"]["navigation"].is_array());
        let screenshot = service
            .call(
                session_id,
                "browser.observation.screenshot",
                json!({"serviceTabId":tab_id,"path":"rust-e2e.png","fullPage":true}),
            )
            .await
            .unwrap();
        assert_eq!(screenshot.artifacts[0].kind, "screenshot");
        assert!(!screenshot.artifacts[0].bytes.is_empty());
        let opened = service
            .call(
                session_id,
                "browser.tab.open",
                json!({"url":format!("{url}/next")}),
            )
            .await
            .unwrap();
        assert!(opened.value["serviceTabId"].is_string());
        let logs = service
            .call(session_id, "browser.observation.logs", json!({}))
            .await
            .unwrap();
        assert!(logs.value["console"].is_array());
        service
            .call(
                session_id,
                "browser.takeover.start",
                json!({"headed":false,"reason":"e2e"}),
            )
            .await
            .unwrap();
        service
            .call(session_id, "browser.takeover.stop", json!({}))
            .await
            .unwrap();

        service.terminate_for_test().await;
        let recovered = service
            .call(session_id, "browser.observation.logs", json!({}))
            .await
            .unwrap();
        assert!(recovered.value["console"].is_array());
        service.close(session_id).await.unwrap();
        service.shutdown().await.unwrap();
        assert!(profile_root.join(profile_name).is_dir());
        fixture.abort();
    }

    #[tokio::test]
    async fn rejects_protocol_mismatch_during_authenticated_handshake() {
        if std::env::var("RETCON_BROWSER_E2E").as_deref() != Ok("1") {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let mut config = service_config(directory.path());
        config.protocol_version = BROWSER_SERVICE_PROTOCOL + 1;
        let transport = NodeStdioTransport::new(config);
        let error = transport
            .request("browser.installation", json!({}))
            .await
            .unwrap_err();
        assert_eq!(error.code, "protocol_mismatch");
        transport.shutdown_process().await.unwrap();
    }
}
