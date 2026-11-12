//! Scenario configuration and its TOML file format (SCOPE 8, M-15, M-16).
//!
//! A scenario holds model and environment settings plus a name, description and learning
//! goal. The run seed is not part of the model: it identifies one deterministic execution
//! and is passed to `Simulation::new` separately. A scenario may suggest one (`seed`).
//!
//! The struct layout is the file layout, so field paths in validation errors
//! (`species.prey.speed.mean`) are exactly the keys a teacher edits. `sim` never reads
//! files (I-08): it parses and produces strings, and the shells do the file I/O.
//!
//! File rules (T-SCN-1 to T-SCN-3):
//! - `format_version` is required and must be [`limits::FORMAT_VERSION`].
//! - Unknown fields are rejected with their name. Missing optional fields and tables take
//!   their defaults from the `limits` module. A `[species.prey]` or `[species.predator]`
//!   table, when present, must be complete, so a partial table cannot silently mix in the
//!   other species' defaults.
//! - Floats are written in their shortest exact form, so save-then-load is lossless.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::limits::{self, FloatLimits, IntLimits};
use crate::rng;
use crate::traits::TraitId;

/// Model and environment settings for a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ScenarioConfig {
    /// File format version; must be [`limits::FORMAT_VERSION`].
    pub format_version: u32,
    pub name: String,
    pub description: String,
    /// Learning goal or experiment description shown with the preset.
    pub learning_goal: String,
    /// Suggested run seed. `None` lets the shell pick one and show it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Fixed ticks per simulated second (I-05). The tick length comes only from here.
    pub ticks_per_second: u32,
    /// Population safety cap (SCOPE 6.7): births are suppressed at this total.
    pub population_cap: u32,
    pub world: WorldConfig,
    pub terrain: TerrainConfig,
    pub food: FoodConfig,
    pub energy: EnergyConfig,
    pub mutation: MutationConfig,
    pub reproduction: ReproductionConfig,
    pub species: SpeciesBlocks,
    pub predation: PredationConfig,
    pub behavior: BehaviorConfig,
    pub navigation: NavigationConfig,
    pub metrics: MetricsConfig,
    /// Optional famine (S-01): from a tick on, every cell's regrowth cap is multiplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub famine: Option<FamineConfig>,
}

/// World rectangle in world units; positions lie in `[0, width] × [0, height]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WorldConfig {
    pub width: f32,
    pub height: f32,
    /// Side of one terrain and food grid cell (SCOPE 6.1: 2 world units, P).
    pub cell_size: f32,
    /// Side of one spatial-grid cell (SCOPE 6.1: 16 world units, P).
    pub spatial_cell_size: f32,
}

/// Procedural terrain settings (M-06).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TerrainConfig {
    /// Seed for the terrain random domain. `None` derives it from the run seed (SCOPE
    /// 6.10), so a fixed value keeps the same map across run seeds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Share of the map under water (an elevation rank, 0 to 0.9).
    pub water_level: f32,
    /// Elevation rank above which cells are rock.
    pub rock_level: f32,
    /// Typical size of lakes and hills, in world units.
    pub feature_scale: f32,
}

/// Food grid settings (SCOPE 6.4). Amounts are abstract food units; rates are per tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FoodConfig {
    /// Food a fully fertile cell holds at abundance 1.
    pub max_per_cell: f32,
    /// Linear regrowth per tick toward each cell's cap.
    pub regrowth_per_tick: f32,
    /// Multiplier on every cell's cap: cap = fertility × max_per_cell × abundance.
    pub abundance: f32,
    /// Starting food as a fraction of each cell's cap.
    pub initial_fraction: f32,
    /// Most food one prey eats per tick (P).
    pub intake_per_tick: f32,
    /// Energy gained per unit of food eaten (P).
    pub energy_per_unit: f32,
}

/// Coefficients of the one energy formula (SCOPE 6.3), shared by both species. Rates are
/// per simulated second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct EnergyConfig {
    /// `c_capacity`: capacity per unit of size.
    pub capacity: f32,
    /// `c_base`: size-independent maintenance.
    pub base: f32,
    /// `c_size`: maintenance per unit of size.
    pub size: f32,
    /// `c_move`: movement cost coefficient (`move` in the scenario file).
    #[serde(rename = "move")]
    pub movement: f32,
    /// `c_sense`: sensing cost per unit of sense range.
    pub sense: f32,
}

/// Mutation on inheritance (SCOPE 6.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MutationConfig {
    /// Per-trait probability that the offspring's value mutates.
    pub probability: f32,
    /// σ: a mutated value is multiplied by `1 + N(0, σ)`.
    pub magnitude: f32,
}

/// Asexual reproduction (SCOPE 6.6), shared by both species.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ReproductionConfig {
    /// Reproduce when energy ≥ `threshold × capacity`.
    pub threshold: f32,
    /// Offspring starting energy as a fraction of the offspring's capacity.
    pub offspring_energy_fraction: f32,
    /// Energy lost per birth on top of the offspring's energy.
    pub overhead: f32,
    /// Offspring are placed within this distance of the parent.
    pub spawn_radius: f32,
}

/// The two fixed species blocks (SCOPE 7.2: two blocks, not a table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesBlocks {
    #[serde(default = "SpeciesConfig::prey_default")]
    pub prey: SpeciesConfig,
    #[serde(default = "SpeciesConfig::predator_default")]
    pub predator: SpeciesConfig,
}

/// One species' starting population and starting trait distributions (SCOPE 6.2).
///
/// Every field except `max_age_seconds` is required when the table is present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesConfig {
    /// Starting population, as a count or a density per 1,000 passable cells.
    pub start: StartPopulation,
    /// Starting energy as a fraction of each organism's capacity (P).
    pub start_energy_fraction: f32,
    pub speed: TraitStart,
    pub size: TraitStart,
    pub sense: TraitStart,
    /// Optional death from old age (S-02), in simulated seconds; 0 turns it off.
    #[serde(default)]
    pub max_age_seconds: f32,
}

/// Starting population of a species (SCOPE 8): `{ count = 2000 }` or
/// `{ density = 8.0 }` organisms per 1,000 passable cells.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartPopulation {
    Count(u32),
    Density(f32),
}

