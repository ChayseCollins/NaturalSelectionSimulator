//! The `experiment` command (C-02): one scenario, several run seeds, the same number of
//! ticks each; a row of final metrics per seed and an aggregate summary. Runs are
//! sequential and independent, so each seed's result is exactly what `headless run` with
//! that seed gives.

use std::io::Write;
use std::time::Instant;

use sim::{Simulation, Species, SummaryMetrics};

use crate::cli::ExperimentArgs;
use crate::json::Json;
use crate::run::load_scenario;
use crate::{HeadlessError, format_state_hash, output};

/// Parses `1-5`, `1,2,7` or `3` into a list of seeds.
pub fn parse_seeds(text: &str) -> Result<Vec<u64>, String> {
    let mut seeds = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some((low, high)) = part.split_once('-') {
            let low: u64 = low
                .trim()
                .parse()
                .map_err(|_| format!("bad seed range {part}"))?;
            let high: u64 = high
                .trim()
                .parse()
                .map_err(|_| format!("bad seed range {part}"))?;
            if high < low || high - low >= 10_000 {
                return Err(format!(
                    "seed range {part} must be ascending and at most 10,000 seeds"
                ));
            }
            seeds.extend(low..=high);
        } else {
            seeds.push(part.parse().map_err(|_| format!("bad seed {part}"))?);
        }
    }
    if seeds.is_empty() {
        return Err("no seeds given".to_string());
    }
    Ok(seeds)
}

