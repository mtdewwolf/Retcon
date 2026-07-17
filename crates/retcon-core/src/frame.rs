//! Bounded newline-delimited frame reads for local RPC transports.

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use crate::error::{CoreError, ErrorCode, ErrorSource};
use retcon_protocol::MAX_FRAME_BYTES;

/// Read one newline-delimited frame, rejecting payloads larger than [`MAX_FRAME_BYTES`].
///
/// Uses `fill_buf` / `consume` so a newline-free sender cannot grow the buffer past the cap.
///
/// # Errors
///
/// Returns [`CoreError`] when I/O fails or the frame exceeds the protocol limit.
pub async fn read_capped_line(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Option<String>, CoreError> {
    let mut buffer = Vec::new();
    loop {
        let filled = reader
            .fill_buf()
            .await
            .map_err(|error| CoreError::io("read RPC frame", error))?;
        if filled.is_empty() {
            if buffer.is_empty() {
                return Ok(None);
            }
            return trimmed_line(buffer);
        }

        if let Some(newline_at) = filled.iter().position(|&byte| byte == b'\n') {
            let take = newline_at + 1;
            if buffer.len().saturating_add(take) > MAX_FRAME_BYTES + 1 {
                return oversized();
            }
            buffer.extend_from_slice(&filled[..take]);
            reader.consume(take);
            return trimmed_line(buffer);
        }

        let take = filled.len();
        if buffer.len().saturating_add(take) > MAX_FRAME_BYTES + 1 {
            return oversized();
        }
        buffer.extend_from_slice(filled);
        reader.consume(take);
    }
}

fn oversized() -> Result<Option<String>, CoreError> {
    Err(CoreError::new(
        ErrorCode::InvalidRequest,
        ErrorSource::Rpc,
        "Retcon received an oversized local request.",
        format!("frame exceeds {MAX_FRAME_BYTES} bytes"),
    ))
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn accepts_framed_line_within_limit() {
        let mut reader = BufReader::new(&b"{\"ok\":true}\n"[..]);
        let line = read_capped_line(&mut reader).await.unwrap().unwrap();
        assert_eq!(line, "{\"ok\":true}");
    }

    #[tokio::test]
    async fn rejects_newline_free_oversized_frame() {
        let oversized = vec![b'a'; MAX_FRAME_BYTES + 64];
        let mut reader = BufReader::new(oversized.as_slice());
        let error = read_capped_line(&mut reader).await.unwrap_err();
        assert!(error.technical_message.contains("frame exceeds"));
    }
}
