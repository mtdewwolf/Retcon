//! RPC adapter for secret scanning and redaction helpers.

use retcon_secrets::{ScanResult, scan_text, scan_texts};
use serde_json::{Value, json};

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn scan_result_json(result: &ScanResult) -> Value {
    json!({
        "clean": result.is_clean(),
        "findingCount": result.findings.len(),
        "findings": result.findings.iter().map(|finding| json!({
            "kind": match finding.kind {
                retcon_secrets::FindingKind::Pattern => "pattern",
                retcon_secrets::FindingKind::Entropy => "entropy",
            },
            "label": finding.label,
            "start": finding.start,
            "end": finding.end,
        })).collect::<Vec<_>>(),
        "summary": result.summary(),
    })
}

/// Handle `secrets.*` requests.
pub async fn handle(_state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "secrets.scan" => {
            if let Some(text) = params.get("text").and_then(Value::as_str) {
                return Response::ok(id, scan_result_json(&scan_text(text)));
            }
            if let Some(texts) = params.get("texts").and_then(Value::as_array) {
                let blobs = texts.iter().filter_map(Value::as_str).collect::<Vec<_>>();
                if blobs.is_empty() {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "Secret scanning requires text or texts.",
                            "missing 'text' or non-empty 'texts' parameter",
                        ),
                    );
                }
                return Response::ok(id, scan_result_json(&scan_texts(blobs)));
            }
            failed(
                id,
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    ErrorSource::Rpc,
                    "Secret scanning requires text or texts.",
                    "missing 'text' or 'texts' parameter",
                ),
            )
        }
        _ => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested secret operation is not available.",
                format!("unknown secrets RPC method: {method}"),
            ),
        ),
    }
}

/// Scan user-provided prompt text before sending a turn.
#[must_use]
pub fn scan_prompt(prompt: &str) -> ScanResult {
    scan_text(prompt)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn secrets_scan_reports_findings_without_values() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let response = handle(
            state,
            Request {
                id: 1,
                method: "secrets.scan".into(),
                params: json!({"text": "password=hunter2"}),
            },
        )
        .await;
        let result = response.result.unwrap();
        assert_eq!(result["clean"], false);
        assert!(result["summary"].as_str().unwrap().contains("finding"));
        assert!(!result["summary"].as_str().unwrap().contains("hunter2"));
    }
}
