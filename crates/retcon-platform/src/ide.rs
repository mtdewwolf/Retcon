//! Safe, cross-platform IDE detection and process launching.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use thiserror::Error;

/// IDE identifiers accepted by the local protocol.
pub const SUPPORTED_IDE_IDS: [&str; 3] = ["vscode", "cursor", "windsurf"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
/// Detection result for one explicitly supported IDE.
pub struct IdeDetection {
    /// Stable protocol identifier.
    pub id: String,
    /// User-facing product name.
    pub name: String,
    /// Whether a directly executable installation was found.
    pub available: bool,
    /// How the installation was found, without executing it.
    pub evidence: Option<String>,
    /// Resolved executable path when available.
    pub executable: Option<String>,
    /// Safe actions exposed by this IDE's documented CLI.
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An IDE operation whose paths have already been validated by Core.
pub enum IdeAction {
    /// Open a project folder.
    OpenProject(PathBuf),
    /// Open a worktree folder.
    OpenWorktree(PathBuf),
    /// Open a file, optionally at a one-based line and column.
    OpenFile {
        /// Absolute canonical file path.
        path: PathBuf,
        /// Optional one-based line.
        line: Option<u32>,
        /// Optional one-based column.
        column: Option<u32>,
    },
    /// Open a two-file comparison.
    OpenDiff {
        /// Absolute canonical left-hand file.
        left: PathBuf,
        /// Absolute canonical right-hand file.
        right: PathBuf,
    },
    /// Request an integrated terminal rooted at a directory.
    OpenTerminalLocation(PathBuf),
}

impl IdeAction {
    /// Stable action name used in RPC receipts.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::OpenProject(_) => "openProject",
            Self::OpenWorktree(_) => "openWorktree",
            Self::OpenFile { .. } => "openFile",
            Self::OpenDiff { .. } => "openDiff",
            Self::OpenTerminalLocation(_) => "openTerminalLocation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A validated launch request.
pub struct IdeLaunchRequest {
    /// Stable supported IDE identifier.
    pub ide_id: String,
    /// IDE operation.
    pub action: IdeAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Direct process invocation, exposed for deterministic tests.
pub struct LaunchSpec {
    /// Executable to start directly (never through a shell).
    pub program: PathBuf,
    /// Individually encoded arguments.
    pub args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
/// Receipt returned after the IDE process is successfully spawned.
pub struct IdeLaunchReceipt {
    /// Selected IDE identifier.
    pub ide_id: String,
    /// Stable action name.
    pub action: String,
    /// Whether process creation succeeded.
    pub launched: bool,
}

#[derive(Debug, Error)]
/// IDE detection or launch error.
pub enum IdeError {
    /// The caller supplied an identifier outside the supported set.
    #[error("unsupported IDE identifier: {0}")]
    UnsupportedIde(String),
    /// A supported IDE is not installed or its executable cannot be found.
    #[error("IDE executable was not found: {0}")]
    NotFound(String),
    /// The IDE CLI cannot perform the requested action safely.
    #[error("IDE action is unsupported: {ide_id} cannot perform {action}")]
    UnsupportedAction {
        /// IDE identifier.
        ide_id: String,
        /// Stable action name.
        action: String,
    },
    /// Direct process creation failed.
    #[error("failed to launch IDE: {0}")]
    Launch(#[source] std::io::Error),
}

/// Testable IDE integration boundary used by Core.
pub trait IdeIntegration: Send + Sync {
    /// Detect every supported IDE without starting it.
    fn detect(&self) -> Vec<IdeDetection>;

    /// Start a selected IDE through a direct argument vector.
    fn launch(&self, request: &IdeLaunchRequest) -> Result<IdeLaunchReceipt, IdeError>;
}

#[derive(Debug, Default)]
/// Production IDE integration backed by the host filesystem and process API.
pub struct SystemIdeIntegration;

impl IdeIntegration for SystemIdeIntegration {
    fn detect(&self) -> Vec<IdeDetection> {
        SUPPORTED_IDE_IDS.into_iter().map(detect_one).collect()
    }

    fn launch(&self, request: &IdeLaunchRequest) -> Result<IdeLaunchReceipt, IdeError> {
        if !SUPPORTED_IDE_IDS.contains(&request.ide_id.as_str()) {
            return Err(IdeError::UnsupportedIde(request.ide_id.clone()));
        }
        let detected = detect_one(&request.ide_id);
        let program = detected
            .executable
            .map(PathBuf::from)
            .ok_or_else(|| IdeError::NotFound(request.ide_id.clone()))?;
        let spec = build_launch_spec(&request.ide_id, program, &request.action)?;
        Command::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(IdeError::Launch)?;
        Ok(IdeLaunchReceipt {
            ide_id: request.ide_id.clone(),
            action: request.action.as_str().to_owned(),
            launched: true,
        })
    }
}

/// Build a direct process invocation for a supported IDE.
///
/// No shell command string is ever constructed. Each user-controlled path is
/// retained as one operating-system argument.
pub fn build_launch_spec(
    ide_id: &str,
    program: PathBuf,
    action: &IdeAction,
) -> Result<LaunchSpec, IdeError> {
    if !SUPPORTED_IDE_IDS.contains(&ide_id) {
        return Err(IdeError::UnsupportedIde(ide_id.to_owned()));
    }
    let args = match action {
        IdeAction::OpenProject(path) | IdeAction::OpenWorktree(path) => {
            vec![path.as_os_str().to_owned()]
        }
        IdeAction::OpenFile { path, line, column } => {
            if let Some(line) = line {
                let mut target = path.as_os_str().to_owned();
                target.push(format!(":{line}"));
                if let Some(column) = column {
                    target.push(format!(":{column}"));
                }
                vec![OsString::from("--goto"), target]
            } else {
                vec![path.as_os_str().to_owned()]
            }
        }
        IdeAction::OpenDiff { left, right } => vec![
            OsString::from("--diff"),
            left.as_os_str().to_owned(),
            right.as_os_str().to_owned(),
        ],
        IdeAction::OpenTerminalLocation(_) => {
            return Err(IdeError::UnsupportedAction {
                ide_id: ide_id.to_owned(),
                action: action.as_str().to_owned(),
            });
        }
    };
    Ok(LaunchSpec { program, args })
}

fn detect_one(ide_id: &str) -> IdeDetection {
    let (name, commands) = ide_definition(ide_id);
    let (path, source) = find_executable(ide_id, commands)
        .map(|(path, source)| (Some(path), Some(source)))
        .unwrap_or((None, None));
    IdeDetection {
        id: ide_id.to_owned(),
        name: name.to_owned(),
        available: path.is_some(),
        evidence: source,
        executable: path.map(|value| value.to_string_lossy().into_owned()),
        capabilities: ["openProject", "openWorktree", "openFile", "openDiff"]
            .map(str::to_owned)
            .to_vec(),
    }
}

fn ide_definition(ide_id: &str) -> (&'static str, &'static [&'static str]) {
    match ide_id {
        "vscode" => ("Visual Studio Code", &["code", "Code"]),
        "cursor" => ("Cursor", &["cursor", "Cursor"]),
        "windsurf" => ("Windsurf", &["windsurf", "Windsurf"]),
        _ => ("Unsupported IDE", &[]),
    }
}

fn find_executable(ide_id: &str, commands: &[&str]) -> Option<(PathBuf, String)> {
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        // Never let an empty or relative PATH entry turn the Core working
        // directory into an implicit executable search location.
        if !directory.is_absolute() {
            continue;
        }
        for command in commands {
            for candidate in executable_names(command) {
                let path = directory.join(candidate);
                if is_direct_executable(&path) {
                    return Some((path, "path".to_owned()));
                }
            }
        }
    }
    for path in known_install_paths(ide_id) {
        if is_direct_executable(&path) {
            return Some((path, "knownInstallLocation".to_owned()));
        }
    }
    None
}

fn executable_names(command: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        vec![OsString::from(format!("{command}.exe"))]
    }
    #[cfg(not(windows))]
    {
        vec![OsString::from(command)]
    }
}

fn is_direct_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        path.extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    }
}

fn known_install_paths(ide_id: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let programs = std::env::var_os("ProgramFiles").map(PathBuf::from);
        let suffixes: &[&str] = match ide_id {
            "vscode" => &["Programs/Microsoft VS Code/Code.exe"],
            "cursor" => &["Programs/cursor/Cursor.exe", "Programs/Cursor/Cursor.exe"],
            "windsurf" => &["Programs/Windsurf/Windsurf.exe"],
            _ => &[],
        };
        if let Some(local) = local {
            paths.extend(suffixes.iter().map(|suffix| local.join(suffix)));
        }
        if let Some(programs) = programs {
            let machine_suffixes: &[&str] = match ide_id {
                "vscode" => &["Microsoft VS Code/Code.exe"],
                "cursor" => &["Cursor/Cursor.exe"],
                "windsurf" => &["Windsurf/Windsurf.exe"],
                _ => &[],
            };
            paths.extend(machine_suffixes.iter().map(|suffix| programs.join(suffix)));
        }
    }
    #[cfg(target_os = "macos")]
    {
        let path = match ide_id {
            "vscode" => {
                Some("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code")
            }
            "cursor" => Some("/Applications/Cursor.app/Contents/Resources/app/bin/cursor"),
            "windsurf" => Some("/Applications/Windsurf.app/Contents/Resources/app/bin/windsurf"),
            _ => None,
        };
        paths.extend(path.map(PathBuf::from));
    }
    let _ = ide_id;
    paths
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn supported_detection_is_stable_and_complete() {
        let detections = SystemIdeIntegration.detect();
        assert_eq!(detections.len(), SUPPORTED_IDE_IDS.len());
        assert_eq!(
            detections
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            SUPPORTED_IDE_IDS
        );
    }

    #[test]
    fn file_line_and_column_are_one_argument() {
        let spec = build_launch_spec(
            "vscode",
            PathBuf::from("editor.exe"),
            &IdeAction::OpenFile {
                path: PathBuf::from(r"C:\work tree\src\main.rs"),
                line: Some(12),
                column: Some(4),
            },
        )
        .unwrap();
        assert_eq!(spec.args.len(), 2);
        assert_eq!(spec.args[0], "--goto");
        assert_eq!(spec.args[1], r"C:\work tree\src\main.rs:12:4");
    }

    #[test]
    fn project_and_worktree_paths_remain_single_arguments() {
        for action in [
            IdeAction::OpenProject(PathBuf::from("project && whoami")),
            IdeAction::OpenWorktree(PathBuf::from("worktree; calc.exe")),
        ] {
            let spec = build_launch_spec("vscode", PathBuf::from("code"), &action).unwrap();
            assert_eq!(spec.args.len(), 1);
            assert_eq!(spec.args[0], action_path(&action));
        }
    }

    #[test]
    fn diff_paths_remain_distinct_arguments() {
        let spec = build_launch_spec(
            "cursor",
            PathBuf::from("cursor.exe"),
            &IdeAction::OpenDiff {
                left: PathBuf::from("left; calc.exe"),
                right: PathBuf::from("right && whoami"),
            },
        )
        .unwrap();
        assert_eq!(
            spec.args,
            ["--diff", "left; calc.exe", "right && whoami"]
                .map(OsString::from)
                .to_vec()
        );
    }

    #[test]
    fn terminal_action_fails_explicitly_instead_of_approximating() {
        let error = build_launch_spec(
            "windsurf",
            PathBuf::from("windsurf"),
            &IdeAction::OpenTerminalLocation(PathBuf::from("workspace")),
        )
        .unwrap_err();
        assert!(matches!(error, IdeError::UnsupportedAction { .. }));
    }

    #[test]
    fn unknown_ide_is_rejected() {
        let error = build_launch_spec(
            "unknown",
            PathBuf::from("unknown"),
            &IdeAction::OpenProject(PathBuf::from("workspace")),
        )
        .unwrap_err();
        assert!(matches!(error, IdeError::UnsupportedIde(_)));
    }

    fn action_path(action: &IdeAction) -> &std::ffi::OsStr {
        match action {
            IdeAction::OpenProject(path) | IdeAction::OpenWorktree(path) => path.as_os_str(),
            _ => std::ffi::OsStr::new(""),
        }
    }
}