/// Normal starting distribution of one trait; samples are clamped to the trait's range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitStart {
    pub mean: f32,
    /// Standard deviation.
    pub spread: f32,
}

/// Predation (SCOPE 6.5): the `[predation]` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PredationConfig {
    /// A predator may eat a prey only if predator size ≥ `size_ratio × prey size`.
    pub size_ratio: f32,
    /// Capture distance in world units.
    pub capture_radius: f32,
    /// Share of the prey's energy content gained.
    pub efficiency: f32,
    /// `c_content`: prey energy content per unit of size.
    pub content: f32,
}

/// Goal selection (SCOPE 6.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BehaviorConfig {
    /// Each organism re-chooses its goal every this many ticks, staggered by ID.
    pub decision_interval: u32,
    /// Prey check for predators every this many ticks, staggered by ID.
    pub threat_interval: u32,
    /// Wandering speed as a fraction of maximum speed; equal to wandering's realized effort.
    pub wander_speed_fraction: f32,
}

/// Bounded route search and stuck recovery (SCOPE 6.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NavigationConfig {
    /// Most A* node expansions for one route request.
    pub expansions_per_request: u32,
    /// Most A* node expansions per tick over all requests.
    pub budget_per_tick: u32,
    /// Search window margin around sense range, in world units.
    pub window_margin: f32,
    /// A moving target this many cells from the route's end makes the route stale.
    pub drift_cells: f32,
    /// Fewest ticks between a hunting predator's re-plans.
    pub hunt_replan_interval: u32,
    /// Ticks without progress before stuck recovery.
    pub stuck_window: u32,
    /// Progress toward the goal, in world units, that resets the stuck window.
    pub stuck_min_progress: f32,
    /// Ticks a target stays marked unreachable.
    pub unreachable_ticks: u32,
}

/// Chart sampling (M-13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MetricsConfig {
    /// Ticks between samples.
    pub interval: u32,
}

/// One scheduled food-supply change (S-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamineConfig {
    /// First tick with the reduced cap.
    pub at_tick: u64,
    /// Multiplier on every cell's regrowth cap from `at_tick` on.
    pub food_multiplier: f32,
}

impl SpeciesConfig {
    pub fn trait_start(&self, id: TraitId) -> TraitStart {
        match id {
            TraitId::Speed => self.speed,
            TraitId::Size => self.size,
            TraitId::Sense => self.sense,
        }
    }

    pub fn trait_start_mut(&mut self, id: TraitId) -> &mut TraitStart {
        match id {
            TraitId::Speed => &mut self.speed,
            TraitId::Size => &mut self.size,
            TraitId::Sense => &mut self.sense,
        }
    }

    /// Prey defaults: density 40 per 1,000 passable cells and the trait ranges' defaults.
    pub fn prey_default() -> Self {
        let start = |id: TraitId| TraitStart {
            mean: id.range().default,
            spread: id.spread_range().default,
        };
        Self {
            start: StartPopulation::Density(limits::START_DENSITY.default),
            start_energy_fraction: limits::START_ENERGY_FRACTION.default,
            speed: start(TraitId::Speed),
            size: start(TraitId::Size),
            sense: start(TraitId::Sense),
            max_age_seconds: limits::MAX_AGE_SECONDS.default,
        }
    }

    /// Predator defaults (SCOPE 8's illustrative block): faster, larger, longer sense.
    pub fn predator_default() -> Self {
        Self {
            start: StartPopulation::Density(limits::PREDATOR_START_DENSITY_DEFAULT),
            start_energy_fraction: limits::START_ENERGY_FRACTION.default,
            speed: TraitStart {
                mean: limits::PREDATOR_SPEED_DEFAULT,
                spread: limits::SPEED_SPREAD.default,
            },
            size: TraitStart {
                mean: limits::PREDATOR_SIZE_DEFAULT,
                spread: limits::SIZE_SPREAD.default,
            },
            sense: TraitStart {
                mean: limits::PREDATOR_SENSE_DEFAULT,
                spread: limits::SENSE_SPREAD.default,
            },
            max_age_seconds: limits::MAX_AGE_SECONDS.default,
        }
    }

    /// Starting count for a map with `passable_cells` passable cells. A density is rounded
    /// to the nearest whole organism.
    pub fn resolved_start_count(&self, passable_cells: usize) -> u64 {
        match self.start {
            StartPopulation::Count(count) => u64::from(count),
            StartPopulation::Density(per_thousand) => {
                (f64::from(per_thousand) * passable_cells as f64 / 1_000.0).round() as u64
            }
        }
    }
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            format_version: limits::FORMAT_VERSION,
            name: "Untitled scenario".to_string(),
            description: String::new(),
            learning_goal: String::new(),
            seed: None,
            ticks_per_second: limits::TICKS_PER_SECOND.default as u32,
            population_cap: limits::POPULATION_CAP.default as u32,
            world: WorldConfig::default(),
            terrain: TerrainConfig::default(),
            food: FoodConfig::default(),
            energy: EnergyConfig::default(),
            mutation: MutationConfig::default(),
            reproduction: ReproductionConfig::default(),
            species: SpeciesBlocks::default(),
            predation: PredationConfig::default(),
            behavior: BehaviorConfig::default(),
            navigation: NavigationConfig::default(),
            metrics: MetricsConfig::default(),
            famine: None,
        }
    }
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            width: limits::WORLD_SIZE.default,
            height: limits::WORLD_SIZE.default,
            cell_size: limits::CELL_SIZE.default,
            spatial_cell_size: limits::SPATIAL_CELL_SIZE.default,
        }
    }
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            seed: None,
            water_level: limits::WATER_LEVEL.default,
            rock_level: limits::ROCK_LEVEL.default,
            feature_scale: limits::FEATURE_SCALE.default,
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

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            probability: limits::MUTATION_PROBABILITY.default,
            magnitude: limits::MUTATION_MAGNITUDE.default,
        }
    }
}

impl Default for ReproductionConfig {
    fn default() -> Self {
        Self {
            threshold: limits::REPRODUCTION_THRESHOLD.default,
            offspring_energy_fraction: limits::OFFSPRING_ENERGY_FRACTION.default,
            overhead: limits::REPRODUCTION_OVERHEAD.default,
            spawn_radius: limits::SPAWN_RADIUS.default,
        }
    }
}

