//! The `retcon` command-line interface.
//!
//! Phase 1 scope: argument-parsing skeleton. Real commands (status, sessions,
//! approvals, headless control) land with the headless core work in Phase 42;
//! until then this exists so the workspace has a second binary exercising the
//! shared toolchain and CI.

use clap::{Parser, Subcommand};

/// Supervise and verify AI coding agents from the command line.
#[derive(Parser, Debug)]
#[command(name = "retcon", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Show the status of the local Retcon core service.
    Status,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Status) => {
            println!("retcon-core: not running (service control arrives in a later phase)");
        }
        None => {
            println!(
                "retcon {} — run `retcon --help` for commands",
                env!("CARGO_PKG_VERSION")
            );
        }
    }
}
