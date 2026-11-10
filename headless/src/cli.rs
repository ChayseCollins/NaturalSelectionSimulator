//! Command-line interface (M-17, M-21). Parsing only; running is in [`crate::run`].

use std::num::NonZeroU64;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Natural Selection Simulator command-line runner (no graphics).
#[derive(Debug, Parser)]
#[command(name = "headless", version, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the built-in default scenario for a fixed number of ticks and print a summary.
    ///
    /// With `--sample-every` and `--csv`, also write a CSV time series with a row at tick
    /// 0, every sample interval, and the final tick.
    Run(RunArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct RunArgs {
    /// Run seed; the same seed and tick count always give the same result.
    #[arg(long, value_name = "U64")]
    pub seed: u64,

    /// Number of fixed ticks to run (30 ticks = 1 simulated second by default). 0 is valid.
    #[arg(long, value_name = "U64")]
    pub ticks: u64,

    /// Write a CSV row every this many ticks (at least 1). Requires --csv.
    #[arg(long, value_name = "TICKS", requires = "csv")]
    pub sample_every: Option<NonZeroU64>,

    /// CSV output file. Requires --sample-every. An existing file is never replaced
    /// unless --overwrite is given.
    #[arg(long, value_name = "PATH", requires = "sample_every")]
    pub csv: Option<PathBuf>,

    /// Allow --csv to replace an existing file.
    #[arg(long, requires = "csv")]
    pub overwrite: bool,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    use super::*;

    fn parse(args: &[&str]) -> Result<RunArgs, clap::Error> {
        let mut full = vec!["headless"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).map(|cli| match cli.command {
            Command::Run(run) => run,
        })
    }

    #[test]
    fn valid_run_commands_parse() {
        let args = parse(&["run", "--seed", "1", "--ticks", "18000"]).expect("valid");
        assert_eq!(
            args,
            RunArgs {
                seed: 1,
                ticks: 18_000,
                sample_every: None,
                csv: None,
                overwrite: false
            }
        );
        let args = parse(&[
            "run",
            "--seed",
            "18446744073709551615",
            "--ticks",
            "0",
            "--sample-every",
            "30",
            "--csv",
            "out.csv",
            "--overwrite",
        ])
        .expect("valid");
        assert_eq!(args.seed, u64::MAX);
        assert_eq!(args.ticks, 0, "zero ticks is valid");
        assert_eq!(args.sample_every, NonZeroU64::new(30));
        assert_eq!(args.csv, Some(PathBuf::from("out.csv")));
        assert!(args.overwrite);
    }

    #[test]
    fn missing_required_options_are_rejected() {
        for args in [&["run", "--ticks", "5"][..], &["run", "--seed", "1"]] {
            let error = parse(args).expect_err("missing option");
            assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument, "{args:?}");
        }
    }

    #[test]
    fn invalid_numbers_are_rejected() {
        for args in [
            &["run", "--seed", "-1", "--ticks", "5"][..],
            &["run", "--seed", "abc", "--ticks", "5"],
            &["run", "--seed", "1", "--ticks", "1.5"],
            &["run", "--seed", "18446744073709551616", "--ticks", "5"],
        ] {
            let error = parse(args).expect_err("invalid number");
            assert!(
                matches!(
                    error.kind(),
                    ErrorKind::ValueValidation | ErrorKind::UnknownArgument
                ),
                "{args:?}: {:?}",
                error.kind()
            );
        }
    }

    /// T-CSV-7: a zero sample interval is rejected.
    #[test]
    fn t_csv_7_zero_sample_interval_is_rejected() {
        let error = parse(&[
            "run",
            "--seed",
            "1",
            "--ticks",
            "5",
            "--sample-every",
            "0",
            "--csv",
            "x.csv",
        ])
        .expect_err("zero interval");
        assert_eq!(error.kind(), ErrorKind::ValueValidation);
        assert!(error.to_string().contains("--sample-every"));
    }

    #[test]
    fn csv_options_must_be_used_together() {
        let cases: [&[&str]; 3] = [
            &["run", "--seed", "1", "--ticks", "5", "--sample-every", "30"],
            &["run", "--seed", "1", "--ticks", "5", "--csv", "x.csv"],
            &["run", "--seed", "1", "--ticks", "5", "--overwrite"],
        ];
        for args in cases {
            let error = parse(args).expect_err("incomplete CSV options");
            assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument, "{args:?}");
        }
    }

    #[test]
    fn unknown_commands_and_options_are_rejected() {
        assert_eq!(
            parse(&["experiment"]).expect_err("unknown").kind(),
            ErrorKind::InvalidSubcommand
        );
        assert_eq!(
            parse(&["run", "--seed", "1", "--ticks", "5", "--scenario", "a.toml"])
                .expect_err("not yet supported")
                .kind(),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn help_describes_the_command_and_every_option() {
        Cli::command().debug_assert();
        let mut command = Cli::command();
        let run_help = command
            .find_subcommand_mut("run")
            .expect("run subcommand")
            .render_long_help()
            .to_string();
        for option in [
            "--seed",
            "--ticks",
            "--sample-every",
            "--csv",
            "--overwrite",
            "--help",
        ] {
            assert!(run_help.contains(option), "{option} missing from help");
        }
    }
}
