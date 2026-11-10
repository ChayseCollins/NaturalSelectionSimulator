//! The `run` command (M-17) with optional CSV sampling (M-21).
//!
//! Every path steps the same `sim::Simulation`; this module only decides when to observe
//! it. Sampling reads summaries between ticks, so a run with CSV output executes exactly
//! the same ticks, draws exactly the same random numbers, and ends in exactly the same
//! state as a run without it (T-CSV-4, T-CSV-5).

use std::io::Write;
use std::num::NonZeroU64;
use std::path::Path;
use std::time::{Duration, Instant};

use sim::{ScenarioConfig, Simulation, SummaryMetrics};

use crate::cli::RunArgs;
use crate::csv::{CsvWriter, ExportError};
use crate::{HeadlessError, format_state_hash, output};

/// Result of a run, before anything is printed.
#[derive(Debug)]
pub struct RunOutcome {
    pub simulation: Simulation,
    /// Data rows written to the CSV, if one was requested.
    pub csv_rows: Option<u64>,
    /// Wall-clock time spent stepping (and writing CSV). Reported only; never part of the
    /// results or the CSV.
    pub elapsed: Duration,
}

/// Steps `simulation` until tick `end`, calling `observe` with a summary at the starting
/// tick, at every later tick that is a multiple of `sample_every`, and at `end` when it is
/// not a multiple. Observations only read state.
pub fn run_sampled<E>(
    simulation: &mut Simulation,
    end: u64,
    sample_every: NonZeroU64,
    mut observe: impl FnMut(&SummaryMetrics) -> Result<(), E>,
) -> Result<(), E> {
    let every = sample_every.get();
    observe(&simulation.summary_metrics())?;
    while simulation.tick() < end {
        let next_multiple = (simulation.tick() / every + 1).saturating_mul(every);
        simulation.step_n(next_multiple.min(end) - simulation.tick());
        observe(&simulation.summary_metrics())?;
    }
    Ok(())
}

/// Runs `simulation` to tick `end`, writing the CSV time series to `writer`. Returns the
/// number of data rows.
pub fn write_csv_run<W: Write>(
    simulation: &mut Simulation,
    end: u64,
    sample_every: NonZeroU64,
    writer: W,
) -> Result<u64, ExportError> {
    let mut csv = CsvWriter::new(writer)?;
    run_sampled(simulation, end, sample_every, |metrics| {
        csv.write_row(metrics)
    })?;
    csv.finish()
}

/// Runs the command with the built-in default scenario, without printing. This is the
/// path the CLI uses.
pub fn execute(args: &RunArgs) -> Result<RunOutcome, HeadlessError> {
    execute_with_config(&ScenarioConfig::default(), args)
}

/// [`execute`] with an explicit scenario, for tests. The CLI always passes the default.
pub fn execute_with_config(
    config: &ScenarioConfig,
    args: &RunArgs,
) -> Result<RunOutcome, HeadlessError> {
    if args.overwrite && args.csv.is_none() {
        return Err(HeadlessError::Usage("--overwrite requires --csv"));
    }
    let mut simulation =
        Simulation::new(config, args.seed).map_err(HeadlessError::InvalidConfig)?;
    let started = Instant::now();
    let csv_rows = match (&args.csv, args.sample_every) {
        (Some(path), Some(every)) => Some(write_csv_file(
            &mut simulation,
            args.ticks,
            every,
            path,
            args.overwrite,
        )?),
        (None, None) => {
            simulation.step_n(args.ticks);
            None
        }
        _ => {
            return Err(HeadlessError::Usage(
                "--sample-every and --csv must be used together",
            ));
        }
    };
    Ok(RunOutcome {
        simulation,
        csv_rows,
        elapsed: started.elapsed(),
    })
}

fn write_csv_file(
    simulation: &mut Simulation,
    end: u64,
    sample_every: NonZeroU64,
    path: &Path,
    overwrite: bool,
) -> Result<u64, HeadlessError> {
    output::write_file_safely(path, overwrite, |writer| {
        write_csv_run(simulation, end, sample_every, writer)
    })
}

