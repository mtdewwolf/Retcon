//! Coding-agent provider framework.
//!
//! Phase 11 scope: provider-neutral normalized events, capability manifests, and
//! the Claude Code adapter used by the session engine.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

static CLAUDE_DETECTION: OnceLock<Mutex<Option<Result<ProviderInfo, String>>>> = OnceLock::new();
fn runtime_metrics_enabled() -> bool {
    retcon_runtime_observability::is_enabled()
}

/// Stable identifier for an agent provider.
pub type ProviderId = String;

/// A capability exposed by an agent provider.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    /// Read files in the workspace.
    FileAccess,
    /// Create or modify workspace files.
    FileEditing,
    /// Execute shell commands.
    ShellExecution,
    /// Control a browser.
    BrowserUse,
    /// Control the local computer.
    ComputerUse,
    /// Use Model Context Protocol servers.
    Mcp,
    /// Accept images as prompt input.
    ImageInput,
    /// Work in an explicit planning mode.
    PlanMode,
    /// Queue messages while a turn is running.
    QueueMode,
    /// Resume a native provider session.
    SessionResume,
    /// Delegate work to subagents.
    Subagents,
    /// Select a model for a session.
    ModelSelection,
    /// Report monetary usage.
    CostReporting,
    /// Report token usage.
    TokenReporting,
    /// Request approval using the provider's native mechanism.
    NativeApprovals,
}

/// A provider's declared support for Retcon features.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityManifest {
    /// Provider identifier this manifest belongs to.
    pub provider_id: ProviderId,
    /// Capabilities the adapter implements.
    pub supported: Vec<ProviderCapability>,
}

impl CapabilityManifest {
    /// Whether a capability is supported by this provider.
    #[must_use]
    pub fn supports(&self, capability: ProviderCapability) -> bool {
        self.supported.contains(&capability)
    }
}

/// Metadata and capabilities used to present a provider before a session starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    /// Stable provider identifier.
    pub id: ProviderId,
    /// Human-readable name.
    pub display_name: String,
    /// CLI executable name.
    pub executable: String,
    /// Provider capability declaration.
    pub capabilities: CapabilityManifest,
}

/// A provider failure in a UI-safe, provider-neutral form.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ProviderError {
    /// The provider executable is absent or could not be launched.
    #[error("provider unavailable: {detail}")]
    Unavailable {
        /// Safe diagnostic information about the unavailable provider.
        detail: String,
    },
    /// The provider is not authenticated.
    #[error("provider authentication required: {detail}")]
    AuthenticationRequired {
        /// Safe diagnostic information for reconnecting the account.
        detail: String,
    },
    /// A requested feature is not implemented by the provider.
    #[error("provider capability unsupported: {capability:?}")]
    UnsupportedCapability {
        /// Feature requested from the provider.
        capability: ProviderCapability,
    },
    /// The provider failed while processing a request.
    #[error("provider failed: {detail}")]
    Failed {
        /// Safe diagnostic information from the provider adapter.
        detail: String,
    },
}

/// The lifecycle state of a normalized agent event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventKind {
    /// A provider session was started.
    SessionStarted,
    /// A provider session was resumed.
    SessionResumed,
    /// A turn started running.
    TurnStarted,
    /// Incremental assistant text.
    TextDelta,
    /// The provider's reasoning status changed.
    ReasoningStatus,
    /// A tool needs permission or input before it can run.
    ToolRequested,
    /// A tool began running.
    ToolStarted,
    /// A tool emitted output.
    ToolOutput,
    /// A tool completed.
    ToolCompleted,
    /// The provider requested an approval.
    ApprovalRequested,
    /// A workspace file changed.
    FileChanged,
    /// A command began running.
    CommandStarted,
    /// A command completed.
    CommandCompleted,
    /// Usage information changed.
    UsageUpdated,
    /// A turn completed normally.
    TurnCompleted,
    /// A turn was cancelled.
    TurnCancelled,
    /// A provider session completed.
    SessionCompleted,
    /// The provider failed or disconnected.
    ProviderFailed,
}

