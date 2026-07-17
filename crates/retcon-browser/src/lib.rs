//! Typed boundary between durable core orchestration and the separate browser service.

#![allow(missing_docs)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

mod node;

pub use node::{NodeBrowserService, NodeBrowserServiceConfig, NodeStdioTransport};

pub const BROWSER_SERVICE_PROTOCOL: u32 = 1;
pub const MAX_SERVICE_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;

pub type BrowserFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BrowserServiceError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserServiceDiagnostics {
    pub service_version: String,
    pub protocol_version: u32,
    pub features: Vec<String>,
    pub healthy: bool,
}

impl BrowserServiceDiagnostics {
    #[must_use]
    pub fn compatible(&self) -> bool {
        self.healthy
            && self.protocol_version == BROWSER_SERVICE_PROTOCOL
            && !self.service_version.trim().is_empty()
            && self.service_version.len() <= 128
            && self.features.len() <= 64
            && self
                .features
                .iter()
                .all(|feature| !feature.trim().is_empty() && feature.len() <= 128)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLaunchRequest {
    pub session_id: Uuid,
    pub profile_path: String,
    #[serde(default)]
    pub persistent_profile: bool,
    pub network_policy: String,
    #[serde(default)]
    pub input_roots: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLaunchResult {
    pub service_session_id: String,
    pub initial_tab: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserServiceArtifact {
    pub kind: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserCallResult {
    pub value: Value,
    pub artifacts: Vec<BrowserServiceArtifact>,
}

impl BrowserCallResult {
    #[must_use]
    pub fn value(value: Value) -> Self {
        Self {
            value,
            artifacts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserServiceError {
    pub code: String,
    pub message: String,
}

impl BrowserServiceError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn is_fatal(&self) -> bool {
        matches!(
            self.code.as_str(),
            "closed" | "crashed" | "decode" | "eof" | "transport" | "unavailable"
        )
    }
}

impl std::fmt::Display for BrowserServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BrowserServiceError {}

pub trait BrowserService: Send + Sync {
    fn diagnostics(&self) -> Result<BrowserServiceDiagnostics, BrowserServiceError>;
    fn launch<'a>(
        &'a self,
        request: &'a BrowserLaunchRequest,
    ) -> BrowserFuture<'a, BrowserLaunchResult>;
    fn close<'a>(&'a self, session_id: Uuid) -> BrowserFuture<'a, ()>;
    fn call<'a>(
        &'a self,
        session_id: Uuid,
        method: &'a str,
        params: Value,
    ) -> BrowserFuture<'a, BrowserCallResult>;
    fn shutdown(&self) -> BrowserFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

pub trait BrowserTransport: Send + Sync {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> BrowserFuture<'a, Value>;
}

pub struct TransportBrowserService<T> {
    transport: Arc<T>,
    diagnostics: BrowserServiceDiagnostics,
}

impl<T> TransportBrowserService<T> {
    #[must_use]
    pub fn new(transport: Arc<T>, diagnostics: BrowserServiceDiagnostics) -> Self {
        Self {
            transport,
            diagnostics,
        }
    }
}

impl<T: BrowserTransport> BrowserService for TransportBrowserService<T> {
    fn diagnostics(&self) -> Result<BrowserServiceDiagnostics, BrowserServiceError> {
        Ok(self.diagnostics.clone())
    }

    fn launch<'a>(
        &'a self,
        request: &'a BrowserLaunchRequest,
    ) -> BrowserFuture<'a, BrowserLaunchResult> {
        Box::pin(async move {
            let value = self
                .transport
                .request(
                    "browser.launch",
                    serde_json::to_value(request)
                        .map_err(|error| BrowserServiceError::new("encode", error.to_string()))?,
                )
                .await?;
            serde_json::from_value(value)
                .map_err(|error| BrowserServiceError::new("decode", error.to_string()))
        })
    }

    fn close<'a>(&'a self, session_id: Uuid) -> BrowserFuture<'a, ()> {
        Box::pin(async move {
            self.transport
                .request("browser.close", json!({"sessionId":session_id}))
                .await?;
            Ok(())
        })
    }

    fn call<'a>(
        &'a self,
        session_id: Uuid,
        method: &'a str,
        params: Value,
    ) -> BrowserFuture<'a, BrowserCallResult> {
        Box::pin(async move {
            let mut params = params.as_object().cloned().ok_or_else(|| {
                BrowserServiceError::new("encode", "browser service params must be an object")
            })?;
            params.remove("approvalId");
            params.insert("sessionId".into(), Value::String(session_id.to_string()));
            let value = self
                .transport
                .request(method, Value::Object(params))
                .await?;
            Ok(BrowserCallResult::value(value))
        })
    }
}

