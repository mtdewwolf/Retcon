//! Shell-free development-server process lifecycle.

#![allow(missing_docs)]

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use regex::Regex;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::{
    DevServerCommand, DevServerEvent, LogStream, PortAllocator, PortReservation, ReadyInfo,
    StartOptions, StartResult,
};

const EVENT_CAPACITY: usize = 256;
const PIPE_CHUNK_BYTES: usize = 8 * 1024;

/// Receiver for bounded live development-server events.
pub type EventStream = broadcast::Receiver<DevServerEvent>;

/// Development-server detection, validation, allocation, or execution failure.
#[derive(Debug, Error)]
pub enum DevServerError {
    #[error("development-server detection failed: {0}")]
    Detection(String),
    #[error("invalid development-server command: {0}")]
    InvalidCommand(String),
    #[error("invalid development-server environment: {0}")]
    InvalidEnvironment(String),
    #[error("invalid port range {first}..={last}")]
    InvalidPortRange { first: u16, last: u16 },
    #[error("localhost port {0} is unavailable")]
    PortUnavailable(u16),
    #[error("no localhost port is available in {first}..={last}")]
    NoAvailablePort { first: u16, last: u16 },
    #[error("development-server runtime state is unavailable")]
    RuntimeState,
    #[error("failed to {0}: {1}")]
    Io(&'static str, #[source] std::io::Error),
    #[error("failed to spawn '{program}': {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("development server exited during startup with code {0:?}")]
    StartupExited(Option<i32>),
    #[error("development server did not become ready before the startup timeout")]
    StartupTimeout,
    #[error("development server was stopped during startup")]
    StartupCancelled,
    #[error("no development-server start configuration is available to restart")]
    NothingToRestart,
    #[error("development-server supervisor failed: {0}")]
    Supervisor(String),
}

#[derive(Debug)]
struct ActiveRun {
    run_id: u64,
    stop: watch::Sender<bool>,
    join: JoinHandle<()>,
}

#[derive(Debug, Default)]
struct RuntimeState {
    active: Option<ActiveRun>,
    last: Option<(DevServerCommand, StartOptions)>,
}

/// One restartable development-server runtime.
#[derive(Debug)]
pub struct DevServer {
    allocator: PortAllocator,
    events: broadcast::Sender<DevServerEvent>,
    control: Mutex<()>,
    state: Arc<Mutex<RuntimeState>>,
    next_run_id: AtomicU64,
}

impl Default for DevServer {
    fn default() -> Self {
        Self::new(PortAllocator::default())
    }
}

impl DevServer {
    /// Construct a runtime using a shared port allocator.
    #[must_use]
    pub fn new(allocator: PortAllocator) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Self {
            allocator,
            events,
            control: Mutex::new(()),
            state: Arc::new(Mutex::new(RuntimeState::default())),
            next_run_id: AtomicU64::new(1),
        }
    }

    /// Subscribe to future lifecycle and bounded log events.
    #[must_use]
    pub fn subscribe(&self) -> EventStream {
        self.events.subscribe()
    }

    /// Validate and start a server, returning only after readiness is detected.
    pub async fn start(
        &self,
        command: DevServerCommand,
        options: StartOptions,
    ) -> Result<StartResult, DevServerError> {
        let _control = self.control.lock().await;
        self.start_inner(command, options).await
    }

    async fn start_inner(
        &self,
        command: DevServerCommand,
        options: StartOptions,
    ) -> Result<StartResult, DevServerError> {
        validate_options(&options)?;
        let command = validate_command(command)?;
        let restart_command = command.clone();
        self.stop_inner().await?;
        let mut reservation = self.allocator.reserve(
            &options.project_key,
            &options.worktree_key,
            options.requested_port,
            options.allow_alternate_port,
        )?;
        let port = reservation.port();
        let command = materialize_port(command, port);
        let run_id = self.next_run_id.fetch_add(1, Ordering::Relaxed);
        let (stop, stop_receiver) = watch::channel(false);
        let (startup_sender, startup_receiver) = oneshot::channel();
        let events = self.events.clone();
        let supervisor_command = command.clone();
        let supervisor_options = options.clone();
        let _ = events.send(DevServerEvent::Starting {
            run_id,
            framework: command.framework,
            port,
        });
        let join = tokio::spawn(async move {
            reservation.release_socket_for_child();
            supervise(
                run_id,
                supervisor_command,
                supervisor_options,
                reservation,
                stop_receiver,
                startup_sender,
                events,
            )
            .await;
        });
        {
            let mut state = self.state.lock().await;
            state.last = Some((restart_command, options));
            state.active = Some(ActiveRun { run_id, stop, join });
        }

        match startup_receiver.await {
            Ok(Ok(ready)) => Ok(StartResult {
                run_id,
                framework: command.framework,
                ready,
            }),
            Ok(Err(error)) => {
                self.join_failed_start(run_id).await;
                Err(error)
            }
            Err(error) => {
                self.join_failed_start(run_id).await;
                Err(DevServerError::Supervisor(error.to_string()))
            }
        }
    }

