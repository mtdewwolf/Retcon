//! Entry point for the `retcon-core` service binary.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use retcon_core::LogFormat;

/// The Retcon core service.
#[derive(Parser, Debug)]
#[command(name = "retcon-core", version, about)]
struct Args {
    /// Log output format.
    #[arg(long, value_parser = ["pretty", "json"], default_value = "pretty")]
    log_format: String,

    /// Initialize, report health, and exit immediately (used by CI and diagnostics).
    #[arg(long)]
    health: bool,

    /// Directory used for the process lock and local discovery file.
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let format = if args.log_format == "json" {
        LogFormat::Json
    } else {
        LogFormat::Pretty
    };

    if let Err(e) = retcon_core::init_logging(format) {
        eprintln!("retcon-core: failed to initialize logging: {e}");
        return ExitCode::FAILURE;
    }
    retcon_core::install_panic_hook();

    if args.health {
        tracing::info!(
            version = retcon_core::version(),
            status = "ok",
            "health check"
        );
        return ExitCode::SUCCESS;
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "failed to start async runtime");
            return ExitCode::FAILURE;
        }
    };

    let data_dir = args.data_dir.unwrap_or_else(|| {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Retcon")
    });

    match runtime.block_on(retcon_core::run(data_dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "core service exited with error");
            ExitCode::FAILURE
        }
    }
}
