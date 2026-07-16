//! Phase 2 spike glue: RPC handlers bridging the local protocol to the
//! domain crates (`retcon-terminal`, `retcon-git`, `retcon-agents`) and the
//! browser service. Disposable by design — the durable subsystems land in
//! Phases 3–24 behind the generated protocol.

pub mod agent;
pub mod browser;
pub mod git;
pub mod terminal;

use serde_json::Value;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::Response;

/// Shorthand for a spike-handler failure response.
pub(crate) fn fail(
    id: u64,
    code: ErrorCode,
    user_message: &str,
    technical_message: impl Into<String>,
) -> Response {
    Response::error(
        id,
        &CoreError::new(code, ErrorSource::System, user_message, technical_message),
    )
}

/// Read a string parameter.
pub(crate) fn param_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

/// Read an unsigned integer parameter.
pub(crate) fn param_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(Value::as_u64)
}
