//! Privacy-first diagnostic records shared by Retcon components.
//!
//! Recorders are local-only. Callers must treat failures as non-fatal and must
//! never place prompts, commands, file contents, raw paths, URLs, or durable
//! identifiers into records. Implementations still sanitize every record at
//! the persistence boundary.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum UTF-8 bytes accepted for one diagnostic message.
pub const MAX_MESSAGE_BYTES: usize = 4_096;
/// Maximum serialized bytes accepted for structured fields or metric tags.
pub const MAX_FIELDS_BYTES: usize = 16_384;
/// Maximum tags accepted for one metric.
pub const MAX_TAGS: usize = 16;

/// Severity of an operational diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Verbose local troubleshooting information.
    Debug,
    /// Normal lifecycle information.
    Info,
    /// A recoverable or reviewable problem.
    Warning,
    /// An operation failed.
    Error,
    /// Retcon cannot safely continue the affected operation.
    Critical,
}

impl Severity {
    /// Stable storage representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

/// Unit of a performance metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    /// Elapsed milliseconds.
    Milliseconds,
    /// An event or item count.
    Count,
    /// Byte quantity.
    Bytes,
    /// Unitless ratio in the inclusive range zero to one.
    Ratio,
}

impl MetricUnit {
    /// Stable storage representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Milliseconds => "milliseconds",
            Self::Count => "count",
            Self::Bytes => "bytes",
            Self::Ratio => "ratio",
        }
    }
}

/// One structured local operational log request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewLogRecord {
    /// Severity used for filtering and retention.
    pub severity: Severity,
    /// Bounded component name such as `core` or `dev_server`.
    pub component: String,
    /// Bounded stable event/code name.
    pub event: String,
    /// Human-readable message after sanitization.
    pub message: String,
    /// Small structured fields. User content is forbidden.
    #[serde(default)]
    pub fields: Value,
}

/// One local performance metric request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMetricSample {
    /// Component producing the metric.
    pub component: String,
    /// Stable metric name.
    pub name: String,
    /// Finite metric value.
    pub value: f64,
    /// Metric unit.
    pub unit: MetricUnit,
    /// Bounded dimensions such as operation, outcome, kind, framework, or provider.
    #[serde(default)]
    pub tags: Value,
}

/// Diagnostic recorder failure. Callers should report it only through their
/// existing local logger and continue the primary operation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DiagnosticsError {
    /// A record exceeded the stable contract.
    #[error("diagnostic record is invalid: {0}")]
    Invalid(String),
    /// Local persistence failed.
    #[error("diagnostic persistence failed: {0}")]
    Persistence(String),
}

/// Cheap component-facing recorder boundary.
pub trait DiagnosticsRecorder: Send + Sync {
    /// Record a structured local log. Failures are non-fatal to the caller.
    fn record_log(&self, record: NewLogRecord) -> Result<(), DiagnosticsError>;
    /// Record a local metric. Failures are non-fatal to the caller.
    fn record_metric(&self, metric: NewMetricSample) -> Result<(), DiagnosticsError>;
}

/// Shared recorder handle used for dependency injection.
pub type SharedDiagnosticsRecorder = Arc<dyn DiagnosticsRecorder>;

/// Default recorder for components running without durable Core state.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopDiagnosticsRecorder;

impl DiagnosticsRecorder for NoopDiagnosticsRecorder {
    fn record_log(&self, _record: NewLogRecord) -> Result<(), DiagnosticsError> {
        Ok(())
    }

    fn record_metric(&self, _metric: NewMetricSample) -> Result<(), DiagnosticsError> {
        Ok(())
    }
}

/// Mandatory server-side sanitizer for persisted diagnostics and exports.
#[derive(Clone, Debug, Default)]
pub struct Sanitizer {
    home: Option<PathBuf>,
    username: Option<String>,
    aliases: Vec<(PathBuf, String)>,
}

