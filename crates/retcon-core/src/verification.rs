//! Durable adapter for verification command detection and execution.

#![allow(missing_docs)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use retcon_storage::{
    NewVerificationArtifact, NewVerificationCommand, NewVerificationTestResult, Storage,
    VerificationCommand, VerificationGate, VerificationRunDetails,
};
use retcon_verification::{
    CancellationHandle, CommandSpec, DetectionSource, ExecutionOptions, GateDefinition, GateKind,
    GateResult, OutputCapture, ParserKind, VerificationStatus, detect_gates, execute,
};
use serde_json::json;
use uuid::Uuid;

const RUNNER_ACTOR: &str = "verification_runner";
const DEFAULT_OUTPUT_LIMIT: usize = 256 * 1024;

/// Receives durable lifecycle transitions after they have been committed.
pub trait VerificationRunner: Send + Sync {
    fn start(&self, run: &VerificationRunDetails) -> Result<(), String>;
    fn cancel(&self, run: &VerificationRunDetails) -> Result<(), String>;
}

/// Default runner used by tests that intentionally supply no executor.
#[derive(Default)]
pub struct NoopVerificationRunner;

impl VerificationRunner for NoopVerificationRunner {
    fn start(&self, _run: &VerificationRunDetails) -> Result<(), String> {
        Ok(())
    }

    fn cancel(&self, _run: &VerificationRunDetails) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RunnerOptions {
    pub default_timeout: Duration,
    pub max_output_bytes: usize,
    pub stop_on_required_failure: bool,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(15 * 60),
            max_output_bytes: DEFAULT_OUTPUT_LIMIT,
            stop_on_required_failure: true,
        }
    }
}

/// Executes snapshotted verification gates and writes evidence to durable storage.
#[derive(Clone)]
pub struct DurableVerificationRunner {
    storage: Storage,
    active: Arc<Mutex<HashMap<Uuid, CancellationHandle>>>,
    options: RunnerOptions,
}

impl DurableVerificationRunner {
    #[must_use]
    pub fn new(storage: Storage) -> Self {
        Self::with_options(storage, RunnerOptions::default())
    }

    #[must_use]
    pub fn with_options(storage: Storage, options: RunnerOptions) -> Self {
        Self {
            storage,
            active: Arc::new(Mutex::new(HashMap::new())),
            options,
        }
    }

    async fn execute_run(
        &self,
        run: VerificationRunDetails,
        cancellation: CancellationHandle,
    ) -> Result<(), String> {
        let root = project_root(&self.storage, run.run.project_id)?;
        let mut gates = run.gates.clone();
        gates.sort_by_key(gate_order);

        for (index, gate) in gates.iter().enumerate() {
            if cancellation.is_cancelled() {
                let _ = self
                    .storage
                    .database()
                    .verification()
                    .cancel(run.run.id, RUNNER_ACTOR);
                return Ok(());
            }
            let parsed_command = command_from_gate(gate, &root);
            let command = parsed_command
                .as_ref()
                .cloned()
                .unwrap_or_else(|_| fallback_command(gate, &root));
            let timeout = gate
                .timeout_ms
                .and_then(|value| u64::try_from(value).ok())
                .map(Duration::from_millis)
                .unwrap_or(self.options.default_timeout);
            let result = match parsed_command {
                Ok(_) => execute(
                    gate_kind(gate),
                    &command,
                    ExecutionOptions {
                        timeout,
                        max_output_bytes: self.options.max_output_bytes,
                    },
                    Some(&cancellation),
                )
                .await
                .unwrap_or_else(|error| failed_result(gate, command, error.to_string())),
                Err(error) => failed_result(gate, command, error),
            };
            if result.status == VerificationStatus::Cancelled {
                let _ = self
                    .storage
                    .database()
                    .verification()
                    .cancel(run.run.id, RUNNER_ACTOR);
                return Ok(());
            }
            let failed = result.status != VerificationStatus::Passed;
            persist_result(&self.storage, run.run.id, gate, &result)?;
            if failed && gate.required && self.options.stop_on_required_failure {
                for skipped in &gates[index + 1..] {
                    self.storage
                        .database()
                        .verification()
                        .record_gate(
                            run.run.id,
                            skipped.id,
                            "skipped",
                            json!({"reason": "stopped_after_required_failure", "blockedByGateId": gate.id}),
                            &[],
                            &[],
                            RUNNER_ACTOR,
                        )
                        .map_err(|error| error.to_string())?;
                }
                break;
            }
        }

        self.storage
            .database()
            .verification()
            .finish(run.run.id, RUNNER_ACTOR)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl VerificationRunner for DurableVerificationRunner {
    fn start(&self, run: &VerificationRunDetails) -> Result<(), String> {
        if run.run.status != "running" {
            return Err("verification runner requires a running durable run".into());
        }
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|error| format!("verification runner needs a Tokio runtime: {error}"))?;
        let cancellation = CancellationHandle::new();
        {
            let mut active = self
                .active
                .lock()
                .map_err(|_| "verification runner state is poisoned".to_owned())?;
            if active.insert(run.run.id, cancellation.clone()).is_some() {
                return Err("verification run is already active".into());
            }
        }
        let runner = self.clone();
        let run = run.clone();
        runtime.spawn(async move {
            let run_id = run.run.id;
            if let Err(error) = runner.execute_run(run, cancellation).await {
                tracing::error!(%run_id, %error, "verification run failed internally");
                let _ = runner
                    .storage
                    .database()
                    .verification()
                    .cancel(run_id, "runner_error");
            }
            if let Ok(mut active) = runner.active.lock() {
                active.remove(&run_id);
            }
        });
        Ok(())
    }

    fn cancel(&self, run: &VerificationRunDetails) -> Result<(), String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "verification runner state is poisoned".to_owned())?;
        if let Some(cancellation) = active.get(&run.run.id) {
            cancellation.cancel();
        }
        Ok(())
    }
}

