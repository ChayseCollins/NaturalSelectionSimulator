//! The three heritable traits and the capabilities they give (SCOPE 6.2, 7.2).
//!
//! Units are abstract (SCOPE 5): speed in world units per simulated second, size in
//! abstract size units, sense range in world units. This is a qualitative model, not a
//! calibrated biological one.

use crate::limits::{self, FloatLimits};

/// Identifies one trait, so mutation, charts and the inspector can loop over traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitId {
    Speed,
    Size,
    Sense,
}

impl TraitId {
    pub const ALL: [TraitId; 3] = [TraitId::Speed, TraitId::Size, TraitId::Sense];

    pub fn name(self) -> &'static str {
        match self {
            TraitId::Speed => "speed",
            TraitId::Size => "size",
            TraitId::Sense => "sense",
        }
    }

    /// Valid range of the trait value (P).
    pub fn range(self) -> FloatLimits {
        match self {
            TraitId::Speed => limits::SPEED,
            TraitId::Size => limits::SIZE,
            TraitId::Sense => limits::SENSE,
        }
    }

    /// Valid range of the trait's starting spread (P).
    pub fn spread_range(self) -> FloatLimits {
        match self {
            TraitId::Speed => limits::SPEED_SPREAD,
            TraitId::Size => limits::SIZE_SPREAD,
            TraitId::Sense => limits::SENSE_SPREAD,
        }
    }
}

/// One organism's trait values, copied out of the organism columns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Traits {
    pub speed: f32,
    pub size: f32,
    pub sense: f32,
}

impl Traits {
    pub fn get(&self, id: TraitId) -> f32 {
        match id {
            TraitId::Speed => self.speed,
            TraitId::Size => self.size,
            TraitId::Sense => self.sense,
        }
    }
}

/// Largest distance an organism may move in one tick on terrain with speed multiplier 1
/// (T-CAP-1): `speed / ticks_per_second`. Terrain multipliers arrive with M-06.
pub fn max_distance_per_tick(speed: f32, ticks_per_second: u32) -> f32 {
    speed / ticks_per_second as f32
}

/// Whether an object at offset `(dx, dy)` is within sense range (T-CAP-1). Sensing is
/// omnidirectional and ignores barriers (SCOPE 6.8, W-10). Compares squared distances.
pub fn within_sense_range(dx: f32, dy: f32, sense: f32) -> bool {
    dx * dx + dy * dy <= sense * sense
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-CAP-1: distance per tick scales with speed.
    #[test]
    fn t_cap_1_distance_per_tick_scales_with_speed() {
        assert_eq!(max_distance_per_tick(1.0, 30), 1.0 / 30.0);
        assert_eq!(max_distance_per_tick(3.0, 30), 3.0 / 30.0);
        assert_eq!(max_distance_per_tick(2.0, 1), 2.0);
        let slow = max_distance_per_tick(1.0, 30);
        let fast = max_distance_per_tick(2.0, 30);
        assert!(fast > slow);
        assert!((fast / slow - 2.0).abs() < 1e-6);
    }

    /// T-CAP-1: an object just inside sense range is detected and one just outside is not.
    #[test]
    fn t_cap_1_sense_range_boundary() {
        let sense = 10.0_f32;
        assert!(within_sense_range(9.99, 0.0, sense));
        assert!(
            within_sense_range(0.0, -10.0, sense),
            "boundary is inclusive"
        );
        assert!(within_sense_range(6.0, 8.0, sense), "distance exactly 10");
        assert!(!within_sense_range(10.01, 0.0, sense));
        assert!(!within_sense_range(6.0, 8.01, sense));
        // A larger sense range detects the object a smaller one misses.
        assert!(!within_sense_range(12.0, 0.0, sense));
        assert!(within_sense_range(12.0, 0.0, 12.5));
    }

    #[test]
    fn trait_table_is_consistent() {
        let traits = Traits {
            speed: 1.0,
            size: 2.0,
            sense: 3.0,
        };
        let values: Vec<f32> = TraitId::ALL.iter().map(|&id| traits.get(id)).collect();
        assert_eq!(values, [1.0, 2.0, 3.0]);
        for id in TraitId::ALL {
            let range = id.range();
            assert!(range.min > 0.0, "{} must stay positive", id.name());
            assert!(range.contains(range.default));
            assert_eq!(id.spread_range().max, range.max - range.min);
        }
    }
}