impl Sanitizer {
    /// Discover the current user's home/name without exposing them in output.
    #[must_use]
    pub fn discover() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from);
        let username = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok()
            .filter(|value| !value.trim().is_empty());
        Self {
            home,
            username,
            aliases: Vec::new(),
        }
    }

    /// Register a safe alias such as `$RETCON_DATA` for a managed root.
    #[must_use]
    pub fn with_alias(mut self, root: impl Into<PathBuf>, alias: impl Into<String>) -> Self {
        self.aliases.push((root.into(), alias.into()));
        self.aliases
            .sort_by(|left, right| right.0.as_os_str().len().cmp(&left.0.as_os_str().len()));
        self
    }

    /// Sanitize free-form text for durable storage or export.
    #[must_use]
    pub fn text(&self, raw: &str) -> String {
        let mut value = retcon_secrets::redact_text(raw);
        for (root, alias) in &self.aliases {
            value = replace_path(&value, root, alias);
        }
        if let Some(home) = &self.home {
            value = replace_path(&value, home, "$HOME");
        }
        if let Some(username) = &self.username {
            value = replace_case_insensitive(&value, username, "[USER]");
        }
        redact_unknown_paths(&value)
    }

    /// Recursively sanitize JSON and remove content-bearing fields.
    #[must_use]
    pub fn value(&self, raw: Value) -> Value {
        self.value_inner(retcon_secrets::scrub_json(raw))
    }

    fn value_inner(&self, value: Value) -> Value {
        match value {
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, nested)| {
                        let normalized = key.to_ascii_lowercase().replace('-', "_");
                        let nested = if is_content_key(&normalized) {
                            Value::String("[REDACTED]".into())
                        } else {
                            self.value_inner(nested)
                        };
                        (key, nested)
                    })
                    .collect(),
            ),
            Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|value| self.value_inner(value))
                    .collect(),
            ),
            Value::String(value) => Value::String(self.text(&value)),
            other => other,
        }
    }

    /// Return only aliases, never their raw roots.
    #[must_use]
    pub fn alias_names(&self) -> Vec<String> {
        self.aliases
            .iter()
            .map(|(_, alias)| alias.clone())
            .collect()
    }
}

/// Validate a log before sanitizing and persisting it.
pub fn validate_log(record: &NewLogRecord) -> Result<(), DiagnosticsError> {
    validate_name(&record.component, "component")?;
    validate_name(&record.event, "event")?;
    if record.message.is_empty() || record.message.len() > MAX_MESSAGE_BYTES {
        return Err(DiagnosticsError::Invalid(
            "message is empty or too large".into(),
        ));
    }
    validate_fields(&record.fields)
}

/// Validate a metric before sanitizing and persisting it.
pub fn validate_metric(metric: &NewMetricSample) -> Result<(), DiagnosticsError> {
    validate_name(&metric.component, "component")?;
    validate_name(&metric.name, "metric name")?;
    if !metric.value.is_finite()
        || (metric.unit == MetricUnit::Ratio && !(0.0..=1.0).contains(&metric.value))
    {
        return Err(DiagnosticsError::Invalid("metric value is invalid".into()));
    }
    validate_tags(&metric.tags)
}

fn validate_name(value: &str, label: &str) -> Result<(), DiagnosticsError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(DiagnosticsError::Invalid(format!("{label} is invalid")))
    } else {
        Ok(())
    }
}

fn validate_fields(value: &Value) -> Result<(), DiagnosticsError> {
    if !value.is_object()
        || serde_json::to_vec(value)
            .map_err(|error| DiagnosticsError::Invalid(error.to_string()))?
            .len()
            > MAX_FIELDS_BYTES
    {
        return Err(DiagnosticsError::Invalid(
            "fields are invalid or too large".into(),
        ));
    }
    Ok(())
}

fn validate_tags(value: &Value) -> Result<(), DiagnosticsError> {
    validate_fields(value)?;
    let Some(tags) = value.as_object() else {
        return Err(DiagnosticsError::Invalid("tags must be an object".into()));
    };
    if tags.len() > MAX_TAGS
        || tags.iter().any(|(key, value)| {
            !matches!(
                key.as_str(),
                "operation" | "outcome" | "kind" | "framework" | "provider" | "method" | "status"
            ) || matches!(value, Value::Array(_) | Value::Object(_) | Value::Null)
                || value.as_str().is_some_and(|value| {
                    value.is_empty()
                        || value.len() > 128
                        || !value.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                        })
                })
        })
    {
        return Err(DiagnosticsError::Invalid("metric tags are invalid".into()));
    }
    Ok(())
}