/// A provider-neutral event emitted while a session runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    /// Provider that emitted the event.
    pub provider_id: ProviderId,
    /// Native provider session ID when supplied by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_session_id: Option<String>,
    /// Normalized event category.
    pub kind: AgentEventKind,
    /// Provider-specific event data preserved for detailed rendering.
    pub data: serde_json::Value,
}

/// Input required to start or resume a provider turn.
#[derive(Debug, Clone)]
pub struct StartTurnRequest {
    /// Working directory visible to the provider.
    pub cwd: PathBuf,
    /// User message for the provider.
    pub prompt: String,
    /// Native session to resume, when supported.
    pub resume_session: Option<String>,
}

/// Common contract each provider adapter exposes to Retcon.
pub trait AgentProvider {
    /// Return static provider metadata and declared capabilities.
    fn metadata(&self) -> ProviderMetadata;

    /// Run safe, non-billing provider health checks.
    fn doctor<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ProviderDoctorReport> + Send + 'a>>;
}

/// Claude Code implementation of the common provider contract.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeCodeProvider;

impl ClaudeCodeProvider {
    /// Provider identifier used by storage and RPC clients.
    pub const ID: &'static str = "claude-code";

    /// Start a Claude Code turn using the provider-neutral request shape.
    pub fn start_turn(
        &self,
        request: &StartTurnRequest,
        on_line: impl FnMut(String) + Send + 'static,
    ) -> Result<AgentTurn, ProviderError> {
        AgentTurn::start_with_session(
            &request.cwd,
            &request.prompt,
            request.resume_session.as_deref(),
            on_line,
        )
        .map_err(|detail| ProviderError::Failed { detail })
    }
}

impl AgentProvider for ClaudeCodeProvider {
    fn metadata(&self) -> ProviderMetadata {
        let supported = vec![
            ProviderCapability::FileAccess,
            ProviderCapability::FileEditing,
            ProviderCapability::ShellExecution,
            ProviderCapability::Mcp,
            ProviderCapability::ImageInput,
            ProviderCapability::PlanMode,
            ProviderCapability::SessionResume,
            ProviderCapability::Subagents,
            ProviderCapability::ModelSelection,
            ProviderCapability::TokenReporting,
            ProviderCapability::NativeApprovals,
        ];
        ProviderMetadata {
            id: Self::ID.to_owned(),
            display_name: "Claude Code".to_owned(),
            executable: "claude".to_owned(),
            capabilities: CapabilityManifest {
                provider_id: Self::ID.to_owned(),
                supported,
            },
        }
    }

    fn doctor<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ProviderDoctorReport> + Send + 'a>>
    {
        Box::pin(doctor_claude())
    }
}

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

/// Severity of an individual provider health check.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// The check completed successfully.
    Ready,
    /// The check found a non-blocking condition needing attention.
    Warning,
    /// The check found a condition that prevents provider use.
    Failure,
}

/// A user-facing repair action the shell can execute locally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RepairAction {
    /// Open provider documentation in the default browser.
    OpenDocs {
        /// Link target.
        url: String,
        /// Button or menu label.
        label: String,
    },
    /// Reveal a configuration directory in the file manager.
    RevealConfig {
        /// Directory to open.
        path: String,
        /// Button or menu label.
        label: String,
    },
    /// Show PATH entries and install-location hints.
    PathHints {
        /// Entries to display in the shell.
        hints: Vec<PathHint>,
        /// Button or menu label.
        label: String,
    },
    /// Re-run one or all provider doctor checks.
    Retry {
        /// Specific check to retry, or `None` for the full report.
        #[serde(skip_serializing_if = "Option::is_none")]
        check_id: Option<String>,
        /// Button or menu label.
        label: String,
    },
}

