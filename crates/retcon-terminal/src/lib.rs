//! PTY management and structured command tracking.
//!
//! Phase 2 spike scope: spawn a shell in a ConPTY, stream output, accept
//! input, resize, and kill. Structured command tracking lands in Phase 14.

mod command_tracker;

pub use command_tracker::CommandLineTracker;

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

fn runtime_metrics_enabled() -> bool {
    retcon_runtime_observability::is_enabled()
}

/// A live PTY session hosting a shell.
///
/// Output is delivered through the `on_output` callback passed to
/// [`PtySession::spawn`], invoked from a dedicated blocking reader thread.
/// Exit detection is by polling [`PtySession::try_exit_code`] — the caller
/// owns the cadence.
pub struct PtySession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
}

impl PtySession {
    /// Spawn `shell` in a new PTY of the given size.
    ///
    /// `on_output` is invoked from a dedicated reader thread for every chunk
    /// the shell writes, and stops when the PTY closes.
    ///
    /// # Errors
    ///
    /// Returns a message if the PTY or the shell process cannot be created.
    pub fn spawn(
        shell: &str,
        cwd: Option<&Path>,
        cols: u16,
        rows: u16,
        mut on_output: impl FnMut(&[u8]) + Send + 'static,
    ) -> Result<Self, String> {
        let span = runtime_metrics_enabled().then(|| {
            tracing::info_span!(
                target: "retcon_runtime",
                "terminal.lifecycle",
                component = "terminal",
                operation = "start"
            )
        });
        let _entered = span.as_ref().map(tracing::Span::enter);
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to open pty: {e}"))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone pty reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("failed to take pty writer: {e}"))?;

        let mut cmd = CommandBuilder::new(shell);
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to spawn shell '{shell}': {e}"))?;
        retcon_runtime_observability::record_count(
            "terminal",
            "terminal.lifecycle.count",
            "start",
            "ok",
        );
        drop(pair.slave);

        // portable-pty reads are synchronous; stream from a blocking thread.
        std::thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => on_output(&buf[..n]),
                    }
                }
            })
            .map_err(|e| format!("failed to start pty reader thread: {e}"))?;

        if runtime_metrics_enabled() {
            tracing::info!(
                target: "retcon_runtime",
                event = "terminal.started",
                component = "terminal",
                operation = "start",
                outcome = "ok"
            );
        }
        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
        })
    }

    /// Write raw bytes (keystrokes) to the shell.
    ///
    /// # Errors
    ///
    /// Returns a message if the PTY writer fails.
    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| "pty writer lock poisoned".to_owned())?;
        writer
            .write_all(data)
            .map_err(|e| format!("pty write failed: {e}"))?;
        writer.flush().map_err(|e| format!("pty flush failed: {e}"))
    }

    /// Resize the PTY.
    ///
    /// # Errors
    ///
    /// Returns a message if the resize is rejected.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let result = self
            .master
            .lock()
            .map_err(|_| "pty master lock poisoned".to_owned())?
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("pty resize failed: {e}"));
        if runtime_metrics_enabled() {
            tracing::debug!(
                target: "retcon_runtime",
                event = "terminal.resized",
                component = "terminal",
                operation = "resize",
                outcome = if result.is_ok() { "ok" } else { "error" }
            );
        }
        result
    }

    /// Check whether the shell has exited, returning its exit code if so.
    pub fn try_exit_code(&self) -> Option<u32> {
        let mut child = self.child.lock().ok()?;
        match child.try_wait() {
            Ok(Some(status)) => {
                retcon_runtime_observability::record_count(
                    "terminal",
                    "terminal.lifecycle.count",
                    "exit",
                    if status.success() { "ok" } else { "error" },
                );
                if runtime_metrics_enabled() {
                    tracing::info!(
                        target: "retcon_runtime",
                        event = "terminal.exited",
                        component = "terminal",
                        operation = "exit",
                        outcome = if status.success() { "ok" } else { "error" }
                    );
                }
                Some(status.exit_code())
            }
            _ => None,
        }
    }

    /// Kill the shell process; ConPTY teardown takes the process tree with it.
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            #[cfg(windows)]
            if let Some(process_id) = child.process_id() {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &process_id.to_string(), "/T", "/F"])
                    .output();
            }
            let _ = child.kill();
            retcon_runtime_observability::record_count(
                "terminal",
                "terminal.lifecycle.count",
                "kill",
                "ok",
            );
            if runtime_metrics_enabled() {
                tracing::info!(
                    target: "retcon_runtime",
                    event = "terminal.killed",
                    component = "terminal",
                    operation = "kill",
                    outcome = "ok"
                );
            }
        }
    }
}

#[cfg(all(test, windows))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "requires an interactive Windows ConPTY host; run manually on the desktop validation matrix"]
    fn powershell_is_interactive_resizable_unicode_and_reports_exit() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&output);
        let session = PtySession::spawn("powershell.exe", None, 80, 24, move |chunk| {
            if let Ok(mut bytes) = captured.lock() {
                bytes.extend_from_slice(chunk);
            }
        })
        .unwrap();
        session.resize(120, 40).unwrap();
        session
            .write("Write-Output 'RETCON_UNICODE_✓'\r\n".as_bytes())
            .unwrap();
        let output_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let seen = output
                .lock()
                .map(|bytes| String::from_utf8_lossy(&bytes).contains("RETCON_UNICODE"))
                .unwrap_or(false);
            if seen {
                break;
            }
            assert!(
                Instant::now() < output_deadline,
                "terminal produced no interactive output"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        session.kill();
        let deadline = Instant::now() + Duration::from_secs(10);
        let code = loop {
            if let Some(code) = session.try_exit_code() {
                break code;
            }
            assert!(Instant::now() < deadline, "shell did not exit");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_ne!(code, u32::MAX);
        let text = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
        assert!(text.contains("RETCON_UNICODE"));
    }
}
