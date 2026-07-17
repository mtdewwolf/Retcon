//! Redact secrets from strings and JSON diagnostic payloads.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

static INLINE_SECRET: OnceLock<Regex> = OnceLock::new();

fn inline_secret_regex() -> &'static Regex {
    INLINE_SECRET.get_or_init(|| {
        // Pattern is a compile-time constant; failure would be a programmer error.
        #[allow(clippy::expect_used)]
        {
            Regex::new(
                r"(?i)(\b(?:token|password|secret|authorization|api[_-]?key|access[_-]?key|auth)\s*[:=]\s*)(\S+)",
            )
            .expect("inline secret regex")
        }
    })
}

const SENSITIVE_KEYS: &[&str] = &[
    "auth",
    "token",
    "password",
    "secret",
    "authorization",
    "api_key",
    "apikey",
    "access_key",
    "accesskey",
    "private_key",
    "privatekey",
    "credential",
    "credentials",
    "prompt",
];

/// Redact obvious inline credential assignments from free-form text.
#[must_use]
pub fn redact_text(message: &str) -> String {
    inline_secret_regex()
        .replace_all(message, "$1[REDACTED]")
        .into_owned()
        .split_whitespace()
        .map(redact_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_token(part: &str) -> &str {
    let lower = part.to_ascii_lowercase();
    if lower.starts_with("token=")
        || lower.starts_with("password=")
        || lower.starts_with("secret=")
        || lower.starts_with("api_key=")
        || lower.starts_with("apikey=")
    {
        "[REDACTED]"
    } else {
        part
    }
}

/// Recursively scrub sensitive JSON keys and inline secrets in string values.
#[must_use]
pub fn scrub_json(value: Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, nested)| {
                    let secret = is_sensitive_key(&key);
                    (
                        key,
                        if secret {
                            Value::String("[REDACTED]".into())
                        } else {
                            scrub_json(nested)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(scrub_json).collect()),
        Value::String(text) => Value::String(redact_text(&text)),
        other => other,
    }
}

/// Returns `true` when a JSON key should always be redacted.
#[must_use]
pub fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    SENSITIVE_KEYS
        .iter()
        .any(|candidate| normalized == *candidate || normalized.ends_with(&format!("_{candidate}")))
}

/// Scrub only the listed keys inside an object, leaving other values intact.
#[must_use]
pub fn scrub_string_fields(value: Value, extra_keys: &[&str]) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, nested)| {
                    let secret = is_sensitive_key(&key)
                        || extra_keys
                            .iter()
                            .any(|candidate| key.eq_ignore_ascii_case(candidate));
                    (
                        key,
                        if secret {
                            Value::String("[REDACTED]".into())
                        } else {
                            scrub_string_fields(nested, extra_keys)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|v| scrub_string_fields(v, extra_keys))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redact_text_hides_inline_assignments() {
        assert_eq!(
            redact_text("request token=abc password=hunter2"),
            "request [REDACTED] [REDACTED]"
        );
    }

    #[test]
    fn scrub_json_redacts_nested_keys_and_strings() {
        let scrubbed = scrub_json(json!({
            "request": {"token": "abc", "note": "password=secret"},
            "prompt": "Bearer abc.def.ghi"
        }));
        assert_eq!(scrubbed["request"]["token"], "[REDACTED]");
        assert_eq!(scrubbed["prompt"], "[REDACTED]");
        assert!(
            scrubbed["request"]["note"]
                .as_str()
                .unwrap()
                .contains("[REDACTED]")
        );
    }
}
