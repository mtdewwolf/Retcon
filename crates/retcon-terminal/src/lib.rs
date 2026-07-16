//! PTY management and structured command tracking.
//!
//! Phase 2 spike scope: spawn a shell in a ConPTY, stream output, accept
//! input, resize, and kill. Structured command tracking lands in Phase 14.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// A live PTY session hosting a shell.
///
/// Output is delivered through the `on_output` callback passed to
/// [`PtySession::spawn`], invoked from a dedicated blocking reader thread.
/// Exit detection is by polling [`PtySession::try_exit_code`] — the caller
/// owns the cadence.
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
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
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to open pty: {e}"))?;

        let mut cmd = CommandBuilder::new(shell);
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to spawn shell '{shell}': {e}"))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone pty reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("failed to take pty writer: {e}"))?;

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

        Ok(Self {
            master: pair.master,
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
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("pty resize failed: {e}"))
    }

    /// Check whether the shell has exited, returning its exit code if so.
    pub fn try_exit_code(&self) -> Option<u32> {
        let mut child = self.child.lock().ok()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code()),
            _ => None,
        }
    }

    /// Kill the shell process; ConPTY teardown takes the process tree with it.
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}
