#![allow(clippy::expect_used, clippy::unwrap_used, missing_docs)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use retcon_verification::{
    CancellationHandle, CommandSpec, DetectionSource, ExecutionOptions, GateKind, GateOverride,
    ParserKind, VerificationStatus, build_rerun_failed, detect_gates, execute, parse_test_output,
};

#[test]
fn detects_mixed_ecosystems_in_stable_gate_order() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write(root.join("Cargo.toml"), "[workspace]\nmembers=[]\n");
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("apps/web")).unwrap();
    write(root.join("apps/web/bun.lock"), "");
    write(
        root.join("apps/web/package.json"),
        r#"{"scripts":{"lint":"biome check .","typecheck":"tsc --noEmit","test":"vitest","build":"vite build","test:e2e":"playwright test","test:a11y":"axe"}}"#,
    );
    std::fs::create_dir_all(root.join("apps/desktop/integration_test")).unwrap();
    write(
        root.join("apps/desktop/pubspec.yaml"),
        "name: demo\ndependencies:\n  flutter:\n    sdk: flutter\nflutter:\n  uses-material-design: true\n",
    );
    std::fs::create_dir_all(root.join("services/api/tests/integration")).unwrap();
    write(
        root.join("services/api/pyproject.toml"),
        "[build-system]\nrequires=[]\n[tool.black]\n[tool.ruff]\n[tool.mypy]\n[tool.bandit]\n",
    );

    let gates = detect_gates(root, &[]).unwrap();

    assert_eq!(
        gates.iter().map(|gate| gate.kind).collect::<Vec<_>>(),
        GateKind::ORDERED
    );
    assert!(gate(&gates, GateKind::Format).commands.len() >= 3);
    assert!(gate(&gates, GateKind::Unit).commands.len() >= 3);
    assert_eq!(
        gate(&gates, GateKind::Browser).commands[0].source,
        DetectionSource::Bun
    );
    assert!(gate(&gates, GateKind::Accessibility).required);
    assert!(gate(&gates, GateKind::GitStatus).required);
    assert!(gate(&gates, GateKind::Security).required);
}

#[test]
fn overrides_replace_or_disable_detected_gates() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path().join("Cargo.toml"),
        "[workspace]\nmembers=[]\n",
    );
    let custom = CommandSpec::new(
        "custom-lint",
        ["--strict"],
        PathBuf::from("."),
        DetectionSource::Rust,
        ParserKind::None,
    );
    let overrides = vec![
        GateOverride {
            gate: GateKind::Lint,
            disabled: false,
            required: Some(true),
            commands: vec![custom],
        },
        GateOverride {
            gate: GateKind::Build,
            disabled: true,
            required: None,
            commands: Vec::new(),
        },
    ];

    let gates = detect_gates(directory.path(), &overrides).unwrap();

    let lint = gate(&gates, GateKind::Lint);
    assert_eq!(lint.commands.len(), 1);
    assert_eq!(lint.commands[0].source, DetectionSource::UserOverride);
    assert!(
        lint.commands[0]
            .cwd
            .starts_with(directory.path().canonicalize().unwrap())
    );
    let build = gate(&gates, GateKind::Build);
    assert!(!build.required);
    assert!(build.commands.is_empty());
    assert_eq!(
        build.skipped_reason.as_deref(),
        Some("Disabled by user override.")
    );
}

#[test]
fn parses_cargo_and_nextest_fixtures() {
    let cargo = parse_test_output(
        ParserKind::CargoTest,
        include_str!("fixtures/cargo-test.txt"),
        "",
    )
    .unwrap();
    assert_eq!(cargo.len(), 3);
    assert_eq!(cargo[0].status, VerificationStatus::Passed);
    assert_eq!(cargo[1].status, VerificationStatus::Skipped);
    assert_eq!(cargo[2].status, VerificationStatus::Failed);
    assert_eq!(cargo[2].locations[0].line, Some(42));

    let nextest = parse_test_output(
        ParserKind::CargoNextest,
        include_str!("fixtures/nextest.txt"),
        "",
    )
    .unwrap();
    assert_eq!(nextest.len(), 3);
    assert_eq!(nextest[2].duration_ms, Some(1250));
    assert_eq!(nextest[2].locations[0].column, Some(5));
}

