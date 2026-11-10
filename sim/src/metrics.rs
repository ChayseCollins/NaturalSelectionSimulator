//! Read-only summary measurements of the current state (M-21; the sampled time series
//! for charts is M-13).
//!
//! Computing a summary never changes the simulation and draws no random numbers. Sums
//! run in storage order with `f64` accumulation, so they are deterministic and finite for
//! any valid state.

/// One observation of a simulation, taken between ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SummaryMetrics {
    /// Ticks completed.
    pub tick: u64,
    /// `tick / ticks_per_second`, for display.
    pub sim_seconds: f64,
    pub prey_population: usize,
    /// Mean speed trait of living prey; `None` when there are none.
    pub avg_speed: Option<f64>,
    /// Mean size trait of living prey; `None` when there are none.
    pub avg_size: Option<f64>,
    /// Mean sense-range trait of living prey; `None` when there are none.
    pub avg_sense: Option<f64>,
    /// Mean energy of living prey; `None` when there are none.
    pub avg_energy: Option<f64>,
    /// Food currently held by all cells.
    pub total_food: f64,
    /// Food removed from cells by feeding since tick 0, in food units (not energy).
    pub cumulative_food_consumed: f64,
    pub cumulative_starvation_deaths: u64,
    /// [`crate::Simulation::state_hash`] at this tick.
    pub state_hash: u64,
}

/// Sum in storage order, accumulated in `f64`.
pub(crate) fn sum(values: &[f32]) -> f64 {
    let mut total = 0.0_f64;
    for &value in values {
        total += f64::from(value);
    }
    total
}

/// Mean in storage order, or `None` for no values.
pub(crate) fn mean(values: &[f32]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(sum(values) / values.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_and_mean_accumulate_in_f64() {
        assert_eq!(sum(&[]), 0.0);
        assert_eq!(sum(&[0.5, 0.25, 1.0]), 1.75);
        assert_eq!(mean(&[]), None);
        assert_eq!(mean(&[1.0, 2.0, 4.5]), Some(2.5));
        // 2^24 + 1 is not representable in f32 but the f64 sum keeps it.
        assert_eq!(sum(&[16_777_216.0, 1.0]), 16_777_217.0);
    }
}
