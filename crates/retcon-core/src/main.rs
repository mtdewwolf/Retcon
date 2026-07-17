//! Entry point for the `retcon-core` service binary.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use retcon_core::LogFormat;
use retcon_storage::{RecoverAction, Storage};

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

    /// Run an offline storage recovery action and exit (`report`, `backup`, `repair`, `reset`).
    #[arg(long, value_parser = ["report", "backup", "repair", "reset"])]
    storage_recover: Option<String>,

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

    let data_dir = args.data_dir.unwrap_or_else(|| {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Retcon")
    });

    if let Some(action) = args.storage_recover {
        let action = match action.as_str() {
            "report" => RecoverAction::Report,
            "backup" => RecoverAction::Backup,
            "repair" => RecoverAction::Repair,
            "reset" => RecoverAction::Reset,
            _ => RecoverAction::Report,
        };
        match Storage::recover_offline(&data_dir, action, None) {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        }
    } else if args.health {
        tracing::info!(
            version = retcon_core::version(),
            status = "ok",
            "health check"
        );
        ExitCode::SUCCESS
    } else {
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

        match runtime.block_on(retcon_core::run(data_dir)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                tracing::error!(error = %e, "core service exited with error");
                ExitCode::FAILURE
            }
        }
    }
}
