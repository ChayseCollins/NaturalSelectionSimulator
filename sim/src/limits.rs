//! Minimum, maximum and default of every scenario parameter (I-07).
//!
//! `ScenarioConfig::validate()` and the parameter UI (M-14) read these constants; no other
//! place may hard-code a parameter range.
//!
//! Every range is inclusive. All values except the tick-rate minimum are provisional (P):
//! SCOPE.md gives no numeric trait ranges or parameter limits, so these are starting
//! points for balancing (Q13). Defaults follow the illustrative scenario file in SCOPE 8
//! where it gives a value.

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

/// Inclusive range and default of one integer parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntLimits {
    pub min: u64,
    pub max: u64,
    pub default: u64,
}

impl IntLimits {
    pub fn contains(&self, value: u64) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

/// The scenario-file format version this build reads and writes (M-16).
pub const FORMAT_VERSION: u32 = 1;

/// Seeds are stored as TOML integers, which are signed 64-bit, so a seed in a scenario
/// file is at most `i64::MAX`. Run seeds passed on the command line may use the full
/// `u64` range.
pub const SEED: IntLimits = IntLimits {
    min: 0,
    max: i64::MAX as u64,
    default: 1,
};

/// Ticks per simulated second (SCOPE 6.1: 30, P). The maximum (240) is a provisional
/// answer to Q5, recorded in README.md: above it a 10× run would need 2,400 ticks per
/// real second.
pub const TICKS_PER_SECOND: IntLimits = IntLimits {
    min: 1,
    max: 240,
    default: 30,
};

/// Population safety cap (SCOPE 6.7: 150,000, P). Births are suppressed at the cap.
pub const POPULATION_CAP: IntLimits = IntLimits {
    min: 1,
    max: 1_000_000,
    default: 150_000,
};

// ----- World and grids -----

/// World width and height in world units. 512 matches tier C; 4096 leaves room above tier G.
pub const WORLD_SIZE: FloatLimits = FloatLimits {
    min: 16.0,
    max: 4096.0,
    default: 512.0,
};

/// Side of one terrain and food grid cell in world units (SCOPE 6.1: 2 × 2, P).
pub const CELL_SIZE: FloatLimits = FloatLimits {
    min: 1.0,
    max: 16.0,
    default: 2.0,
};

/// Most terrain and food cells a world may have (4,194,304 = 2,048²), checked across
/// world size and cell size. Four times tier G's grid.
pub const MAX_GRID_CELLS: u64 = 2_048 * 2_048;

/// Side of one spatial-grid cell in world units (SCOPE 6.1: 16, P).
pub const SPATIAL_CELL_SIZE: FloatLimits = FloatLimits {
    min: 4.0,
    max: 128.0,
    default: 16.0,
};

// ----- Terrain (M-06) -----

/// Share of the map that is water: cells below this elevation rank are water.
pub const WATER_LEVEL: FloatLimits = FloatLimits {
    min: 0.0,
    max: 0.9,
    default: 0.3,
};

/// Elevation rank above which cells are rock. Cross-field rule: at least
/// [`MIN_PASSABLE_SHARE`] lies between the water and rock levels.
pub const ROCK_LEVEL: FloatLimits = FloatLimits {
    min: 0.1,
    max: 1.0,
    default: 0.85,
};

/// Least passable share of the map (SCOPE T-VAL-2: 20%, P), enforced as
/// `rock_level - water_level >= 0.2` because the levels are elevation ranks.
pub const MIN_PASSABLE_SHARE: f32 = 0.2;

/// Typical size of terrain features (lakes, hills) in world units.
pub const FEATURE_SCALE: FloatLimits = FloatLimits {
    min: 8.0,
    max: 1024.0,
    default: 96.0,
};

// ----- Food (M-07) -----

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

/// `food.initial_fraction`: starting food as a fraction of each cell's cap.
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
pub const FOOD_ENERGY_PER_UNIT: FloatLimits = FloatLimits {
    min: 0.01,
    max: 100.0,
    default: 1.0,
};

// ----- Energy (SCOPE 6.3) -----

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

// ----- Mutation and reproduction (M-04, SCOPE 6.6) -----

/// Per-trait mutation probability `p`.
pub const MUTATION_PROBABILITY: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1.0,
    default: 0.1,
};

/// Mutation magnitude `σ`: a mutated value is multiplied by `1 + N(0, σ)`.
pub const MUTATION_MAGNITUDE: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1.0,
    default: 0.05,
};

/// Reproduce when energy ≥ this fraction of capacity.
pub const REPRODUCTION_THRESHOLD: FloatLimits = FloatLimits {
    min: 0.1,
    max: 1.0,
    default: 0.8,
};

