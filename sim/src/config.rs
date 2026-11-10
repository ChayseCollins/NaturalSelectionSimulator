//! Scenario configuration (subset through M-03).
//!
//! A scenario holds model and environment settings. The run seed is not part of it: it
//! identifies one deterministic execution of a scenario and is passed to
//! `Simulation::new` separately (SCOPE 8).
//!
//! Only fields used by implemented milestones exist. Food, predator, terrain-generation,
//! reproduction, mutation and navigation settings are added with their milestones. Serde
//! support and TOML parsing arrive with scenario loading (M-16, M-17); `sim` never reads
//! files itself (I-08). Field paths in validation errors (`prey.speed.mean`) follow the
//! scenario-file layout of SCOPE 8.

use std::fmt;

use crate::limits::{self, FloatLimits};
use crate::rng;
use crate::traits::TraitId;

/// Model and environment settings for a scenario.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioConfig {
    /// Fixed ticks per simulated second (I-05). The tick length comes only from here.
    pub ticks_per_second: u32,
    /// Seed for the terrain random domain. `None` derives it from the run seed
    /// (SCOPE 6.10), see [`ScenarioConfig::resolved_terrain_seed`].
    pub terrain_seed: Option<u64>,
    pub world: WorldConfig,
    pub food: FoodConfig,
    pub energy: EnergyConfig,
    pub behavior: BehaviorConfig,
    /// The prey species' starting block. Predators (M-05) get a second fixed block.
    pub prey: SpeciesConfig,
}

/// World rectangle in world units; positions lie in `[0, width] × [0, height]`.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldConfig {
    pub width: f32,
    pub height: f32,
    /// Side of one terrain and food grid cell (SCOPE 6.1: 2 world units, P). The grid has
    /// `ceil(width / cell_size) × ceil(height / cell_size)` cells.
    pub cell_size: f32,
}

/// Food grid settings (SCOPE 6.4, 8). Amounts are abstract food units; rates are per tick.
#[derive(Debug, Clone, PartialEq)]
pub struct FoodConfig {
    /// Food a fully fertile cell holds at abundance 1.
    pub max_per_cell: f32,
    /// Linear regrowth per tick toward each cell's cap.
    pub regrowth_per_tick: f32,
    /// Multiplier on every cell's cap: cap = fertility × max_per_cell × abundance.
    pub abundance: f32,
    /// Starting food as a fraction of each cell's cap (uniform, P).
    pub initial_fraction: f32,
    /// Most food one prey eats per tick (P).
    pub intake_per_tick: f32,
    /// Energy gained per unit of food eaten (P).
    pub energy_per_unit: f32,
}

/// Behavior settings for basic food seeking and wandering (SCOPE 6.8, M-09 Week 1 part).
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorConfig {
    /// Each organism re-chooses its goal every this many ticks, staggered by ID.
    pub decision_interval: u32,
    /// Wandering speed as a fraction of maximum speed; equal to wandering's realized effort.
    pub wander_speed_fraction: f32,
}

/// Coefficients of the one energy formula (SCOPE 6.3), shared by all species.
/// Rates are per simulated second.
#[derive(Debug, Clone, PartialEq)]
pub struct EnergyConfig {
    /// `c_capacity`: capacity per unit of size.
    pub capacity: f32,
    /// `c_base`: size-independent maintenance.
    pub base: f32,
    /// `c_size`: maintenance per unit of size.
    pub size: f32,
    /// `c_move`: movement cost coefficient (`move` in the scenario file).
    pub movement: f32,
    /// `c_sense`: sensing cost per unit of sense range.
    pub sense: f32,
}

/// One species' starting population and starting trait distributions (SCOPE 6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeciesConfig {
    /// Number of organisms created at tick 0. A density option arrives with terrain (M-06).
    pub start_count: u32,
    /// Starting energy as a fraction of each organism's capacity (P).
    pub start_energy_fraction: f32,
    pub speed: TraitStart,
    pub size: TraitStart,
    pub sense: TraitStart,
}

/// Normal starting distribution of one trait; samples are clamped to the trait's range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraitStart {
    pub mean: f32,
    /// Standard deviation.
    pub spread: f32,
}

impl SpeciesConfig {
    pub fn trait_start(&self, id: TraitId) -> TraitStart {
        match id {
            TraitId::Speed => self.speed,
            TraitId::Size => self.size,
            TraitId::Sense => self.sense,
        }
    }
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            ticks_per_second: limits::TICKS_PER_SECOND_DEFAULT,
            terrain_seed: None,
            world: WorldConfig::default(),
            food: FoodConfig::default(),
            energy: EnergyConfig::default(),
            behavior: BehaviorConfig::default(),
            prey: SpeciesConfig::default(),
        }
    }
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            width: limits::WORLD_SIZE.default,
            height: limits::WORLD_SIZE.default,
            cell_size: limits::CELL_SIZE.default,
        }
    }
}