    /// Stop the active direct child and await cleanup. A stopped runtime is a no-op.
    pub async fn stop(&self) -> Result<(), DevServerError> {
        let _control = self.control.lock().await;
        self.stop_inner().await
    }

    async fn stop_inner(&self) -> Result<(), DevServerError> {
        let active = self.state.lock().await.active.take();
        if let Some(active) = active {
            active.stop.send_replace(true);
            active
                .join
                .await
                .map_err(|error| DevServerError::Supervisor(error.to_string()))?;
        }
        Ok(())
    }

    /// Stop and start again with the most recent validated configuration.
    pub async fn restart(&self) -> Result<StartResult, DevServerError> {
        let _control = self.control.lock().await;
        let previous = self.state.lock().await.last.clone();
        let (command, options) = previous.ok_or(DevServerError::NothingToRestart)?;
        self.stop_inner().await?;
        self.start_inner(command, options).await
    }

    async fn join_failed_start(&self, run_id: u64) {
        let active = {
            let mut state = self.state.lock().await;
            match state.active.as_ref() {
                Some(active) if active.run_id == run_id => state.active.take(),
                _ => None,
            }
        };
        if let Some(active) = active {
            let _ = active.join.await;
        }
    }
}

impl Drop for DevServer {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.try_lock()
            && let Some(active) = state.active.take()
        {
            active.stop.send_replace(true);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn supervise(
    run_id: u64,
    command: DevServerCommand,
    options: StartOptions,
    _reservation: PortReservation,
    mut stop: watch::Receiver<bool>,
    startup: oneshot::Sender<Result<ReadyInfo, DevServerError>>,
    events: broadcast::Sender<DevServerEvent>,
) {
    let port = command
        .env
        .get("PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_default();
    let mut child = match spawn_child(&command) {
        Ok(child) => child,
        Err(error) => {
            let _ = events.send(DevServerEvent::StartupFailed {
                run_id,
                message: error.to_string(),
            });
            let _ = startup.send(Err(error));
            return;
        }
    };
    let (output_sender, mut output_receiver) = mpsc::channel(64);
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(pump_output(
            stdout,
            LogStream::Stdout,
            output_sender.clone(),
        ));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(pump_output(
            stderr,
            LogStream::Stderr,
            output_sender.clone(),
        ));
    }
    drop(output_sender);

    let mut startup = Some(startup);
    let mut ready = false;
    let mut output_open = true;
    let mut stdout_budget = StreamBudget::default();
    let mut stderr_budget = StreamBudget::default();
    let mut readiness_poll = tokio::time::interval(Duration::from_millis(75));
    readiness_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let startup_timer = tokio::time::sleep(options.startup_timeout);
    tokio::pin!(startup_timer);

    loop {
        tokio::select! {
            status = child.wait() => {
                let code = status.ok().and_then(|status| status.code());
                if !ready {
                    let error = DevServerError::StartupExited(code);
                    let _ = events.send(DevServerEvent::StartupFailed {
                        run_id,
                        message: error.to_string(),
                    });
                    send_startup(&mut startup, Err(error));
                }
                let event = if code == Some(0) {
                    DevServerEvent::Exited { run_id, exit_code: code }
                } else {
                    DevServerEvent::Crashed { run_id, exit_code: code }
                };
                let _ = events.send(event);
                return;
            }
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    terminate_child(&mut child).await;
                    send_startup(&mut startup, Err(DevServerError::StartupCancelled));
                    let _ = events.send(DevServerEvent::Stopped { run_id });
                    return;
                }
            }
            () = &mut startup_timer, if !ready => {
                terminate_child(&mut child).await;
                let error = DevServerError::StartupTimeout;
                let _ = events.send(DevServerEvent::StartupFailed {
                    run_id,
                    message: error.to_string(),
                });
                send_startup(&mut startup, Err(error));
                return;
            }
            _ = readiness_poll.tick(), if !ready => {
                if local_port_is_listening(port).await {
                    mark_ready(run_id, port, None, false, &events, &mut startup);
                    ready = true;
                }
            }
            output = output_receiver.recv(), if output_open => {
                match output {
                    Some(output) => {
                        let text = String::from_utf8_lossy(&output.bytes);
                        let budget = match output.stream {
                            LogStream::Stdout => &mut stdout_budget,
                            LogStream::Stderr => &mut stderr_budget,
                        };
                        let recognized = update_recognition(budget, &text);
                        let hot_reload = is_hot_reload(&recognized) && !budget.hot_reload_emitted;
                        budget.hot_reload_emitted |= hot_reload;
                        emit_bounded_log(
                            run_id,
                            output.stream,
                            &output.bytes,
                            options.max_output_bytes,
                            budget,
                            &events,
                        );
                        if hot_reload {
                            let _ = events.send(DevServerEvent::HotReload {
                                run_id,
                                message: bounded_message(&recognized),
                            });
                        }
                        if !ready
                            && let Some(url) = detect_local_url(&recognized, port)
                        {
                            mark_ready(run_id, port, Some(url), true, &events, &mut startup);
                            ready = true;
                        }
                    }
                    None => output_open = false,
                }
            }
        }
    }
}

