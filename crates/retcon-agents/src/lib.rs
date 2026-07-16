//! Coding-agent provider framework.
//!
//! Phase 2 spike scope: drive one provider (Claude Code) end to end — detect
//! the CLI, read its version, run a prompt with streaming JSON output, and
//! cancel a running turn. The provider-neutral framework (normalized events,
//! capability manifests) is Phase 11.

use std::path::Path;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

static CLAUDE_DETECTION: OnceLock<Mutex<Option<Result<ProviderInfo, String>>>> = OnceLock::new();

/// Information about a detected provider CLI.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    /// Provider identifier (`claude-code` for the spike).
    pub id: String,
    /// Version string reported by the CLI.
    pub version: String,
    /// Whether the CLI reports an authenticated account.
    pub authenticated: bool,
    /// The spike's conservative outdated check (major version zero).
    pub outdated: bool,
}

/// Detect the Claude Code CLI and read its version.
///
/// # Errors
///
/// Returns a message when the CLI is missing or does not respond.
pub async fn detect_claude() -> Result<ProviderInfo, String> {
    let cache = CLAUDE_DETECTION.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock()
        && let Some(cached) = guard.as_ref()
    {
        return cached.clone();
    }
    let detected = detect_claude_uncached().await;
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(detected.clone());
    }
    detected
}

async fn detect_claude_uncached() -> Result<ProviderInfo, String> {
    let output = Command::new("cmd")
        .args(["/C", "claude", "--version"])
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("failed to launch claude CLI: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "claude --version exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if version.is_empty() {
        return Err("claude --version produced no output".to_owned());
    }
    let auth = Command::new("cmd")
        .args(["/C", "claude", "auth", "status"])
        .stdin(Stdio::null())
        .output()
        .await;
    let authenticated = auth.is_ok_and(|result| result.status.success());
    let outdated = version
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .starts_with("0.");
    Ok(ProviderInfo {
        id: "claude-code".to_owned(),
        version,
        authenticated,
        outdated,
    })
}

/// A running provider turn (one prompt being processed).
pub struct AgentTurn {
    child: Child,
    exit_code: Option<i32>,
}

impl AgentTurn {
    /// Start a non-interactive Claude Code turn in `cwd` for `prompt`,
    /// streaming newline-delimited JSON events.
    pub fn start(
        cwd: &Path,
        prompt: &str,
        on_line: impl FnMut(String) + Send + 'static,
        _on_exit: impl FnOnce(Option<i32>) + Send + 'static,
    ) -> Result<Self, String> {
        Self::start_with_session(cwd, prompt, None, on_line)
    }

    /// Start a new turn or resume a provider session by its native ID.
    pub fn start_with_session(
        cwd: &Path,
        prompt: &str,
        resume_session: Option<&str>,
        mut on_line: impl FnMut(String) + Send + 'static,
    ) -> Result<Self, String> {
        let mut args = vec![
            "/C",
            "claude",
            "-p",
            prompt,
            "--output-format",
            "stream-json",
            "--verbose",
        ];
        if let Some(session) = resume_session {
            args.push("--resume");
            args.push(session);
        }
        let mut child = Command::new("cmd")
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("failed to spawn claude: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "no stdout handle".to_owned())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "no stderr handle".to_owned())?;

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "retcon_agents::stderr", "{line}");
            }
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                on_line(line);
            }
        });

        Ok(Self {
            child,
            exit_code: None,
        })
    }

    /// Cancel the running turn by killing the provider process.
    pub async fn cancel(&mut self) {
        let _ = self.child.kill().await;
    }

    /// Poll whether the turn's process has exited, returning its code.
    pub fn try_exit_code(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code();
                self.exit_code = code;
                code
            }
            _ => None,
        }
    }

    /// Wait until the provider process exits.
    pub async fn wait(&mut self) -> Option<i32> {
        if let Some(code) = self.exit_code {
            return Some(code);
        }
        match self.child.wait().await {
            Ok(status) => {
                let code = status.code();
                self.exit_code = code;
                code
            }
            Err(_) => None,
        }
    }

    /// Return the last known exit code after the process has exited.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}
