//! Read-only measurements of the simulation (M-13, M-17, M-21).
//!
//! Computing a summary never changes the simulation and draws no random numbers. Sums run
//! in storage order with `f64` accumulation, so they are deterministic and finite for any
//! valid state. A species with no living organisms has no means (`None`): charts show a
//! gap, never 0 or NaN (T-CHART-2).
//!
//! The chart history is a fixed struct of per-species series (SCOPE 7.2), sampled every
//! `metrics.interval` ticks. It keeps at most [`limits::METRICS_HISTORY_CAP`] samples:
//! when full, every other sample is dropped (keeping the first and the latest) and the
//! sampling interval doubles (T-CHART-3).

use crate::limits;
use crate::organisms::{Organisms, Species};

/// One observation of a simulation, taken between ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SummaryMetrics {
    /// Ticks completed.
    pub tick: u64,
    /// `tick / ticks_per_second`, for display.
    pub sim_seconds: f64,
    pub prey: SpeciesStats,
    pub predator: SpeciesStats,
    /// Food currently held by all cells.
    pub total_food: f64,
    /// Food removed from cells by feeding since tick 0, in food units (not energy).
    pub cumulative_food_consumed: f64,
    /// Cumulative births and deaths by cause, per species.
    pub counters: Counters,
    /// Tick at which each species reached zero, if it has.
    pub prey_extinct_at: Option<u64>,
    pub predator_extinct_at: Option<u64>,
    /// The population safety cap has suppressed births at least once.
    pub population_cap_reached: bool,
    /// [`crate::Simulation::state_hash`] at this tick.
    pub state_hash: u64,
}

impl SummaryMetrics {
    pub fn species(&self, species: Species) -> &SpeciesStats {
        match species {
            Species::Prey => &self.prey,
            Species::Predator => &self.predator,
        }
    }
}

/// Statistics of one species' living organisms at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpeciesStats {
    pub population: usize,
    pub mean_speed: Option<f64>,
    pub mean_size: Option<f64>,
    pub mean_sense: Option<f64>,
    pub mean_energy: Option<f64>,
    pub mean_generation: Option<f64>,
    pub max_generation: Option<u32>,
}

impl SpeciesStats {
    /// Computes the statistics of `species` in storage order.
    pub(crate) fn of(organisms: &Organisms, species: Species) -> Self {
        let mut count = 0usize;
        let (mut speed, mut size, mut sense, mut energy, mut generation) =
            (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        let mut max_generation: Option<u32> = None;
        for i in 0..organisms.len() {
            if organisms.species[i] != species {
                continue;
            }
            count += 1;
            speed += f64::from(organisms.speed[i]);
            size += f64::from(organisms.size[i]);
            sense += f64::from(organisms.sense[i]);
            energy += f64::from(organisms.energy[i]);
            generation += f64::from(organisms.generation[i]);
            let g = organisms.generation[i];
            max_generation = Some(max_generation.map_or(g, |m| m.max(g)));
        }
        let mean = |total: f64| (count > 0).then(|| total / count as f64);
        Self {
            population: count,
            mean_speed: mean(speed),
            mean_size: mean(size),
            mean_sense: mean(sense),
            mean_energy: mean(energy),
            mean_generation: mean(generation),
            max_generation,
        }
    }
}

/// Cumulative events of one species since tick 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpeciesCounters {
    pub births: u64,
    pub starvation_deaths: u64,
    /// Organisms of this species eaten by predators.
    pub predation_deaths: u64,
    /// Deaths from old age (S-02).
    pub age_deaths: u64,
}

impl SpeciesCounters {
    pub fn deaths(&self) -> u64 {
        self.starvation_deaths + self.predation_deaths + self.age_deaths
    }
}

/// Cumulative events of both species.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    pub prey: SpeciesCounters,
    pub predator: SpeciesCounters,
}

impl Counters {
    pub fn species(&self, species: Species) -> &SpeciesCounters {
        match species {
            Species::Prey => &self.prey,
            Species::Predator => &self.predator,
        }
    }

    pub(crate) fn species_mut(&mut self, species: Species) -> &mut SpeciesCounters {
        match species {
            Species::Prey => &mut self.prey,
            Species::Predator => &mut self.predator,
        }
    }

