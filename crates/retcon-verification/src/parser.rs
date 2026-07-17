//! Test output parsers that normalize common runner formats.

#![allow(clippy::expect_used)] // Static regular expressions are validated once at startup.

use std::path::PathBuf;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;
use thiserror::Error;

use crate::{FailureLocation, ParserKind, TestCaseResult, VerificationStatus};

/// Test output parsing failure.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid Jest/Vitest JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Parse stdout and stderr into normalized test cases.
pub fn parse_test_output(
    parser: ParserKind,
    stdout: &str,
    stderr: &str,
) -> Result<Vec<TestCaseResult>, ParseError> {
    let parser = if parser == ParserKind::Auto {
        infer_parser(stdout, stderr)
    } else {
        parser
    };
    let output = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    match parser {
        ParserKind::None | ParserKind::Auto => Ok(Vec::new()),
        ParserKind::CargoTest => Ok(parse_cargo_test(output)),
        ParserKind::CargoNextest => Ok(parse_nextest(output)),
        ParserKind::FlutterTest => Ok(parse_flutter(output)),
        ParserKind::JestJson | ParserKind::VitestJson => parse_jest_json(output),
        ParserKind::Pytest => Ok(parse_pytest(output)),
        ParserKind::Junit => Ok(parse_junit(output)),
    }
}

fn infer_parser(stdout: &str, stderr: &str) -> ParserKind {
    let output = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    let trimmed = output.trim_start();
    if trimmed.starts_with('{') && trimmed.contains("testResults") {
        ParserKind::JestJson
    } else if output.contains(" PASSED") || output.contains(" FAILED") {
        if output
            .lines()
            .any(|line| line.trim_start().starts_with("PASS ["))
        {
            ParserKind::CargoNextest
        } else if output.lines().any(|line| line.contains("::")) {
            ParserKind::Pytest
        } else {
            ParserKind::CargoTest
        }
    } else if output
        .lines()
        .any(|line| line.len() > 6 && line.as_bytes().get(2) == Some(&b':') && line.contains(" +"))
    {
        ParserKind::FlutterTest
    } else {
        ParserKind::None
    }
}

