//! The `bench` command (M-18, SCOPE 10.4 to 10.6).
//!
//! For each requested tier (the committed `bench/scenarios/b-*.scenario.toml` files,
//! embedded at build time, run seed 1) and each mode (frozen population, then
//! ecological), each repetition builds a fresh simulation, runs the warm-up ticks, then
//! times every measured `step()` with the monotonic clock. Reported per tier and mode:
//! the median of the repetitions' median tick times, and the p95, p99 and maximum over
//! all measured ticks, plus throughput (ticks per second, the real-time factor = ticks per
//! second ÷ 30, and organism-ticks per second), the final population, and whether the
//! SCOPE 10.2 companion rules hold (real-time factor ≥ 2 × the tier's speed; p99 ≤ 3 ×
//! median). Built with `--features profile`, it also reports each phase's share of the
//! measured tick time.

use std::io::Write;
use std::time::{Duration, Instant};

use sim::{ScenarioConfig, Simulation};

use crate::cli::{BenchArgs, Tier};
use crate::json::Json;
use crate::{HeadlessError, output};

const B_C: &str = include_str!("../../bench/scenarios/b-c.scenario.toml");
const B_M: &str = include_str!("../../bench/scenarios/b-m.scenario.toml");
const B_G: &str = include_str!("../../bench/scenarios/b-g.scenario.toml");

/// Run seed of every benchmark (SCOPE 10.4).
pub const BENCH_SEED: u64 = 1;

/// The embedded scenario of a tier.
pub fn tier_scenario(tier: Tier) -> Result<ScenarioConfig, HeadlessError> {
    let text = match tier {
        Tier::C => B_C,
        Tier::M => B_M,
        Tier::G => B_G,
    };
    ScenarioConfig::from_toml_str(text)
        .map_err(|error| HeadlessError::Scenario { path: None, error })
}

/// Timing results of one tier in one mode.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeResult {
    pub tier: Tier,
    pub frozen: bool,
    pub reps: u32,
    pub measured_ticks: u64,
    pub median_of_medians: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub max: Duration,
    pub ticks_per_second: f64,
    pub organisms_start: usize,
    pub organisms_end: usize,
    pub organism_ticks_per_second: f64,
    /// Phase name and share of measured tick time (feature `profile` only).
    pub phase_shares: Vec<(&'static str, f64)>,
}

impl ModeResult {
    pub fn real_time_factor(&self, ticks_per_simulated_second: u32) -> f64 {
        self.ticks_per_second / f64::from(ticks_per_simulated_second)
    }
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// Runs one tier in one mode.
pub fn run_mode(
    tier: Tier,
    config: &ScenarioConfig,
    frozen: bool,
    args: &BenchArgs,
) -> Result<ModeResult, HeadlessError> {
    let mut all = Vec::new();
    let mut medians = Vec::new();
    let mut total_time = Duration::ZERO;
    let mut organism_ticks = 0u64;
    let (mut organisms_start, mut organisms_end) = (0, 0);
    #[allow(unused_mut)]
    let mut phase_totals = vec![Duration::ZERO; 11];
    for _ in 0..args.reps.get() {
        let mut s = Simulation::new(config, BENCH_SEED).map_err(HeadlessError::InvalidConfig)?;
        s.set_frozen_population(frozen);
        s.step_n(args.warmup);
        #[cfg(feature = "profile")]
        s.reset_phase_times();
        organisms_start = s.organisms().len();
        let mut times = Vec::with_capacity(args.ticks as usize);
        for _ in 0..args.ticks {
            organism_ticks += s.organisms().len() as u64;
            let started = Instant::now();
            s.step();
            times.push(started.elapsed());
        }
        organisms_end = s.organisms().len();
        #[cfg(feature = "profile")]
        for (total, phase) in phase_totals.iter_mut().zip(s.phase_times().totals()) {
            *total += *phase;
        }
        total_time += times.iter().sum::<Duration>();
        let mut sorted = times.clone();
        sorted.sort_unstable();
        medians.push(percentile(&sorted, 0.5));
        all.extend(times);
    }
    all.sort_unstable();
    medians.sort_unstable();
    let seconds = total_time.as_secs_f64().max(f64::MIN_POSITIVE);
    let measured = args.ticks * u64::from(args.reps.get());
    let phase_sum: Duration = phase_totals.iter().sum();
    let phase_shares = if phase_sum.is_zero() {
        Vec::new()
    } else {
        phase_names()
            .iter()
            .zip(&phase_totals)
            .map(|(&name, &time)| (name, time.as_secs_f64() / phase_sum.as_secs_f64()))
            .collect()
    };
    Ok(ModeResult {
        tier,
        frozen,
        reps: args.reps.get(),
        measured_ticks: measured,
        median_of_medians: percentile(&medians, 0.5),
        p95: percentile(&all, 0.95),
        p99: percentile(&all, 0.99),
        max: all.last().copied().unwrap_or_default(),
        ticks_per_second: measured as f64 / seconds,
        organisms_start,
        organisms_end,
        organism_ticks_per_second: organism_ticks as f64 / seconds,
        phase_shares,
    })
}

#[cfg(feature = "profile")]
fn phase_names() -> &'static [&'static str] {
    &sim::profile::PHASES
}