    /// Successful predations: prey eaten by predators.
    pub fn predations(&self) -> u64 {
        self.prey.predation_deaths
    }
}

/// One chart sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricSample {
    pub tick: u64,
    pub sim_seconds: f64,
    pub prey: SpeciesStats,
    pub predator: SpeciesStats,
}

impl MetricSample {
    pub fn species(&self, species: Species) -> &SpeciesStats {
        match species {
            Species::Prey => &self.prey,
            Species::Predator => &self.predator,
        }
    }
}

/// Sampled time series for the live charts (M-13).
#[derive(Debug, Clone, PartialEq)]
pub struct MetricsHistory {
    interval: u64,
    /// Current spacing in units of `interval`; doubles each time the history is thinned.
    stride: u64,
    samples: Vec<MetricSample>,
    cap: usize,
}

impl MetricsHistory {
    pub(crate) fn new(interval: u32) -> Self {
        Self::with_cap(interval, limits::METRICS_HISTORY_CAP)
    }

    pub(crate) fn with_cap(interval: u32, cap: usize) -> Self {
        Self {
            interval: u64::from(interval.max(1)),
            stride: 1,
            samples: Vec::new(),
            cap: cap.max(2),
        }
    }

    /// Whether a sample is due at `tick`.
    pub(crate) fn due(&self, tick: u64) -> bool {
        tick.is_multiple_of(self.interval * self.stride)
    }

    pub(crate) fn push(&mut self, sample: MetricSample) {
        self.samples.push(sample);
        if self.samples.len() > self.cap {
            let latest = *self.samples.last().expect("non-empty");
            let mut kept: Vec<MetricSample> = self.samples.iter().step_by(2).copied().collect();
            if kept.last().map(|s| s.tick) != Some(latest.tick) {
                kept.push(latest);
            }
            self.samples = kept;
            self.stride *= 2;
        }
    }

    /// Samples in tick order.
    pub fn samples(&self) -> &[MetricSample] {
        &self.samples
    }

    /// Ticks between samples at the current resolution.
    pub fn spacing(&self) -> u64 {
        self.interval * self.stride
    }
}

/// Sum in storage order, accumulated in `f64`.
pub(crate) fn sum(values: &[f32]) -> f64 {
    let mut total = 0.0_f64;
    for &value in values {
        total += f64::from(value);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(tick: u64) -> MetricSample {
        MetricSample {
            tick,
            sim_seconds: tick as f64 / 30.0,
            prey: SpeciesStats::default(),
            predator: SpeciesStats::default(),
        }
    }

    #[test]
    fn sum_accumulates_in_f64() {
        assert_eq!(sum(&[]), 0.0);
        assert_eq!(sum(&[0.5, 0.25, 1.0]), 1.75);
        // 2^24 + 1 is not representable in f32 but the f64 sum keeps it.
        assert_eq!(sum(&[16_777_216.0, 1.0]), 16_777_217.0);
    }

    /// T-CHART-3: decimation keeps the first and latest samples and never exceeds the cap.
    #[test]
    fn t_chart_3_history_is_thinned_and_keeps_both_ends() {
        let mut history = MetricsHistory::with_cap(30, 16);
        let mut tick = 0;
        let mut pushed = 0;
        while pushed < 200 {
            if history.due(tick) {
                history.push(sample(tick));
                pushed += 1;
                assert!(history.samples().len() <= 16);
                assert_eq!(history.samples()[0].tick, 0, "first sample kept");
                assert_eq!(
                    history.samples().last().map(|s| s.tick),
                    Some(tick),
                    "latest kept"
                );
            }
            tick += 30;
        }
        let ticks: Vec<u64> = history.samples().iter().map(|s| s.tick).collect();
        assert!(
            ticks.windows(2).all(|w| w[0] < w[1]),
            "ascending: {ticks:?}"
        );
        assert!(history.spacing() > 30, "resolution coarsened");
    }

    /// T-CHART-2: a species with no organisms records no mean (a gap), never 0 or NaN.
    #[test]
    fn t_chart_2_empty_species_has_no_means() {
        let stats = SpeciesStats::of(&Organisms::default(), Species::Predator);
        assert_eq!(stats.population, 0);
        assert_eq!(stats.mean_speed, None);
        assert_eq!(stats.mean_generation, None);
        assert_eq!(stats.max_generation, None);
    }
}
