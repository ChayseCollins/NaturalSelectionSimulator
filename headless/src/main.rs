//! `headless` binary: parses the command line and runs it (M-17, M-18, M-21).
//!
//! Exit status: 0 on success, 1 when a command fails (for example an invalid scenario or a
//! file destination problem), and 2 for command-line usage errors (reported by the parser
//! with usage help).

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use headless::bench::bench_command;
use headless::cli::{Cli, Command};
use headless::experiment::experiment_command;
use headless::run::run_command;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let result = match &cli.command {
        Command::Run(args) => run_command(args, &mut out).map(|_| ()),
        Command::Bench(args) => bench_command(args, &mut out).map(|_| ()),
        Command::Experiment(args) => experiment_command(args, &mut out).map(|_| ()),
    };
    match result.and_then(|()| out.flush().map_err(headless::HeadlessError::Stdout)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