fn spawn_child(command: &DevServerCommand) -> Result<Child, DevServerError> {
    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .current_dir(&command.cwd)
        .envs(&command.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process.spawn().map_err(|source| DevServerError::Spawn {
        program: command.program.clone(),
        source,
    })
}

async fn terminate_child(child: &mut Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

#[derive(Debug)]
struct OutputChunk {
    stream: LogStream,
    bytes: Vec<u8>,
}

async fn pump_output<R: AsyncRead + Unpin>(
    mut reader: R,
    stream: LogStream,
    sender: mpsc::Sender<OutputChunk>,
) {
    let mut buffer = [0_u8; PIPE_CHUNK_BYTES];
    loop {
        let Ok(read) = reader.read(&mut buffer).await else {
            return;
        };
        if read == 0 {
            return;
        }
        if sender
            .send(OutputChunk {
                stream,
                bytes: buffer[..read].to_vec(),
            })
            .await
            .is_err()
        {
            return;
        }
    }
}

#[derive(Debug, Default)]
struct StreamBudget {
    retained: usize,
    total: u64,
    truncated_event_sent: bool,
    recognition: String,
    hot_reload_emitted: bool,
}

fn update_recognition(budget: &mut StreamBudget, text: &str) -> String {
    budget.recognition.push_str(text);
    if budget.recognition.len() > 4_096 {
        let keep_from = budget.recognition.len() - 4_096;
        let keep_from = budget
            .recognition
            .char_indices()
            .find_map(|(index, _)| (index >= keep_from).then_some(index))
            .unwrap_or(0);
        budget.recognition.drain(..keep_from);
    }
    budget.recognition.clone()
}

fn emit_bounded_log(
    run_id: u64,
    stream: LogStream,
    bytes: &[u8],
    limit: usize,
    budget: &mut StreamBudget,
    events: &broadcast::Sender<DevServerEvent>,
) {
    budget.total = budget
        .total
        .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    let retained = bytes.len().min(limit.saturating_sub(budget.retained));
    if retained > 0 {
        budget.retained = budget.retained.saturating_add(retained);
        let _ = events.send(DevServerEvent::Log {
            run_id,
            stream,
            text: String::from_utf8_lossy(&bytes[..retained]).into_owned(),
            retained_bytes: u64::try_from(budget.retained).unwrap_or(u64::MAX),
            total_bytes: budget.total,
        });
    }
    if retained < bytes.len() && !budget.truncated_event_sent {
        budget.truncated_event_sent = true;
        let _ = events.send(DevServerEvent::OutputTruncated {
            run_id,
            stream,
            limit_bytes: limit,
        });
    }
}

fn mark_ready(
    run_id: u64,
    port: u16,
    url: Option<String>,
    detected_from_output: bool,
    events: &broadcast::Sender<DevServerEvent>,
    startup: &mut Option<oneshot::Sender<Result<ReadyInfo, DevServerError>>>,
) {
    let info = ReadyInfo {
        port,
        url: url.unwrap_or_else(|| format!("http://localhost:{port}")),
        detected_from_output,
    };
    let _ = events.send(DevServerEvent::Ready {
        run_id,
        info: info.clone(),
    });
    send_startup(startup, Ok(info));
}

fn send_startup(
    startup: &mut Option<oneshot::Sender<Result<ReadyInfo, DevServerError>>>,
    result: Result<ReadyInfo, DevServerError>,
) {
    if let Some(sender) = startup.take() {
        let _ = sender.send(result);
    }
}

async fn local_port_is_listening(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    tokio::time::timeout(
        Duration::from_millis(50),
        TcpStream::connect(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

fn detect_local_url(output: &str, expected_port: u16) -> Option<String> {
    let regex =
        Regex::new(r"https?://(?:localhost|127\.0\.0\.1|\[::1\])(?::(?P<port>\d{1,5}))?[^\s\x1b]*")
            .ok()?;
    for capture in regex.captures_iter(output) {
        let port = capture
            .name("port")
            .and_then(|value| value.as_str().parse::<u16>().ok())
            .unwrap_or(80);
        if port == expected_port {
            return capture.get(0).map(|value| {
                value
                    .as_str()
                    .trim_end_matches(|character: char| ",.;)]}".contains(character))
                    .to_owned()
            });
        }
    }
    None
}

fn is_hot_reload(output: &str) -> bool {
    let output = output.to_ascii_lowercase();
    [
        "hot reload",
        "hot-reload",
        "hmr update",
        "hmr connected",
        "reloading",
        "compiled successfully",
    ]
    .iter()
    .any(|marker| output.contains(marker))
}

fn bounded_message(message: &str) -> String {
    message.chars().take(512).collect::<String>()
}

fn validate_options(options: &StartOptions) -> Result<(), DevServerError> {
    if options.project_key.trim().is_empty() {
        return Err(DevServerError::InvalidCommand(
            "project_key cannot be empty".into(),
        ));
    }
    if options.worktree_key.trim().is_empty() {
        return Err(DevServerError::InvalidCommand(
            "worktree_key cannot be empty".into(),
        ));
    }
    if options.startup_timeout.is_zero() {
        return Err(DevServerError::InvalidCommand(
            "startup_timeout must be positive".into(),
        ));
    }
    Ok(())
}

fn validate_command(mut command: DevServerCommand) -> Result<DevServerCommand, DevServerError> {
    if command.program.trim().is_empty() || command.program.contains('\0') {
        return Err(DevServerError::InvalidCommand(
            "program cannot be empty or contain NUL".into(),
        ));
    }
    if command.args.iter().any(|argument| argument.contains('\0')) {
        return Err(DevServerError::InvalidCommand(
            "arguments cannot contain NUL".into(),
        ));
    }
    if !command.cwd.is_dir() {
        return Err(DevServerError::InvalidCommand(format!(
            "working directory is not a directory: {}",
            command.cwd.display()
        )));
    }
    command.cwd = command.cwd.canonicalize().map_err(|error| {
        DevServerError::Io("resolve development-server working directory", error)
    })?;
    validate_environment(&command.env)?;
    Ok(command)
}

fn validate_environment(environment: &BTreeMap<String, String>) -> Result<(), DevServerError> {
    for (key, value) in environment {
        if key.is_empty() || key.contains(['=', '\0']) {
            return Err(DevServerError::InvalidEnvironment(format!(
                "invalid environment variable name '{key}'"
            )));
        }
        if value.contains('\0') {
            return Err(DevServerError::InvalidEnvironment(format!(
                "environment variable '{key}' contains NUL"
            )));
        }
    }
    Ok(())
}

fn materialize_port(mut command: DevServerCommand, port: u16) -> DevServerCommand {
    let port = port.to_string();
    command.args = command
        .args
        .into_iter()
        .map(|argument| argument.replace("{port}", &port))
        .collect();
    command.env = command
        .env
        .into_iter()
        .map(|(key, value)| (key, value.replace("{port}", &port)))
        .collect();
    command.env.entry("PORT".into()).or_insert(port);
    command
        .env
        .entry("HOST".into())
        .or_insert_with(|| "127.0.0.1".into());
    command
}