/// One PATH or install-location hint for provider setup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathHint {
    /// Human-readable description.
    pub label: String,
    /// Path or PATH entry.
    pub path: String,
    /// Whether the path currently exists or resolves.
    pub present: bool,
}

/// One diagnostic performed by the provider doctor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Stable identifier for the check.
    pub id: String,
    /// Human-readable check name.
    pub label: String,
    /// Result severity.
    pub status: HealthStatus,
    /// Safe diagnostic detail.
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// A user-facing next step, when available.
    pub suggested_action: Option<String>,
    /// Repair actions the shell can offer for this check.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repair_actions: Vec<RepairAction>,
}

/// A complete, safe-to-share setup report for the supported provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDoctorReport {
    /// Stable provider identifier.
    pub provider_id: String,
    /// Human-readable provider name.
    pub provider_name: String,
    /// Highest severity across all checks.
    pub overall_status: HealthStatus,
    /// Resolved executable path.
    pub executable_path: Option<String>,
    /// Version returned by the executable.
    pub version: Option<String>,
    /// Lowest version Retcon supports.
    pub minimum_supported_version: String,
    /// Whether the provider reports a signed-in account.
    pub authenticated: bool,
    /// Existing provider configuration directory.
    pub configuration_path: Option<String>,
    /// Provider documentation URL for setup help.
    pub documentation_url: String,
    /// PATH and install-location hints for the provider executable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_hints: Vec<PathHint>,
    /// Report-level repair actions the shell can offer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repair_actions: Vec<RepairAction>,
    /// Individual health-check results.
    pub checks: Vec<HealthCheck>,
}

impl ProviderDoctorReport {
    /// Build a support-safe diagnostic bundle suitable for export or clipboard copy.
    #[must_use]
    pub fn diagnostic_bundle(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": "provider_doctor_bundle",
            "generated_at": diagnostic_timestamp(),
            "report": self,
        })
    }
}

fn diagnostic_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "unknown".to_owned())
}

const MINIMUM_CLAUDE_VERSION: &str = "1.0.0";
const CLAUDE_DOCS_URL: &str = "https://docs.anthropic.com/en/docs/claude-code/overview";

/// Clear cached provider detection so the next doctor run re-probes the CLI.
pub fn clear_detection_cache() {
    if let Some(cache) = CLAUDE_DETECTION.get()
        && let Ok(mut guard) = cache.lock()
    {
        *guard = None;
    }
}