#[test]
fn parses_flutter_jest_pytest_and_junit_fixtures() {
    let flutter = parse_test_output(
        ParserKind::FlutterTest,
        include_str!("fixtures/flutter-test.txt"),
        "",
    )
    .unwrap();
    assert_eq!(flutter.len(), 3);
    assert_eq!(flutter[1].status, VerificationStatus::Failed);
    assert_eq!(flutter[1].locations[0].line, Some(27));

    let jest =
        parse_test_output(ParserKind::JestJson, include_str!("fixtures/jest.json"), "").unwrap();
    assert_eq!(jest.len(), 2);
    assert_eq!(jest[1].status, VerificationStatus::Failed);
    assert_eq!(jest[1].duration_ms, Some(11));
    assert_eq!(jest[1].locations[0].line, Some(12));

    let pytest =
        parse_test_output(ParserKind::Pytest, include_str!("fixtures/pytest.txt"), "").unwrap();
    assert_eq!(pytest.len(), 3);
    assert_eq!(
        pytest[2].source_id.as_deref(),
        Some("tests/test_math.py::test_divide")
    );
    assert_eq!(pytest[2].locations[0].line, Some(19));

    let junit =
        parse_test_output(ParserKind::Junit, include_str!("fixtures/junit.xml"), "").unwrap();
    assert_eq!(junit.len(), 3);
    assert_eq!(junit[0].status, VerificationStatus::Passed);
    assert_eq!(junit[1].status, VerificationStatus::Skipped);
    assert_eq!(junit[2].status, VerificationStatus::Failed);
    assert_eq!(junit[2].duration_ms, Some(1250));
    assert_eq!(
        junit[2].locations[0].message.as_deref(),
        Some("expected 2 & got 3")
    );
}

#[test]
fn builds_framework_specific_failed_test_reruns() {
    let tests =
        parse_test_output(ParserKind::Pytest, include_str!("fixtures/pytest.txt"), "").unwrap();
    let base = CommandSpec::new(
        "python",
        ["-m", "pytest", "-q"],
        ".",
        DetectionSource::Python,
        ParserKind::Pytest,
    );

    let reruns = build_rerun_failed(ParserKind::Pytest, &base, &tests);

    assert_eq!(reruns.len(), 1);
    assert_eq!(
        reruns[0].args.last().map(String::as_str),
        Some("tests/test_math.py::test_divide")
    );
}

#[tokio::test]
async fn execution_bounds_output_and_normalizes_exit_status() {
    let directory = tempfile::tempdir().unwrap();
    let command = output_command(directory.path(), 4096);

    let result = execute(
        GateKind::Unit,
        &command,
        ExecutionOptions {
            timeout: Duration::from_secs(5),
            max_output_bytes: 128,
        },
        None,
    )
    .await
    .unwrap();

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(result.stdout.text.len(), 128);
    assert!(result.stdout.truncated);
    assert!(result.stdout.total_bytes >= 4096);
}

#[tokio::test]
async fn execution_supports_timeout_and_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let command = sleep_command(directory.path());
    let timed_out = execute(
        GateKind::Unit,
        &command,
        ExecutionOptions {
            timeout: Duration::from_millis(50),
            max_output_bytes: 1024,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(timed_out.status, VerificationStatus::TimedOut);

    let cancellation = CancellationHandle::new();
    let cancel_from_task = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_from_task.cancel();
    });
    let cancelled = execute(
        GateKind::Unit,
        &command,
        ExecutionOptions {
            timeout: Duration::from_secs(5),
            max_output_bytes: 1024,
        },
        Some(&cancellation),
    )
    .await
    .unwrap();
    assert_eq!(cancelled.status, VerificationStatus::Cancelled);
}

fn gate(
    gates: &[retcon_verification::GateDefinition],
    kind: GateKind,
) -> &retcon_verification::GateDefinition {
    gates.iter().find(|gate| gate.kind == kind).unwrap()
}

fn write(path: PathBuf, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[cfg(windows)]
fn output_command(cwd: &Path, bytes: usize) -> CommandSpec {
    CommandSpec::new(
        "powershell",
        [
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            format!("[Console]::Out.Write([string]::new([char]'x', {bytes}))"),
        ],
        cwd,
        DetectionSource::UserOverride,
        ParserKind::None,
    )
}

#[cfg(not(windows))]
fn output_command(cwd: &Path, bytes: usize) -> CommandSpec {
    CommandSpec::new(
        "sh",
        [
            "-c".to_owned(),
            format!("head -c {bytes} /dev/zero | tr '\\0' x"),
        ],
        cwd,
        DetectionSource::UserOverride,
        ParserKind::None,
    )
}

#[cfg(windows)]
fn sleep_command(cwd: &Path) -> CommandSpec {
    CommandSpec::new(
        "powershell",
        ["-NoProfile", "-Command", "Start-Sleep -Seconds 5"],
        cwd,
        DetectionSource::UserOverride,
        ParserKind::None,
    )
}

#[cfg(not(windows))]
fn sleep_command(cwd: &Path) -> CommandSpec {
    CommandSpec::new(
        "sh",
        ["-c", "sleep 5"],
        cwd,
        DetectionSource::UserOverride,
        ParserKind::None,
    )
}
