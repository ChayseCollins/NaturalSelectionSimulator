//! Command-line interface (M-17, M-18, M-21). Parsing only; running is in [`crate::run`]
//! and [`crate::bench`].

use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Natural Selection Simulator command-line runner (no graphics).
#[derive(Debug, Parser)]
#[command(name = "headless", version, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a scenario for a fixed number of ticks and print a summary.
    ///
    /// Without --scenario the built-in baseline classroom preset runs (the same one the
    /// app opens). With --out, also write the final metrics as JSON. With --sample-every
    /// and --csv, also write a CSV time series with a row at tick 0, every sample
    /// interval, and the final tick.
    Run(RunArgs),
    /// Run the benchmark suite (SCOPE 10.4): tiers C, M and G, in frozen-population and
    /// ecological modes, and report tick-time statistics.
    Bench(BenchArgs),
    /// Run one scenario with several seeds and summarize the final metrics (C-02).
    Experiment(ExperimentArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ExperimentArgs {
    /// Scenario file (TOML). Default: the built-in baseline preset.
    #[arg(long, value_name = "PATH")]
    pub scenario: Option<PathBuf>,

    /// Run seeds: a range like 1-5, a list like 1,4,9, or both.
    #[arg(long, value_name = "SEEDS")]
    pub seeds: String,

    /// Ticks per run.
    #[arg(long, value_name = "U64")]
    pub ticks: u64,

    /// Write per-run results and the summary as JSON to this file.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,

    /// Allow --out to replace an existing file.
    #[arg(long)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct RunArgs {
    /// Scenario file (TOML). Default: the built-in baseline preset.
    #[arg(long, value_name = "PATH")]
    pub scenario: Option<PathBuf>,

    /// Run seed; the same scenario, seed and tick count always give the same result.
    #[arg(long, value_name = "U64")]
    pub seed: u64,

    /// Number of fixed ticks to run (30 ticks = 1 simulated second by default). 0 is valid.
    #[arg(long, value_name = "U64")]
    pub ticks: u64,

    /// Write the final metrics, seeds and state hash as JSON to this file.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,

    /// Write a CSV row every this many ticks (at least 1). Requires --csv.
    #[arg(long, value_name = "TICKS", requires = "csv")]
    pub sample_every: Option<NonZeroU64>,

    /// CSV output file. Requires --sample-every. An existing file is never replaced
    /// unless --overwrite is given.
    #[arg(long, value_name = "PATH", requires = "sample_every")]
    pub csv: Option<PathBuf>,

    /// Allow --csv and --out to replace existing files.
    #[arg(long)]
    pub overwrite: bool,
}

/// A benchmark tier (SCOPE 10.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Tier {
    /// b-c: 512 × 512, 2,000 prey, 200 predators.
    C,
    /// b-m: 1024 × 1024, 9,000 prey, 1,000 predators.
    M,
    /// b-g: 2048 × 2048, 45,000 prey, 5,000 predators.
    G,
}

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Self::C => "b-c",
            Self::M => "b-m",
            Self::G => "b-g",
        }
    }

    /// The tier's required speed multiplier (SCOPE 10.4).
    pub fn speed_multiplier(self) -> u32 {
        match self {
            Self::C => 10,
            Self::M | Self::G => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct BenchArgs {
    /// Repetitions per tier and mode; the report gives the median of their medians.
    #[arg(long, value_name = "N", default_value = "3")]
    pub reps: NonZeroU32,

    /// Measured ticks per repetition (SCOPE 10.6: 3,000, P).
    #[arg(long, value_name = "TICKS", default_value_t = 3_000)]
    pub ticks: u64,

    /// Warm-up ticks before measuring (SCOPE 10.6: 300, P).
    #[arg(long, value_name = "TICKS", default_value_t = 300)]
    pub warmup: u64,

    /// Tiers to run, comma-separated.
    #[arg(long, value_enum, value_delimiter = ',', default_values = ["c", "m", "g"])]
    pub tiers: Vec<Tier>,

    /// Write the results as JSON to this file.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,

    /// Allow --out to replace an existing file.
    #[arg(long)]
    pub overwrite: bool,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    use super::*;

    fn parse(args: &[&str]) -> Result<Command, clap::Error> {
        let mut full = vec!["headless"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).map(|cli| cli.command)
    }

    fn parse_run(args: &[&str]) -> Result<RunArgs, clap::Error> {
        parse(args).map(|command| match command {
            Command::Run(run) => run,
            Command::Bench(_) | Command::Experiment(_) => panic!("expected run"),
        })
    }

    #[test]
    fn valid_run_commands_parse() {
        let args = parse_run(&["run", "--seed", "1", "--ticks", "18000"]).expect("valid");
        assert_eq!(
            args,
            RunArgs {
                scenario: None,
                seed: 1,
                ticks: 18_000,
                out: None,
                sample_every: None,
                csv: None,
                overwrite: false
            }
        );
        let args = parse_run(&[
            "run",
            "--scenario",
            "presets/baseline.scenario.toml",
            "--seed",
            "18446744073709551615",
            "--ticks",
            "0",
            "--out",
            "run.json",
            "--sample-every",
            "30",
            "--csv",
            "out.csv",
            "--overwrite",
        ])
        .expect("valid");
        assert_eq!(args.seed, u64::MAX);
        assert_eq!(args.ticks, 0, "zero ticks is valid");
        assert_eq!(
            args.scenario,
            Some(PathBuf::from("presets/baseline.scenario.toml"))
        );
        assert_eq!(args.out, Some(PathBuf::from("run.json")));
        assert_eq!(args.sample_every, NonZeroU64::new(30));
        assert_eq!(args.csv, Some(PathBuf::from("out.csv")));
        assert!(args.overwrite);
    }

    #[test]
    fn missing_required_options_are_rejected() {
        for args in [&["run", "--ticks", "5"][..], &["run", "--seed", "1"]] {
            let error = parse_run(args).expect_err("missing option");
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
            &[
                "run",
                "--seed",
                "1",
                "--ticks",
                "5",
                "--sample-every",
                "0",
                "--csv",
                "x",
            ],
            &["bench", "--reps", "0"],
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

    #[test]
    fn bench_defaults_follow_the_benchmark_procedure() {
        let Command::Bench(args) = parse(&["bench"]).expect("valid") else {
            panic!("expected bench");
        };
        assert_eq!(args.reps.get(), 3);
        assert_eq!((args.warmup, args.ticks), (300, 3_000));
        assert_eq!(args.tiers, [Tier::C, Tier::M, Tier::G]);
        let Command::Bench(args) =
            parse(&["bench", "--tiers", "c,m", "--ticks", "10"]).expect("valid")
        else {
            panic!("expected bench");
        };
        assert_eq!(args.tiers, [Tier::C, Tier::M]);
        assert_eq!(args.ticks, 10);
    }

    #[test]
    fn cli_definition_is_consistent() {
        Cli::command().debug_assert();
    }
}