/// Run non-invasive setup checks for Claude Code.
///
/// The doctor never starts an agent session or sends a prompt, so it does not
/// spend provider credits or expose project content.
pub async fn doctor_claude() -> ProviderDoctorReport {
    let executable_path = find_executable("claude").await;
    let configuration_path = claude_configuration_path();
    let path_hints = collect_path_hints(&executable_path);
    let mut checks = Vec::new();
    let report_repair_actions = vec![
        RepairAction::OpenDocs {
            url: CLAUDE_DOCS_URL.to_owned(),
            label: "Open Claude Code docs".to_owned(),
        },
        RepairAction::Retry {
            check_id: None,
            label: "Retry all checks".to_owned(),
        },
    ];

    let Some(path) = executable_path.clone() else {
        checks.push(check_with_actions(
            "executable",
            "Claude Code installation",
            HealthStatus::Failure,
            "The `claude` command was not found on PATH.",
            Some("Install Claude Code, or repair PATH and then retry this check.".to_owned()),
            vec![
                RepairAction::OpenDocs {
                    url: CLAUDE_DOCS_URL.to_owned(),
                    label: "Open install docs".to_owned(),
                },
                RepairAction::PathHints {
                    hints: path_hints.clone(),
                    label: "Show PATH hints".to_owned(),
                },
                RepairAction::Retry {
                    check_id: Some("executable".to_owned()),
                    label: "Retry installation check".to_owned(),
                },
            ],
        ));
        checks.push(check_with_actions(
            "authentication",
            "Account authentication",
            HealthStatus::Failure,
            "Authentication cannot be checked until Claude Code is installed.",
            Some("Install Claude Code first.".to_owned()),
            vec![RepairAction::OpenDocs {
                url: CLAUDE_DOCS_URL.to_owned(),
                label: "Open install docs".to_owned(),
            }],
        ));
        return ProviderDoctorReport {
            provider_id: "claude-code".to_owned(),
            provider_name: "Claude Code".to_owned(),
            overall_status: HealthStatus::Failure,
            executable_path: None,
            version: None,
            minimum_supported_version: MINIMUM_CLAUDE_VERSION.to_owned(),
            authenticated: false,
            configuration_path,
            documentation_url: CLAUDE_DOCS_URL.to_owned(),
            path_hints,
            repair_actions: report_repair_actions,
            checks,
        };
    };

    let version_result = run_claude(&["--version"]).await;
    let version = version_result
        .as_ref()
        .ok()
        .map(|output| output.trim().to_owned())
        .filter(|output| !output.is_empty());
    match &version {
        Some(version) if version_is_supported(version) => checks.push(check_with_actions(
            "version",
            "Supported version",
            HealthStatus::Ready,
            format!("Claude Code {version} meets the minimum supported version."),
            None,
            vec![],
        )),
        Some(version) => checks.push(check_with_actions(
            "version",
            "Supported version",
            HealthStatus::Warning,
            format!("Claude Code {version} is older than {MINIMUM_CLAUDE_VERSION}."),
            Some("Update Claude Code, then retry this check.".to_owned()),
            vec![
                RepairAction::OpenDocs {
                    url: CLAUDE_DOCS_URL.to_owned(),
                    label: "Open update docs".to_owned(),
                },
                RepairAction::Retry {
                    check_id: Some("version".to_owned()),
                    label: "Retry version check".to_owned(),
                },
            ],
        )),
        None => checks.push(check_with_actions(
            "version",
            "Executable response",
            HealthStatus::Failure,
            version_result
                .err()
                .unwrap_or_else(|| "Claude Code returned no version.".to_owned()),
            Some("Reinstall Claude Code or select another executable.".to_owned()),
            vec![
                RepairAction::OpenDocs {
                    url: CLAUDE_DOCS_URL.to_owned(),
                    label: "Open install docs".to_owned(),
                },
                RepairAction::Retry {
                    check_id: Some("version".to_owned()),
                    label: "Retry version check".to_owned(),
                },
            ],
        )),
    }

    checks.push(check_with_actions(
        "executable",
        "Claude Code installation",
        HealthStatus::Ready,
        format!("Found executable at {path}."),
        None,
        vec![RepairAction::PathHints {
            hints: path_hints.clone(),
            label: "Show PATH hints".to_owned(),
        }],
    ));

    let authenticated = run_claude(&["auth", "status"])
        .await
        .is_ok_and(|output| !output.trim().is_empty());
    checks.push(check_with_actions(
        "authentication",
        "Account authentication",
        if authenticated {
            HealthStatus::Ready
        } else {
            HealthStatus::Failure
        },
        if authenticated {
            "Claude Code reports an active authenticated account.".to_owned()
        } else {
            "Claude Code did not report an authenticated account.".to_owned()
        },
        (!authenticated).then_some("Run `claude auth login` to reconnect your account.".to_owned()),
        if authenticated {
            vec![]
        } else {
            vec![
                RepairAction::OpenDocs {
                    url: CLAUDE_DOCS_URL.to_owned(),
                    label: "Open auth docs".to_owned(),
                },
                RepairAction::Retry {
                    check_id: Some("authentication".to_owned()),
                    label: "Retry auth check".to_owned(),
                },
            ]
        },
    ));

    let config_status = if configuration_path.is_some() {
        HealthStatus::Ready
    } else {
        HealthStatus::Warning
    };
    let mut config_actions = vec![RepairAction::OpenDocs {
        url: CLAUDE_DOCS_URL.to_owned(),
        label: "Open setup docs".to_owned(),
    }];
    if let Some(config_path) = &configuration_path {
        config_actions.insert(
            0,
            RepairAction::RevealConfig {
                path: config_path.clone(),
                label: "Reveal config folder".to_owned(),
            },
        );
    }
    checks.push(check_with_actions(
        "configuration",
        "Configuration location",
        config_status,
        configuration_path.clone().map_or_else(
            || "No Claude configuration directory was found yet.".to_owned(),
            |path| format!("Configuration directory: {path}"),
        ),
        configuration_path
            .is_none()
            .then_some("Sign in to Claude Code to create its configuration.".to_owned()),
        config_actions,
    ));
    checks.push(check_with_actions(
        "network_and_model_access",
        "Network and model access",
        HealthStatus::Warning,
        "Not tested: this check would need to start a provider session and could consume usage."
            .to_owned(),
        Some(
            "Start a session when ready; Retcon will report any connection or model-access error."
                .to_owned(),
        ),
        vec![RepairAction::OpenDocs {
            url: CLAUDE_DOCS_URL.to_owned(),
            label: "Open troubleshooting docs".to_owned(),
        }],
    ));

    let overall_status = checks
        .iter()
        .map(|check| check.status)
        .max_by_key(status_rank)
        .unwrap_or(HealthStatus::Failure);
    let mut repair_actions = report_repair_actions;
    if let Some(config_path) = configuration_path.clone() {
        repair_actions.insert(
            1,
            RepairAction::RevealConfig {
                path: config_path,
                label: "Reveal config folder".to_owned(),
            },
        );
    }
    repair_actions.push(RepairAction::PathHints {
        hints: path_hints.clone(),
        label: "Show PATH hints".to_owned(),
    });
    ProviderDoctorReport {
        provider_id: "claude-code".to_owned(),
        provider_name: "Claude Code".to_owned(),
        overall_status,
        executable_path: Some(path),
        version,
        minimum_supported_version: MINIMUM_CLAUDE_VERSION.to_owned(),
        authenticated,
        configuration_path,
        documentation_url: CLAUDE_DOCS_URL.to_owned(),
        path_hints,
        repair_actions,
        checks,
    }
}

