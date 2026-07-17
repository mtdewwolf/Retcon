//! Structured, actionable errors shared by core subsystems and RPC responses.

#![allow(missing_docs)] // Phase 2 API; public documentation lands with the generated protocol.

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
    PermissionDenied,
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
        self.diagnostic = Some(Box::new(redact_value(diagnostic)));
        self
    }

    /// Produce a copyable, redacted support record including the cause chain.
    pub fn support_bundle(&self) -> Value {
        let mut causes = Vec::new();
        let mut current = self.source();
        while let Some(cause) = current {
            causes.push(redact(cause.to_string()));
            current = cause.source();
        }
        serde_json::json!({
            "crash_id": self.id,
            "error": self,
            "causes": causes,
            "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        })
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

impl From<retcon_storage::StorageError> for CoreError {
    fn from(error: retcon_storage::StorageError) -> Self {
        let (user_message, suggested_fix, retryable) = match &error {
            retcon_storage::StorageError::Corrupt { .. } => (
                "Retcon's local database is damaged.",
                "Restore a known-good backup or move the damaged database aside and restart Retcon.",
                false,
            ),
            retcon_storage::StorageError::SchemaTooNew { .. } => (
                "This data was created by a newer version of Retcon.",
                "Upgrade Retcon before opening this data directory.",
                false,
            ),
            _ => (
                "Retcon could not initialize its local database.",
                "Check available disk space and file permissions, then try again.",
                true,
            ),
        };
        let technical_message = error.to_string();
        Self::new(
            ErrorCode::Internal,
            ErrorSource::Storage,
            user_message,
            technical_message,
        )
        .suggested_fix(suggested_fix)
        .retryable(retryable)
        .with_cause(error)
    }
}

fn redact(message: String) -> String {
    retcon_secrets::redact_text(&message)
}

fn redact_value(value: Value) -> Value {
    retcon_secrets::scrub_json(value)
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

    #[test]
    fn support_bundles_redact_nested_diagnostics() {
        let error = CoreError::new(ErrorCode::Internal, ErrorSource::System, "failed", "safe")
            .diagnostic(serde_json::json!({"request":{"token":"abc"}}));
        assert_eq!(
            error.support_bundle()["error"]["diagnostic"]["request"]["token"],
            "[REDACTED]"
        );
    }
}
