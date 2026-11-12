//! Per-phase wall-clock timers for benchmarks (M-18, feature `profile`).
//!
//! The only clock reads in `sim`. They time phases for `headless bench` and never feed
//! back into the simulation, so results are identical with and without the feature.

use std::time::Duration;

/// Phase names in `PhaseTimes` index order.
pub const PHASES: [&str; 11] = [
    "spatial grid",
    "sense and decide",
    "route requests",
    "movement",
    "feeding and predation",
    "energy",
    "aging and deaths",
    "reproduction",
    "removals and births",
    "food regrowth",
    "metrics",
];

/// Total time spent in each phase since the simulation was created or last reset.
#[derive(Debug, Clone, Default)]
pub struct PhaseTimes {
    totals: [Duration; 11],
}

/// Timings are not simulation state: two simulations in the same state compare equal
/// whatever their timings.
impl PartialEq for PhaseTimes {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl PhaseTimes {
    pub(crate) fn add(&mut self, phase: usize, elapsed: Duration) {
        self.totals[phase] += elapsed;
    }

    pub fn totals(&self) -> &[Duration; 11] {
        &self.totals
    }

    pub fn reset(&mut self) {
        self.totals = [Duration::ZERO; 11];
    }
}
