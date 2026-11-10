//! `headless` binary: parses the command line and runs it (M-17, M-21).
//!
//! Exit status: 0 on success, 1 when a run fails (for example a CSV destination problem),
//! and 2 for command-line usage errors (reported by the parser with usage help).

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use headless::cli::{Cli, Command};
use headless::run::run_command;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            match run_command(&args, &mut out) {
                Ok(_) => match out.flush() {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => {
                        eprintln!("error: could not print the summary: {error}");
                        ExitCode::FAILURE
                    }
                },
                Err(error) => {
                    eprintln!("error: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
