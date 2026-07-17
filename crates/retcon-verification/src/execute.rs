//! Shell-free command execution with cancellation, timeouts, and bounded output.

use std::process::Stdio;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::watch;

use crate::{CommandSpec, GateKind, GateResult, OutputCapture, VerificationStatus};

/// Per-command execution limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionOptions {
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15 * 60),
            max_output_bytes: 1024 * 1024,
        }
    }
}

/// Cooperative cancellation handle for an executing command.
#[derive(Debug, Clone)]
pub struct CancellationHandle {
    sender: watch::Sender<bool>,
}

impl Default for CancellationHandle {
    fn default() -> Self {
        let (sender, _) = watch::channel(false);
        Self { sender }
    }
}

impl CancellationHandle {
    /// Create a non-cancelled handle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. Returns whether receivers were notified.
    pub fn cancel(&self) -> bool {
        !self.sender.send_replace(true)
    }

    async fn cancelled(&self) {
        let mut receiver = self.sender.subscribe();
        if *receiver.borrow() {
            return;
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                return;
            }
        }
    }
}

/// Failure to start, monitor, or capture a verification command.
#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("failed to start {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{stream} was not piped for {program}")]
    MissingPipe {
        program: String,
        stream: &'static str,
    },
    #[error("failed while waiting for {program}: {source}")]
    Wait {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to capture {stream} for {program}: {message}")]
    Capture {
        program: String,
        stream: &'static str,
        message: String,
    },
}

enum Completion {
    Exited(std::process::ExitStatus),
    TimedOut,
    Cancelled,
}

/// Execute one command without a shell and return a normalized gate result.
pub async fn execute(
    gate: GateKind,
    command: &CommandSpec,
    options: ExecutionOptions,
    cancellation: Option<&CancellationHandle>,
) -> Result<GateResult, ExecutionError> {
    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .current_dir(&command.cwd)
        .envs(&command.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = process.spawn().map_err(|source| ExecutionError::Spawn {
        program: command.program.clone(),
        source,
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExecutionError::MissingPipe {
            program: command.program.clone(),
            stream: "stdout",
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ExecutionError::MissingPipe {
            program: command.program.clone(),
            stream: "stderr",
        })?;
    let stdout_task = tokio::spawn(read_bounded(stdout, options.max_output_bytes));
    let stderr_task = tokio::spawn(read_bounded(stderr, options.max_output_bytes));
    let fallback_cancellation = CancellationHandle::new();
    let cancellation = cancellation.unwrap_or(&fallback_cancellation);
    let started = Instant::now();

    let completion = tokio::select! {
        status = child.wait() => Completion::Exited(status.map_err(|source| ExecutionError::Wait {
            program: command.program.clone(),
            source,
        })?),
        () = tokio::time::sleep(options.timeout) => Completion::TimedOut,
        () = cancellation.cancelled() => Completion::Cancelled,
    };
    if !matches!(completion, Completion::Exited(_)) {
        let _ = child.start_kill();
        child.wait().await.map_err(|source| ExecutionError::Wait {
            program: command.program.clone(),
            source,
        })?;
    }

    let stdout = join_capture(stdout_task, &command.program, "stdout").await?;
    let stderr = join_capture(stderr_task, &command.program, "stderr").await?;
    let (status, exit_code) = match completion {
        Completion::Exited(exit) if exit.success() => (VerificationStatus::Passed, exit.code()),
        Completion::Exited(exit) => (VerificationStatus::Failed, exit.code()),
        Completion::TimedOut => (VerificationStatus::TimedOut, None),
        Completion::Cancelled => (VerificationStatus::Cancelled, None),
    };
    let tests =
        crate::parse_test_output(command.parser, &stdout.text, &stderr.text).unwrap_or_default();
    Ok(GateResult {
        gate,
        command: command.clone(),
        status,
        exit_code,
        duration_ms: millis(started.elapsed()),
        stdout,
        stderr,
        tests,
    })
}

async fn read_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> Result<OutputCapture, std::io::Error> {
    let mut captured = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    let mut total_bytes = 0_u64;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        let remaining = limit.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(OutputCapture {
        text: String::from_utf8_lossy(&captured).into_owned(),
        truncated: total_bytes > u64::try_from(captured.len()).unwrap_or(u64::MAX),
        total_bytes,
    })
}

async fn join_capture(
    task: tokio::task::JoinHandle<Result<OutputCapture, std::io::Error>>,
    program: &str,
    stream: &'static str,
) -> Result<OutputCapture, ExecutionError> {
    task.await
        .map_err(|error| ExecutionError::Capture {
            program: program.to_owned(),
            stream,
            message: error.to_string(),
        })?
        .map_err(|error| ExecutionError::Capture {
            program: program.to_owned(),
            stream,
            message: error.to_string(),
        })
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