fn check_with_actions(
    id: &str,
    label: &str,
    status: HealthStatus,
    detail: impl Into<String>,
    suggested_action: Option<String>,
    repair_actions: Vec<RepairAction>,
) -> HealthCheck {
    HealthCheck {
        id: id.to_owned(),
        label: label.to_owned(),
        status,
        detail: detail.into(),
        suggested_action,
        repair_actions,
    }
}

fn collect_path_hints(executable_path: &Option<String>) -> Vec<PathHint> {
    let mut hints = Vec::new();
    if let Some(path) = executable_path {
        hints.push(PathHint {
            label: "Resolved Claude executable".to_owned(),
            path: path.clone(),
            present: Path::new(path).exists(),
        });
    } else {
        hints.push(PathHint {
            label: "Resolved Claude executable".to_owned(),
            path: "Not found on PATH".to_owned(),
            present: false,
        });
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for entry in path_var.split(';').take(8) {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            hints.push(PathHint {
                label: "PATH entry".to_owned(),
                path: trimmed.to_owned(),
                present: Path::new(trimmed).is_dir(),
            });
        }
    }

    if let Some(home) = std::env::var_os("USERPROFILE") {
        for candidate in [
            Path::new(&home).join(".local/bin/claude.exe"),
            Path::new(&home).join("AppData/Roaming/npm/claude.cmd"),
            Path::new(&home).join("AppData/Local/Programs/claude/claude.exe"),
        ] {
            hints.push(PathHint {
                label: "Common install location".to_owned(),
                path: candidate.display().to_string(),
                present: candidate.exists(),
            });
        }
    }

    hints
}

fn status_rank(status: &HealthStatus) -> u8 {
    match status {
        HealthStatus::Ready => 0,
        HealthStatus::Warning => 1,
        HealthStatus::Failure => 2,
    }
}

