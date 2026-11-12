//! The `run` command (M-17) with optional JSON results (X4) and CSV sampling (M-21).
//!
//! Every path steps the same `sim::Simulation`; this module only decides when to observe
//! it. Sampling reads summaries between ticks, so a run with CSV or JSON output executes
//! exactly the same ticks, draws exactly the same random numbers, and ends in exactly the
//! same state as a run without them (T-CSV-4, T-CSV-5).

use std::fs;
use std::io::Write;
use std::num::NonZeroU64;
use std::path::Path;
use std::time::{Duration, Instant};

use sim::{ScenarioConfig, Simulation, Species, SummaryMetrics};

use crate::cli::RunArgs;
use crate::csv::{CsvWriter, ExportError};
use crate::json::Json;
use crate::{BASELINE_PRESET, HeadlessError, format_state_hash, output};

/// Result of a run, before anything is printed.
#[derive(Debug)]
pub struct RunOutcome {
    pub simulation: Simulation,
    /// Data rows written to the CSV, if one was requested.
    pub csv_rows: Option<u64>,
    /// Wall-clock time spent stepping (and writing CSV). Reported only; never part of the
    /// simulation results or the CSV.
    pub elapsed: Duration,
}

/// The scenario at `path`, or the built-in baseline preset when there is none.
pub fn load_scenario(path: Option<&Path>) -> Result<ScenarioConfig, HeadlessError> {
    match path {
        None => ScenarioConfig::from_toml_str(BASELINE_PRESET)
            .map_err(|error| HeadlessError::Scenario { path: None, error }),
        Some(path) => {
            let text = fs::read_to_string(path).map_err(|source| HeadlessError::ScenarioRead {
                path: path.to_path_buf(),
                source,
            })?;
            ScenarioConfig::from_toml_str(&text).map_err(|error| HeadlessError::Scenario {
                path: Some(path.to_path_buf()),
                error,
            })
        }
    }
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

/// Runs the command without printing: loads the scenario (`--scenario`, or the built-in
/// baseline preset), runs it, and writes the optional CSV and JSON files.
pub fn execute(args: &RunArgs) -> Result<RunOutcome, HeadlessError> {
    let config = load_scenario(args.scenario.as_deref())?;
    execute_with_config(&config, args)
}

/// [`execute`] with an explicit scenario (`args.scenario` is used only as the label in
/// the JSON output).
pub fn execute_with_config(
    config: &ScenarioConfig,
    args: &RunArgs,
) -> Result<RunOutcome, HeadlessError> {
    if args.overwrite && args.csv.is_none() && args.out.is_none() {
        return Err(HeadlessError::Usage("--overwrite requires --csv or --out"));
    }
    let mut simulation =
        Simulation::new(config, args.seed).map_err(HeadlessError::InvalidConfig)?;
    let started = Instant::now();
    let csv_rows = match (&args.csv, args.sample_every) {
        (Some(path), Some(every)) => {
            Some(output::write_file_safely(path, args.overwrite, |writer| {
                write_csv_run(&mut simulation, args.ticks, every, writer)
            })?)
        }
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
    let elapsed = started.elapsed();
    let outcome = RunOutcome {
        simulation,
        csv_rows,
        elapsed,
    };
    if let Some(path) = &args.out {
        let text = run_json(args, &outcome).to_pretty_string();
        output::write_file_safely(path, args.overwrite, |writer| {
            writer.write_all(text.as_bytes())?;
            writer.flush()?;
            Ok(())
        })?;
    }
    Ok(outcome)
}

fn species_json(summary: &SummaryMetrics, species: Species) -> Json {
    let stats = summary.species(species);
    let counters = summary.counters.species(species);
    let extinct = match species {
        Species::Prey => summary.prey_extinct_at,
        Species::Predator => summary.predator_extinct_at,
    };
    Json::Object(vec![
        ("population", Json::uint(stats.population as u64)),
        ("mean_speed", Json::opt_f64(stats.mean_speed)),
        ("mean_size", Json::opt_f64(stats.mean_size)),
        ("mean_sense", Json::opt_f64(stats.mean_sense)),
        ("mean_energy", Json::opt_f64(stats.mean_energy)),
        ("mean_generation", Json::opt_f64(stats.mean_generation)),
        (
            "max_generation",
            Json::opt_u64(stats.max_generation.map(u64::from)),
        ),
        ("cumulative_births", Json::uint(counters.births)),
        ("cumulative_deaths", Json::uint(counters.deaths())),
        (
            "cumulative_starvation_deaths",
            Json::uint(counters.starvation_deaths),
        ),
        (
            "cumulative_predation_deaths",
            Json::uint(counters.predation_deaths),
        ),
        ("cumulative_age_deaths", Json::uint(counters.age_deaths)),
        ("extinct_at_tick", Json::opt_u64(extinct)),
    ])
}

/// The JSON results (X4): scenario name and format version, seeds, tick count, final
/// metrics per species, state hash and elapsed time. Only `elapsed_seconds` depends on
/// the machine's speed (T-HL-4).
pub fn run_json(args: &RunArgs, outcome: &RunOutcome) -> Json {
    let s = &outcome.simulation;
    let m = s.summary_metrics();
    Json::Object(vec![
        (
            "tool",
            Json::str("natural-selection-simulator headless run"),
        ),
        ("output_version", Json::uint(1)),
        (
            "scenario",
            Json::Object(vec![
                ("name", Json::str(s.config().name.clone())),
                (
                    "format_version",
                    Json::uint(u64::from(s.config().format_version)),
                ),
                (
                    "file",
                    args.scenario
                        .as_ref()
                        .map_or(Json::Null, |p| Json::str(p.display().to_string())),
                ),
            ]),
        ),
        ("run_seed", Json::uint(s.run_seed())),
        ("terrain_seed", Json::uint(s.terrain_seed())),
        ("ticks", Json::uint(m.tick)),
        (
            "ticks_per_second",
            Json::uint(u64::from(s.ticks_per_second())),
        ),
        ("sim_seconds", Json::Float(m.sim_seconds)),
        ("state_hash", Json::str(format_state_hash(m.state_hash))),
        (
            "final",
            Json::Object(vec![
                ("prey", species_json(&m, Species::Prey)),
                ("predator", species_json(&m, Species::Predator)),
                ("successful_predations", Json::uint(m.counters.predations())),
                ("total_food", Json::Float(m.total_food)),
                (
                    "cumulative_food_consumed",
                    Json::Float(m.cumulative_food_consumed),
                ),
                (
                    "population_cap_reached",
                    Json::Bool(m.population_cap_reached),
                ),
            ]),
        ),
        (
            "elapsed_seconds",
            Json::Float(outcome.elapsed.as_secs_f64()),
        ),
    ])
}

/// Runs the command and prints the summary to `out`.
pub fn run_command(args: &RunArgs, out: &mut impl Write) -> Result<RunOutcome, HeadlessError> {
    let outcome = execute(args)?;
    print_summary(out, args, &outcome).map_err(HeadlessError::Stdout)?;
    Ok(outcome)
}

/// Prints the final summary. Averages with no organisms print as `n/a`.
pub fn print_summary(
    out: &mut impl Write,
    args: &RunArgs,
    outcome: &RunOutcome,
) -> std::io::Result<()> {
    let s = &outcome.simulation;
    let m = s.summary_metrics();
    let average = |value: Option<f64>| value.map_or_else(|| "n/a".to_string(), |v| v.to_string());
    let generation =
        |value: Option<u32>| value.map_or_else(|| "n/a".to_string(), |v| v.to_string());
    let extinct = |value: Option<u64>| {
        value.map_or_else(
            || "no".to_string(),
            |t| {
                format!(
                    "yes, at tick {t} ({} s)",
                    t as f64 / f64::from(s.ticks_per_second())
                )
            },
        )
    };
    let scenario = match &args.scenario {
        Some(path) => format!("{} ({})", s.config().name, path.display()),
        None => format!("{} (built-in)", s.config().name),
    };
    let c = &m.counters;
    let lines = [
        ("scenario", scenario),
        ("run seed", args.seed.to_string()),
        ("terrain seed", s.terrain_seed().to_string()),
        (
            "ticks",
            format!("{} requested, {} completed", args.ticks, m.tick),
        ),
        ("simulated seconds", m.sim_seconds.to_string()),
        ("prey population", m.prey.population.to_string()),
        ("average speed", average(m.prey.mean_speed)),
        ("average size", average(m.prey.mean_size)),
        ("average sense range", average(m.prey.mean_sense)),
        ("average energy", average(m.prey.mean_energy)),
        ("prey max generation", generation(m.prey.max_generation)),
        ("predator population", m.predator.population.to_string()),
        ("predator average speed", average(m.predator.mean_speed)),
        ("predator average size", average(m.predator.mean_size)),
        (
            "predator average sense range",
            average(m.predator.mean_sense),
        ),
        (
            "predator max generation",
            generation(m.predator.max_generation),
        ),
        ("total food", m.total_food.to_string()),
        (
            "cumulative food consumed",
            m.cumulative_food_consumed.to_string(),
        ),
        (
            "cumulative starvation deaths",
            c.prey.starvation_deaths.to_string(),
        ),
        ("cumulative prey births", c.prey.births.to_string()),
        ("cumulative predator births", c.predator.births.to_string()),
        ("cumulative predations", c.predations().to_string()),
        (
            "predator starvation deaths",
            c.predator.starvation_deaths.to_string(),
        ),
        ("prey extinct", extinct(m.prey_extinct_at)),
        ("predators extinct", extinct(m.predator_extinct_at)),
        (
            "population cap reached",
            if m.population_cap_reached {
                "yes"
            } else {
                "no"
            }
            .to_string(),
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
    if let Some(path) = &args.out {
        writeln!(out, "{:<30}written to {}", "json:", path.display())?;
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
    use sim::StartPopulation;

    use super::*;

    /// A small scenario so unit tests stay fast: 50 prey and 5 predators on 64 × 64.
    fn small() -> ScenarioConfig {
        let mut config = load_scenario(None).expect("baseline");
        config.world.width = 64.0;
        config.world.height = 64.0;
        config.species.prey.start = StartPopulation::Count(50);
        config.species.predator.start = StartPopulation::Count(5);
        config
    }

    fn args(seed: u64, ticks: u64) -> RunArgs {
        RunArgs {
            scenario: None,
            seed,
            ticks,
            out: None,
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
        config.species.prey.start = StartPopulation::Count(0);
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
            assert_eq!(fields.len(), crate::csv::COLUMNS.len());
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
    fn inconsistent_options_are_rejected_without_running() {
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
        config.species.prey.start = StartPopulation::Count(0);
        let run_args = args(1, 3);
        let outcome = execute_with_config(&config, &run_args).expect("run");
        let mut out = Vec::new();
        print_summary(&mut out, &run_args, &outcome).expect("in-memory print");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(text.contains("ticks:                        3 requested, 3 completed"));
        assert!(text.contains("average speed:                n/a"), "{text}");
        let hash = format_state_hash(outcome.simulation.state_hash());
        assert!(text.contains(&format!("state hash:                   {hash}")));
        assert!(!text.contains("NaN"));
    }

    /// T-HL-1 (library part): the JSON names the scenario, format version, seeds, ticks,
    /// per-species final metrics with births, deaths and predations, the hash and timing.
    #[test]
    fn t_hl_1_json_has_every_required_field() {
        let outcome = execute_with_config(&small(), &args(3, 90)).expect("run");
        let text = run_json(&args(3, 90), &outcome).to_pretty_string();
        for key in [
            "\"name\": \"Baseline predator-prey\"",
            "\"format_version\": 1",
            "\"run_seed\": 3",
            "\"terrain_seed\": 1001",
            "\"ticks\": 90",
            "\"state_hash\": \"0x",
            "\"prey\": {",
            "\"predator\": {",
            "\"cumulative_births\"",
            "\"cumulative_deaths\"",
            "\"successful_predations\"",
            "\"max_generation\"",
            "\"elapsed_seconds\"",
        ] {
            assert!(text.contains(key), "{key} missing:\n{text}");
        }
    }

    #[test]
    fn the_built_in_scenario_is_the_baseline_preset() {
        let config = load_scenario(None).expect("valid");
        assert_eq!(config.name, "Baseline predator-prey");
        assert_eq!(config.terrain.seed, Some(1001));
    }
}
