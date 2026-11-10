//! Minimum, maximum and default of every scenario parameter (I-07).
//!
//! `ScenarioConfig::validate()` and, later, the parameter UI (M-14) read these
//! constants; no other place may hard-code a parameter range.
//!
//! Every float range is inclusive. All values except the tick-rate minimum are
//! provisional (P): SCOPE.md 2.1 gives no numeric trait ranges or parameter limits, so
//! these are starting points for balancing (Q13). Defaults follow the illustrative
//! scenario file in SCOPE 8 where it gives a value.

/// Inclusive range and default of one floating-point parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatLimits {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl FloatLimits {
    /// True when `value` is finite and inside `[min, max]`.
    pub fn contains(&self, value: f32) -> bool {
        value.is_finite() && value >= self.min && value <= self.max
    }
}

/// Minimum ticks per simulated second. Zero would make simulated time undefined.
pub const TICKS_PER_SECOND_MIN: u32 = 1;

/// Default ticks per simulated second (SCOPE 6.1: 30 ticks = 1 simulated second, provisional).
///
/// No `TICKS_PER_SECOND_MAX` exists yet: SCOPE.md 2.1 defines no upper bound, and the
/// final tick rate is open question Q5 (decided by the end of Week 3). Add the maximum
/// here, and its check in `validate()`, when Q5 is settled.
pub const TICKS_PER_SECOND_DEFAULT: u32 = 30;

/// World width and height in world units. 512 matches tier C; 4096 leaves room above tier G.
pub const WORLD_SIZE: FloatLimits = FloatLimits {
    min: 16.0,
    max: 4096.0,
    default: 512.0,
};

/// Starting prey count. The maximum equals the population safety cap (SCOPE 6.7,
/// 150,000, P); the cap's birth suppression arrives with births (M-04).
pub const PREY_START_COUNT_MIN: u32 = 0;
pub const PREY_START_COUNT_MAX: u32 = 150_000;
/// 2,000 prey, as in benchmark tier C (SCOPE 10.4).
pub const PREY_START_COUNT_DEFAULT: u32 = 2_000;

/// Starting energy as a fraction of each organism's capacity. Above zero so no organism
/// starts starved.
pub const START_ENERGY_FRACTION: FloatLimits = FloatLimits {
    min: 0.01,
    max: 1.0,
    default: 0.5,
};

/// Valid range of the speed trait (world units per simulated second); default is the prey
/// starting mean.
pub const SPEED: FloatLimits = FloatLimits {
    min: 0.1,
    max: 5.0,
    default: 1.0,
};

/// Valid range of the size trait (abstract units); default is the prey starting mean.
pub const SIZE: FloatLimits = FloatLimits {
    min: 0.2,
    max: 5.0,
    default: 1.0,
};

/// Valid range of the sense-range trait (world units); default is the prey starting mean.
pub const SENSE: FloatLimits = FloatLimits {
    min: 1.0,
    max: 50.0,
    default: 10.0,
};

/// Starting spread (standard deviation) of each trait. At most the width of the trait's
/// range; zero gives every organism exactly the mean.
pub const SPEED_SPREAD: FloatLimits = FloatLimits {
    min: 0.0,
    max: SPEED.max - SPEED.min,
    default: 0.1,
};
pub const SIZE_SPREAD: FloatLimits = FloatLimits {
    min: 0.0,
    max: SIZE.max - SIZE.min,
    default: 0.1,
};
pub const SENSE_SPREAD: FloatLimits = FloatLimits {
    min: 0.0,
    max: SENSE.max - SENSE.min,
    default: 1.0,
};

/// `c_capacity`: energy capacity per unit of size. Positive so capacity is never zero.
pub const ENERGY_CAPACITY: FloatLimits = FloatLimits {
    min: 0.1,
    max: 1000.0,
    default: 10.0,
};

/// `c_base`: size-independent maintenance cost per simulated second.
pub const ENERGY_BASE: FloatLimits = FloatLimits {
    min: 0.0,
    max: 10.0,
    default: 0.01,
};

/// `c_size`: maintenance cost per unit of size per simulated second.
pub const ENERGY_SIZE: FloatLimits = FloatLimits {
    min: 0.0,
    max: 10.0,
    default: 0.02,
};

/// `c_move`: movement cost coefficient, multiplied by effort × size × speed².
pub const ENERGY_MOVEMENT: FloatLimits = FloatLimits {
    min: 0.0,
    max: 10.0,
    default: 0.01,
};

