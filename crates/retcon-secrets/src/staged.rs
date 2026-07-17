//! Scanning for secrets introduced by staged Git changes.

use std::path::Path;
use std::process::Command;

use thiserror::Error;

use crate::{ScanResult, scan_texts};

/// Failure to obtain the staged Git diff for secret scanning.
#[derive(Debug, Error)]
pub enum StagedScanError {
    #[error("failed to run git diff --cached: {0}")]
    Launch(#[source] std::io::Error),
    #[error("git diff --cached failed: {stderr}")]
    Git { stderr: String },
}

/// Scan only lines added by the staged Git diff in `repo`.
///
/// Removed lines and diff metadata are excluded so deleting an existing secret
/// remains possible and sensitive-looking filenames do not create findings.
pub fn scan_staged(repo: &Path) -> Result<ScanResult, StagedScanError> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--no-color", "--unified=0"])
        .current_dir(repo)
        .output()
        .map_err(StagedScanError::Launch)?;
    if !output.status.success() {
        return Err(StagedScanError::Git {
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let diff = String::from_utf8_lossy(&output.stdout);
    let added_lines = diff
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .map(|line| &line[1..]);
    Ok(scan_texts(added_lines))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repository() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        git(directory.path(), &["init"]);
        git(
            directory.path(),
            &["config", "user.email", "retcon@example.invalid"],
        );
        git(directory.path(), &["config", "user.name", "Retcon Test"]);
        directory
    }

    #[test]
    fn detects_secret_added_to_staged_diff() {
        let directory = repository();
        std::fs::write(
            directory.path().join("config.txt"),
            "API_KEY=not-a-real-secret-for-testing",
        )
        .unwrap();
        git(directory.path(), &["add", "config.txt"]);

        let result = scan_staged(directory.path()).unwrap();

        assert!(!result.is_clean());
        assert!(result.summary().contains("assignment"));
        assert!(!result.summary().contains("not-a-real-secret-for-testing"));
    }

    #[test]
    fn removing_existing_secret_is_allowed() {
        let directory = repository();
        let path = directory.path().join("config.txt");
        std::fs::write(&path, "password=not-a-real-secret-for-testing\nkeep=true\n").unwrap();
        git(directory.path(), &["add", "config.txt"]);
        git(directory.path(), &["commit", "-m", "fixture"]);
        std::fs::write(&path, "keep=true\n").unwrap();
        git(directory.path(), &["add", "config.txt"]);

        assert!(scan_staged(directory.path()).unwrap().is_clean());
    }

    #[test]
    fn rejects_non_repository_without_leaking_diff_output() {
        let directory = tempfile::tempdir().unwrap();

        let error = scan_staged(directory.path()).unwrap_err();

        assert!(matches!(error, StagedScanError::Git { .. }));
        assert!(!error.to_string().contains("API_KEY="));
    }
}