/// Return stored project commands, installing detected defaults only when none exist.
pub fn ensure_default_commands(
    storage: &Storage,
    project_id: Uuid,
) -> Result<Vec<VerificationCommand>, String> {
    let existing = storage
        .database()
        .verification()
        .commands(project_id)
        .map_err(|error| error.to_string())?;
    if !existing.is_empty() {
        return Ok(existing);
    }
    let root = project_root(storage, project_id)?;
    let gates = detect_gates(&root, &[]).map_err(|error| error.to_string())?;
    let defaults = detected_commands(&gates);
    if defaults.is_empty() {
        return Err("no verification commands were detected for this project".into());
    }
    storage
        .database()
        .verification()
        .replace_commands(project_id, &defaults)
        .map_err(|error| error.to_string())
}

fn detected_commands(gates: &[GateDefinition]) -> Vec<NewVerificationCommand> {
    let mut detected = Vec::new();
    for (gate_index, gate) in gates.iter().enumerate() {
        for (command_index, command) in gate.commands.iter().enumerate() {
            detected.push(NewVerificationCommand {
                id: Uuid::new_v4(),
                key: format!(
                    "{gate_index:02}-{}-{}-{command_index:02}",
                    gate_label(gate.kind),
                    source_label(&command.source)
                ),
                kind: storage_gate_kind(gate.kind).into(),
                command: render_command(command),
                cwd: Some(command.cwd.to_string_lossy().into_owned()),
                required: gate.required,
                enabled: true,
                timeout_ms: None,
            });
        }
    }
    detected
}

