//! The one energy formula (SCOPE 6.3). Coefficients are provisional (P) and come from the
//! scenario's `[energy]` block; the units are abstract, so this is a qualitative trade-off
//! model, not a calibrated metabolic law.
//!
//! ```text
//! E_max         = c_capacity × size
//! cost per tick = ( c_base
//!                 + c_size  × size
//!                 + c_move  × effort × size × speed²
//!                 + c_sense × sense ) × dt          where dt = 1 / ticks_per_second
//! ```
//!
//! Effort is the realized movement effort of the tick (decision of 2026-11-10, refining
//! SCOPE 6.3's per-behavior effort level): distance actually moved divided by the distance
//! available (`max_distance_per_tick × terrain speed multiplier`), clamped to `[0, 1]`.
//! Standing still costs no movement energy. Because the available distance already
//! includes the terrain multiplier, full-effort movement on slow terrain (M-06) costs the
//! same per tick while covering less ground, as SCOPE 6.3's terrain rule requires.

use crate::config::EnergyConfig;
use crate::traits::Traits;

/// Energy capacity: `c_capacity × size`.
pub fn capacity(coefficients: &EnergyConfig, size: f32) -> f32 {
    coefficients.capacity * size
}

/// Maintenance cost per simulated second: `c_base + c_size × size`.
pub fn maintenance_rate(coefficients: &EnergyConfig, size: f32) -> f32 {
    coefficients.base + coefficients.size * size
}

/// Movement cost per simulated second: `c_move × effort × size × speed²`.
pub fn movement_rate(coefficients: &EnergyConfig, traits: Traits, effort: f32) -> f32 {
    coefficients.movement * effort * traits.size * traits.speed * traits.speed
}

/// Sensing cost per simulated second: `c_sense × sense`.
pub fn sensing_rate(coefficients: &EnergyConfig, sense: f32) -> f32 {
    coefficients.sense * sense
}

/// Total cost per simulated second, summed in the fixed order of the formula.
pub fn cost_rate(coefficients: &EnergyConfig, traits: Traits, effort: f32) -> f32 {
    maintenance_rate(coefficients, traits.size)
        + movement_rate(coefficients, traits, effort)
        + sensing_rate(coefficients, traits.sense)
}