/// Runs the command and prints the summary to `out`.
pub fn run_command(args: &RunArgs, out: &mut impl Write) -> Result<RunOutcome, HeadlessError> {
    let outcome = execute(args)?;
    print_summary(out, args, &outcome).map_err(HeadlessError::Stdout)?;
    Ok(outcome)
}

/// Prints the final summary. Averages with no prey print as `n/a`.
pub fn print_summary(
    out: &mut impl Write,
    args: &RunArgs,
    outcome: &RunOutcome,
) -> std::io::Result<()> {
    let m = outcome.simulation.summary_metrics();
    let average = |value: Option<f64>| value.map_or_else(|| "n/a".to_string(), |v| v.to_string());
    let lines = [
        ("scenario", "built-in default".to_string()),
        ("run seed", args.seed.to_string()),
        (
            "ticks",
            format!("{} requested, {} completed", args.ticks, m.tick),
        ),
        ("simulated seconds", m.sim_seconds.to_string()),
        ("prey population", m.prey_population.to_string()),
        ("average speed", average(m.avg_speed)),
        ("average size", average(m.avg_size)),
        ("average sense range", average(m.avg_sense)),
        ("average energy", average(m.avg_energy)),
        ("total food", m.total_food.to_string()),
        (
            "cumulative food consumed",
            m.cumulative_food_consumed.to_string(),
        ),
        (
            "cumulative starvation deaths",
            m.cumulative_starvation_deaths.to_string(),
        ),
        ("state hash", format_state_hash(m.state_hash)),
    ];
    for (label, value) in lines {
        writeln!(out, "{:<30}{value}", format!("{label}:"))?;
    }
    if let (Some(path), Some(rows)) = (&args.csv, outcome.csv_rows) {
        writeln!(
            out,
            "{:<30}{rows} rows written to {}",
            "csv:",
            path.display()
        )?;
    }
    writeln!(
        out,
        "{:<30}{:.3} s (not part of the results)",
        "elapsed (wall clock):",
        outcome.elapsed.as_secs_f64()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small scenario so unit tests stay fast: 50 prey on a 64 × 64 world.
    fn small() -> ScenarioConfig {
        let mut config = ScenarioConfig::default();
        config.world.width = 64.0;
        config.world.height = 64.0;
        config.prey.start_count = 50;
        config
    }

    fn args(seed: u64, ticks: u64) -> RunArgs {
        RunArgs {
            seed,
            ticks,
            sample_every: None,
            csv: None,
            overwrite: false,
        }
    }

    fn sampled_ticks(end: u64, every: u64) -> Vec<u64> {
        let mut simulation = Simulation::new(&small(), 1).expect("valid");
        let mut ticks = Vec::new();
        run_sampled(
            &mut simulation,
            end,
            NonZeroU64::new(every).expect("non-zero"),
            |m| {
                ticks.push(m.tick);
                Ok::<(), ()>(())
            },
        )
        .expect("infallible observer");
        assert_eq!(simulation.tick(), end, "exactly the requested ticks");
        ticks
    }

    /// T-CSV-1 and T-CSV-2 sampling rules, and the zero-tick case.
    #[test]
    fn samples_start_every_interval_and_final_tick_once() {
        let regular: Vec<u64> = (0..=10).map(|k| k * 30).collect();
        assert_eq!(sampled_ticks(300, 30), regular, "11 rows, no duplicate 300");
        let mut with_final = regular.clone();
        with_final.push(305);
        assert_eq!(sampled_ticks(305, 30), with_final);
        assert_eq!(sampled_ticks(0, 30), [0], "zero ticks: tick 0 only");
        assert_eq!(sampled_ticks(7, 10), [0, 7], "interval longer than the run");
        assert_eq!(sampled_ticks(3, 1), [0, 1, 2, 3]);
    }

    /// T-CSV-4 and T-CSV-5: sampling and CSV writing leave the run exactly as it would be
    /// without them, including every random-domain position (compared through `==`).
    #[test]
    fn t_csv_4_and_5_sampling_does_not_change_the_run() {
        let mut direct = Simulation::new(&small(), 9).expect("valid");
        direct.step_n(305);
        let mut sampled = Simulation::new(&small(), 9).expect("valid");
        let mut out = Vec::new();
        let rows = write_csv_run(
            &mut sampled,
            305,
            NonZeroU64::new(30).expect("non-zero"),
            &mut out,
        )
        .expect("in-memory write");
        assert_eq!(rows, 12);
        assert_eq!(sampled, direct);
        assert_eq!(sampled.state_hash(), direct.state_hash());
    }

    /// T-CSV-3 (library level): identical runs give byte-identical CSV.
    #[test]
    fn t_csv_3_identical_runs_give_identical_bytes() {
        let write = |seed: u64| {
            let mut simulation = Simulation::new(&small(), seed).expect("valid");
            let mut out = Vec::new();
            write_csv_run(
                &mut simulation,
                200,
                NonZeroU64::new(30).expect("non-zero"),
                &mut out,
            )
            .expect("in-memory write");
            out
        };
        assert_eq!(write(4), write(4));
        assert_ne!(write(4), write(5), "a different seed changes the data");
    }

    /// T-CSV-6 (run level): with no prey the averages are empty fields throughout.
    #[test]
    fn t_csv_6_empty_population_rows_have_empty_averages() {
        let mut config = small();
        config.prey.start_count = 0;
        let mut simulation = Simulation::new(&config, 1).expect("valid");
        let mut out = Vec::new();
        write_csv_run(
            &mut simulation,
            60,
            NonZeroU64::new(30).expect("non-zero"),
            &mut out,
        )
        .expect("in-memory write");
        let text = String::from_utf8(out).expect("utf-8");
        for row in text.lines().skip(1) {
            let fields: Vec<&str> = row.split(',').collect();
            assert_eq!(fields.len(), 11);
            assert_eq!(fields[2], "0");
            assert_eq!(&fields[3..7], ["", "", "", ""], "{row}");
        }
        assert!(!text.contains("NaN") && !text.contains("inf"));
    }

    #[test]
    fn execute_runs_exactly_the_requested_ticks() {
        for ticks in [0, 1, 37] {
            let outcome = execute_with_config(&small(), &args(1, ticks)).expect("run");
            assert_eq!(outcome.simulation.tick(), ticks);
            assert_eq!(outcome.csv_rows, None);
        }
    }

    #[test]
    fn same_seed_same_hash_and_different_seeds_differ_in_real_state() {
        let a = execute_with_config(&small(), &args(1, 120)).expect("run");
        let b = execute_with_config(&small(), &args(1, 120)).expect("run");
        let c = execute_with_config(&small(), &args(2, 120)).expect("run");
        assert_eq!(a.simulation.state_hash(), b.simulation.state_hash());
        assert_ne!(a.simulation.organisms(), c.simulation.organisms());
        assert_ne!(a.simulation.state_hash(), c.simulation.state_hash());
    }

    #[test]
    fn inconsistent_csv_options_are_rejected_without_running() {
        let mut overwrite_only = args(1, 5);
        overwrite_only.overwrite = true;
        assert!(matches!(
            execute_with_config(&small(), &overwrite_only),
            Err(HeadlessError::Usage(_))
        ));
        let mut interval_only = args(1, 5);
        interval_only.sample_every = NonZeroU64::new(30);
        assert!(matches!(
            execute_with_config(&small(), &interval_only),
            Err(HeadlessError::Usage(_))
        ));
    }

    #[test]
    fn summary_prints_unavailable_averages_and_fixed_width_hash() {
        let mut config = small();
        config.prey.start_count = 0;
        let run_args = args(1, 3);
        let outcome = execute_with_config(&config, &run_args).expect("run");
        let mut out = Vec::new();
        print_summary(&mut out, &run_args, &outcome).expect("in-memory print");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(text.contains("ticks:                        3 requested, 3 completed"));
        assert!(text.contains("average speed:                n/a"), "{text}");
        assert!(text.contains("average energy:               n/a"));
        let hash = format_state_hash(outcome.simulation.state_hash());
        assert!(text.contains(&format!("state hash:                   {hash}")));
        assert!(!text.contains("NaN"));
    }
}
