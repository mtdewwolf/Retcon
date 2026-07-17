//! Serializable verification domain types shared with the core service.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable verification gate order used by plans and reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    Format,
    Lint,
    Typecheck,
    Unit,
    Integration,
    Build,
    GitStatus,
    Secret,
    Browser,
    Accessibility,
    Security,
}

impl GateKind {
    /// All gate kinds in execution order.
    pub const ORDERED: [Self; 11] = [
        Self::Format,
        Self::Lint,
        Self::Typecheck,
        Self::Unit,
        Self::Integration,
        Self::Build,
        Self::GitStatus,
        Self::Secret,
        Self::Browser,
        Self::Accessibility,
        Self::Security,
    ];
}

/// Why a verification command appears in a gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    Rust,
    Flutter,
    Dart,
    Bun,
    Npm,
    Pnpm,
    Yarn,
    Python,
    Git,
    UserOverride,
}

/// A shell-free process invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: PathBuf,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub source: DetectionSource,
    pub parser: ParserKind,
}

impl CommandSpec {
    /// Construct a detected command with no environment overrides.
    #[must_use]
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        cwd: impl Into<PathBuf>,
        source: DetectionSource,
        parser: ParserKind,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
            source,
            parser,
        }
    }
}

/// Parser used to normalize a command's test output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserKind {
    None,
    Auto,
    CargoTest,
    CargoNextest,
    FlutterTest,
    JestJson,
    VitestJson,
    Pytest,
    Junit,
}

/// One ordered gate and its commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateDefinition {
    pub kind: GateKind,
    pub required: bool,
    pub commands: Vec<CommandSpec>,
    pub skipped_reason: Option<String>,
}

/// Explicit replacement or disablement supplied by a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateOverride {
    pub gate: GateKind,
    #[serde(default)]
    pub disabled: bool,
    pub required: Option<bool>,
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
}

/// Normalized verification state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Passed,
    Failed,
    Skipped,
    Cancelled,
    TimedOut,
}

/// Captured output with explicit truncation metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputCapture {
    pub text: String,
    pub truncated: bool,
    pub total_bytes: u64,
}

/// Source location extracted from a test failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureLocation {
    pub file: PathBuf,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: Option<String>,
}

/// One normalized test case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCaseResult {
    pub name: String,
    pub suite: Option<String>,
    pub status: VerificationStatus,
    pub duration_ms: Option<u64>,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub locations: Vec<FailureLocation>,
    pub source_id: Option<String>,
}

/// Complete result for one command in a verification gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub gate: GateKind,
    pub command: CommandSpec,
    pub status: VerificationStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout: OutputCapture,
    pub stderr: OutputCapture,
    #[serde(default)]
    pub tests: Vec<TestCaseResult>,
}
