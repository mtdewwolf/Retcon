//! Runtime seam for the separately packaged development-server process manager.

use retcon_storage::{DevServerInstance, DevServerLaunchConfig};
use serde_json::Value;

/// Process metadata returned after a development server starts.
#[derive(Clone, Debug)]
pub struct DevServerStarted {
    /// Operating-system process identifier, when supplied by the runtime.
    pub pid: Option<i64>,
    /// Preview URL bound by the server.
    pub url: String,
    /// Runtime-specific preview metadata safe to expose to clients.
    pub preview: Value,
}

/// Runtime boundary used by durable RPC lifecycle orchestration.
pub trait DevServerRuntime: Send + Sync {
    /// Start the snapshotted configuration for a prepared durable instance.
    fn start(
        &self,
        config: &DevServerLaunchConfig,
        instance: &DevServerInstance,
    ) -> Result<DevServerStarted, String>;
    /// Stop the process and return its final bounded log bytes.
    fn stop(&self, instance: &DevServerInstance) -> Result<Vec<u8>, String>;
    /// Return the current log snapshot.
    fn logs(&self, instance: &DevServerInstance) -> Result<Vec<u8>, String>;
}

/// Inert runtime used until the standalone process runtime is injected.
#[derive(Default)]
pub struct NoopDevServerRuntime;

impl DevServerRuntime for NoopDevServerRuntime {
    fn start(
        &self,
        _config: &DevServerLaunchConfig,
        _instance: &DevServerInstance,
    ) -> Result<DevServerStarted, String> {
        Err("development server runtime is not installed".into())
    }
    fn stop(&self, _instance: &DevServerInstance) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
    fn logs(&self, _instance: &DevServerInstance) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
}