/// Offspring starting energy as a fraction of the offspring's capacity; paid by the parent.
pub const OFFSPRING_ENERGY_FRACTION: FloatLimits = FloatLimits {
    min: 0.01,
    max: 1.0,
    default: 0.3,
};

/// Energy lost per birth on top of the offspring's energy.
pub const REPRODUCTION_OVERHEAD: FloatLimits = FloatLimits {
    min: 0.0,
    max: 100.0,
    default: 0.5,
};

/// Offspring are placed within this distance of the parent, in world units.
pub const SPAWN_RADIUS: FloatLimits = FloatLimits {
    min: 0.0,
    max: 32.0,
    default: 2.0,
};

// ----- Species starting blocks (SCOPE 6.2) -----

/// Starting count of one species. The maximum equals the default safety cap.
pub const START_COUNT: IntLimits = IntLimits {
    min: 0,
    max: 150_000,
    default: 2_000,
};

/// Starting density of one species, in organisms per 1,000 passable cells (SCOPE 8).
pub const START_DENSITY: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1_000.0,
    default: 40.0,
};

/// Default predator starting density: a tenth of the prey density, as in tier C.
pub const PREDATOR_START_DENSITY_DEFAULT: f32 = 4.0;

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

/// Predator starting trait means (SCOPE 8's illustrative values).
pub const PREDATOR_SPEED_DEFAULT: f32 = 1.2;
pub const PREDATOR_SIZE_DEFAULT: f32 = 1.5;
pub const PREDATOR_SENSE_DEFAULT: f32 = 14.0;

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

/// Optional death from old age (S-02): maximum lifespan in simulated seconds; 0 turns it
/// off (the default, Q10).
pub const MAX_AGE_SECONDS: FloatLimits = FloatLimits {
    min: 0.0,
    max: 100_000.0,
    default: 0.0,
};

// ----- Predation (M-05, SCOPE 6.5) -----

/// A predator may eat a prey only if predator size ≥ `size_ratio` × prey size.
pub const SIZE_RATIO: FloatLimits = FloatLimits {
    min: 0.1,
    max: 10.0,
    default: 1.2,
};

/// Capture happens within this distance, in world units.
pub const CAPTURE_RADIUS: FloatLimits = FloatLimits {
    min: 0.1,
    max: 16.0,
    default: 1.0,
};

/// Share of the prey's energy content the predator gains.
pub const EFFICIENCY: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1.0,
    default: 0.6,
};

/// `c_content`: a prey's energy content per unit of its size.
pub const PREY_CONTENT: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1_000.0,
    default: 10.0,
};

// ----- Behavior (SCOPE 6.8) -----

/// Ticks between goal decisions, staggered by ID.
pub const DECISION_INTERVAL: IntLimits = IntLimits {
    min: 1,
    max: 300,
    default: 10,
};

/// Ticks between a prey's threat checks, staggered by ID.
pub const THREAT_INTERVAL: IntLimits = IntLimits {
    min: 1,
    max: 60,
    default: 2,
};

/// Wandering speed as a fraction of the maximum, which is also its realized effort. 0.5
/// matches SCOPE 6.3's wandering effort (P).
pub const WANDER_SPEED_FRACTION: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1.0,
    default: 0.5,
};

// ----- Navigation (SCOPE 6.8) -----

/// Most A* node expansions for one route request.
pub const ROUTE_EXPANSIONS_PER_REQUEST: IntLimits = IntLimits {
    min: 16,
    max: 65_536,
    default: 512,
};

/// Most A* node expansions in one tick, over all requests. At least one request's cap.
pub const ROUTE_BUDGET_PER_TICK: IntLimits = IntLimits {
    min: 16,
    max: 1_000_000,
    default: 4_000,
};

/// The search window is the square of sense range plus this margin around the organism.
pub const ROUTE_WINDOW_MARGIN: FloatLimits = FloatLimits {
    min: 0.0,
    max: 256.0,
    default: 16.0,
};

/// A moving target that ends up more than this many cells from the route's end makes
/// the route stale.
pub const ROUTE_DRIFT_CELLS: FloatLimits = FloatLimits {
    min: 0.5,
    max: 64.0,
    default: 2.0,
};

/// A predator chasing moving prey re-plans at most this often, in ticks.
pub const HUNT_REPLAN_INTERVAL: IntLimits = IntLimits {
    min: 1,
    max: 600,
    default: 15,
};

/// Ticks without at least [`STUCK_MIN_PROGRESS`] of progress before stuck recovery.
pub const STUCK_WINDOW: IntLimits = IntLimits {
    min: 1,
    max: 3_600,
    default: 60,
};

/// Progress toward the goal, in world units, that resets the stuck window.
pub const STUCK_MIN_PROGRESS: FloatLimits = FloatLimits {
    min: 0.0,
    max: 16.0,
    default: 1.0,
};