impl Default for FoodConfig {
    fn default() -> Self {
        Self {
            max_per_cell: limits::FOOD_MAX_PER_CELL.default,
            regrowth_per_tick: limits::FOOD_REGROWTH_PER_TICK.default,
            abundance: limits::FOOD_ABUNDANCE.default,
            initial_fraction: limits::FOOD_INITIAL_FRACTION.default,
            intake_per_tick: limits::FOOD_INTAKE_PER_TICK.default,
            energy_per_unit: limits::FOOD_ENERGY_PER_UNIT.default,
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            decision_interval: limits::DECISION_INTERVAL_DEFAULT,
            wander_speed_fraction: limits::WANDER_SPEED_FRACTION.default,
        }
    }
}

impl Default for EnergyConfig {
    fn default() -> Self {
        Self {
            capacity: limits::ENERGY_CAPACITY.default,
            base: limits::ENERGY_BASE.default,
            size: limits::ENERGY_SIZE.default,
            movement: limits::ENERGY_MOVEMENT.default,
            sense: limits::ENERGY_SENSE.default,
        }
    }
}

impl Default for SpeciesConfig {
    fn default() -> Self {
        let start = |id: TraitId| TraitStart {
            mean: id.range().default,
            spread: id.spread_range().default,
        };
        Self {
            start_count: limits::PREY_START_COUNT_DEFAULT,
            start_energy_fraction: limits::START_ENERGY_FRACTION.default,
            speed: start(TraitId::Speed),
            size: start(TraitId::Size),
            sense: start(TraitId::Sense),
        }
    }
}