fn is_content_key(key: &str) -> bool {
    matches!(
        key,
        "prompt"
            | "command"
            | "content"
            | "contents"
            | "body"
            | "request"
            | "response"
            | "arguments"
            | "file_contents"
            | "file_content"
            | "url"
            | "headers"
            | "header"
            | "cookies"
            | "cookie"
            | "storage"
            | "environment"
            | "env"
            | "env_values"
            | "terminal_output"
            | "stdout"
            | "stderr"
            | "diff"
            | "patch"
            | "sql"
            | "tool_input"
            | "account_id"
            | "provider_account_id"
            | "username"
    )
}

fn redact_unknown_paths(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| {
            let trimmed = part.trim_matches(|character: char| {
                matches!(character, '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';')
            });
            let bytes = trimmed.as_bytes();
            let windows = bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'/' | b'\\');
            let unix = trimmed.starts_with('/') && !trimmed.starts_with("//");
            if windows || unix { "[PATH]" } else { part }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn replace_path(value: &str, path: &Path, replacement: &str) -> String {
    let raw = path.to_string_lossy();
    let replaced = replace_case_insensitive(value, &raw, replacement);
    let alternate = raw.replace('\\', "/");
    replace_case_insensitive(&replaced, &alternate, replacement)
}

fn replace_case_insensitive(value: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return value.to_owned();
    }
    let lower_value = value.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut output = String::with_capacity(value.len());
    let mut offset = 0;
    while let Some(relative) = lower_value[offset..].find(&lower_needle) {
        let start = offset + relative;
        output.push_str(&value[offset..start]);
        output.push_str(replacement);
        offset = start + needle.len();
    }
    output.push_str(&value[offset..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitizer_removes_secrets_user_paths_and_content_fields() {
        let sanitizer = Sanitizer {
            home: Some(PathBuf::from("C:/Users/alice")),
            username: Some("alice".into()),
            aliases: vec![(PathBuf::from("D:/Retcon/data"), "$RETCON_DATA".into())],
        };
        let value = sanitizer.value(json!({
            "path":"D:/Retcon/data/logs/core.log",
            "other":"C:/Users/alice token=hunter2",
            "prompt":"private request",
            "nested":{"apiKey":"canary-secret","Tool-Input":"raw tool content","STDERR":"private output","headers":{"Authorization":"Bearer secret"}},
            "unknownPath":"E:/private/project/file.rs"
        }));
        let encoded = value.to_string();
        assert!(encoded.contains("$RETCON_DATA"));
        assert!(encoded.contains("$HOME"));
        assert!(!encoded.contains("alice"));
        assert!(!encoded.contains("hunter2"));
        assert!(!encoded.contains("canary-secret"));
        assert!(!encoded.contains("private request"));
        assert!(!encoded.contains("raw tool content"));
        assert!(!encoded.contains("private output"));
        assert!(!encoded.contains("E:/private"));
    }

    #[test]
    fn validation_rejects_unbounded_and_nested_tags() {
        assert!(
            validate_log(&NewLogRecord {
                severity: Severity::Info,
                component: "core".into(),
                event: "ready".into(),
                message: "ready".into(),
                fields: json!({}),
            })
            .is_ok()
        );
        assert!(
            validate_metric(&NewMetricSample {
                component: "core".into(),
                name: "ipc.duration".into(),
                value: 1.0,
                unit: MetricUnit::Milliseconds,
                tags: json!({"operation":"health","outcome":"ok"}),
            })
            .is_ok()
        );
        assert!(
            validate_metric(&NewMetricSample {
                component: "core".into(),
                name: "ipc.duration".into(),
                value: 1.0,
                unit: MetricUnit::Milliseconds,
                tags: json!({"operation":{"raw":"forbidden"}}),
            })
            .is_err()
        );
        assert!(
            validate_metric(&NewMetricSample {
                component: "core".into(),
                name: "ipc.duration".into(),
                value: 1.0,
                unit: MetricUnit::Milliseconds,
                tags: json!({"operation":"https://example.test/private"}),
            })
            .is_err()
        );
    }
}