/// Ticks a target stays marked unreachable after stuck recovery fails.
pub const UNREACHABLE_TICKS: IntLimits = IntLimits {
    min: 1,
    max: 36_000,
    default: 300,
};

// ----- Metrics (M-13) -----

/// Ticks between metric samples for the charts (SCOPE 6.9: 30, P).
pub const METRICS_INTERVAL: IntLimits = IntLimits {
    min: 1,
    max: 3_600,
    default: 30,
};

/// Most samples kept per run; older samples are thinned (T-CHART-3).
pub const METRICS_HISTORY_CAP: usize = 2_048;

// ----- Famine (S-01) -----

/// Tick from which the famine multiplier applies.
pub const FAMINE_AT_TICK: IntLimits = IntLimits {
    min: 0,
    max: 100_000_000,
    default: 36_000,
};

/// Multiplier on every cell's regrowth cap during the famine.
pub const FAMINE_FOOD_MULTIPLIER: FloatLimits = FloatLimits {
    min: 0.0,
    max: 1.0,
    default: 0.4,
};

/// Every float limit, for range self-checks.
pub const ALL_FLOAT_LIMITS: [FloatLimits; 41] = [
    WORLD_SIZE,
    CELL_SIZE,
    SPATIAL_CELL_SIZE,
    WATER_LEVEL,
    ROCK_LEVEL,
    FEATURE_SCALE,
    FOOD_MAX_PER_CELL,
    FOOD_REGROWTH_PER_TICK,
    FOOD_ABUNDANCE,
    FOOD_INITIAL_FRACTION,
    FOOD_INTAKE_PER_TICK,
    FOOD_ENERGY_PER_UNIT,
    ENERGY_CAPACITY,
    ENERGY_BASE,
    ENERGY_SIZE,
    ENERGY_MOVEMENT,
    ENERGY_SENSE,
    MUTATION_PROBABILITY,
    MUTATION_MAGNITUDE,
    REPRODUCTION_THRESHOLD,
    OFFSPRING_ENERGY_FRACTION,
    REPRODUCTION_OVERHEAD,
    SPAWN_RADIUS,
    START_DENSITY,
    START_ENERGY_FRACTION,
    SPEED,
    SIZE,
    SENSE,
    SPEED_SPREAD,
    SIZE_SPREAD,
    SENSE_SPREAD,
    MAX_AGE_SECONDS,
    SIZE_RATIO,
    CAPTURE_RADIUS,
    EFFICIENCY,
    PREY_CONTENT,
    WANDER_SPEED_FRACTION,
    ROUTE_WINDOW_MARGIN,
    ROUTE_DRIFT_CELLS,
    STUCK_MIN_PROGRESS,
    FAMINE_FOOD_MULTIPLIER,
];

/// Every integer limit, for range self-checks.
pub const ALL_INT_LIMITS: [IntLimits; 13] = [
    SEED,
    TICKS_PER_SECOND,
    POPULATION_CAP,
    START_COUNT,
    DECISION_INTERVAL,
    THREAT_INTERVAL,
    ROUTE_EXPANSIONS_PER_REQUEST,
    ROUTE_BUDGET_PER_TICK,
    HUNT_REPLAN_INTERVAL,
    STUCK_WINDOW,
    UNREACHABLE_TICKS,
    METRICS_INTERVAL,
    FAMINE_AT_TICK,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// T-VAL-1 (defaults part): every default lies in its range.
    #[test]
    fn every_default_lies_in_its_range() {
        for limits in ALL_FLOAT_LIMITS {
            assert!(limits.min <= limits.max, "{limits:?}");
            assert!(limits.contains(limits.default), "{limits:?}");
        }
        for limits in ALL_INT_LIMITS {
            assert!(limits.min <= limits.max, "{limits:?}");
            assert!(limits.contains(limits.default), "{limits:?}");
        }
        for predator_mean in [
            (PREDATOR_SPEED_DEFAULT, SPEED),
            (PREDATOR_SIZE_DEFAULT, SIZE),
            (PREDATOR_SENSE_DEFAULT, SENSE),
            (PREDATOR_START_DENSITY_DEFAULT, START_DENSITY),
        ] {
            assert!(predator_mean.1.contains(predator_mean.0));
        }
        const { assert!(ROCK_LEVEL.default - WATER_LEVEL.default >= MIN_PASSABLE_SHARE) };
        const { assert!(ROUTE_BUDGET_PER_TICK.default >= ROUTE_EXPANSIONS_PER_REQUEST.default) };
    }

    #[test]
    fn contains_rejects_non_finite_values() {
        assert!(!SPEED.contains(f32::NAN));
        assert!(!SPEED.contains(f32::INFINITY));
        assert!(!SPEED.contains(f32::NEG_INFINITY));
    }
}