/// `c_sense`: sensing cost per unit of sense range per simulated second.
pub const ENERGY_SENSE: FloatLimits = FloatLimits {
    min: 0.0,
    max: 10.0,
    default: 0.001,
};

/// Side of one terrain and food grid cell in world units (SCOPE 6.1: 2 × 2, P). The
/// minimum keeps the largest grid (4,096 × 4,096 cells) within memory.
pub const CELL_SIZE: FloatLimits = FloatLimits {
    min: 1.0,
    max: 16.0,
    default: 2.0,
};

/// `food.max_per_cell`: food a fully fertile cell holds at abundance 1 (SCOPE 8).
pub const FOOD_MAX_PER_CELL: FloatLimits = FloatLimits {
    min: 0.01,
    max: 100.0,
    default: 1.0,
};

/// `food.regrowth_per_tick`: linear regrowth toward the cell's cap (SCOPE 6.4, 8).
pub const FOOD_REGROWTH_PER_TICK: FloatLimits = FloatLimits {
    min: 0.0,
    max: 100.0,
    default: 0.002,
};

/// `food.abundance`: multiplier on every cell's cap (SCOPE 6.4). Zero means no food.
pub const FOOD_ABUNDANCE: FloatLimits = FloatLimits {
    min: 0.0,
    max: 10.0,
    default: 1.0,
};

/// `food.initial_fraction`: starting food as a fraction of each cell's cap. Not specified
/// by SCOPE; uniform and full by default.
pub const FOOD_INITIAL_FRACTION: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1.0,
    default: 1.0,
};

/// `food.intake_per_tick`: most food one prey eats per tick (SCOPE 6.4 intake rate, P).
pub const FOOD_INTAKE_PER_TICK: FloatLimits = FloatLimits {
    min: 0.0,
    max: 100.0,
    default: 0.05,
};

/// `food.energy_per_unit`: energy gained per unit of food (SCOPE 6.4 fixed ratio, P).
/// Positive so the food needed to fill an organism is always defined.
pub const FOOD_ENERGY_PER_UNIT: FloatLimits = FloatLimits {
    min: 0.01,
    max: 100.0,
    default: 1.0,
};

/// `behavior.decision_interval`: ticks between goal decisions, staggered by ID (SCOPE 6.8).
pub const DECISION_INTERVAL_MIN: u32 = 1;
pub const DECISION_INTERVAL_MAX: u32 = 300;
pub const DECISION_INTERVAL_DEFAULT: u32 = 10;

/// `behavior.wander_speed_fraction`: wandering speed as a fraction of the maximum, which is
/// also its realized effort. 0.5 matches SCOPE 6.3's wandering effort (P).
pub const WANDER_SPEED_FRACTION: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1.0,
    default: 0.5,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_lies_in_its_range() {
        for limits in [
            WORLD_SIZE,
            START_ENERGY_FRACTION,
            SPEED,
            SIZE,
            SENSE,
            SPEED_SPREAD,
            SIZE_SPREAD,
            SENSE_SPREAD,
            ENERGY_CAPACITY,
            ENERGY_BASE,
            ENERGY_SIZE,
            ENERGY_MOVEMENT,
            ENERGY_SENSE,
            CELL_SIZE,
            FOOD_MAX_PER_CELL,
            FOOD_REGROWTH_PER_TICK,
            FOOD_ABUNDANCE,
            FOOD_INITIAL_FRACTION,
            FOOD_INTAKE_PER_TICK,
            FOOD_ENERGY_PER_UNIT,
            WANDER_SPEED_FRACTION,
        ] {
            assert!(limits.min <= limits.max, "{limits:?}");
            assert!(limits.contains(limits.default), "{limits:?}");
        }
        assert!((PREY_START_COUNT_MIN..=PREY_START_COUNT_MAX).contains(&PREY_START_COUNT_DEFAULT));
        assert!(
            (DECISION_INTERVAL_MIN..=DECISION_INTERVAL_MAX).contains(&DECISION_INTERVAL_DEFAULT)
        );
    }

    #[test]
    fn contains_rejects_non_finite_values() {
        assert!(!SPEED.contains(f32::NAN));
        assert!(!SPEED.contains(f32::INFINITY));
        assert!(!SPEED.contains(f32::NEG_INFINITY));
    }
}