impl ScenarioConfig {
    /// Checks every implemented field against the `limits` module (I-07): integers against
    /// their bounds, floats for finiteness and their inclusive range. Returns all problems
    /// found, not just the first. Nothing is silently corrected.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if self.ticks_per_second < limits::TICKS_PER_SECOND_MIN {
            errors.push(ValidationError::BelowMinimum {
                field: "ticks_per_second".to_string(),
                value: u64::from(self.ticks_per_second),
                minimum: u64::from(limits::TICKS_PER_SECOND_MIN),
            });
        }

        check_float(
            &mut errors,
            "world.width",
            self.world.width,
            limits::WORLD_SIZE,
        );
        check_float(
            &mut errors,
            "world.height",
            self.world.height,
            limits::WORLD_SIZE,
        );
        check_float(
            &mut errors,
            "world.cell_size",
            self.world.cell_size,
            limits::CELL_SIZE,
        );

        let f = &self.food;
        for (field, value, range) in [
            (
                "food.max_per_cell",
                f.max_per_cell,
                limits::FOOD_MAX_PER_CELL,
            ),
            (
                "food.regrowth_per_tick",
                f.regrowth_per_tick,
                limits::FOOD_REGROWTH_PER_TICK,
            ),
            ("food.abundance", f.abundance, limits::FOOD_ABUNDANCE),
            (
                "food.initial_fraction",
                f.initial_fraction,
                limits::FOOD_INITIAL_FRACTION,
            ),
            (
                "food.intake_per_tick",
                f.intake_per_tick,
                limits::FOOD_INTAKE_PER_TICK,
            ),
            (
                "food.energy_per_unit",
                f.energy_per_unit,
                limits::FOOD_ENERGY_PER_UNIT,
            ),
        ] {
            check_float(&mut errors, field, value, range);
        }

        let e = &self.energy;
        check_float(
            &mut errors,
            "energy.capacity",
            e.capacity,
            limits::ENERGY_CAPACITY,
        );
        check_float(&mut errors, "energy.base", e.base, limits::ENERGY_BASE);
        check_float(&mut errors, "energy.size", e.size, limits::ENERGY_SIZE);
        check_float(
            &mut errors,
            "energy.move",
            e.movement,
            limits::ENERGY_MOVEMENT,
        );
        check_float(&mut errors, "energy.sense", e.sense, limits::ENERGY_SENSE);

        let b = &self.behavior;
        if !(limits::DECISION_INTERVAL_MIN..=limits::DECISION_INTERVAL_MAX)
            .contains(&b.decision_interval)
        {
            let value = u64::from(b.decision_interval);
            let field = "behavior.decision_interval".to_string();
            errors.push(if b.decision_interval < limits::DECISION_INTERVAL_MIN {
                ValidationError::BelowMinimum {
                    field,
                    value,
                    minimum: u64::from(limits::DECISION_INTERVAL_MIN),
                }
            } else {
                ValidationError::AboveMaximum {
                    field,
                    value,
                    maximum: u64::from(limits::DECISION_INTERVAL_MAX),
                }
            });
        }
        check_float(
            &mut errors,
            "behavior.wander_speed_fraction",
            b.wander_speed_fraction,
            limits::WANDER_SPEED_FRACTION,
        );

        let prey = &self.prey;
        if prey.start_count > limits::PREY_START_COUNT_MAX {
            errors.push(ValidationError::AboveMaximum {
                field: "prey.start_count".to_string(),
                value: u64::from(prey.start_count),
                maximum: u64::from(limits::PREY_START_COUNT_MAX),
            });
        }
        check_float(
            &mut errors,
            "prey.start_energy_fraction",
            prey.start_energy_fraction,
            limits::START_ENERGY_FRACTION,
        );
        for id in TraitId::ALL {
            let start = prey.trait_start(id);
            let field = |part: &str| format!("prey.{}.{part}", id.name());
            check_float(&mut errors, &field("mean"), start.mean, id.range());
            check_float(
                &mut errors,
                &field("spread"),
                start.spread,
                id.spread_range(),
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// The terrain seed used for a run with `run_seed`: the explicit value if present,
    /// otherwise one derived deterministically from `run_seed`, so different run seeds
    /// give different terrain unless a terrain seed is fixed (SCOPE 6.10).
    pub fn resolved_terrain_seed(&self, run_seed: u64) -> u64 {
        match self.terrain_seed {
            Some(seed) => seed,
            None => rng::derive_terrain_seed(run_seed),
        }
    }
}

fn check_float(errors: &mut Vec<ValidationError>, field: &str, value: f32, limits: FloatLimits) {
    if !value.is_finite() {
        errors.push(ValidationError::NotFinite {
            field: field.to_string(),
        });
    } else if !limits.contains(value) {
        errors.push(ValidationError::OutOfRange {
            field: field.to_string(),
            value,
            minimum: limits.min,
            maximum: limits.max,
        });
    }
}

/// A configuration value that failed validation. Each variant names the field by its
/// scenario-file path, for example `prey.speed.mean`.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// An integer field is below its minimum in the `limits` module.
    BelowMinimum {
        field: String,
        value: u64,
        minimum: u64,
    },
    /// An integer field is above its maximum in the `limits` module.
    AboveMaximum {
        field: String,
        value: u64,
        maximum: u64,
    },
    /// A float field is NaN or infinite.
    NotFinite { field: String },
    /// A finite float field is outside its inclusive range in the `limits` module.
    OutOfRange {
        field: String,
        value: f32,
        minimum: f32,
        maximum: f32,
    },
}

impl ValidationError {
    /// Path of the offending configuration field.
    pub fn field(&self) -> &str {
        match self {
            Self::BelowMinimum { field, .. }
            | Self::AboveMaximum { field, .. }
            | Self::NotFinite { field }
            | Self::OutOfRange { field, .. } => field,
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BelowMinimum {
                field,
                value,
                minimum,
            } => write!(f, "{field} = {value} is below the minimum of {minimum}"),
            Self::AboveMaximum {
                field,
                value,
                maximum,
            } => write!(f, "{field} = {value} is above the maximum of {maximum}"),
            Self::NotFinite { field } => write!(f, "{field} must be a finite number"),
            Self::OutOfRange {
                field,
                value,
                minimum,
                maximum,
            } => write!(
                f,
                "{field} = {value} is outside the range {minimum} to {maximum}"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(config: &ScenarioConfig) -> Vec<String> {
        config
            .validate()
            .expect_err("configuration should be invalid")
            .iter()
            .map(|e| e.field().to_string())
            .collect()
    }

    #[test]
    fn default_is_valid_and_uses_limits_defaults() {
        let config = ScenarioConfig::default();
        assert_eq!(config.validate(), Ok(()));
        assert_eq!(config.ticks_per_second, 30);
        assert_eq!(config.terrain_seed, None);
        assert_eq!(config.world.width, 512.0);
        assert_eq!(config.prey.start_count, 2_000);
        assert_eq!(config.prey.speed.mean, limits::SPEED.default);
    }

    #[test]
    fn zero_ticks_per_second_is_rejected_naming_the_field() {
        let config = ScenarioConfig {
            ticks_per_second: 0,
            ..ScenarioConfig::default()
        };
        let errors = config
            .validate()
            .expect_err("zero tick rate must be rejected");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field(), "ticks_per_second");
        assert!(errors[0].to_string().contains("ticks_per_second"));
    }

    #[test]
    fn valid_tick_rates_are_accepted() {
        for ticks_per_second in [limits::TICKS_PER_SECOND_MIN, 30, 60, u32::MAX] {
            let config = ScenarioConfig {
                ticks_per_second,
                ..ScenarioConfig::default()
            };
            assert_eq!(config.validate(), Ok(()), "{ticks_per_second}");
        }
    }

    #[test]
    fn non_finite_floats_are_rejected() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut config = ScenarioConfig::default();
            config.world.width = bad;
            config.energy.movement = bad;
            config.prey.sense.spread = bad;
            config.prey.start_energy_fraction = bad;
            assert_eq!(
                fields(&config),
                [
                    "world.width",
                    "energy.move",
                    "prey.start_energy_fraction",
                    "prey.sense.spread"
                ]
            );
            let errors = config.validate().expect_err("invalid");
            assert!(
                errors
                    .iter()
                    .all(|e| matches!(e, ValidationError::NotFinite { .. }))
            );
        }
    }