#[derive(Default)]
pub struct UnavailableBrowserService;

impl BrowserService for UnavailableBrowserService {
    fn diagnostics(&self) -> Result<BrowserServiceDiagnostics, BrowserServiceError> {
        Ok(BrowserServiceDiagnostics {
            service_version: "unavailable".into(),
            protocol_version: BROWSER_SERVICE_PROTOCOL,
            features: Vec::new(),
            healthy: false,
        })
    }

    fn launch<'a>(
        &'a self,
        _request: &'a BrowserLaunchRequest,
    ) -> BrowserFuture<'a, BrowserLaunchResult> {
        Box::pin(async {
            Err(BrowserServiceError::new(
                "unavailable",
                "browser service is not installed",
            ))
        })
    }

    fn close<'a>(&'a self, _session_id: Uuid) -> BrowserFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn call<'a>(
        &'a self,
        _session_id: Uuid,
        _method: &'a str,
        _params: Value,
    ) -> BrowserFuture<'a, BrowserCallResult> {
        Box::pin(async {
            Err(BrowserServiceError::new(
                "unavailable",
                "browser service is not installed",
            ))
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingTransport {
        request: Mutex<Option<(String, Value)>>,
    }

    impl BrowserTransport for RecordingTransport {
        fn request<'a>(&'a self, method: &'a str, params: Value) -> BrowserFuture<'a, Value> {
            *self.request.lock().unwrap() = Some((method.into(), params));
            Box::pin(async { Ok(json!({"ok":true})) })
        }
    }

    #[tokio::test]
    async fn transport_flattens_service_params_and_strips_approval() {
        let transport = Arc::new(RecordingTransport::default());
        let service = TransportBrowserService::new(
            transport.clone(),
            BrowserServiceDiagnostics {
                service_version: "0.1.0".into(),
                protocol_version: BROWSER_SERVICE_PROTOCOL,
                features: vec!["tabs".into()],
                healthy: true,
            },
        );
        let session_id = Uuid::new_v4();
        service
            .call(
                session_id,
                "browser.navigate",
                json!({"url":"http://127.0.0.1:3000","approvalId":Uuid::new_v4()}),
            )
            .await
            .unwrap();

        let recorded = transport.request.lock().unwrap().clone().unwrap();
        assert_eq!(recorded.0, "browser.navigate");
        assert_eq!(recorded.1["sessionId"], session_id.to_string());
        assert_eq!(recorded.1["url"], "http://127.0.0.1:3000");
        assert!(recorded.1.get("approvalId").is_none());
        assert!(recorded.1.get("params").is_none());
    }

    #[test]
    fn diagnostics_reject_unbounded_or_empty_identity_fields() {
        assert!(
            !BrowserServiceDiagnostics {
                service_version: "x".repeat(129),
                protocol_version: BROWSER_SERVICE_PROTOCOL,
                features: vec![],
                healthy: true,
            }
            .compatible()
        );
        assert!(
            !BrowserServiceDiagnostics {
                service_version: "0.1.0".into(),
                protocol_version: BROWSER_SERVICE_PROTOCOL,
                features: vec![String::new()],
                healthy: true,
            }
            .compatible()
        );
    }
}
