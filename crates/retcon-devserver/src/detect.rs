//! Framework and start-command detection.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{DevServerCommand, DevServerError, Framework, ProjectCommand};

/// Detect a development-server command, preferring enabled project overrides.
pub fn detect_start_command(
    root: &Path,
    project_commands: &[ProjectCommand],
) -> Result<DevServerCommand, DevServerError> {
    let root = validate_root(root)?;
    let mut candidates = Vec::new();
    for configured in project_commands.iter().filter(|command| command.enabled) {
        if let Some(candidate) = command_candidate(&root, configured) {
            candidates.push(candidate?);
        }
    }
    if let Some(command) = candidates
        .into_iter()
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, command)| command)
    {
        return Ok(command);
    }

    if root.join("package.json").is_file() {
        return detect_node(&root);
    }
    if root.join("pubspec.yaml").is_file() {
        return Ok(DevServerCommand::new(
            Framework::Flutter,
            "flutter",
            [
                "run",
                "-d",
                "web-server",
                "--web-hostname",
                "127.0.0.1",
                "--web-port",
                "{port}",
            ],
            root,
        ));
    }
    if root.join("manage.py").is_file() {
        return Ok(DevServerCommand::new(
            Framework::Django,
            python_program(),
            ["manage.py", "runserver", "127.0.0.1:{port}"],
            root,
        ));
    }
    if root.join("Cargo.toml").is_file() {
        return Ok(DevServerCommand::new(
            Framework::Rust,
            "cargo",
            ["run"],
            root,
        ));
    }
    Err(DevServerError::Detection(
        "no supported development-server command was detected".into(),
    ))
}

fn command_candidate(
    root: &Path,
    configured: &ProjectCommand,
) -> Option<Result<(usize, DevServerCommand), DevServerError>> {
    let identity = format!("{} {}", configured.key, configured.kind).to_ascii_lowercase();
    let command = configured.command.to_ascii_lowercase();
    let priority = if has_word(&identity, "dev") {
        0
    } else if has_word(&identity, "start") {
        1
    } else if has_word(&identity, "serve") || has_word(&identity, "server") {
        2
    } else if command.contains("test") || command.contains("check") || command.contains("lint") {
        return None;
    } else if has_word(&command, "dev") {
        0
    } else if has_word(&command, "start") {
        1
    } else if has_word(&command, "serve") || has_word(&command, "server") {
        2
    } else if configured.kind == "custom" && has_word(&command, "run") {
        3
    } else {
        return None;
    };
    Some(parse_project_command(root, configured).map(|command| (priority, command)))
}

fn parse_project_command(
    root: &Path,
    configured: &ProjectCommand,
) -> Result<DevServerCommand, DevServerError> {
    let mut words = shell_words::split(&configured.command).map_err(|error| {
        DevServerError::Detection(format!(
            "invalid project start command '{}': {error}",
            configured.command
        ))
    })?;
    if words.is_empty() {
        return Err(DevServerError::Detection(
            "project start command is empty".into(),
        ));
    }
    let program = words.remove(0);
    let cwd = configured
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .map(|cwd| {
            if cwd.is_absolute() {
                cwd
            } else {
                root.join(cwd)
            }
        })
        .unwrap_or_else(|| root.to_path_buf());
    Ok(DevServerCommand::new(
        framework_from_text(&configured.command, root),
        program,
        words,
        cwd,
    ))
}

fn detect_node(root: &Path) -> Result<DevServerCommand, DevServerError> {
    let raw = fs::read_to_string(root.join("package.json")).map_err(|error| {
        DevServerError::Detection(format!("failed to read package.json: {error}"))
    })?;
    let package: Value = serde_json::from_str(&raw).map_err(|error| {
        DevServerError::Detection(format!("failed to parse package.json: {error}"))
    })?;
    let scripts = package
        .get("scripts")
        .and_then(Value::as_object)
        .ok_or_else(|| DevServerError::Detection("package.json has no scripts object".into()))?;
    let script = ["dev", "start", "serve"]
        .into_iter()
        .find(|name| scripts.get(*name).and_then(Value::as_str).is_some())
        .ok_or_else(|| {
            DevServerError::Detection("package.json has no dev, start, or serve script".into())
        })?;
    let manager = package_manager(root);
    let mut args = vec!["run".to_owned(), script.to_owned()];
    let framework = framework_from_package(&package);
    if matches!(framework, Framework::NextJs | Framework::Vite) {
        args.extend(["--".into(), "--port".into(), "{port}".into()]);
    }
    let mut command = DevServerCommand::new(framework, manager, args, root);
    command.env = BTreeMap::new();
    Ok(command)
}

fn package_manager(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if root.join("yarn.lock").is_file() {
        "yarn"
    } else if root.join("bun.lockb").is_file() || root.join("bun.lock").is_file() {
        "bun"
    } else {
        "npm"
    }
}

fn framework_from_package(package: &Value) -> Framework {
    let dependencies = ["dependencies", "devDependencies"]
        .into_iter()
        .filter_map(|key| package.get(key).and_then(Value::as_object));
    let names: Vec<_> = dependencies.flat_map(|values| values.keys()).collect();
    if names.iter().any(|name| name.as_str() == "next") {
        Framework::NextJs
    } else if names.iter().any(|name| name.as_str() == "vite") {
        Framework::Vite
    } else if names.iter().any(|name| name.as_str() == "react-scripts") {
        Framework::ReactScripts
    } else {
        Framework::Node
    }
}

fn framework_from_text(command: &str, root: &Path) -> Framework {
    let command = command.to_ascii_lowercase();
    if command.contains("flutter") {
        Framework::Flutter
    } else if command.contains("next") || root.join("next.config.js").is_file() {
        Framework::NextJs
    } else if command.contains("vite") || root.join("vite.config.ts").is_file() {
        Framework::Vite
    } else if command.contains("react-scripts") {
        Framework::ReactScripts
    } else if command.contains("manage.py") {
        Framework::Django
    } else if command.contains("cargo run") {
        Framework::Rust
    } else {
        Framework::Custom
    }
}

fn validate_root(root: &Path) -> Result<PathBuf, DevServerError> {
    if !root.is_dir() {
        return Err(DevServerError::InvalidCommand(format!(
            "project root is not a directory: {}",
            root.display()
        )));
    }
    root.canonicalize().map_err(|error| {
        DevServerError::InvalidCommand(format!(
            "failed to resolve project root '{}': {error}",
            root.display()
        ))
    })
}

fn has_word(text: &str, needle: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word == needle)
}

const fn python_program() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn project_start_command_precedes_framework_default() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("package.json"),
            r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"latest"}}"#,
        )
        .unwrap();
        let command = detect_start_command(
            directory.path(),
            &[ProjectCommand {
                key: "dev-server".into(),
                kind: "custom".into(),
                command: "custom-runner serve --port {port}".into(),
                cwd: None,
                enabled: true,
            }],
        )
        .unwrap();
        assert_eq!(command.framework, Framework::Custom);
        assert_eq!(command.program, "custom-runner");
        assert_eq!(command.args, ["serve", "--port", "{port}"]);
    }

    #[test]
    fn detects_vite_with_lockfile_and_port_forwarding() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("package.json"),
            r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"latest"}}"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: 9",
        )
        .unwrap();
        let command = detect_start_command(directory.path(), &[]).unwrap();
        assert_eq!(command.framework, Framework::Vite);
        assert_eq!(command.program, "pnpm");
        assert_eq!(command.args.last().map(String::as_str), Some("{port}"));
    }
}