fn parse_cargo_test(output: &str) -> Vec<TestCaseResult> {
    static TEST: OnceLock<Regex> = OnceLock::new();
    let regex = TEST.get_or_init(|| {
        Regex::new(r"(?m)^test (?P<name>.+) \.\.\. (?P<status>ok|FAILED|ignored)$")
            .expect("valid cargo test regex")
    });
    let locations = failure_locations(output);
    regex
        .captures_iter(output)
        .map(|capture| {
            let status = match &capture["status"] {
                "ok" => VerificationStatus::Passed,
                "ignored" => VerificationStatus::Skipped,
                _ => VerificationStatus::Failed,
            };
            let name = capture["name"].to_owned();
            TestCaseResult {
                source_id: Some(name.clone()),
                name,
                suite: None,
                status,
                duration_ms: None,
                stdout: String::new(),
                stderr: String::new(),
                locations: if status == VerificationStatus::Failed {
                    locations.clone()
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

fn parse_nextest(output: &str) -> Vec<TestCaseResult> {
    static TEST: OnceLock<Regex> = OnceLock::new();
    let regex = TEST.get_or_init(|| {
        Regex::new(
            r"(?m)^\s*(?P<status>PASS|FAIL|SKIP)\s+\[\s*(?P<duration>[0-9.]+)s\]\s+(?P<name>.+)$",
        )
        .expect("valid nextest regex")
    });
    let locations = failure_locations(output);
    regex
        .captures_iter(output)
        .map(|capture| {
            let status = match &capture["status"] {
                "PASS" => VerificationStatus::Passed,
                "SKIP" => VerificationStatus::Skipped,
                _ => VerificationStatus::Failed,
            };
            let name = capture["name"].trim().to_owned();
            TestCaseResult {
                source_id: Some(name.clone()),
                name,
                suite: None,
                status,
                duration_ms: seconds_to_millis(&capture["duration"]),
                stdout: String::new(),
                stderr: String::new(),
                locations: if status == VerificationStatus::Failed {
                    locations.clone()
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

fn parse_flutter(output: &str) -> Vec<TestCaseResult> {
    static TEST: OnceLock<Regex> = OnceLock::new();
    let regex = TEST.get_or_init(|| {
        Regex::new(
            r"(?m)^\d\d:\d\d\s+\+\d+(?:\s+-\d+)?(?:\s+~\d+)?:\s+(?P<name>.+?)(?:\s+\[(?P<mark>E|SKIP)\])?$",
        )
        .expect("valid Flutter test regex")
    });
    let locations = failure_locations(output);
    regex
        .captures_iter(output)
        .filter_map(|capture| {
            let name = capture["name"].trim();
            if name.starts_with("loading ") || name == "All tests passed!" {
                return None;
            }
            let status = match capture.name("mark").map(|mark| mark.as_str()) {
                Some("E") => VerificationStatus::Failed,
                Some("SKIP") => VerificationStatus::Skipped,
                _ => VerificationStatus::Passed,
            };
            Some(TestCaseResult {
                source_id: Some(name.to_owned()),
                name: name.to_owned(),
                suite: None,
                status,
                duration_ms: None,
                stdout: String::new(),
                stderr: String::new(),
                locations: if status == VerificationStatus::Failed {
                    locations.clone()
                } else {
                    Vec::new()
                },
            })
        })
        .collect()
}

fn parse_jest_json(output: &str) -> Result<Vec<TestCaseResult>, ParseError> {
    let root: Value = serde_json::from_str(output)?;
    let mut tests = Vec::new();
    for suite in root
        .get("testResults")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let file = suite.get("name").and_then(Value::as_str).map(PathBuf::from);
        let assertions = suite
            .get("assertionResults")
            .or_else(|| suite.get("testResults"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for assertion in assertions {
            let name = assertion
                .get("fullName")
                .or_else(|| assertion.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("unnamed test")
                .to_owned();
            let status = match assertion.get("status").and_then(Value::as_str) {
                Some("passed" | "pass") => VerificationStatus::Passed,
                Some("pending" | "skipped" | "todo") => VerificationStatus::Skipped,
                _ => VerificationStatus::Failed,
            };
            let failure_message = assertion
                .get("failureMessages")
                .and_then(Value::as_array)
                .and_then(|messages| messages.first())
                .and_then(Value::as_str)
                .map(str::to_owned);
            let mut locations = Vec::new();
            if status == VerificationStatus::Failed
                && let Some(file) = file.clone()
            {
                let location = assertion.get("location");
                locations.push(FailureLocation {
                    file,
                    line: location
                        .and_then(|value| value.get("line"))
                        .and_then(Value::as_u64)
                        .and_then(|line| u32::try_from(line).ok()),
                    column: location
                        .and_then(|value| value.get("column"))
                        .and_then(Value::as_u64)
                        .and_then(|column| u32::try_from(column).ok()),
                    message: failure_message,
                });
            }
            tests.push(TestCaseResult {
                source_id: Some(name.clone()),
                name,
                suite: assertion
                    .get("ancestorTitles")
                    .and_then(Value::as_array)
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" > ")
                    })
                    .filter(|suite| !suite.is_empty()),
                status,
                duration_ms: assertion.get("duration").and_then(Value::as_u64),
                stdout: String::new(),
                stderr: String::new(),
                locations,
            });
        }
    }
    Ok(tests)
}

fn parse_pytest(output: &str) -> Vec<TestCaseResult> {
    static TEST: OnceLock<Regex> = OnceLock::new();
    let regex = TEST.get_or_init(|| {
        Regex::new(r"(?m)^(?P<id>\S+::\S+)\s+(?P<status>PASSED|FAILED|SKIPPED)(?:\s+\[\s*\d+%\])?")
            .expect("valid pytest regex")
    });
    let locations = failure_locations(output);
    regex
        .captures_iter(output)
        .map(|capture| {
            let source_id = capture["id"].to_owned();
            let (suite, name) = source_id
                .rsplit_once("::")
                .map_or((None, source_id.clone()), |(suite, name)| {
                    (Some(suite.to_owned()), name.to_owned())
                });
            let status = match &capture["status"] {
                "PASSED" => VerificationStatus::Passed,
                "SKIPPED" => VerificationStatus::Skipped,
                _ => VerificationStatus::Failed,
            };
            TestCaseResult {
                name,
                suite,
                status,
                duration_ms: None,
                stdout: String::new(),
                stderr: String::new(),
                locations: if status == VerificationStatus::Failed {
                    locations.clone()
                } else {
                    Vec::new()
                },
                source_id: Some(source_id),
            }
        })
        .collect()
}

fn parse_junit(output: &str) -> Vec<TestCaseResult> {
    static TEST_CASE: OnceLock<Regex> = OnceLock::new();
    let regex = TEST_CASE.get_or_init(|| {
        Regex::new(r"(?s)<testcase\b(?P<self>[^>]*)/>|<testcase\b(?P<attrs>[^>]*)>(?P<body>.*?)</testcase>")
            .expect("valid JUnit testcase regex")
    });
    regex
        .captures_iter(output)
        .map(|capture| {
            let attrs = capture
                .name("attrs")
                .or_else(|| capture.name("self"))
                .map(|value| value.as_str())
                .unwrap_or_default();
            let body = capture
                .name("body")
                .map(|value| value.as_str())
                .unwrap_or_default();
            let status = if body.contains("<failure") || body.contains("<error") {
                VerificationStatus::Failed
            } else if body.contains("<skipped") {
                VerificationStatus::Skipped
            } else {
                VerificationStatus::Passed
            };
            let file = attribute(attrs, "file");
            let line = attribute(attrs, "line").and_then(|line| line.parse().ok());
            let message = failure_message(body);
            let locations = if status == VerificationStatus::Failed {
                file.map(|file| {
                    vec![FailureLocation {
                        file: PathBuf::from(file),
                        line,
                        column: None,
                        message,
                    }]
                })
                .unwrap_or_default()
            } else {
                Vec::new()
            };
            let name = attribute(attrs, "name").unwrap_or_else(|| "unnamed test".into());
            TestCaseResult {
                source_id: Some(name.clone()),
                name,
                suite: attribute(attrs, "classname"),
                status,
                duration_ms: attribute(attrs, "time").and_then(|time| seconds_to_millis(&time)),
                stdout: String::new(),
                stderr: String::new(),
                locations,
            }
        })
        .collect()
}

fn failure_locations(output: &str) -> Vec<FailureLocation> {
    static LOCATION: OnceLock<Regex> = OnceLock::new();
    let regex = LOCATION.get_or_init(|| {
        Regex::new(
            r"(?m)(?:-->\s*|\bat\s+|^)(?P<file>[A-Za-z0-9_./\\ -]+\.[A-Za-z0-9]+):(?P<line>\d+)(?::(?P<column>\d+))?",
        )
        .expect("valid failure location regex")
    });
    let mut locations = Vec::new();
    for capture in regex.captures_iter(output) {
        let location = FailureLocation {
            file: PathBuf::from(capture["file"].trim()),
            line: capture["line"].parse().ok(),
            column: capture
                .name("column")
                .and_then(|column| column.as_str().parse().ok()),
            message: None,
        };
        if !locations.contains(&location) {
            locations.push(location);
        }
    }
    locations
}

fn attribute(attrs: &str, name: &str) -> Option<String> {
    let pattern = format!(r#"\b{}\s*=\s*"([^"]*)""#, regex::escape(name));
    let regex = Regex::new(&pattern).ok()?;
    regex
        .captures(attrs)
        .and_then(|capture| capture.get(1))
        .map(|value| xml_unescape(value.as_str()))
}

fn failure_message(body: &str) -> Option<String> {
    static FAILURE: OnceLock<Regex> = OnceLock::new();
    let regex = FAILURE.get_or_init(|| {
        Regex::new(r#"(?s)<(?:failure|error)\b[^>]*message="([^"]*)""#)
            .expect("valid JUnit failure regex")
    });
    regex
        .captures(body)
        .and_then(|capture| capture.get(1))
        .map(|value| xml_unescape(value.as_str()))
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn seconds_to_millis(value: &str) -> Option<u64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| (seconds * 1000.0).round() as u64)
}
