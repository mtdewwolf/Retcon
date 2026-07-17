//! Minimal helpers to drive an in-process `CoreRuntime` over the local transport.

use std::path::PathBuf;

use retcon_core::{CoreConfig, CoreRuntime};
use retcon_platform::{LocalEndpoint, TransportStream};
use retcon_protocol::{Discovery, Request, VERSION};
use serde_json::Value;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Enables `RETCON_PERMISSIONS_BYPASS=1` for the current process until dropped.
pub struct PermissionsBypassGuard;

impl PermissionsBypassGuard {
    /// Turn on the development permissions bypass for mutating RPC methods.
    pub fn enable() -> Self {
        // SAFETY: integration tests use `serial_test` when toggling process env.
        unsafe {
            std::env::set_var("RETCON_PERMISSIONS_BYPASS", "1");
        }
        Self
    }
}

impl Drop for PermissionsBypassGuard {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("RETCON_PERMISSIONS_BYPASS");
        }
    }
}

/// A running core instance backed by a temporary data directory.
pub struct TestCore {
    /// Live runtime; call [`TestCore::shutdown`] to stop it.
    pub runtime: CoreRuntime,
    /// Parsed `core.json` discovery payload (includes auth token).
    pub discovery: Discovery,
    /// Core data directory path.
    pub data_dir: PathBuf,
    _temp: TempDir,
    _bypass: PermissionsBypassGuard,
}

impl TestCore {
    /// Start an in-process core with permissions bypass enabled.
    pub async fn start() -> Self {
        let _bypass = PermissionsBypassGuard::enable();
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().to_path_buf();
        let runtime = CoreRuntime::start(CoreConfig::new(data_dir.clone()))
            .await
            .expect("start core");
        let discovery: Discovery = serde_json::from_slice(
            &std::fs::read(data_dir.join("core.json")).expect("read core.json"),
        )
        .expect("parse discovery");
        Self {
            runtime,
            discovery,
            data_dir,
            _temp: temp,
            _bypass,
        }
    }

    /// Local transport endpoint advertised by the core.
    pub fn endpoint(&self) -> &LocalEndpoint {
        self.runtime.endpoint()
    }

    /// Gracefully shut down the core (terminal/session cleanup included).
    pub async fn shutdown(self) {
        self.runtime.shutdown().await.expect("shutdown core");
    }
}

/// One authenticated RPC connection to the core transport.
pub struct RpcClient {
    reader: tokio::io::Lines<BufReader<retcon_platform::TransportReadHalf>>,
    writer: retcon_platform::TransportWriteHalf,
}

impl RpcClient {
    /// Connect and complete the auth + client hello handshake.
    pub async fn connect(endpoint: &LocalEndpoint, token: &str) -> Self {
        let stream = TransportStream::connect(endpoint).await.expect("connect transport");
        let (reader, mut writer) = stream.into_split();
        writer
            .write_all(format!("{{\"auth\":\"{token}\"}}\n").as_bytes())
            .await
            .expect("write auth");
        writer
            .write_all(
                format!(
                    "{{\"kind\":\"client.hello\",\"protocolVersion\":{VERSION},\"clientVersion\":\"integration-test\",\"features\":[\"events.replay\"]}}\n"
                )
                .as_bytes(),
            )
            .await
            .expect("write client hello");
        writer.flush().await.expect("flush hello");
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();
        buf_reader
            .read_line(&mut line)
            .await
            .expect("read server hello");
        assert!(
            line.contains("server.hello"),
            "expected server.hello, got: {line}"
        );
        Self {
            reader: buf_reader.lines(),
            writer,
        }
    }

    /// Send one request and wait for its matching response (skipping push events).
    pub async fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        let request = Request {
            id,
            method: method.to_owned(),
            params,
        };
        let mut encoded = serde_json::to_vec(&request).expect("encode request");
        encoded.push(b'\n');
        self.writer
            .write_all(&encoded)
            .await
            .expect("write request");
        self.writer.flush().await.expect("flush request");
        self.read_response().await
    }

    async fn read_response(&mut self) -> Value {
        while let Ok(Some(line)) = self.reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(&line).expect("json line");
            if value.get("id").is_some() {
                return value;
            }
        }
        panic!("transport closed before RPC response");
    }
}

/// Default shell executable for the current platform.
pub fn default_shell() -> &'static str {
    if cfg!(windows) {
        "cmd.exe"
    } else {
        "/bin/sh"
    }
}

/// Assert an RPC envelope succeeded and return its `result` object.
pub fn assert_ok(response: &Value) -> &Value {
    assert!(
        response.get("result").is_some(),
        "expected ok response, got: {response}"
    );
    response.get("result").expect("result field")
}
