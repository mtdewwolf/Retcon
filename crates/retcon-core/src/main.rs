use std::path::PathBuf;

use retcon_core::{CoreConfig, CoreRuntime};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("retcon_core=info")),
        )
        .init();

    let data_dir = data_dir();
    let runtime = CoreRuntime::start(CoreConfig::new(data_dir)).await?;
    tracing::info!(address = %runtime.address(), "Retcon core is ready");

    runtime.wait_for_shutdown_signal().await?;
    runtime.shutdown().await?;
    Ok(())
}

fn data_dir() -> PathBuf {
    if let Some(path) = std::env::args_os().skip(1).find_map(|arg| {
        arg.to_string_lossy()
            .strip_prefix("--data-dir=")
            .map(PathBuf::from)
    }) {
        return path;
    }

    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Retcon")
}
