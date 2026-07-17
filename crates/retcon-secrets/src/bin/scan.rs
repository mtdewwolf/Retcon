//! CLI for secret scanning (Git pre-commit hook and local diagnostics).

use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::{Command, ExitCode};

use retcon_secrets::{ScanResult, scan_text, scan_texts};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("retcon-secrets: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        Some("scan") => scan_paths(&args[1..]),
        Some("scan-staged") => scan_staged(),
        Some("scan-stdin") => scan_stdin(),
        other => Err(format!(
            "unknown command `{}`; expected scan, scan-staged, or scan-stdin",
            other.unwrap_or("<missing>")
        )),
    }
}

fn print_usage() {
    eprintln!(
        "Usage:
  retcon-secrets scan [files...]
  retcon-secrets scan-staged
  retcon-secrets scan-stdin

Scans content for likely secrets using regex and entropy heuristics."
    );
}

fn scan_paths(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return scan_stdin();
    }
    let mut combined = ScanResult::default();
    for path in paths {
        let content = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {path}: {error}"))?;
        combined = combined.merge(scan_text(&content));
        if !combined.is_clean() {
            eprintln!("secret scan failed for {path}: {}", combined.summary());
        }
    }
    if combined.is_clean() {
        Ok(())
    } else {
        Err(format!("secret scan found {}", combined.summary()))
    }
}

fn scan_stdin() -> Result<(), String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    report(scan_text(&buffer), "stdin")
}

fn scan_staged() -> Result<(), String> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--no-color", "--unified=0"])
        .output()
        .map_err(|error| format!("failed to run git diff --cached: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git diff --cached failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let diff = String::from_utf8_lossy(&output.stdout);
    if diff.trim().is_empty() {
        return Ok(());
    }
    let added_lines = diff
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .map(|line| line.trim_start_matches('+'))
        .collect::<Vec<_>>();
    report(scan_texts(added_lines), "staged changes")
}

fn report(result: ScanResult, label: &str) -> Result<(), String> {
    if result.is_clean() {
        Ok(())
    } else {
        eprintln!("secret scan failed for {label}: {}", result.summary());
        Err(format!("secret scan found {}", result.summary()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_paths_rejects_secret_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("secret.txt");
        std::fs::write(&path, "password=not-a-real-secret-for-test").unwrap();
        let error = scan_paths(&[path.display().to_string()]).unwrap_err();
        assert!(error.contains("finding"));
    }

    #[test]
    fn scan_paths_accepts_clean_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clean.txt");
        std::fs::write(&path, "hello world").unwrap();
        assert!(scan_paths(&[path.display().to_string()]).is_ok());
    }
}