fn persist_result(
    storage: &Storage,
    run_id: Uuid,
    gate: &VerificationGate,
    result: &GateResult,
) -> Result<(), String> {
    let status = status_label(result.status);
    let mut tests: Vec<_> = result
        .tests
        .iter()
        .map(|test| {
            let location = test.locations.first();
            NewVerificationTestResult {
                id: Uuid::new_v4(),
                suite: test.suite.clone(),
                name: test.name.clone(),
                status: status_label(test.status).into(),
                duration_ms: test.duration_ms.and_then(|value| i64::try_from(value).ok()),
                file_path: location.map(|value| value.file.to_string_lossy().into_owned()),
                line: location.and_then(|value| value.line).map(i64::from),
                message: location.and_then(|value| value.message.clone()),
                metadata: json!({"sourceId": test.source_id, "locations": test.locations}),
            }
        })
        .collect();
    if tests.is_empty() {
        tests.push(NewVerificationTestResult {
            id: Uuid::new_v4(),
            suite: None,
            name: gate.key.clone(),
            status: status.into(),
            duration_ms: Some(i64::try_from(result.duration_ms).unwrap_or(i64::MAX)),
            file_path: None,
            line: None,
            message: (result.status != VerificationStatus::Passed)
                .then(|| result.stderr.text.clone())
                .filter(|message| !message.is_empty()),
            metadata: json!({"commandLevel": true}),
        });
    }
    let mut artifacts = Vec::new();
    for (kind, output) in [("stdout", &result.stdout), ("stderr", &result.stderr)] {
        if output.total_bytes == 0 && output.text.is_empty() {
            continue;
        }
        let stored = storage
            .artifacts()
            .store_bytes(output.text.as_bytes())
            .map_err(|error| error.to_string())?;
        artifacts.push(NewVerificationArtifact {
            id: Uuid::new_v4(),
            kind: kind.into(),
            hash: stored.hash,
            size_bytes: i64::try_from(stored.size).unwrap_or(i64::MAX),
            metadata: json!({
                "truncated": output.truncated,
                "originalBytes": output.total_bytes,
                "retainedBytes": output.text.len(),
            }),
        });
    }
    storage
        .database()
        .verification()
        .record_gate(
            run_id,
            gate.id,
            status,
            json!({
                "exitCode": result.exit_code,
                "durationMs": result.duration_ms,
                "testCount": tests.len(),
                "stdoutTruncated": result.stdout.truncated,
                "stderrTruncated": result.stderr.truncated,
            }),
            &tests,
            &artifacts,
            RUNNER_ACTOR,
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn command_from_gate(gate: &VerificationGate, root: &Path) -> Result<CommandSpec, String> {
    let mut parts = shell_words::split(&gate.command)
        .map_err(|error| format!("invalid command '{}': {error}", gate.command))?;
    if parts.is_empty() {
        return Err("verification command is empty".into());
    }
    let program = parts.remove(0);
    let cwd = gate
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.to_path_buf());
    let parser = infer_parser(&program, &parts);
    Ok(CommandSpec::new(
        program,
        parts,
        cwd,
        DetectionSource::UserOverride,
        parser,
    ))
}

fn fallback_command(gate: &VerificationGate, root: &Path) -> CommandSpec {
    CommandSpec::new(
        gate.command.clone(),
        std::iter::empty::<String>(),
        root,
        DetectionSource::UserOverride,
        ParserKind::None,
    )
}

fn failed_result(gate: &VerificationGate, command: CommandSpec, error: String) -> GateResult {
    GateResult {
        gate: gate_kind(gate),
        command,
        status: VerificationStatus::Failed,
        exit_code: None,
        duration_ms: 0,
        stdout: OutputCapture::default(),
        stderr: OutputCapture {
            total_bytes: u64::try_from(error.len()).unwrap_or(u64::MAX),
            text: error,
            truncated: false,
        },
        tests: Vec::new(),
    }
}

fn project_root(storage: &Storage, project_id: Uuid) -> Result<PathBuf, String> {
    storage
        .database()
        .projects()
        .primary_location_path(project_id)
        .map_err(|error| error.to_string())?
        .map(PathBuf::from)
        .ok_or_else(|| "project has no repository location for verification".into())
}

fn infer_parser(program: &str, args: &[String]) -> ParserKind {
    let executable = Path::new(program)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    let joined = args.join(" ").to_ascii_lowercase();
    if executable == "cargo" && args.first().is_some_and(|value| value == "nextest") {
        ParserKind::CargoNextest
    } else if executable == "cargo" && args.iter().any(|value| value == "test") {
        ParserKind::CargoTest
    } else if matches!(executable.as_str(), "flutter" | "dart")
        && args.iter().any(|value| value == "test")
    {
        ParserKind::FlutterTest
    } else if joined.contains("pytest") {
        ParserKind::Pytest
    } else if joined.contains("junit") {
        ParserKind::Junit
    } else if joined.contains("jest") && joined.contains("json") {
        ParserKind::JestJson
    } else if joined.contains("vitest") && joined.contains("json") {
        ParserKind::VitestJson
    } else {
        ParserKind::Auto
    }
}

fn render_command(command: &CommandSpec) -> String {
    std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .map(quote_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-./:\\=".contains(character))
    {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn status_label(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Passed => "passed",
        VerificationStatus::Failed => "failed",
        VerificationStatus::Skipped => "skipped",
        VerificationStatus::Cancelled | VerificationStatus::TimedOut => "error",
    }
}

fn gate_order(gate: &VerificationGate) -> (usize, String) {
    let index = GateKind::ORDERED
        .iter()
        .position(|kind| *kind == gate_kind(gate))
        .unwrap_or(usize::MAX);
    (index, gate.key.clone())
}

fn gate_kind(gate: &VerificationGate) -> GateKind {
    for kind in GateKind::ORDERED {
        if gate.key.contains(gate_label(kind)) {
            return kind;
        }
    }
    match gate.kind.as_str() {
        "build" => GateKind::Build,
        "browser" => GateKind::Browser,
        "accessibility" => GateKind::Accessibility,
        "security" => GateKind::Security,
        "git" => GateKind::GitStatus,
        _ => GateKind::Unit,
    }
}

fn storage_gate_kind(kind: GateKind) -> &'static str {
    match kind {
        GateKind::Build => "build",
        GateKind::Browser => "browser",
        GateKind::Accessibility => "accessibility",
        GateKind::Security | GateKind::Secret => "security",
        GateKind::GitStatus => "git",
        GateKind::Format
        | GateKind::Lint
        | GateKind::Typecheck
        | GateKind::Unit
        | GateKind::Integration => "test",
    }
}

fn gate_label(kind: GateKind) -> &'static str {
    match kind {
        GateKind::Format => "format",
        GateKind::Lint => "lint",
        GateKind::Typecheck => "typecheck",
        GateKind::Unit => "unit",
        GateKind::Integration => "integration",
        GateKind::Build => "build",
        GateKind::GitStatus => "git-status",
        GateKind::Secret => "secret",
        GateKind::Browser => "browser",
        GateKind::Accessibility => "accessibility",
        GateKind::Security => "security",
    }
}

fn source_label(source: &DetectionSource) -> &'static str {
    match source {
        DetectionSource::Rust => "rust",
        DetectionSource::Flutter => "flutter",
        DetectionSource::Dart => "dart",
        DetectionSource::Bun => "bun",
        DetectionSource::Npm => "npm",
        DetectionSource::Pnpm => "pnpm",
        DetectionSource::Yarn => "yarn",
        DetectionSource::Python => "python",
        DetectionSource::Git => "git",
        DetectionSource::UserOverride => "override",
    }
}
