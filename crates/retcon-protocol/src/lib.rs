//! Typed local RPC protocol shared by the desktop shell, core service, and browser service.
//!
//! `schemas/protocol/v1.json` is the wire-format source of truth. Run
//! `scripts/protocol/generate-all.ps1` to refresh generated Dart and TypeScript
//! clients; Rust bindings are maintained here and validated against fixtures.

#![allow(missing_docs)]

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Server-advertised protocol features.
pub const SERVER_FEATURES: &[&str] = &[
    "events.replay",
    "request.cancel",
    "ping",
    "core.health",
    "core.version",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthLine {
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResponseEnvelope {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub event: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Discovery {
    pub transport: String,
    pub path: String,
    pub token: String,
    pub pid: u32,
    pub version: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientHello {
    pub kind: ClientHelloKind,
    pub protocol_version: u32,
    pub client_version: String,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClientHelloKind {
    #[serde(rename = "client.hello")]
    Hello,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerHello {
    pub kind: ServerHelloKind,
    pub protocol_version: u32,
    pub server_version: String,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerHelloKind {
    #[serde(rename = "server.hello")]
    Hello,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    pub kind: CancelRequestKind,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CancelRequestKind {
    #[serde(rename = "request.cancel")]
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Ping {
    pub kind: PingKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PingKind {
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pong {
    pub kind: PongKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PongKind {
    #[serde(rename = "pong")]
    Pong,
}

/// Parsed inbound client frame after authentication.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    Request(Request),
    Ping,
    Cancel(CancelRequest),
}

impl ClientMessage {
    /// Decode a JSON line into a typed client message.
    pub fn parse(line: &str) -> Result<Self, ParseError> {
        let value: Value = serde_json::from_str(line).map_err(|error| ParseError {
            technical_message: error.to_string(),
        })?;
        if value.get("kind").and_then(Value::as_str) == Some("ping") {
            let _: Ping = serde_json::from_value(value.clone()).map_err(|error| ParseError {
                technical_message: error.to_string(),
            })?;
            return Ok(Self::Ping);
        }
        if value.get("kind").and_then(Value::as_str) == Some("request.cancel") {
            let cancel: CancelRequest = serde_json::from_value(value).map_err(|error| ParseError {
                technical_message: error.to_string(),
            })?;
            return Ok(Self::Cancel(cancel));
        }
        let request: Request = serde_json::from_value(value).map_err(|error| ParseError {
            technical_message: error.to_string(),
        })?;
        Ok(Self::Request(request))
    }
}

/// Structured parse failure for malformed protocol frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub technical_message: String,
}

pub fn negotiate_features(client: &[String], server: &[String]) -> Vec<String> {
    client
        .iter()
        .filter(|feature| server.contains(feature))
        .cloned()
        .collect()
}

/// Redact secrets before writing protocol frames to trace logs.
#[must_use]
pub fn redact_line_for_trace(line: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(line) {
        return serde_json::to_string(&redact_value(value)).unwrap_or_else(|_| "[REDACTED]".into());
    }
    line.chars()
        .map(|character| if character.is_control() { '?' } else { character })
        .collect()
}

fn redact_value(value: Value) -> Value {
    retcon_secrets::scrub_json(value)
}

/// All RPC methods routed by the core server grouped by namespace.
#[must_use]
pub fn method_catalog() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        (
            "core",
            &[
                "core.health",
                "core.version",
                "core.capabilities",
                "core.diagnostics",
                "core.shutdown",
            ],
        ),
        ("events", &["events.replay", "events.emit"]),
        (
            "jobs",
            &[
                "jobs.list",
                "jobs.get",
                "jobs.cancel",
                "jobs.forceStop",
            ],
        ),
        (
            "project",
            &[
                "project.open",
                "project.clone",
                "project.list",
                "project.inspect",
                "project.updateMetadata",
                "project.remove",
            ],
        ),
        ("provider", &["provider.metadata", "provider.doctor"]),
        (
            "session",
            &[
                "session.create",
                "session.list",
                "session.start",
                "session.cancel",
                "session.pause",
                "session.resume",
            ],
        ),
        ("turn", &["turn.send", "turn.cancel"]),
        (
            "terminal",
            &[
                "terminal.detectShells",
                "terminal.start",
                "terminal.input",
                "terminal.resize",
                "terminal.kill",
                "terminal.list",
                "terminal.scrollback",
            ],
        ),
        (
            "git",
            &[
                "git.status",
                "git.defaultBranch",
                "git.branchList",
                "git.branchCreate",
                "git.branchDelete",
                "git.checkout",
                "git.stage",
                "git.unstage",
                "git.stageHunk",
                "git.discardHunk",
                "git.commit",
                "git.push",
                "git.conflicts",
                "git.submodules",
                "git.worktreeList",
                "git.worktreeListStored",
                "git.worktreeAdd",
                "git.worktreeRemove",
                "git.worktreeAssign",
                "git.diff",
            ],
        ),
        ("agent", &["agent.detect", "agent.start", "agent.cancel"]),
        (
            "browser",
            &[
                "browser.startService",
                "browser.stopService",
                "browser.call",
            ],
        ),
        (
            "file",
            &[
                "file.list",
                "file.read",
                "file.write",
                "file.watch",
                "file.unwatch",
            ],
        ),
        (
            "checkpoint",
            &[
                "checkpoint.create",
                "checkpoint.list",
                "checkpoint.get",
                "checkpoint.preview",
                "checkpoint.restore",
            ],
        ),
        ("approval", &["approval.list", "approval.decide"]),
        (
            "permission",
            &[
                "permission.rules.list",
                "permission.rules.create",
                "permission.rules.delete",
            ],
        ),
        ("secrets", &["secrets.scan"]),
        (
            "storage",
            &[
                "storage.status",
                "storage.recover",
                "storage.layout.get",
                "storage.layout.save",
                "storage.layout.list",
            ],
        ),
    ]
}

#[must_use]
pub fn all_methods() -> HashSet<&'static str> {
    method_catalog()
        .iter()
        .flat_map(|(_, methods)| *methods)
        .copied()
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::iter_cloned_collect)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn hello_uses_the_schema_wire_names() {
        let hello = ClientHello {
            kind: ClientHelloKind::Hello,
            protocol_version: VERSION,
            client_version: "0.1.0".into(),
            features: vec!["events.replay".into()],
        };
        assert_eq!(
            serde_json::to_value(hello).ok(),
            Some(serde_json::json!({
                "kind": "client.hello", "protocolVersion": 1,
                "clientVersion": "0.1.0", "features": ["events.replay"]
            }))
        );
    }

    #[test]
    fn feature_negotiation_is_an_intersection() {
        assert_eq!(
            negotiate_features(
                &["events.replay".into(), "streaming".into()],
                &["streaming".into()]
            ),
            vec!["streaming"]
        );
    }

    #[test]
    fn malformed_request_fields_are_rejected() {
        let request = r#"{"id":1,"method":"core.health","params":{},"extra":true}"#;
        assert!(serde_json::from_str::<Request>(request).is_err());
    }

    #[test]
    fn redact_line_hides_auth_tokens() {
        let redacted = redact_line_for_trace(r#"{"auth":"super-secret-token-value-here"}"#);
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("super-secret"));
    }

    #[test]
    fn method_catalog_matches_schema_file() {
        let schema = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/protocol/v1.json"))
        .expect("read protocol schema");
        let root: Value = serde_json::from_str(&schema).expect("parse schema");
        let catalog = root
            .get("methodCatalog")
            .and_then(Value::as_object)
            .expect("methodCatalog");
        for (namespace, methods) in method_catalog() {
            let listed = catalog
                .get(*namespace)
                .and_then(Value::as_array)
                .expect("namespace present");
            let expected: Vec<_> = methods.iter().copied().collect();
            let actual: Vec<_> = listed
                .iter()
                .filter_map(Value::as_str)
                .collect();
            assert_eq!(expected, actual, "namespace {namespace}");
        }
    }

    #[test]
    fn protocol_fixtures_round_trip_or_reject() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/protocol");
        for entry in std::fs::read_dir(&fixtures).expect("read fixtures dir") {
            let entry = entry.expect("fixture entry");
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let contents = std::fs::read_to_string(&path).expect("read fixture");
            if name.starts_with("valid-") {
                assert!(
                    ClientMessage::parse(contents.trim()).is_ok()
                        || serde_json::from_str::<AuthLine>(contents.trim()).is_ok()
                        || serde_json::from_str::<ClientHello>(contents.trim()).is_ok(),
                    "expected valid fixture {name}"
                );
            } else if name.starts_with("invalid-") {
                assert!(
                    ClientMessage::parse(contents.trim()).is_err()
                        && serde_json::from_str::<AuthLine>(contents.trim()).is_err(),
                    "expected invalid fixture {name}"
                );
            }
        }
    }
}