#[cfg(not(feature = "profile"))]
fn phase_names() -> &'static [&'static str] {
    &[]
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn mode_json(result: &ModeResult, config: &ScenarioConfig) -> Json {
    let rtf = result.real_time_factor(config.ticks_per_second);
    let required = 2.0 * f64::from(result.tier.speed_multiplier());
    let median = millis(result.median_of_medians);
    let p99 = millis(result.p99);
    Json::Object(vec![
        (
            "mode",
            Json::str(if result.frozen {
                "frozen"
            } else {
                "ecological"
            }),
        ),
        ("median_tick_ms", Json::Float(median)),
        ("p95_tick_ms", Json::Float(millis(result.p95))),
        ("p99_tick_ms", Json::Float(p99)),
        ("max_tick_ms", Json::Float(millis(result.max))),
        ("ticks_per_second", Json::Float(result.ticks_per_second)),
        ("real_time_factor", Json::Float(rtf)),
        (
            "organism_ticks_per_second",
            Json::Float(result.organism_ticks_per_second),
        ),
        (
            "organisms_at_start",
            Json::uint(result.organisms_start as u64),
        ),
        ("organisms_at_end", Json::uint(result.organisms_end as u64)),
        ("rtf_rule_met", Json::Bool(rtf >= required)),
        ("p99_rule_met", Json::Bool(p99 <= 3.0 * median)),
        (
            "phase_shares",
            Json::Object(
                result
                    .phase_shares
                    .iter()
                    .map(|&(name, share)| (name, Json::Float(share)))
                    .collect(),
            ),
        ),
    ])
}

/// Runs the suite, prints a table to `out`, and writes JSON to `--out` when given.
pub fn bench_command(args: &BenchArgs, out: &mut impl Write) -> Result<Json, HeadlessError> {
    let mut tiers = Vec::new();
    writeln!(
        out,
        "{:<5} {:<10} {:>8} {:>8} {:>8} {:>9} {:>10} {:>9}",
        "tier", "mode", "median", "p99", "max", "ticks/s", "RTF (≥)", "organisms"
    )
    .map_err(HeadlessError::Stdout)?;
    for &tier in &args.tiers {
        let config = tier_scenario(tier)?;
        let mut modes = Vec::new();
        for frozen in [true, false] {
            let result = run_mode(tier, &config, frozen, args)?;
            let rtf = result.real_time_factor(config.ticks_per_second);
            writeln!(
                out,
                "{:<5} {:<10} {:>6.2}ms {:>6.2}ms {:>6.2}ms {:>9.0} {:>5.1} ({:>2}) {:>9}",
                tier.name(),
                if frozen { "frozen" } else { "ecological" },
                millis(result.median_of_medians),
                millis(result.p99),
                millis(result.max),
                result.ticks_per_second,
                rtf,
                2 * tier.speed_multiplier(),
                result.organisms_end,
            )
            .map_err(HeadlessError::Stdout)?;
            for (name, share) in &result.phase_shares {
                writeln!(out, "        {name:<24} {:>5.1}%", share * 100.0)
                    .map_err(HeadlessError::Stdout)?;
            }
            modes.push(mode_json(&result, &config));
        }
        tiers.push(Json::Object(vec![
            ("tier", Json::str(tier.name())),
            ("scenario", Json::str(config.name.clone())),
            ("world_size", Json::Float(f64::from(config.world.width))),
            (
                "speed_multiplier",
                Json::uint(u64::from(tier.speed_multiplier())),
            ),
            ("modes", Json::Array(modes)),
        ]));
    }
    let report = Json::Object(vec![
        (
            "tool",
            Json::str("natural-selection-simulator headless bench"),
        ),
        ("output_version", Json::uint(1)),
        ("run_seed", Json::uint(BENCH_SEED)),
        ("reps", Json::uint(u64::from(args.reps.get()))),
        ("warmup_ticks", Json::uint(args.warmup)),
        ("measured_ticks", Json::uint(args.ticks)),
        (
            "build",
            Json::str(if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }),
        ),
        ("profile_feature", Json::Bool(cfg!(feature = "profile"))),
        ("os", Json::str(std::env::consts::OS)),
        ("arch", Json::str(std::env::consts::ARCH)),
        (
            "logical_cpus",
            Json::uint(std::thread::available_parallelism().map_or(0, |n| n.get() as u64)),
        ),
        ("tiers", Json::Array(tiers)),
    ]);
    if let Some(path) = &args.out {
        let text = report.to_pretty_string();
        output::write_file_safely(path, args.overwrite, |writer| {
            writer.write_all(text.as_bytes())?;
            writer.flush()?;
            Ok(())
        })?;
        writeln!(out, "results written to {}", path.display()).map_err(HeadlessError::Stdout)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn tier_scenarios_parse() {
        for tier in [Tier::C, Tier::M, Tier::G] {
            let config = tier_scenario(tier).expect("valid");
            assert!(config.name.contains("Benchmark"));
        }
    }

    #[test]
    fn a_tiny_bench_reports_every_field() {
        let args = BenchArgs {
            reps: NonZeroU32::new(1).expect("non-zero"),
            ticks: 3,
            warmup: 1,
            tiers: vec![Tier::C],
            out: None,
            overwrite: false,
        };
        let mut printed = Vec::new();
        let report = bench_command(&args, &mut printed).expect("bench");
        let text = report.to_pretty_string();
        for key in [
            "\"tier\": \"b-c\"",
            "\"mode\": \"frozen\"",
            "\"mode\": \"ecological\"",
            "\"median_tick_ms\"",
            "\"p99_tick_ms\"",
            "\"real_time_factor\"",
            "\"organisms_at_end\"",
        ] {
            assert!(text.contains(key), "{key} missing:\n{text}");
        }
        let table = String::from_utf8(printed).expect("utf-8");
        assert!(table.contains("b-c"));
    }

    #[test]
    fn percentiles_pick_ranked_values() {
        let values: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&values, 0.5), Duration::from_millis(51));
        assert_eq!(percentile(&values, 0.99), Duration::from_millis(99));
        assert_eq!(percentile(&[], 0.5), Duration::ZERO);
    }
}
