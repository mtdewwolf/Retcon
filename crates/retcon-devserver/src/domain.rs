//! Serializable development-server domain types.

#![allow(missing_docs)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Framework associated with a detected start command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    NextJs,
    Vite,
    ReactScripts,
    Flutter,
    Rust,
    Django,
    Node,
    Custom,
}

/// Durable project command shape consumed from Phase 22 configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCommand {
    pub key: String,
    pub kind: String,
    pub command: String,
    pub cwd: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A validated shell-free process invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerCommand {
    pub framework: Framework,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: PathBuf,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl DevServerCommand {
    /// Create a command without environment overrides.
    #[must_use]
    pub fn new(
        framework: Framework,
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        cwd: impl Into<PathBuf>,
    ) -> Self {
        Self {
            framework,
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
        }
    }
}

/// Runtime settings for one start attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOptions {
    pub project_key: String,
    pub worktree_key: String,
    pub requested_port: Option<u16>,
    pub allow_alternate_port: bool,
    pub startup_timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for StartOptions {
    fn default() -> Self {
        Self {
            project_key: String::new(),
            worktree_key: String::new(),
            requested_port: None,
            allow_alternate_port: true,
            startup_timeout: Duration::from_secs(60),
            max_output_bytes: 256 * 1024,
        }
    }
}

/// Output stream for a bounded live log event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// Confirmed local listening endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyInfo {
    pub port: u16,
    pub url: String,
    pub detected_from_output: bool,
}

/// Successful start response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub run_id: u64,
    pub framework: Framework,
    pub ready: ReadyInfo,
}

/// Bounded lifecycle and live-log events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DevServerEvent {
    Starting {
        run_id: u64,
        framework: Framework,
        port: u16,
    },
    Log {
        run_id: u64,
        stream: LogStream,
        text: String,
        retained_bytes: u64,
        total_bytes: u64,
    },
    OutputTruncated {
        run_id: u64,
        stream: LogStream,
        limit_bytes: usize,
    },
    Ready {
        run_id: u64,
        info: ReadyInfo,
    },
    HotReload {
        run_id: u64,
        message: String,
    },
    StartupFailed {
        run_id: u64,
        message: String,
    },
    Crashed {
        run_id: u64,
        exit_code: Option<i32>,
    },
    Exited {
        run_id: u64,
        exit_code: Option<i32>,
    },
    Stopped {
        run_id: u64,
    },
}

const fn default_true() -> bool {
    true
}