/// Mean, minimum and maximum of a sample.
fn spread(values: &[f64]) -> Json {
    if values.is_empty() {
        return Json::Null;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Json::Object(vec![
        ("mean", Json::Float(mean)),
        ("min", Json::Float(min)),
        ("max", Json::Float(max)),
        ("runs", Json::uint(values.len() as u64)),
    ])
}

/// Runs the experiment, prints a table, and writes JSON to `--out` when given.
pub fn experiment_command(
    args: &ExperimentArgs,
    out: &mut impl Write,
) -> Result<Json, HeadlessError> {
    let config = load_scenario(args.scenario.as_deref())?;
    let seeds = parse_seeds(&args.seeds).map_err(HeadlessError::Seeds)?;
    let started = Instant::now();
    let print =
        |out: &mut dyn Write, line: String| writeln!(out, "{line}").map_err(HeadlessError::Stdout);
    print(
        out,
        format!(
            "{:>8} {:>8} {:>9} {:>8} {:>9} {:>11} {:>12} {:>10}  state hash",
            "seed",
            "prey",
            "predators",
            "prey gen",
            "births",
            "predations",
            "speed (prey)",
            "pred. extinct"
        ),
    )?;
    let mut finals: Vec<(u64, SummaryMetrics)> = Vec::new();
    for &seed in &seeds {
        let mut s = Simulation::new(&config, seed).map_err(HeadlessError::InvalidConfig)?;
        s.step_n(args.ticks);
        let m = s.summary_metrics();
        print(
            out,
            format!(
                "{:>8} {:>8} {:>9} {:>8} {:>9} {:>11} {:>12} {:>10}  {}",
                seed,
                m.prey.population,
                m.predator.population,
                m.prey.max_generation.map_or("-".into(), |g| g.to_string()),
                m.counters.prey.births,
                m.counters.predations(),
                m.prey.mean_speed.map_or("-".into(), |v| format!("{v:.3}")),
                m.predator_extinct_at.map_or("no".into(), |t| format!(
                    "{:.1} min",
                    t as f64 / f64::from(config.ticks_per_second) / 60.0
                )),
                format_state_hash(m.state_hash)
            ),
        )?;
        finals.push((seed, m));
    }
    let collect = |f: &dyn Fn(&SummaryMetrics) -> Option<f64>| -> Vec<f64> {
        finals.iter().filter_map(|(_, m)| f(m)).collect()
    };
    let runs: Vec<Json> = finals
        .iter()
        .map(|(seed, m)| {
            Json::Object(vec![
                ("seed", Json::uint(*seed)),
                ("prey_population", Json::uint(m.prey.population as u64)),
                (
                    "predator_population",
                    Json::uint(m.predator.population as u64),
                ),
                ("prey_mean_speed", Json::opt_f64(m.prey.mean_speed)),
                ("prey_mean_size", Json::opt_f64(m.prey.mean_size)),
                ("prey_mean_sense", Json::opt_f64(m.prey.mean_sense)),
                (
                    "prey_max_generation",
                    Json::opt_u64(m.prey.max_generation.map(u64::from)),
                ),
                ("prey_births", Json::uint(m.counters.prey.births)),
                ("prey_deaths", Json::uint(m.counters.prey.deaths())),
                ("successful_predations", Json::uint(m.counters.predations())),
                ("prey_extinct_at_tick", Json::opt_u64(m.prey_extinct_at)),
                (
                    "predators_extinct_at_tick",
                    Json::opt_u64(m.predator_extinct_at),
                ),
                (
                    "population_cap_reached",
                    Json::Bool(m.population_cap_reached),
                ),
                ("state_hash", Json::str(format_state_hash(m.state_hash))),
            ])
        })
        .collect();
    let extinct = |species: Species| {
        finals
            .iter()
            .filter(|(_, m)| match species {
                Species::Prey => m.prey_extinct_at.is_some(),
                Species::Predator => m.predator_extinct_at.is_some(),
            })
            .count() as u64
    };
    let summary = Json::Object(vec![
        (
            "prey_population",
            spread(&collect(&|m| Some(m.prey.population as f64))),
        ),
        (
            "predator_population",
            spread(&collect(&|m| Some(m.predator.population as f64))),
        ),
        ("prey_mean_speed", spread(&collect(&|m| m.prey.mean_speed))),
        ("prey_mean_size", spread(&collect(&|m| m.prey.mean_size))),
        ("prey_mean_sense", spread(&collect(&|m| m.prey.mean_sense))),
        (
            "predator_mean_speed",
            spread(&collect(&|m| m.predator.mean_speed)),
        ),
        (
            "prey_max_generation",
            spread(&collect(&|m| m.prey.max_generation.map(f64::from))),
        ),
        (
            "successful_predations",
            spread(&collect(&|m| Some(m.counters.predations() as f64))),
        ),
        ("runs_with_prey_extinct", Json::uint(extinct(Species::Prey))),
        (
            "runs_with_predators_extinct",
            Json::uint(extinct(Species::Predator)),
        ),
    ]);
    let report = Json::Object(vec![
        (
            "tool",
            Json::str("natural-selection-simulator headless experiment"),
        ),
        ("output_version", Json::uint(1)),
        ("scenario", Json::str(config.name.clone())),
        ("ticks", Json::uint(args.ticks)),
        ("runs", Json::Array(runs)),
        ("summary", summary),
        (
            "elapsed_seconds",
            Json::Float(started.elapsed().as_secs_f64()),
        ),
    ]);
    if let Some(path) = &args.out {
        let text = report.to_pretty_string();
        output::write_file_safely(path, args.overwrite, |writer| {
            writer.write_all(text.as_bytes())?;
            writer.flush()?;
            Ok(())
        })?;
        print(out, format!("results written to {}", path.display()))?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_lists_and_ranges_parse() {
        assert_eq!(parse_seeds("1-5"), Ok(vec![1, 2, 3, 4, 5]));
        assert_eq!(parse_seeds("7, 3,9"), Ok(vec![7, 3, 9]));
        assert_eq!(parse_seeds("1-2,10"), Ok(vec![1, 2, 10]));
        assert!(parse_seeds("5-1").is_err());
        assert!(parse_seeds("x").is_err());
        assert!(parse_seeds("").is_err());
        assert!(parse_seeds("0-20000").is_err());
    }

    /// Each run equals `headless run` with the same seed (independent, deterministic runs).
    #[test]
    fn runs_match_single_runs() {
        let args = ExperimentArgs {
            scenario: None,
            seeds: "2-3".to_string(),
            ticks: 60,
            out: None,
            overwrite: false,
        };
        let mut printed = Vec::new();
        let report = experiment_command(&args, &mut printed).expect("experiment");
        let text = report.to_pretty_string();
        let config = load_scenario(None).expect("baseline");
        for seed in [2, 3] {
            let mut s = Simulation::new(&config, seed).expect("valid");
            s.step_n(60);
            assert!(
                text.contains(&format_state_hash(s.state_hash())),
                "seed {seed}"
            );
        }
        assert!(text.contains("\"runs_with_predators_extinct\""));
    }
}