    #[test]
    fn out_of_range_values_are_rejected_with_field_names() {
        let mut config = ScenarioConfig::default();
        config.world.height = 0.0;
        config.energy.capacity = 0.0;
        config.energy.base = -0.01;
        config.prey.start_count = limits::PREY_START_COUNT_MAX + 1;
        config.prey.start_energy_fraction = 0.0;
        config.prey.speed.mean = limits::SPEED.max * 2.0;
        config.prey.size.spread = -1.0;
        assert_eq!(
            fields(&config),
            [
                "world.height",
                "energy.capacity",
                "energy.base",
                "prey.start_count",
                "prey.start_energy_fraction",
                "prey.speed.mean",
                "prey.size.spread",
            ]
        );
    }

    #[test]
    fn food_cell_and_behavior_fields_are_validated() {
        let mut config = ScenarioConfig::default();
        config.world.cell_size = 0.0;
        config.food.max_per_cell = 0.0;
        config.food.regrowth_per_tick = -0.001;
        config.food.abundance = f32::NAN;
        config.food.initial_fraction = 1.5;
        config.food.intake_per_tick = f32::INFINITY;
        config.food.energy_per_unit = 0.0;
        config.behavior.decision_interval = 0;
        config.behavior.wander_speed_fraction = 2.0;
        assert_eq!(
            fields(&config),
            [
                "world.cell_size",
                "food.max_per_cell",
                "food.regrowth_per_tick",
                "food.abundance",
                "food.initial_fraction",
                "food.intake_per_tick",
                "food.energy_per_unit",
                "behavior.decision_interval",
                "behavior.wander_speed_fraction",
            ]
        );
        config = ScenarioConfig::default();
        config.behavior.decision_interval = limits::DECISION_INTERVAL_MAX + 1;
        assert!(matches!(
            config.validate().expect_err("too long")[0],
            ValidationError::AboveMaximum { .. }
        ));
    }

    #[test]
    fn range_boundaries_are_inclusive() {
        let mut config = ScenarioConfig::default();
        config.world.width = limits::WORLD_SIZE.min;
        config.world.height = limits::WORLD_SIZE.max;
        config.prey.start_count = limits::PREY_START_COUNT_MIN;
        config.prey.start_energy_fraction = limits::START_ENERGY_FRACTION.max;
        config.prey.size.mean = limits::SIZE.min;
        config.prey.size.spread = 0.0;
        config.prey.sense.spread = limits::SENSE_SPREAD.max;
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn explicit_terrain_seed_is_preserved_for_every_run_seed() {
        let config = ScenarioConfig {
            terrain_seed: Some(1001),
            ..ScenarioConfig::default()
        };
        for run_seed in [0, 1, 2, u64::MAX] {
            assert_eq!(config.resolved_terrain_seed(run_seed), 1001);
        }
    }

    #[test]
    fn omitted_terrain_seed_is_derived_deterministically_from_run_seed() {
        let config = ScenarioConfig::default();
        assert_eq!(
            config.resolved_terrain_seed(1),
            config.clone().resolved_terrain_seed(1)
        );
        // SCOPE 6.10: the omitted terrain seed is derived from the run seed.
        assert_ne!(
            config.resolved_terrain_seed(1),
            config.resolved_terrain_seed(2)
        );
        // The derived seed is not just the run seed reused.
        assert_ne!(config.resolved_terrain_seed(1), 1);
    }
}
