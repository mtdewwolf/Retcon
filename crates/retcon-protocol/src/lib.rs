//! Typed local RPC protocol shared by the desktop shell, core service, and browser service.
//!
//! `schemas/protocol/v1.json` is the wire-format source of truth. These bindings
//! model transport-neutral envelopes while Phase 4 transport work proceeds.

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

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
    pub address: String,
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

pub fn negotiate_features(client: &[String], server: &[String]) -> Vec<String> {
    client
        .iter()
        .filter(|feature| server.contains(feature))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