fn version_is_supported(version: &str) -> bool {
    let found = version
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()));
    let parse = |value: &str| {
        value
            .split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    match found {
        Some(value) => parse(value) >= parse(MINIMUM_CLAUDE_VERSION),
        None => false,
    }
}

async fn find_executable(name: &str) -> Option<String> {
    let output = Command::new("cmd")
        .args(["/C", "where", name])
        .stdin(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .filter(|path| !path.is_empty())
}

async fn run_claude(args: &[&str]) -> Result<String, String> {
    let output = Command::new("cmd")
        .args(["/C", "claude"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| format!("Could not launch Claude Code: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn claude_configuration_path() -> Option<String> {
    let home = std::env::var_os("USERPROFILE")?;
    let path = Path::new(&home).join(".claude");
    path.is_dir().then(|| path.display().to_string())
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
    let report = doctor_claude().await;
    let version = report
        .version
        .ok_or_else(|| "Claude Code was not found or did not return a version.".to_owned())?;
    let outdated = !version_is_supported(&version);
    Ok(ProviderInfo {
        id: "claude-code".to_owned(),
        version,
        authenticated: report.authenticated,
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
        let span = runtime_metrics_enabled().then(|| {
            tracing::info_span!(
                target: "retcon_runtime",
                "provider.turn",
                component = "agents",
                operation = "provider_start",
                provider = "claude_code",
                resumed = resume_session.is_some()
            )
        });
        let _entered = span.as_ref().map(tracing::Span::enter);
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
        retcon_runtime_observability::record_count(
            "agents",
            "provider.lifecycle.count",
            "start",
            "ok",
        );
        if runtime_metrics_enabled() {
            tracing::info!(
                target: "retcon_runtime",
                event = "provider_start.completed",
                component = "agents",
                operation = "provider_start",
                provider = "claude_code",
                outcome = "ok"
            );
        }

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
                tracing::debug!(
                    target: "retcon_agents::stderr",
                    bytes = line.len(),
                    "provider emitted stderr"
                );
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

/// Map a Claude Code `stream-json` line to a provider-neutral [`AgentEvent`].
#[must_use]
pub fn normalize_claude_stream_event(provider_id: &str, line: &str) -> AgentEvent {
    let data: serde_json::Value =
        serde_json::from_str(line).unwrap_or_else(|_| serde_json::json!({ "raw": line }));
    let source_type = data.get("type").and_then(serde_json::Value::as_str);
    let kind = match source_type {
        Some("system") => AgentEventKind::SessionStarted,
        Some("assistant") | Some("content_block_delta") | Some("content_block_start") => {
            AgentEventKind::TextDelta
        }
        Some("user") => AgentEventKind::TurnStarted,
        Some("result") => AgentEventKind::TurnCompleted,
        Some("error") => AgentEventKind::ProviderFailed,
        Some("tool_use") | Some("tool_use_block") => AgentEventKind::ToolRequested,
        Some("tool_result") => AgentEventKind::ToolCompleted,
        Some("command") => AgentEventKind::CommandStarted,
        _ => AgentEventKind::ReasoningStatus,
    };
    let telemetry_event = match (source_type, kind) {
        (Some("assistant" | "content_block_start"), AgentEventKind::TextDelta) => {
            Some("model_response")
        }
        (_, AgentEventKind::ToolRequested) => Some("tool_execution_started"),
        (_, AgentEventKind::ToolCompleted) => Some("tool_execution_completed"),
        (_, AgentEventKind::TurnCompleted) => Some("model_turn_completed"),
        (_, AgentEventKind::ProviderFailed) => Some("model_turn_failed"),
        _ => None,
    };
    if runtime_metrics_enabled()
        && let Some(event) = telemetry_event
    {
        let (name, operation, outcome) = match event {
            "model_response" => ("model.response.count", "response", "ok"),
            "tool_execution_started" => ("tool.execution.count", "start", "pending"),
            "tool_execution_completed" => ("tool.execution.count", "complete", "ok"),
            "model_turn_completed" => ("model.response.count", "complete", "ok"),
            _ => ("model.response.count", "complete", "error"),
        };
        retcon_runtime_observability::record_count("agents", name, operation, outcome);
        tracing::info!(
            target: "retcon_runtime",
            event,
            component = "agents",
            provider = "claude_code"
        );
    }
    AgentEvent {
        provider_id: provider_id.to_owned(),
        native_session_id: data
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        kind,
        data,
    }
}

#[cfg(test)]
mod doctor_tests {
    use super::{
        AgentProvider, ClaudeCodeProvider, HealthStatus, ProviderCapability, RepairAction,
        collect_path_hints, version_is_supported,
    };

    #[test]
    fn version_check_handles_prefixes_and_old_versions() {
        assert!(version_is_supported("Claude Code 1.2.3"));
        assert!(version_is_supported("1.0.0"));
        assert!(!version_is_supported("0.9.9"));
        assert!(!version_is_supported("unknown"));
    }

    #[test]
    fn normalize_claude_stream_event_maps_assistant_lines() {
        let event = super::normalize_claude_stream_event(
            "claude-code",
            r#"{"type":"assistant","message":{"content":"hi"}}"#,
        );
        assert_eq!(event.provider_id, "claude-code");
        assert_eq!(event.kind, super::AgentEventKind::TextDelta);
    }

    #[test]
    fn claude_manifest_exposes_only_supported_capabilities() {
        let manifest = ClaudeCodeProvider.metadata().capabilities;
        assert!(manifest.supports(ProviderCapability::SessionResume));
        assert!(manifest.supports(ProviderCapability::TokenReporting));
        assert!(!manifest.supports(ProviderCapability::BrowserUse));
        assert!(!manifest.supports(ProviderCapability::CostReporting));
    }

    #[test]
    fn path_hints_mark_missing_executable() {
        let hints = collect_path_hints(&None);
        assert!(hints.iter().any(|hint| !hint.present));
    }

    #[test]
    fn diagnostic_bundle_includes_report_metadata() {
        let report = super::ProviderDoctorReport {
            provider_id: "claude-code".to_owned(),
            provider_name: "Claude Code".to_owned(),
            overall_status: HealthStatus::Warning,
            executable_path: None,
            version: None,
            minimum_supported_version: "1.0.0".to_owned(),
            authenticated: false,
            configuration_path: None,
            documentation_url: "https://example.com".to_owned(),
            path_hints: Vec::new(),
            repair_actions: vec![RepairAction::Retry {
                check_id: None,
                label: "Retry all checks".to_owned(),
            }],
            checks: Vec::new(),
        };
        let bundle = report.diagnostic_bundle();
        assert_eq!(bundle["kind"], "provider_doctor_bundle");
        assert_eq!(bundle["report"]["provider_id"], "claude-code");
        assert!(bundle["generated_at"].is_string());
    }

    #[tokio::test]
    #[ignore = "requires authenticated Claude Code; run with RETCON_AGENT_E2E=1"]
    async fn authenticated_claude_turn_can_be_cancelled() -> Result<(), String> {
        assert_eq!(std::env::var("RETCON_AGENT_E2E").as_deref(), Ok("1"));
        let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
        let mut turn = super::AgentTurn::start(
            &cwd,
            "Use Bash to run powershell -NoProfile -Command Start-Sleep -Seconds 30, then reply RETCON_CANCEL_TOO_LATE. Do not modify files.",
            |_| {},
            |_| {},
        )?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        assert!(
            turn.try_exit_code().is_none(),
            "turn exited before cancellation"
        );
        turn.cancel().await;
        let code = tokio::time::timeout(std::time::Duration::from_secs(10), turn.wait())
            .await
            .map_err(|_| "cancelled turn did not exit promptly".to_owned())?;
        assert!(code.is_some(), "cancelled turn should report an exit code");
        Ok(())
    }
}
