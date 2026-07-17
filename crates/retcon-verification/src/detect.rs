//! Project verification command detection.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use crate::{CommandSpec, DetectionSource, GateDefinition, GateKind, GateOverride, ParserKind};

/// Command detection failure.
#[derive(Debug, Error)]
pub enum DetectionError {
    #[error("failed to inspect {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid package.json at {path}: {source}")]
    PackageJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("gate {0:?} has more than one override")]
    DuplicateOverride(GateKind),
}

/// Detect ordered verification gates and apply explicit user overrides.
pub fn detect_gates(
    root: &Path,
    overrides: &[GateOverride],
) -> Result<Vec<GateDefinition>, DetectionError> {
    let root = root.canonicalize().map_err(|source| DetectionError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let mut commands: HashMap<GateKind, Vec<CommandSpec>> = HashMap::new();

    detect_rust(&root, &mut commands);
    detect_dart(&root, &mut commands)?;
    detect_javascript(&root, &mut commands)?;
    detect_python(&root, &mut commands)?;
    detect_repository_checks(&root, &mut commands);

    let mut definitions: Vec<_> = GateKind::ORDERED
        .into_iter()
        .map(|kind| {
            let detected = commands.remove(&kind).unwrap_or_default();
            GateDefinition {
                kind,
                required: !detected.is_empty(),
                skipped_reason: detected
                    .is_empty()
                    .then(|| "No applicable command was detected.".to_owned()),
                commands: detected,
            }
        })
        .collect();
    apply_overrides(&root, &mut definitions, overrides)?;
    Ok(definitions)
}

fn detect_rust(root: &Path, commands: &mut HashMap<GateKind, Vec<CommandSpec>>) {
    let manifest = if root.join("Cargo.toml").is_file() {
        Some(root.join("Cargo.toml"))
    } else {
        find_files(root, "Cargo.toml", 2).into_iter().next()
    };
    let Some(manifest) = manifest else {
        return;
    };
    let cwd = manifest.parent().unwrap_or(root);
    add(
        commands,
        GateKind::Format,
        command(
            cwd,
            DetectionSource::Rust,
            "cargo",
            ["fmt", "--all", "--", "--check"],
        ),
    );
    add(
        commands,
        GateKind::Lint,
        command(
            cwd,
            DetectionSource::Rust,
            "cargo",
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
    );
    add(
        commands,
        GateKind::Typecheck,
        command(
            cwd,
            DetectionSource::Rust,
            "cargo",
            ["check", "--all-targets"],
        ),
    );
    add(
        commands,
        GateKind::Unit,
        CommandSpec::new(
            "cargo",
            ["test", "--workspace", "--lib", "--bins"],
            cwd,
            DetectionSource::Rust,
            ParserKind::CargoTest,
        ),
    );
    if cwd.join("tests").is_dir() {
        add(
            commands,
            GateKind::Integration,
            CommandSpec::new(
                "cargo",
                ["test", "--workspace", "--tests"],
                cwd,
                DetectionSource::Rust,
                ParserKind::CargoTest,
            ),
        );
    }
    add(
        commands,
        GateKind::Build,
        command(
            cwd,
            DetectionSource::Rust,
            "cargo",
            ["build", "--workspace"],
        ),
    );
    if cwd.join("deny.toml").is_file() {
        add(
            commands,
            GateKind::Security,
            command(cwd, DetectionSource::Rust, "cargo", ["deny", "check"]),
        );
    }
    if cwd.join("crates/retcon-secrets/Cargo.toml").is_file() {
        add(
            commands,
            GateKind::Secret,
            command(
                cwd,
                DetectionSource::Rust,
                "cargo",
                [
                    "run",
                    "--quiet",
                    "-p",
                    "retcon-secrets",
                    "--",
                    "scan-staged",
                ],
            ),
        );
    }
}

fn detect_dart(
    root: &Path,
    commands: &mut HashMap<GateKind, Vec<CommandSpec>>,
) -> Result<(), DetectionError> {
    for manifest in find_files(root, "pubspec.yaml", 3) {
        let content = read_to_string(&manifest)?;
        let cwd = manifest.parent().unwrap_or(root);
        let flutter = content.contains("flutter:\n") || content.contains("sdk: flutter");
        add(
            commands,
            GateKind::Format,
            command(
                cwd,
                if flutter {
                    DetectionSource::Flutter
                } else {
                    DetectionSource::Dart
                },
                "dart",
                ["format", "--output=none", "--set-exit-if-changed", "."],
            ),
        );
        add(
            commands,
            GateKind::Lint,
            command(
                cwd,
                if flutter {
                    DetectionSource::Flutter
                } else {
                    DetectionSource::Dart
                },
                if flutter { "flutter" } else { "dart" },
                ["analyze"],
            ),
        );
        let unit = if flutter {
            CommandSpec::new(
                "flutter",
                ["test"],
                cwd,
                DetectionSource::Flutter,
                ParserKind::FlutterTest,
            )
        } else {
            CommandSpec::new(
                "dart",
                ["test"],
                cwd,
                DetectionSource::Dart,
                ParserKind::FlutterTest,
            )
        };
        add(commands, GateKind::Unit, unit);
        if cwd.join("integration_test").is_dir() {
            add(
                commands,
                GateKind::Integration,
                CommandSpec::new(
                    if flutter { "flutter" } else { "dart" },
                    ["test", "integration_test"],
                    cwd,
                    if flutter {
                        DetectionSource::Flutter
                    } else {
                        DetectionSource::Dart
                    },
                    ParserKind::FlutterTest,
                ),
            );
        }
    }
    Ok(())
}

fn detect_javascript(
    root: &Path,
    commands: &mut HashMap<GateKind, Vec<CommandSpec>>,
) -> Result<(), DetectionError> {
    for manifest in find_files(root, "package.json", 3) {
        let content = read_to_string(&manifest)?;
        let package: Value =
            serde_json::from_str(&content).map_err(|source| DetectionError::PackageJson {
                path: manifest.clone(),
                source,
            })?;
        let Some(scripts) = package.get("scripts").and_then(Value::as_object) else {
            continue;
        };
        let cwd = manifest.parent().unwrap_or(root);
        let (program, source) = javascript_runner(cwd);
        for (gate, candidates, parser) in [
            (
                GateKind::Format,
                &["format:check", "format"][..],
                ParserKind::None,
            ),
            (GateKind::Lint, &["lint"][..], ParserKind::None),
            (
                GateKind::Typecheck,
                &["typecheck", "check:types"][..],
                ParserKind::None,
            ),
            (GateKind::Unit, &["test:unit", "test"][..], ParserKind::Auto),
            (
                GateKind::Integration,
                &["test:integration", "integration"][..],
                ParserKind::Auto,
            ),
            (GateKind::Build, &["build"][..], ParserKind::None),
            (
                GateKind::Secret,
                &["scan:secrets", "secrets", "secret:scan"][..],
                ParserKind::None,
            ),
            (
                GateKind::Browser,
                &["test:e2e", "e2e", "playwright"][..],
                ParserKind::Auto,
            ),
            (
                GateKind::Accessibility,
                &["test:a11y", "a11y", "accessibility"][..],
                ParserKind::Auto,
            ),
            (
                GateKind::Security,
                &["test:security", "security", "audit"][..],
                ParserKind::Auto,
            ),
        ] {
            if let Some(script) = candidates.iter().find(|name| scripts.contains_key(**name)) {
                add(
                    commands,
                    gate,
                    CommandSpec::new(program, ["run", *script], cwd, source.clone(), parser),
                );
            }
        }
    }
    Ok(())
}

fn detect_python(
    root: &Path,
    commands: &mut HashMap<GateKind, Vec<CommandSpec>>,
) -> Result<(), DetectionError> {
    let mut projects = find_files(root, "pyproject.toml", 3);
    if root.join("pytest.ini").is_file() && projects.is_empty() {
        projects.push(root.join("pytest.ini"));
    }
    for manifest in projects {
        let cwd = manifest.parent().unwrap_or(root);
        let content = read_to_string(&manifest)?;
        if content.contains("[tool.black]") {
            add(
                commands,
                GateKind::Format,
                command(
                    cwd,
                    DetectionSource::Python,
                    "python",
                    ["-m", "black", "--check", "."],
                ),
            );
        }
        if content.contains("[tool.ruff") {
            add(
                commands,
                GateKind::Lint,
                command(
                    cwd,
                    DetectionSource::Python,
                    "python",
                    ["-m", "ruff", "check", "."],
                ),
            );
        }
        if content.contains("[tool.mypy]") {
            add(
                commands,
                GateKind::Typecheck,
                command(cwd, DetectionSource::Python, "python", ["-m", "mypy", "."]),
            );
        }
        add(
            commands,
            GateKind::Unit,
            CommandSpec::new(
                "python",
                ["-m", "pytest", "-q"],
                cwd,
                DetectionSource::Python,
                ParserKind::Pytest,
            ),
        );
        if cwd.join("tests/integration").is_dir() {
            add(
                commands,
                GateKind::Integration,
                CommandSpec::new(
                    "python",
                    ["-m", "pytest", "-q", "tests/integration"],
                    cwd,
                    DetectionSource::Python,
                    ParserKind::Pytest,
                ),
            );
        }
        if manifest.file_name().and_then(|name| name.to_str()) == Some("pyproject.toml") {
            add(
                commands,
                GateKind::Build,
                command(cwd, DetectionSource::Python, "python", ["-m", "build"]),
            );
        }
        if content.contains("[tool.bandit]") {
            add(
                commands,
                GateKind::Security,
                command(
                    cwd,
                    DetectionSource::Python,
                    "python",
                    ["-m", "bandit", "-r", "."],
                ),
            );
        }
    }
    Ok(())
}

fn detect_repository_checks(root: &Path, commands: &mut HashMap<GateKind, Vec<CommandSpec>>) {
    if root.join(".git").exists() {
        add(
            commands,
            GateKind::GitStatus,
            command(
                root,
                DetectionSource::Git,
                "git",
                ["status", "--porcelain=v1"],
            ),
        );
    }
    if root.join(".gitleaks.toml").is_file() {
        add(
            commands,
            GateKind::Secret,
            command(
                root,
                DetectionSource::Git,
                "gitleaks",
                ["protect", "--staged"],
            ),
        );
    }
}

fn apply_overrides(
    root: &Path,
    definitions: &mut [GateDefinition],
    overrides: &[GateOverride],
) -> Result<(), DetectionError> {
    let mut seen = HashSet::new();
    for gate_override in overrides {
        if !seen.insert(gate_override.gate) {
            return Err(DetectionError::DuplicateOverride(gate_override.gate));
        }
        let Some(definition) = definitions
            .iter_mut()
            .find(|definition| definition.kind == gate_override.gate)
        else {
            continue;
        };
        definition.required = gate_override.required.unwrap_or(definition.required);
        if gate_override.disabled {
            definition.commands.clear();
            definition.required = false;
            definition.skipped_reason = Some("Disabled by user override.".into());
            continue;
        }
        if !gate_override.commands.is_empty() {
            definition.commands = gate_override
                .commands
                .iter()
                .cloned()
                .map(|mut command| {
                    command.source = DetectionSource::UserOverride;
                    if command.cwd.is_relative() {
                        let joined = root.join(&command.cwd);
                        command.cwd = joined.canonicalize().unwrap_or(joined);
                    }
                    command
                })
                .collect();
            definition.required = gate_override.required.unwrap_or(true);
            definition.skipped_reason = None;
        }
    }
    Ok(())
}

fn javascript_runner(cwd: &Path) -> (&'static str, DetectionSource) {
    if cwd.join("bun.lock").is_file() || cwd.join("bun.lockb").is_file() {
        ("bun", DetectionSource::Bun)
    } else if cwd.join("pnpm-lock.yaml").is_file() {
        ("pnpm", DetectionSource::Pnpm)
    } else if cwd.join("yarn.lock").is_file() {
        ("yarn", DetectionSource::Yarn)
    } else {
        ("npm", DetectionSource::Npm)
    }
}

fn command<const N: usize>(
    cwd: &Path,
    source: DetectionSource,
    program: &str,
    args: [&str; N],
) -> CommandSpec {
    CommandSpec::new(program, args, cwd, source, ParserKind::None)
}

fn add(commands: &mut HashMap<GateKind, Vec<CommandSpec>>, gate: GateKind, command: CommandSpec) {
    let existing = commands.entry(gate).or_default();
    if !existing.iter().any(|candidate| candidate == &command) {
        existing.push(command);
    }
}

fn read_to_string(path: &Path) -> Result<String, DetectionError> {
    fs::read_to_string(path).map_err(|source| DetectionError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn find_files(root: &Path, file_name: &str, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    visit(root, file_name, max_depth, 0, &mut found);
    found.sort();
    found
}

fn visit(root: &Path, file_name: &str, max_depth: usize, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
            found.push(path);
        } else if path.is_dir()
            && depth < max_depth
            && !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".git" | "node_modules" | "target" | ".dart_tool" | ".venv" | "build")
            )
        {
            visit(&path, file_name, max_depth, depth + 1, found);
        }
    }
}