impl Default for SpeciesBlocks {
    fn default() -> Self {
        Self {
            prey: SpeciesConfig::prey_default(),
            predator: SpeciesConfig::predator_default(),
        }
    }
}

impl Default for PredationConfig {
    fn default() -> Self {
        Self {
            size_ratio: limits::SIZE_RATIO.default,
            capture_radius: limits::CAPTURE_RADIUS.default,
            efficiency: limits::EFFICIENCY.default,
            content: limits::PREY_CONTENT.default,
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            decision_interval: limits::DECISION_INTERVAL.default as u32,
            threat_interval: limits::THREAT_INTERVAL.default as u32,
            wander_speed_fraction: limits::WANDER_SPEED_FRACTION.default,
        }
    }
}

impl Default for NavigationConfig {
    fn default() -> Self {
        Self {
            expansions_per_request: limits::ROUTE_EXPANSIONS_PER_REQUEST.default as u32,
            budget_per_tick: limits::ROUTE_BUDGET_PER_TICK.default as u32,
            window_margin: limits::ROUTE_WINDOW_MARGIN.default,
            drift_cells: limits::ROUTE_DRIFT_CELLS.default,
            hunt_replan_interval: limits::HUNT_REPLAN_INTERVAL.default as u32,
            stuck_window: limits::STUCK_WINDOW.default as u32,
            stuck_min_progress: limits::STUCK_MIN_PROGRESS.default,
            unreachable_ticks: limits::UNREACHABLE_TICKS.default as u32,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            interval: limits::METRICS_INTERVAL.default as u32,
        }
    }
}

impl Default for FamineConfig {
    fn default() -> Self {
        Self {
            at_tick: limits::FAMINE_AT_TICK.default,
            food_multiplier: limits::FAMINE_FOOD_MULTIPLIER.default,
        }
    }
}

