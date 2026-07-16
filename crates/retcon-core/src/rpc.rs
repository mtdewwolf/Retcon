//! Temporary newline-delimited JSON RPC used until Phase 4 generates protocol types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::CoreError;

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

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

#[derive(Debug, Deserialize)]
pub struct AuthLine {
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discovery {
    pub address: String,
    pub token: String,
    pub pid: u32,
    pub version: String,
}
