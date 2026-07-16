//! Temporary newline-delimited JSON transport helpers.
//!
//! Message envelopes live in `retcon-protocol`; this module adapts core errors
//! to the existing Phase 2 transport.

use serde::Serialize;
use serde_json::Value;

use crate::CoreError;
pub use retcon_protocol::{Discovery, Request};

#[derive(Debug, Clone, Serialize)]
/// A response produced by the temporary Phase 2 server adapter.
pub struct Response {
    /// The request identifier this response resolves.
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Successful result payload.
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Structured error payload.
    pub error: Option<Value>,
}

impl Response {
    /// Construct a successful response.
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Construct an error response from a core error.
    pub fn error(id: u64, error: &CoreError) -> Self {
        Self {
            id,
            result: None,
            error: Some(serde_json::to_value(error).unwrap_or_else(
                |_| serde_json::json!({"code": "internal", "user_message": "Unknown error"}),
            )),
        }
    }
}
