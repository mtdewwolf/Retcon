//! Bounded NDJSON frame I/O for local RPC transports.

#![allow(missing_docs)]

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use retcon_protocol::MAX_FRAME_BYTES;

use crate::error::{CoreError, ErrorCode, ErrorSource};

/// Read one newline-delimited frame, rejecting payloads larger than `MAX_FRAME_BYTES`.
///
/// Returns `Ok(None)` on clean EOF with no buffered bytes.
pub async fn read_frame_line<R>(reader: &mut R) -> Result<Option<String>, CoreError>
where
    R: AsyncBufRead + Unpin,
{
    let mut buf = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|error| CoreError::io("read RPC frame", error))?;
        if available.is_empty() {
            return if buf.is_empty() {
                Ok(None)
            } else {
                Ok(Some(decode_frame(buf)?))
            };
        }

        if let Some(newline_at) = available.iter().position(|&byte| byte == b'\n') {
            let end = newline_at + 1;
            if buf.len() + end > MAX_FRAME_BYTES {
                reader.consume(end);
                return Err(frame_too_large());
            }
            buf.extend_from_slice(&available[..end]);
            reader.consume(end);
            return Ok(Some(decode_frame(buf)?));
        }

        if buf.len() + available.len() > MAX_FRAME_BYTES {
            let consume = available.len();
            reader.consume(consume);
            return Err(frame_too_large());
        }

        let consume = available.len();
        buf.extend_from_slice(available);
        reader.consume(consume);
    }
}

fn decode_frame(mut buf: Vec<u8>) -> Result<String, CoreError> {
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|error| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "Retcon received a malformed local frame.",
            error.to_string(),
        )
    })
}

fn frame_too_large() -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        ErrorSource::Rpc,
        "Retcon rejected an oversized local frame.",
        format!("RPC frame exceeded {MAX_FRAME_BYTES} bytes"),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn accepts_frames_under_limit() {
        let mut reader = BufReader::new(Cursor::new(b"{\"ok\":true}\n".as_slice()));
        let line = read_frame_line(&mut reader).await.unwrap().unwrap();
        assert_eq!(line, "{\"ok\":true}");
    }

    #[tokio::test]
    async fn rejects_oversized_frames() {
        let mut payload = vec![b'a'; MAX_FRAME_BYTES + 8];
        payload.push(b'\n');
        let mut reader = BufReader::new(Cursor::new(payload));
        let error = read_frame_line(&mut reader).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
    }
}