/// Why a scenario file could not be used.
#[derive(Debug, Clone, PartialEq)]
pub enum ScenarioError {
    /// The text is not valid TOML for a scenario: a syntax error, a wrong type, a missing
    /// required field or an unknown field. The message names the field and its line.
    Parse(String),
    /// The file parsed but one or more values are invalid.
    Invalid(Vec<ValidationError>),
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "{message}"),
            Self::Invalid(errors) => {
                for (index, error) in errors.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ScenarioError {}

impl ScenarioConfig {
    /// Parses and validates a scenario file (M-16). The format version is checked before
    /// anything else, so a file from a newer format gets a version message, not a list of
    /// unknown fields.
    pub fn from_toml_str(text: &str) -> Result<Self, ScenarioError> {
        let table: toml::Table = toml::from_str(text).map_err(|e| parse_error(&e))?;
        match table.get("format_version") {
            None => {
                return Err(ScenarioError::Invalid(vec![ValidationError::Invalid {
                    field: "format_version".to_string(),
                    message: format!(
                        "is required; this build reads format_version = {}",
                        limits::FORMAT_VERSION
                    ),
                }]));
            }
            Some(toml::Value::Integer(version))
                if *version != i64::from(limits::FORMAT_VERSION) =>
            {
                return Err(ScenarioError::Invalid(vec![unsupported_version(
                    &version.to_string(),
                )]));
            }
            Some(toml::Value::Integer(_)) => {}
            Some(other) => {
                return Err(ScenarioError::Invalid(vec![unsupported_version(
                    &other.to_string(),
                )]));
            }
        }
        let config: Self = toml::from_str(text).map_err(|e| parse_error(&e))?;
        config.validate().map_err(ScenarioError::Invalid)?;
        Ok(config)
    }

    /// Human-readable TOML with every field written out, so a saved file documents the
    /// whole model. Fails only for values a valid configuration cannot hold.
    pub fn to_toml_string(&self) -> Result<String, String> {
        let body = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        Ok(format!(
            "# Natural Selection Simulator scenario (format version {}).\n\
             # Every value is editable; see docs/SCENARIO_FORMAT.md for meanings and ranges.\n\n{body}",
            limits::FORMAT_VERSION
        ))
    }

    /// Checks every field against the `limits` module (I-07), plus the cross-field rules.
    /// Returns all problems found, not just the first. Nothing is silently corrected.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut e = Vec::new();
        if self.format_version != limits::FORMAT_VERSION {
            e.push(unsupported_version(&self.format_version.to_string()));
        }
        if let Some(seed) = self.seed {
            check_int(&mut e, "seed", seed, limits::SEED);
        }
        check_int(
            &mut e,
            "ticks_per_second",
            u64::from(self.ticks_per_second),
            limits::TICKS_PER_SECOND,
        );
        check_int(
            &mut e,
            "population_cap",
            u64::from(self.population_cap),
            limits::POPULATION_CAP,
        );

        let w = &self.world;
        check_float(&mut e, "world.width", w.width, limits::WORLD_SIZE);
        check_float(&mut e, "world.height", w.height, limits::WORLD_SIZE);
        check_float(&mut e, "world.cell_size", w.cell_size, limits::CELL_SIZE);
        check_float(
            &mut e,
            "world.spatial_cell_size",
            w.spatial_cell_size,
            limits::SPATIAL_CELL_SIZE,
        );
        if limits::WORLD_SIZE.contains(w.width)
            && limits::WORLD_SIZE.contains(w.height)
            && limits::CELL_SIZE.contains(w.cell_size)
        {
            let cells =
                (w.width / w.cell_size).ceil() as u64 * (w.height / w.cell_size).ceil() as u64;
            if cells > limits::MAX_GRID_CELLS {
                e.push(ValidationError::Invalid {
                    field: "world.cell_size".to_string(),
                    message: format!(
                        "gives {cells} terrain cells; at most {} are allowed, so use larger \
                         cells or a smaller world",
                        limits::MAX_GRID_CELLS
                    ),
                });
            }
        }

        let t = &self.terrain;
        if let Some(seed) = t.seed {
            check_int(&mut e, "terrain.seed", seed, limits::SEED);
        }
        check_float(
            &mut e,
            "terrain.water_level",
            t.water_level,
            limits::WATER_LEVEL,
        );
        check_float(
            &mut e,
            "terrain.rock_level",
            t.rock_level,
            limits::ROCK_LEVEL,
        );
        check_float(
            &mut e,
            "terrain.feature_scale",
            t.feature_scale,
            limits::FEATURE_SCALE,
        );
        if t.water_level.is_finite()
            && t.rock_level.is_finite()
            && t.rock_level - t.water_level < limits::MIN_PASSABLE_SHARE - 1e-6
        {
            e.push(ValidationError::Invalid {
                field: "terrain.rock_level".to_string(),
                message: format!(
                    "must be at least {} above terrain.water_level so at least {}% of the map \
                     is passable",
                    limits::MIN_PASSABLE_SHARE,
                    limits::MIN_PASSABLE_SHARE * 100.0
                ),
            });
        }

        let f = &self.food;
        check_float(
            &mut e,
            "food.max_per_cell",
            f.max_per_cell,
            limits::FOOD_MAX_PER_CELL,
        );
        check_float(
            &mut e,
            "food.regrowth_per_tick",
            f.regrowth_per_tick,
            limits::FOOD_REGROWTH_PER_TICK,
        );
        check_float(
            &mut e,
            "food.abundance",
            f.abundance,
            limits::FOOD_ABUNDANCE,
        );
        check_float(
            &mut e,
            "food.initial_fraction",
            f.initial_fraction,
            limits::FOOD_INITIAL_FRACTION,
        );
        check_float(
            &mut e,
            "food.intake_per_tick",
            f.intake_per_tick,
            limits::FOOD_INTAKE_PER_TICK,
        );
        check_float(
            &mut e,
            "food.energy_per_unit",
            f.energy_per_unit,
            limits::FOOD_ENERGY_PER_UNIT,
        );

        let en = &self.energy;
        check_float(
            &mut e,
            "energy.capacity",
            en.capacity,
            limits::ENERGY_CAPACITY,
        );
        check_float(&mut e, "energy.base", en.base, limits::ENERGY_BASE);
        check_float(&mut e, "energy.size", en.size, limits::ENERGY_SIZE);
        check_float(&mut e, "energy.move", en.movement, limits::ENERGY_MOVEMENT);
        check_float(&mut e, "energy.sense", en.sense, limits::ENERGY_SENSE);

        let m = &self.mutation;
        check_float(
            &mut e,
            "mutation.probability",
            m.probability,
            limits::MUTATION_PROBABILITY,
        );
        check_float(
            &mut e,
            "mutation.magnitude",
            m.magnitude,
            limits::MUTATION_MAGNITUDE,
        );

        let r = &self.reproduction;
        check_float(
            &mut e,
            "reproduction.threshold",
            r.threshold,
            limits::REPRODUCTION_THRESHOLD,
        );
        check_float(
            &mut e,
            "reproduction.offspring_energy_fraction",
            r.offspring_energy_fraction,
            limits::OFFSPRING_ENERGY_FRACTION,
        );
        check_float(
            &mut e,
            "reproduction.overhead",
            r.overhead,
            limits::REPRODUCTION_OVERHEAD,
        );
        check_float(
            &mut e,
            "reproduction.spawn_radius",
            r.spawn_radius,
            limits::SPAWN_RADIUS,
        );

        for (name, species) in [
            ("prey", &self.species.prey),
            ("predator", &self.species.predator),
        ] {
            let path = |part: &str| format!("species.{name}.{part}");
            match species.start {
                StartPopulation::Count(count) => {
                    check_int(
                        &mut e,
                        &path("start.count"),
                        u64::from(count),
                        limits::START_COUNT,
                    );
                    if u64::from(count) > u64::from(self.population_cap) {
                        e.push(ValidationError::Invalid {
                            field: path("start.count"),
                            message: format!(
                                "{count} is above the population cap of {}",
                                self.population_cap
                            ),
                        });
                    }
                }
                StartPopulation::Density(density) => {
                    check_float(
                        &mut e,
                        &path("start.density"),
                        density,
                        limits::START_DENSITY,
                    );
                }
            }
            check_float(
                &mut e,
                &path("start_energy_fraction"),
                species.start_energy_fraction,
                limits::START_ENERGY_FRACTION,
            );
            for id in TraitId::ALL {
                let start = species.trait_start(id);
                check_float(
                    &mut e,
                    &path(&format!("{}.mean", id.name())),
                    start.mean,
                    id.range(),
                );
                check_float(
                    &mut e,
                    &path(&format!("{}.spread", id.name())),
                    start.spread,
                    id.spread_range(),
                );
            }
            check_float(
                &mut e,
                &path("max_age_seconds"),
                species.max_age_seconds,
                limits::MAX_AGE_SECONDS,
            );
        }

        let p = &self.predation;
        check_float(
            &mut e,
            "predation.size_ratio",
            p.size_ratio,
            limits::SIZE_RATIO,
        );
        check_float(
            &mut e,
            "predation.capture_radius",
            p.capture_radius,
            limits::CAPTURE_RADIUS,
        );
        check_float(
            &mut e,
            "predation.efficiency",
            p.efficiency,
            limits::EFFICIENCY,
        );
        check_float(&mut e, "predation.content", p.content, limits::PREY_CONTENT);

        let b = &self.behavior;
        check_int(
            &mut e,
            "behavior.decision_interval",
            u64::from(b.decision_interval),
            limits::DECISION_INTERVAL,
        );
        check_int(
            &mut e,
            "behavior.threat_interval",
            u64::from(b.threat_interval),
            limits::THREAT_INTERVAL,
        );
        check_float(
            &mut e,
            "behavior.wander_speed_fraction",
            b.wander_speed_fraction,
            limits::WANDER_SPEED_FRACTION,
        );

        let n = &self.navigation;
        check_int(
            &mut e,
            "navigation.expansions_per_request",
            u64::from(n.expansions_per_request),
            limits::ROUTE_EXPANSIONS_PER_REQUEST,
        );
        check_int(
            &mut e,
            "navigation.budget_per_tick",
            u64::from(n.budget_per_tick),
            limits::ROUTE_BUDGET_PER_TICK,
        );
        if n.budget_per_tick < n.expansions_per_request {
            e.push(ValidationError::Invalid {
                field: "navigation.budget_per_tick".to_string(),
                message: "must be at least navigation.expansions_per_request".to_string(),
            });
        }
        check_float(
            &mut e,
            "navigation.window_margin",
            n.window_margin,
            limits::ROUTE_WINDOW_MARGIN,
        );
        check_float(
            &mut e,
            "navigation.drift_cells",
            n.drift_cells,
            limits::ROUTE_DRIFT_CELLS,
        );
        check_int(
            &mut e,
            "navigation.hunt_replan_interval",
            u64::from(n.hunt_replan_interval),
            limits::HUNT_REPLAN_INTERVAL,
        );
        check_int(
            &mut e,
            "navigation.stuck_window",
            u64::from(n.stuck_window),
            limits::STUCK_WINDOW,
        );
        check_float(
            &mut e,
            "navigation.stuck_min_progress",
            n.stuck_min_progress,
            limits::STUCK_MIN_PROGRESS,
        );
        check_int(
            &mut e,
            "navigation.unreachable_ticks",
            u64::from(n.unreachable_ticks),
            limits::UNREACHABLE_TICKS,
        );

        check_int(
            &mut e,
            "metrics.interval",
            u64::from(self.metrics.interval),
            limits::METRICS_INTERVAL,
        );

        if let Some(famine) = &self.famine {
            check_int(
                &mut e,
                "famine.at_tick",
                famine.at_tick,
                limits::FAMINE_AT_TICK,
            );
            check_float(
                &mut e,
                "famine.food_multiplier",
                famine.food_multiplier,
                limits::FAMINE_FOOD_MULTIPLIER,
            );
        }

        if e.is_empty() { Ok(()) } else { Err(e) }
    }

    /// The terrain seed used for a run with `run_seed`: the explicit value if present,
    /// otherwise one derived deterministically from `run_seed`, so different run seeds
    /// give different terrain unless a terrain seed is fixed (SCOPE 6.10).
    pub fn resolved_terrain_seed(&self, run_seed: u64) -> u64 {
        match self.terrain.seed {
            Some(seed) => seed,
            None => rng::derive_terrain_seed(run_seed),
        }
    }

    /// The species block for `species`.
    pub fn species(&self, species: crate::Species) -> &SpeciesConfig {
        match species {
            crate::Species::Prey => &self.species.prey,
            crate::Species::Predator => &self.species.predator,
        }
    }
}

