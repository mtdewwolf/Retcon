//! Bounded newline-delimited frame reads for local RPC transports.

use tokio::io::AsyncBufReadExt;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use retcon_protocol::MAX_FRAME_BYTES;

/// Read one newline-delimited frame, rejecting payloads larger than [`MAX_FRAME_BYTES`].
///
/// # Errors
///
/// Returns [`CoreError`] when I/O fails or the frame exceeds the protocol limit.
pub async fn read_capped_line(
    reader: &mut (impl AsyncBufReadExt + Unpin),
) -> Result<Option<String>, CoreError> {
    let mut buffer = Vec::new();
    loop {
        let read = reader
            .read_until(b'\n', &mut buffer)
            .await
            .map_err(|error| CoreError::io("read RPC frame", error))?;
        if read == 0 {
            if buffer.is_empty() {
                return Ok(None);
            }
            return trimmed_line(buffer);
        }
        if buffer.len() > MAX_FRAME_BYTES + 1 {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "Retcon received an oversized local request.",
                format!("frame exceeds {MAX_FRAME_BYTES} bytes"),
            ));
        }
        if buffer.ends_with(b"\n") {
            return trimmed_line(buffer);
        }
    }
}

fn trimmed_line(mut buffer: Vec<u8>) -> Result<Option<String>, CoreError> {
    if buffer.ends_with(b"\n") {
        buffer.pop();
    }
    if buffer.ends_with(b"\r") {
        buffer.pop();
    }
    if buffer.is_empty() {
        return Ok(None);
    }
    String::from_utf8(buffer).map(Some).map_err(|error| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "Retcon received an invalid local request.",
            error.to_string(),
        )
    })
}
