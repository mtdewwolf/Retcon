//! Structured, actionable errors shared by core subsystems and RPC responses.

use std::error::Error;
use std::fmt::{self, Display};

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    AlreadyRunning,
    AuthenticationFailed,
    InvalidRequest,
    Io,
    NotFound,
    Shutdown,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSource {
    Lifecycle,
    Rpc,
    Storage,
    System,
}

#[derive(Debug, Serialize)]
pub struct CoreError {
    pub id: Box<str>,
    pub code: ErrorCode,
    pub source: ErrorSource,
    pub user_message: Box<str>,
    pub technical_message: Box<str>,
    pub suggested_fix: Option<Box<str>>,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<Box<Value>>,
    #[serde(skip)]
    cause: Option<Box<dyn Error + Send + Sync>>,
}

impl CoreError {
    pub fn new(
        code: ErrorCode,
        source: ErrorSource,
        user_message: impl Into<String>,
        technical_message: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string().into_boxed_str(),
            code,
            source,
            user_message: user_message.into().into_boxed_str(),
            technical_message: redact(technical_message.into()).into_boxed_str(),
            suggested_fix: None,
            retryable: false,
            diagnostic: None,
            cause: None,
        }
    }

    pub fn suggested_fix(mut self, fix: impl Into<String>) -> Self {
        self.suggested_fix = Some(fix.into().into_boxed_str());
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn diagnostic(mut self, diagnostic: Value) -> Self {
        self.diagnostic = Some(Box::new(diagnostic));
        self
    }

    pub fn with_cause(mut self, cause: impl Error + Send + Sync + 'static) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    pub fn io(context: &str, cause: std::io::Error) -> Self {
        let technical = format!("{context}: {cause}");
        Self::new(
            ErrorCode::Io,
            ErrorSource::System,
            "Retcon could not access a required local resource.",
            technical,
        )
        .suggested_fix("Check file permissions and try again.")
        .retryable(true)
        .with_cause(cause)
    }
}

impl Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} [{}]", self.user_message, self.id)
    }
}

impl Error for CoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause
            .as_deref()
            .map(|error| error as &(dyn Error + 'static))
    }
}

/// Keep obvious credentials out of diagnostics until the dedicated redaction system lands.
fn redact(message: String) -> String {
    message
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.starts_with("token=")
                || lower.starts_with("password=")
                || lower.starts_with("secret=")
            {
                "[REDACTED]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn technical_messages_redact_obvious_credentials() {
        let error = CoreError::new(
            ErrorCode::Internal,
            ErrorSource::System,
            "failed",
            "request token=abc password=hunter2",
        );

        assert_eq!(
            error.technical_message.as_ref(),
            "request [REDACTED] [REDACTED]"
        );
    }
}