fn parse_error(error: &toml::de::Error) -> ScenarioError {
    ScenarioError::Parse(error.to_string().trim_end().to_string())
}

fn unsupported_version(found: &str) -> ValidationError {
    ValidationError::Invalid {
        field: "format_version".to_string(),
        message: format!(
            "{found} is not supported; this build reads format_version = {}",
            limits::FORMAT_VERSION
        ),
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

fn check_int(errors: &mut Vec<ValidationError>, field: &str, value: u64, limits: IntLimits) {
    if value < limits.min {
        errors.push(ValidationError::BelowMinimum {
            field: field.to_string(),
            value,
            minimum: limits.min,
        });
    } else if value > limits.max {
        errors.push(ValidationError::AboveMaximum {
            field: field.to_string(),
            value,
            maximum: limits.max,
        });
    }
}

/// A configuration value that failed validation. Each variant names the field by its
/// scenario-file path, for example `species.prey.speed.mean`.
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
    /// A cross-field rule, version check or other constraint failed.
    Invalid { field: String, message: String },
}

impl ValidationError {
    /// Path of the offending configuration field.
    pub fn field(&self) -> &str {
        match self {
            Self::BelowMinimum { field, .. }
            | Self::AboveMaximum { field, .. }
            | Self::NotFinite { field }
            | Self::OutOfRange { field, .. }
            | Self::Invalid { field, .. } => field,
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
            Self::Invalid { field, message } => write!(f, "{field} {message}"),
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
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
        assert_eq!(config.terrain.seed, None);
        assert_eq!(config.world.width, 512.0);
        assert_eq!(config.species.prey.speed.mean, limits::SPEED.default);
        assert_eq!(
            config.species.predator.size.mean,
            limits::PREDATOR_SIZE_DEFAULT
        );
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

    /// T-VAL-1: for every float parameter, below the minimum, above the maximum and NaN
    /// are rejected naming the field, and the default is accepted.
    #[test]
    fn t_val_1_every_float_parameter_rejects_out_of_range_and_nan() {
        type Setter = fn(&mut ScenarioConfig, f32);
        let params: Vec<(&str, FloatLimits, Setter)> = vec![
            ("world.width", limits::WORLD_SIZE, |c, v| c.world.width = v),
            ("world.height", limits::WORLD_SIZE, |c, v| {
                c.world.height = v
            }),
            ("world.cell_size", limits::CELL_SIZE, |c, v| {
                c.world.cell_size = v
            }),
            (
                "world.spatial_cell_size",
                limits::SPATIAL_CELL_SIZE,
                |c, v| c.world.spatial_cell_size = v,
            ),
            ("terrain.water_level", limits::WATER_LEVEL, |c, v| {
                c.terrain.water_level = v;
                c.terrain.rock_level = 1.0;
            }),
            ("terrain.rock_level", limits::ROCK_LEVEL, |c, v| {
                c.terrain.rock_level = v;
                c.terrain.water_level = 0.0;
            }),
            ("terrain.feature_scale", limits::FEATURE_SCALE, |c, v| {
                c.terrain.feature_scale = v
            }),
            ("food.max_per_cell", limits::FOOD_MAX_PER_CELL, |c, v| {
                c.food.max_per_cell = v
            }),
            (
                "food.regrowth_per_tick",
                limits::FOOD_REGROWTH_PER_TICK,
                |c, v| c.food.regrowth_per_tick = v,
            ),
            ("food.abundance", limits::FOOD_ABUNDANCE, |c, v| {
                c.food.abundance = v
            }),
            (
                "food.initial_fraction",
                limits::FOOD_INITIAL_FRACTION,
                |c, v| c.food.initial_fraction = v,
            ),
            (
                "food.intake_per_tick",
                limits::FOOD_INTAKE_PER_TICK,
                |c, v| c.food.intake_per_tick = v,
            ),
            (
                "food.energy_per_unit",
                limits::FOOD_ENERGY_PER_UNIT,
                |c, v| c.food.energy_per_unit = v,
            ),
            ("energy.capacity", limits::ENERGY_CAPACITY, |c, v| {
                c.energy.capacity = v
            }),
            ("energy.base", limits::ENERGY_BASE, |c, v| c.energy.base = v),
            ("energy.size", limits::ENERGY_SIZE, |c, v| c.energy.size = v),
            ("energy.move", limits::ENERGY_MOVEMENT, |c, v| {
                c.energy.movement = v
            }),
            ("energy.sense", limits::ENERGY_SENSE, |c, v| {
                c.energy.sense = v
            }),
            (
                "mutation.probability",
                limits::MUTATION_PROBABILITY,
                |c, v| c.mutation.probability = v,
            ),
            ("mutation.magnitude", limits::MUTATION_MAGNITUDE, |c, v| {
                c.mutation.magnitude = v
            }),
            (
                "reproduction.threshold",
                limits::REPRODUCTION_THRESHOLD,
                |c, v| c.reproduction.threshold = v,
            ),
            (
                "reproduction.offspring_energy_fraction",
                limits::OFFSPRING_ENERGY_FRACTION,
                |c, v| c.reproduction.offspring_energy_fraction = v,
            ),
            (
                "reproduction.overhead",
                limits::REPRODUCTION_OVERHEAD,
                |c, v| c.reproduction.overhead = v,
            ),
            ("reproduction.spawn_radius", limits::SPAWN_RADIUS, |c, v| {
                c.reproduction.spawn_radius = v
            }),
            (
                "species.prey.start.density",
                limits::START_DENSITY,
                |c, v| c.species.prey.start = StartPopulation::Density(v),
            ),
            (
                "species.predator.start.density",
                limits::START_DENSITY,
                |c, v| c.species.predator.start = StartPopulation::Density(v),
            ),
            (
                "species.prey.start_energy_fraction",
                limits::START_ENERGY_FRACTION,
                |c, v| c.species.prey.start_energy_fraction = v,
            ),
            (
                "species.predator.start_energy_fraction",
                limits::START_ENERGY_FRACTION,
                |c, v| c.species.predator.start_energy_fraction = v,
            ),
            ("species.prey.speed.mean", limits::SPEED, |c, v| {
                c.species.prey.speed.mean = v
            }),
            ("species.prey.size.mean", limits::SIZE, |c, v| {
                c.species.prey.size.mean = v
            }),
            ("species.prey.sense.mean", limits::SENSE, |c, v| {
                c.species.prey.sense.mean = v
            }),
            ("species.prey.speed.spread", limits::SPEED_SPREAD, |c, v| {
                c.species.prey.speed.spread = v
            }),
            ("species.prey.size.spread", limits::SIZE_SPREAD, |c, v| {
                c.species.prey.size.spread = v
            }),
            ("species.prey.sense.spread", limits::SENSE_SPREAD, |c, v| {
                c.species.prey.sense.spread = v
            }),
            ("species.predator.speed.mean", limits::SPEED, |c, v| {
                c.species.predator.speed.mean = v
            }),
            ("species.predator.size.mean", limits::SIZE, |c, v| {
                c.species.predator.size.mean = v
            }),
            ("species.predator.sense.mean", limits::SENSE, |c, v| {
                c.species.predator.sense.mean = v
            }),
            (
                "species.predator.speed.spread",
                limits::SPEED_SPREAD,
                |c, v| c.species.predator.speed.spread = v,
            ),
            (
                "species.predator.size.spread",
                limits::SIZE_SPREAD,
                |c, v| c.species.predator.size.spread = v,
            ),
            (
                "species.predator.sense.spread",
                limits::SENSE_SPREAD,
                |c, v| c.species.predator.sense.spread = v,
            ),
            (
                "species.prey.max_age_seconds",
                limits::MAX_AGE_SECONDS,
                |c, v| c.species.prey.max_age_seconds = v,
            ),
            (
                "species.predator.max_age_seconds",
                limits::MAX_AGE_SECONDS,
                |c, v| c.species.predator.max_age_seconds = v,
            ),
            ("predation.size_ratio", limits::SIZE_RATIO, |c, v| {
                c.predation.size_ratio = v
            }),
            (
                "predation.capture_radius",
                limits::CAPTURE_RADIUS,
                |c, v| c.predation.capture_radius = v,
            ),
            ("predation.efficiency", limits::EFFICIENCY, |c, v| {
                c.predation.efficiency = v
            }),
            ("predation.content", limits::PREY_CONTENT, |c, v| {
                c.predation.content = v
            }),
            (
                "behavior.wander_speed_fraction",
                limits::WANDER_SPEED_FRACTION,
                |c, v| c.behavior.wander_speed_fraction = v,
            ),
            (
                "navigation.window_margin",
                limits::ROUTE_WINDOW_MARGIN,
                |c, v| c.navigation.window_margin = v,
            ),
            (
                "navigation.drift_cells",
                limits::ROUTE_DRIFT_CELLS,
                |c, v| c.navigation.drift_cells = v,
            ),
            (
                "navigation.stuck_min_progress",
                limits::STUCK_MIN_PROGRESS,
                |c, v| c.navigation.stuck_min_progress = v,
            ),
            (
                "famine.food_multiplier",
                limits::FAMINE_FOOD_MULTIPLIER,
                |c, v| {
                    c.famine
                        .get_or_insert_with(FamineConfig::default)
                        .food_multiplier = v
                },
            ),
        ];
        for (field, range, set) in params {
            let mut ok = ScenarioConfig::default();
            set(&mut ok, range.default);
            assert_eq!(ok.validate(), Ok(()), "{field} default must be valid");
            let below = range.min - (range.max - range.min).max(1.0) * 0.5;
            let above = range.max + (range.max - range.min).max(1.0) * 0.5;
            for bad in [below, above, f32::NAN, f32::INFINITY] {
                let mut config = ScenarioConfig::default();
                set(&mut config, bad);
                let errors = config.validate().expect_err(field);
                assert!(
                    errors.iter().any(|e| e.field() == field),
                    "{field} = {bad}: {errors:?}"
                );
            }
        }
    }

    /// T-VAL-1 for integer parameters: below the minimum and above the maximum are
    /// rejected naming the field; the default is accepted.
    #[test]
    fn t_val_1_every_integer_parameter_rejects_out_of_range() {
        type Setter = fn(&mut ScenarioConfig, u64);
        let params: Vec<(&str, IntLimits, Setter)> = vec![
            ("ticks_per_second", limits::TICKS_PER_SECOND, |c, v| {
                c.ticks_per_second = v as u32
            }),
            ("population_cap", limits::POPULATION_CAP, |c, v| {
                c.population_cap = v as u32
            }),
            (
                "behavior.decision_interval",
                limits::DECISION_INTERVAL,
                |c, v| c.behavior.decision_interval = v as u32,
            ),
            (
                "behavior.threat_interval",
                limits::THREAT_INTERVAL,
                |c, v| c.behavior.threat_interval = v as u32,
            ),
            (
                "navigation.expansions_per_request",
                limits::ROUTE_EXPANSIONS_PER_REQUEST,
                |c, v| {
                    c.navigation.expansions_per_request = v as u32;
                    c.navigation.budget_per_tick = limits::ROUTE_BUDGET_PER_TICK.max as u32;
                },
            ),
            (
                "navigation.budget_per_tick",
                limits::ROUTE_BUDGET_PER_TICK,
                |c, v| {
                    c.navigation.budget_per_tick = v as u32;
                    c.navigation.expansions_per_request =
                        limits::ROUTE_EXPANSIONS_PER_REQUEST.min as u32;
                },
            ),
            (
                "navigation.hunt_replan_interval",
                limits::HUNT_REPLAN_INTERVAL,
                |c, v| c.navigation.hunt_replan_interval = v as u32,
            ),
            ("navigation.stuck_window", limits::STUCK_WINDOW, |c, v| {
                c.navigation.stuck_window = v as u32
            }),
            (
                "navigation.unreachable_ticks",
                limits::UNREACHABLE_TICKS,
                |c, v| c.navigation.unreachable_ticks = v as u32,
            ),
            ("metrics.interval", limits::METRICS_INTERVAL, |c, v| {
                c.metrics.interval = v as u32
            }),
            ("species.prey.start.count", limits::START_COUNT, |c, v| {
                c.species.prey.start = StartPopulation::Count(v as u32)
            }),
            (
                "species.predator.start.count",
                limits::START_COUNT,
                |c, v| c.species.predator.start = StartPopulation::Count(v as u32),
            ),
            ("famine.at_tick", limits::FAMINE_AT_TICK, |c, v| {
                c.famine.get_or_insert_with(FamineConfig::default).at_tick = v
            }),
            ("seed", limits::SEED, |c, v| c.seed = Some(v)),
            ("terrain.seed", limits::SEED, |c, v| {
                c.terrain.seed = Some(v)
            }),
        ];
        for (field, range, set) in params {
            let mut ok = ScenarioConfig::default();
            set(&mut ok, range.default);
            assert_eq!(ok.validate(), Ok(()), "{field} default must be valid");
            let mut candidates = vec![range.max + 1];
            if range.min > 0 {
                candidates.push(range.min - 1);
            }
            for bad in candidates {
                let mut config = ScenarioConfig::default();
                set(&mut config, bad);
                let errors = config.validate().expect_err(field);
                assert!(
                    errors.iter().any(|e| e.field() == field),
                    "{field} = {bad}: {errors:?}"
                );
            }
        }
    }

    /// T-VAL-2: cross-field rules.
    #[test]
    fn t_val_2_cross_field_rules() {
        let mut config = ScenarioConfig::default();
        config.terrain.water_level = 0.5;
        config.terrain.rock_level = 0.6;
        assert_eq!(fields(&config), ["terrain.rock_level"]);

        let mut config = ScenarioConfig::default();
        config.world.width = 4096.0;
        config.world.height = 4096.0;
        config.world.cell_size = 1.0;
        assert_eq!(
            fields(&config),
            ["world.cell_size"],
            "16.7M cells is too many"
        );

        let mut config = ScenarioConfig::default();
        config.navigation.budget_per_tick = 100;
        config.navigation.expansions_per_request = 200;
        assert_eq!(fields(&config), ["navigation.budget_per_tick"]);

        let mut config = ScenarioConfig::default();
        config.population_cap = 100;
        config.species.prey.start = StartPopulation::Count(101);
        assert_eq!(fields(&config), ["species.prey.start.count"]);
    }

    #[test]
    fn range_boundaries_are_inclusive() {
        let mut config = ScenarioConfig::default();
        config.world.width = limits::WORLD_SIZE.min;
        config.world.height = limits::WORLD_SIZE.max;
        config.species.prey.start = StartPopulation::Count(0);
        config.species.prey.start_energy_fraction = limits::START_ENERGY_FRACTION.max;
        config.species.prey.size.mean = limits::SIZE.min;
        config.species.prey.size.spread = 0.0;
        config.species.prey.sense.spread = limits::SENSE_SPREAD.max;
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn explicit_terrain_seed_is_preserved_for_every_run_seed() {
        let mut config = ScenarioConfig::default();
        config.terrain.seed = Some(1001);
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
        assert_ne!(
            config.resolved_terrain_seed(1),
            config.resolved_terrain_seed(2)
        );
        assert_ne!(config.resolved_terrain_seed(1), 1);
    }

    #[test]
    fn densities_resolve_against_passable_cells() {
        let species = SpeciesConfig::prey_default();
        assert_eq!(species.resolved_start_count(50_000), 2_000);
        let counted = SpeciesConfig {
            start: StartPopulation::Count(7),
            ..species
        };
        assert_eq!(counted.resolved_start_count(50_000), 7);
    }

    /// A configuration with every optional part present and awkward float values.
    fn rich_config() -> ScenarioConfig {
        let mut config = ScenarioConfig::default();
        config.name = "Round trip \"quoted\" name".to_string();
        config.description = "Line one\nline two".to_string();
        config.learning_goal = "Watch speed".to_string();
        config.seed = Some(42);
        config.terrain.seed = Some(1001);
        config.terrain.water_level = 0.1;
        config.food.regrowth_per_tick = 0.0005;
        config.mutation.magnitude = 1.0 / 3.0;
        config.species.prey.speed.mean = 0.1 + 0.2;
        config.species.prey.start = StartPopulation::Count(1234);
        config.species.predator.sense.spread = 1e-7;
        config.species.predator.max_age_seconds = 600.5;
        config.predation.efficiency = 0.6;
        config.famine = Some(FamineConfig {
            at_tick: 36_000,
            food_multiplier: 0.4,
        });
        config
    }

    /// T-SCN-1: configuration to TOML and back is equal, with exact float round trips.
    #[test]
    fn t_scn_1_toml_round_trip_is_exact() {
        for config in [ScenarioConfig::default(), rich_config()] {
            let text = config.to_toml_string().expect("serializes");
            let back = ScenarioConfig::from_toml_str(&text).expect("parses");
            assert_eq!(back, config, "{text}");
            for (a, b) in [
                (back.species.prey.speed.mean, config.species.prey.speed.mean),
                (back.mutation.magnitude, config.mutation.magnitude),
                (
                    back.species.predator.sense.spread,
                    config.species.predator.sense.spread,
                ),
            ] {
                assert_eq!(a.to_bits(), b.to_bits(), "bit-exact");
            }
        }
        // Many awkward f32 values survive the text form bit for bit.
        let mut config = ScenarioConfig::default();
        let mut value = 0.001_f32;
        while value < 0.9 {
            config.food.initial_fraction = value;
            let text = config.to_toml_string().expect("serializes");
            let back = ScenarioConfig::from_toml_str(&text).expect("parses");
            assert_eq!(
                back.food.initial_fraction.to_bits(),
                value.to_bits(),
                "{value}"
            );
            value = value * 1.37 + 0.000_123;
        }
    }

    #[test]
    fn saved_files_are_readable_toml_with_the_documented_layout() {
        let text = rich_config().to_toml_string().expect("serializes");
        for needle in [
            "format_version = 1",
            "[terrain]",
            "seed = 1001",
            "[species.prey]",
            "[species.predator]",
            "[predation]",
            "size_ratio = 1.2",
            "[famine]",
            "move = 0.01",
        ] {
            assert!(text.contains(needle), "missing {needle}:\n{text}");
        }
        assert!(!text.contains("0.6000000"), "shortest float form:\n{text}");
    }

    /// Missing optional fields and tables take their defaults.
    #[test]
    fn minimal_file_takes_defaults() {
        let config =
            ScenarioConfig::from_toml_str("format_version = 1\nname = \"Tiny\"\n").expect("valid");
        assert_eq!(config.name, "Tiny");
        assert_eq!(config.species, SpeciesBlocks::default());
        assert_eq!(config.food, FoodConfig::default());
        let config = ScenarioConfig::from_toml_str(
            "format_version = 1\n[world]\nwidth = 256.0\n[species.predator]\n\
             start = { count = 5 }\nstart_energy_fraction = 0.5\n\
             speed = { mean = 1.5, spread = 0.0 }\nsize = { mean = 2.0, spread = 0.0 }\n\
             sense = { mean = 12.0, spread = 0.0 }\n",
        )
        .expect("valid");
        assert_eq!(config.world.width, 256.0);
        assert_eq!(config.world.height, 512.0, "default");
        assert_eq!(config.species.predator.start, StartPopulation::Count(5));
        assert_eq!(config.species.predator.max_age_seconds, 0.0);
        assert_eq!(config.species.prey, SpeciesConfig::prey_default());
    }

    /// T-SCN-3: out-of-range values, unknown fields and unsupported versions are rejected
    /// with messages naming the field.
    #[test]
    fn t_scn_3_invalid_files_are_rejected_naming_the_field() {
        let unknown = ScenarioConfig::from_toml_str("format_version = 1\n[food]\nbogus = 1.0\n")
            .expect_err("unknown field");
        assert!(
            matches!(&unknown, ScenarioError::Parse(m) if m.contains("bogus")),
            "{unknown}"
        );

        let unknown_top = ScenarioConfig::from_toml_str("format_version = 1\nspeed = 3\n")
            .expect_err("unknown field");
        assert!(unknown_top.to_string().contains("speed"), "{unknown_top}");

        let range =
            ScenarioConfig::from_toml_str("format_version = 1\n[predation]\nsize_ratio = 50.0\n")
                .expect_err("out of range");
        match &range {
            ScenarioError::Invalid(errors) => {
                assert_eq!(errors[0].field(), "predation.size_ratio");
                assert!(range.to_string().contains("predation.size_ratio = 50"));
            }
            other => panic!("{other:?}"),
        }

        for version in ["format_version = 2", "format_version = \"1\""] {
            let error = ScenarioConfig::from_toml_str(&format!("{version}\nname = \"x\"\n"))
                .expect_err("unsupported version");
            assert!(error.to_string().contains("format_version"), "{error}");
            assert!(error.to_string().contains("not supported"), "{error}");
        }
        let missing = ScenarioConfig::from_toml_str("name = \"x\"\n").expect_err("no version");
        assert!(missing.to_string().contains("format_version"), "{missing}");

        let wrong_type =
            ScenarioConfig::from_toml_str("format_version = 1\n[world]\nwidth = \"wide\"\n")
                .expect_err("wrong type");
        assert!(wrong_type.to_string().contains("width"), "{wrong_type}");

        let partial_species = ScenarioConfig::from_toml_str(
            "format_version = 1\n[species.predator]\nstart = { count = 5 }\n",
        )
        .expect_err("incomplete species table");
        assert!(
            partial_species
                .to_string()
                .contains("start_energy_fraction"),
            "{partial_species}"
        );

        let bad_start = ScenarioConfig::from_toml_str(
            "format_version = 1\n[species.prey]\nstart = { amount = 5 }\n\
             start_energy_fraction = 0.5\nspeed = { mean = 1.0, spread = 0.1 }\n\
             size = { mean = 1.0, spread = 0.1 }\nsense = { mean = 10.0, spread = 1.0 }\n",
        )
        .expect_err("unknown start kind");
        assert!(bad_start.to_string().contains("amount"), "{bad_start}");

        let syntax =
            ScenarioConfig::from_toml_str("format_version = 1\n[world\n").expect_err("syntax");
        assert!(matches!(syntax, ScenarioError::Parse(_)));
    }
}