/// Energy charged in one tick: the cost rate divided by the tick rate.
pub fn cost_per_tick(
    coefficients: &EnergyConfig,
    traits: Traits,
    effort: f32,
    ticks_per_second: u32,
) -> f32 {
    cost_rate(coefficients, traits, effort) / ticks_per_second as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits;

    fn coefficients() -> EnergyConfig {
        EnergyConfig {
            capacity: 10.0,
            base: 0.1,
            size: 0.2,
            movement: 0.3,
            sense: 0.01,
        }
    }

    fn traits(speed: f32, size: f32, sense: f32) -> Traits {
        Traits { speed, size, sense }
    }

    /// T-EN-1: speed raises the movement term.
    #[test]
    fn t_en_1_speed_raises_movement_cost() {
        let c = coefficients();
        let slow = movement_rate(&c, traits(1.0, 1.0, 10.0), 1.0);
        let fast = movement_rate(&c, traits(2.0, 1.0, 10.0), 1.0);
        assert!(fast > slow);
        assert_eq!(fast, 4.0 * slow, "movement cost grows with speed squared");
        assert!(
            cost_rate(&c, traits(2.0, 1.0, 10.0), 1.0) > cost_rate(&c, traits(1.0, 1.0, 10.0), 1.0)
        );
    }

    /// T-EN-1: size raises both maintenance and movement.
    #[test]
    fn t_en_1_size_raises_maintenance_and_movement_cost() {
        let c = coefficients();
        assert!(maintenance_rate(&c, 2.0) > maintenance_rate(&c, 1.0));
        assert!(
            movement_rate(&c, traits(1.0, 2.0, 10.0), 1.0)
                > movement_rate(&c, traits(1.0, 1.0, 10.0), 1.0)
        );
    }

    /// T-EN-1: sense range raises sensing cost.
    #[test]
    fn t_en_1_sense_raises_sensing_cost() {
        let c = coefficients();
        assert!(sensing_rate(&c, 20.0) > sensing_rate(&c, 10.0));
        assert!(
            cost_rate(&c, traits(1.0, 1.0, 20.0), 0.0) > cost_rate(&c, traits(1.0, 1.0, 10.0), 0.0)
        );
    }

    #[test]
    fn movement_cost_is_zero_without_movement_effort() {
        let c = coefficients();
        assert_eq!(movement_rate(&c, traits(5.0, 5.0, 10.0), 0.0), 0.0);
        assert_eq!(
            cost_rate(&c, traits(5.0, 1.0, 10.0), 0.0),
            cost_rate(&c, traits(0.1, 1.0, 10.0), 0.0),
            "without movement, speed costs nothing"
        );
    }

    /// Speed trade-off at equal realized effort: twice the speed covers twice the distance
    /// per tick but costs four times the movement energy per tick, so twice the energy per
    /// unit distance.
    #[test]
    fn higher_speed_costs_more_per_unit_distance_at_equal_effort() {
        let c = coefficients();
        let slow = traits(1.0, 1.0, 10.0);
        let fast = traits(2.0, 1.0, 10.0);
        let per_distance = |t: Traits| {
            movement_rate(&c, t, 1.0) / crate::traits::max_distance_per_tick(t.speed, 1)
        };
        assert_eq!(
            movement_rate(&c, fast, 1.0),
            4.0 * movement_rate(&c, slow, 1.0)
        );
        assert_eq!(per_distance(fast), 2.0 * per_distance(slow));
        assert_eq!(
            movement_rate(&c, fast, 0.5),
            0.5 * movement_rate(&c, fast, 1.0),
            "movement cost is proportional to realized effort"
        );
    }

    #[test]
    fn total_cost_is_the_sum_of_terms_divided_by_tick_rate() {
        let c = coefficients();
        let t = traits(2.0, 1.5, 10.0);
        let expected = (0.1 + 0.2 * 1.5) + (0.3 * 0.5 * 1.5 * 2.0 * 2.0) + (0.01 * 10.0);
        assert!((cost_rate(&c, t, 0.5) - expected).abs() < 1e-6);
        assert_eq!(cost_per_tick(&c, t, 0.5, 30), cost_rate(&c, t, 0.5) / 30.0);
    }

    /// T-CAP-1: energy capacity scales with size, and is finite and positive across the
    /// valid size and coefficient ranges.
    #[test]
    fn t_cap_1_capacity_scales_with_size() {
        let c = coefficients();
        assert_eq!(capacity(&c, 2.0), 2.0 * capacity(&c, 1.0));
        for coefficient in [limits::ENERGY_CAPACITY.min, limits::ENERGY_CAPACITY.max] {
            let c = EnergyConfig {
                capacity: coefficient,
                ..coefficients()
            };
            for size in [limits::SIZE.min, limits::SIZE.max] {
                let e_max = capacity(&c, size);
                assert!(e_max.is_finite() && e_max > 0.0, "{coefficient} × {size}");
            }
        }
    }

    #[test]
    fn worst_case_cost_is_finite() {
        let c = EnergyConfig {
            capacity: limits::ENERGY_CAPACITY.max,
            base: limits::ENERGY_BASE.max,
            size: limits::ENERGY_SIZE.max,
            movement: limits::ENERGY_MOVEMENT.max,
            sense: limits::ENERGY_SENSE.max,
        };
        let t = traits(limits::SPEED.max, limits::SIZE.max, limits::SENSE.max);
        let cost = cost_per_tick(&c, t, 1.0, limits::TICKS_PER_SECOND_MIN);
        assert!(cost.is_finite() && cost > 0.0);
    }
}
