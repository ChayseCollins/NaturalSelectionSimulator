//! Natural Selection Simulator core library.
//!
//! Every simulation rule lives in this crate; `app` and `headless` are thin shells over its
//! public API (I-06). This crate never depends on graphics, UI or windowing crates (I-01),
//! performs no file I/O (I-08) and never reads the clock outside the optional `profile`
//! timers, which never affect results.
//!
//! Implemented: the deterministic core (M-02) with the complete state hash; prey and
//! predators with three heritable traits, the energy formula, starvation, age and the
//! population safety cap (M-03); asexual reproduction with mutation and generations
//! (M-04); predation by contact (M-05); procedural terrain with food fertility, speed
//! multipliers and impassable cells (M-06); the food grid (M-07); the spatial grid (M-08);
//! goal-directed navigation with bounded, budgeted route search and stuck recovery
//! (M-09); chart metrics (M-13); scenario files (M-16); the frozen-population benchmark
//! option (M-18); and the optional famine (S-01) and old-age death (S-02).

#![forbid(unsafe_code)]

mod behavior;
mod config;
pub mod energy;
mod food;
mod hash;
pub mod limits;
mod metrics;
mod nav;
mod organisms;
#[cfg(feature = "profile")]
pub mod profile;
pub mod reproduction;
mod rng;
mod spatial;
pub mod terrain;
pub mod traits;

use std::collections::VecDeque;

pub use behavior::{MIN_USEFUL_TARGET_BITES, TARGET_SWITCH_FRACTION};
pub use config::{
    BehaviorConfig, EnergyConfig, FamineConfig, FoodConfig, MetricsConfig, MutationConfig,
    NavigationConfig, PredationConfig, ReproductionConfig, ScenarioConfig, ScenarioError,
    SpeciesBlocks, SpeciesConfig, StartPopulation, TerrainConfig, TraitStart, ValidationError,
    WorldConfig,
};
pub use food::FoodGrid;
pub use metrics::{
    Counters, MetricSample, MetricsHistory, SpeciesCounters, SpeciesStats, SummaryMetrics,
};
pub use organisms::{Goal, Organisms, Species};
pub use spatial::SpatialGrid;
pub use terrain::{GridGeometry, Terrain, TerrainClass};
pub use traits::{TraitId, Traits};

use hash::Fnv1a64;
use nav::{RouteSearch, SearchOutcome};
use organisms::OrganismRow;
use rng::RandomDomains;

/// Organisms blocked by terrain for at least this many consecutive ticks count as stuck
/// in [`Simulation::stuck_count`] (T-NAV-7a, P): pressed against terrain for a second.
pub const STUCK_REPORT_TICKS: u32 = 30;

/// Why an organism is removed at the end of a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeathCause {
    Starvation,
    Predation,
    Age,
}

/// A queued route request (SCOPE 6.8 step 4), served in (tick, ID) order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RouteRequest {
    tick: u64,
    id: u64,
}

/// One hand-placed organism for a constructed experiment or test
/// ([`Simulation::from_placements`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub species: Species,
    pub x: f32,
    pub y: f32,
    pub traits: Traits,
    /// Starting energy; `None` uses the species' starting fraction of capacity.
    pub energy: Option<f32>,
}

/// Everything the inspector shows about one organism (M-12, T-INSP-2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrganismSnapshot {
    pub id: u64,
    pub species: Species,
    pub x: f32,
    pub y: f32,
    pub energy: f32,
    pub capacity: f32,
    pub age_ticks: u32,
    pub age_seconds: f64,
    pub generation: u32,
    pub traits: Traits,
    pub goal: Goal,
    pub terrain: TerrainClass,
}

/// A deterministic, fixed-step simulation run: one scenario executed with one run seed.
///
/// The same configuration and run seed always produce the same sequence of states: there
/// is no wall-clock time, OS randomness or hash-map iteration anywhere in stepping
/// (SCOPE 6.10).
#[derive(Debug, Clone, PartialEq)]
pub struct Simulation {
    config: ScenarioConfig,
    run_seed: u64,
    terrain_seed: u64,
    tick: u64,
    rng: RandomDomains,
    terrain: Terrain,
    organisms: Organisms,
    food: FoodGrid,
    /// ID given to the next organism created; IDs are never reused within a run.
    next_id: u64,
    counters: Counters,
    /// Cumulative food removed from cells by feeding, in food units (not energy).
    food_consumed: f64,
    /// Tick each species reached zero (prey, predator), if it has.
    extinct_at: [Option<u64>; 2],
    /// Whether each species has ever had a living organism.
    ever_alive: [bool; 2],
    population_cap_reached: bool,
    /// Benchmark option: births and deaths are suppressed (SCOPE 10.4).
    frozen_population: bool,
    route_queue: VecDeque<RouteRequest>,
    metrics: MetricsHistory,

    // ----- Derived and per-tick scratch: not simulation state, not hashed. -----
    prey_grid: SpatialGrid,
    predator_grid: SpatialGrid,
    /// `(id, storage index)` sorted by ID; rebuilt with the grids.
    id_order: Vec<(u64, u32)>,
    effort: Vec<f32>,
    dying: Vec<bool>,
    removals: Vec<(usize, DeathCause)>,
    births: Vec<OrganismRow>,
    new_requests: Vec<RouteRequest>,
    route_search: RouteSearch,
    route_expansions_this_tick: u32,
    max_request_expansions_this_tick: u32,
    #[cfg(feature = "profile")]
    profile: profile::PhaseTimes,
}

impl Simulation {
    /// Validates `config`, generates terrain from the terrain random domain, and builds
    /// the run identified by `run_seed` at tick 0: the food grid (no randomness) and the
    /// starting populations (initial-population domain), prey first, then predators.
    pub fn new(config: &ScenarioConfig, run_seed: u64) -> Result<Self, Vec<ValidationError>> {
        config.validate()?;
        let terrain_seed = config.resolved_terrain_seed(run_seed);
        let mut rng = RandomDomains::new(terrain_seed, run_seed);
        let terrain = Terrain::generate(&config.world, &config.terrain, &mut rng.terrain);
        let mut simulation = Self::empty(config.clone(), run_seed, terrain_seed, rng, terrain);
        simulation.spawn_initial_population()?;
        simulation.finish_construction();
        Ok(simulation)
    }

    /// Like [`Simulation::new`] on a given terrain, such as an ASCII test map. The world
    /// size and cell size are taken from the terrain; the terrain random domain is unused.
    pub fn with_terrain(
        config: &ScenarioConfig,
        run_seed: u64,
        terrain: Terrain,
    ) -> Result<Self, Vec<ValidationError>> {
        let config = Self::config_for_terrain(config, &terrain);
        config.validate()?;
        let terrain_seed = config.resolved_terrain_seed(run_seed);
        let rng = RandomDomains::new(terrain_seed, run_seed);
        let mut simulation = Self::empty(config, run_seed, terrain_seed, rng, terrain);
        simulation.spawn_initial_population()?;
        simulation.finish_construction();
        Ok(simulation)
    }

    /// A constructed experiment: the given terrain and exactly the given organisms, in
    /// order (IDs 0, 1, ...), with no random starting population. Each organism must stand
    /// on a passable cell and have traits within range.
    pub fn from_placements(
        config: &ScenarioConfig,
        run_seed: u64,
        terrain: Terrain,
        placements: &[Placement],
    ) -> Result<Self, Vec<ValidationError>> {
        let config = Self::config_for_terrain(config, &terrain);
        config.validate()?;
        let terrain_seed = config.resolved_terrain_seed(run_seed);
        let rng = RandomDomains::new(terrain_seed, run_seed);
        let mut simulation = Self::empty(config, run_seed, terrain_seed, rng, terrain);
        let mut errors = Vec::new();
        for (k, placement) in placements.iter().enumerate() {
            if !simulation.terrain.passable_at(placement.x, placement.y) {
                errors.push(ValidationError::Invalid {
                    field: format!("placement[{k}]"),
                    message: "is not on a passable cell inside the world".to_string(),
                });
            }
            for id in TraitId::ALL {
                if !id.range().contains(placement.traits.get(id)) {
                    errors.push(ValidationError::Invalid {
                        field: format!("placement[{k}].{}", id.name()),
                        message: "is outside the trait's range".to_string(),
                    });
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        for placement in placements {
            let capacity = energy::capacity(&simulation.config.energy, placement.traits.size);
            let energy = placement.energy.unwrap_or_else(|| {
                simulation
                    .config
                    .species(placement.species)
                    .start_energy_fraction
                    * capacity
            });
            let id = simulation.next_id;
            simulation.organisms.push_row(OrganismRow::new(
                id,
                placement.species,
                (placement.x, placement.y),
                energy.clamp(0.0, capacity),
                0,
                placement.traits,
            ));
            simulation.next_id += 1;
        }
        simulation.finish_construction();
        Ok(simulation)
    }

    fn config_for_terrain(config: &ScenarioConfig, terrain: &Terrain) -> ScenarioConfig {
        let mut config = config.clone();
        let g = terrain.geometry();
        config.world.width = g.world_width();
        config.world.height = g.world_height();
        config.world.cell_size = g.cell_size();
        config
    }

    fn empty(
        config: ScenarioConfig,
        run_seed: u64,
        terrain_seed: u64,
        rng: RandomDomains,
        terrain: Terrain,
    ) -> Self {
        let food = FoodGrid::new(&terrain, &config.food);
        let world = &config.world;
        let grid = SpatialGrid::new(world.width, world.height, world.spatial_cell_size);
        Self {
            metrics: MetricsHistory::new(config.metrics.interval),
            run_seed,
            terrain_seed,
            tick: 0,
            rng,
            organisms: Organisms::default(),
            food,
            terrain,
            next_id: 0,
            counters: Counters::default(),
            food_consumed: 0.0,
            extinct_at: [None, None],
            ever_alive: [false, false],
            population_cap_reached: false,
            frozen_population: false,
            route_queue: VecDeque::new(),
            prey_grid: grid.clone(),
            predator_grid: grid,
            id_order: Vec::new(),
            effort: Vec::new(),
            dying: Vec::new(),
            removals: Vec::new(),
            births: Vec::new(),
            new_requests: Vec::new(),
            route_search: RouteSearch::default(),
            route_expansions_this_tick: 0,
            max_request_expansions_this_tick: 0,
            #[cfg(feature = "profile")]
            profile: profile::PhaseTimes::default(),
            config,
        }
    }

    fn finish_construction(&mut self) {
        self.rebuild_spatial_grid();
        for species in Species::ALL {
            self.ever_alive[species.index()] = self.organisms.count(species) > 0;
        }
        let sample = self.metric_sample(0);
        self.metrics.push(sample);
    }

    /// Creates the starting prey, then the starting predators, from the initial-population
    /// random domain only.
    ///
    /// Counts come from each species' `start` (a count, or a density per 1,000 passable
    /// cells). For each organism in ID order the draws are, in this order: one 64-bit draw
    /// choosing a passable cell uniformly, one whose two 24-bit halves give the position
    /// inside that cell, then speed, size and sense (two draws each, normal with the
    /// configured mean and spread, clamped to the trait's range). That is eight 64-bit
    /// draws per organism whatever the parameters. Starting energy is
    /// `start_energy_fraction × capacity(size)`.
    fn spawn_initial_population(&mut self) -> Result<(), Vec<ValidationError>> {
        let passable = self.terrain.passable_cells().len();
        let prey = self.config.species.prey.resolved_start_count(passable);
        let predators = self.config.species.predator.resolved_start_count(passable);
        if prey + predators > u64::from(self.config.population_cap) {
            return Err(vec![ValidationError::Invalid {
                field: "species".to_string(),
                message: format!(
                    "starting populations ({prey} prey and {predators} predators on {passable} \
                     passable cells) exceed the population cap of {}",
                    self.config.population_cap
                ),
            }]);
        }
        if passable == 0 && prey + predators > 0 {
            return Err(vec![ValidationError::Invalid {
                field: "terrain".to_string(),
                message: "has no passable cell to place organisms on".to_string(),
            }]);
        }
        self.organisms = Organisms::with_capacity((prey + predators) as usize);
        for (species, count) in [(Species::Prey, prey), (Species::Predator, predators)] {
            for _ in 0..count {
                self.spawn_random(species);
            }
        }
        Ok(())
    }

    fn spawn_random(&mut self, species: Species) {
        let block = self.config.species(species).clone();
        let rng = &mut self.rng.initial_population;
        let cells = self.terrain.passable_cells();
        let pick = rng::below(rng, cells.len() as u64) as usize;
        let cell = cells[pick] as usize;
        let (fx, fy) = rng::two_unit_f32(rng);
        let g = self.terrain.geometry();
        let (cx, cy) = g.coords(cell);
        let size = g.cell_size();
        let x = ((cx as f32 + fx) * size).min(g.world_width());
        let y = ((cy as f32 + fy) * size).min(g.world_height());
        let mut sample = |id: TraitId| {
            let start = block.trait_start(id);
            let range = id.range();
            let value = f64::from(start.mean) + f64::from(start.spread) * rng::standard_normal(rng);
            (value as f32).clamp(range.min, range.max)
        };
        let traits = Traits {
            speed: sample(TraitId::Speed),
            size: sample(TraitId::Size),
            sense: sample(TraitId::Sense),
        };
        let energy =
            block.start_energy_fraction * energy::capacity(&self.config.energy, traits.size);
        self.organisms.push_row(OrganismRow::new(
            self.next_id,
            species,
            (x, y),
            energy,
            0,
            traits,
        ));
        self.next_id += 1;
    }

    /// Advances exactly one fixed tick, running the phases of SCOPE 6.9 in order. Takes no
    /// time argument (I-05).
    ///
    /// Phase 1 (rebuild the spatial grid) runs at the end of the previous tick and at
    /// construction, after births and deaths, so between ticks the grids always match the
    /// stored positions (picking uses them too).
    pub fn step(&mut self) {
        self.route_expansions_this_tick = 0;
        self.max_request_expansions_this_tick = 0;
        self.timed(1, Self::sense_and_decide);
        self.timed(2, Self::serve_route_requests);
        self.timed(3, Self::move_organisms);
        self.timed(4, Self::feed);
        self.timed(5, Self::apply_energy_costs);
        self.timed(6, Self::age_and_check_deaths);
        self.timed(7, Self::collect_births);
        self.timed(8, Self::apply_removals_and_births);
        self.timed(9, Self::regrow_food);
        self.timed(0, Self::rebuild_spatial_grid);
        self.timed(10, Self::sample_metrics);
        // Requests made by stuck recovery during movement wait for the next tick's phase 3.
        self.flush_route_requests();
        self.tick += 1;
        debug_assert!(self.organisms.columns_aligned());
    }

    #[cfg(feature = "profile")]
    fn timed(&mut self, phase: usize, run: fn(&mut Self)) {
        let started = std::time::Instant::now();
        run(self);
        self.profile.add(phase, started.elapsed());
    }

    #[cfg(not(feature = "profile"))]
    fn timed(&mut self, _phase: usize, run: fn(&mut Self)) {
        run(self);
    }

    /// Accumulated wall time per phase (feature `profile` only). Never affects results.
    #[cfg(feature = "profile")]
    pub fn phase_times(&self) -> &profile::PhaseTimes {
        &self.profile
    }

    /// Clears the phase timers, for example after benchmark warm-up.
    #[cfg(feature = "profile")]
    pub fn reset_phase_times(&mut self) {
        self.profile.reset();
    }

    /// Advances exactly `n` ticks; identical to calling [`Simulation::step`] `n` times.
    pub fn step_n(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    // ----- Read-only API -----

    /// Number of ticks completed. This integer is the authoritative simulation clock.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Fixed ticks per simulated second, from the configuration.
    pub fn ticks_per_second(&self) -> u32 {
        self.config.ticks_per_second
    }

    /// Simulated seconds elapsed, for display only: computed as `tick / ticks_per_second`
    /// in `f64` each call, never accumulated.
    pub fn sim_seconds(&self) -> f64 {
        self.tick as f64 / f64::from(self.config.ticks_per_second)
    }

    pub fn run_seed(&self) -> u64 {
        self.run_seed
    }

    /// The terrain seed in use (explicit or derived from the run seed).
    pub fn terrain_seed(&self) -> u64 {
        self.terrain_seed
    }

    /// The validated configuration this simulation was built with.
    pub fn config(&self) -> &ScenarioConfig {
        &self.config
    }

    /// Read-only view of the living organisms, for rendering.
    pub fn organisms(&self) -> &Organisms {
        &self.organisms
    }

    pub fn food(&self) -> &FoodGrid {
        &self.food
    }

    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    /// Cumulative births and deaths by cause, per species.
    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    /// Cumulative prey and predators removed by starvation.
    pub fn starvation_deaths(&self) -> u64 {
        self.counters.prey.starvation_deaths + self.counters.predator.starvation_deaths
    }

    /// Cumulative food removed from cells by feeding since tick 0, in food units.
    pub fn cumulative_food_consumed(&self) -> f64 {
        self.food_consumed
    }

    /// Tick at which `species` reached zero, if it has (M-13 extinction notice).
    pub fn extinct_at(&self, species: Species) -> Option<u64> {
        self.extinct_at[species.index()]
    }

    /// Whether the population safety cap has suppressed births (SCOPE 6.7).
    pub fn population_cap_reached(&self) -> bool {
        self.population_cap_reached
    }

    /// Chart history (M-13).
    pub fn metrics(&self) -> &MetricsHistory {
        &self.metrics
    }

    /// Statistics of one species now.
    pub fn species_stats(&self, species: Species) -> SpeciesStats {
        SpeciesStats::of(&self.organisms, species)
    }

    /// Number of organisms pressed against terrain for at least [`STUCK_REPORT_TICKS`]
    /// ticks while trying to reach a goal (T-NAV-7a).
    pub fn stuck_count(&self) -> usize {
        self.organisms
            .blocked_ticks
            .iter()
            .filter(|&&ticks| ticks >= STUCK_REPORT_TICKS)
            .count()
    }

    /// A* node expansions used in the last tick, over all requests (T-NAV-6).
    pub fn route_expansions_last_tick(&self) -> u32 {
        self.route_expansions_this_tick
    }

    /// Largest expansion count of a single request in the last tick (T-NAV-6).
    pub fn max_request_expansions_last_tick(&self) -> u32 {
        self.max_request_expansions_this_tick
    }

    /// Route requests waiting in the queue.
    pub fn queued_route_requests(&self) -> usize {
        self.route_queue.len()
    }

    /// Benchmark option (M-18): while frozen, births and deaths are suppressed and every
    /// other phase runs, giving a constant workload. Energy stays clamped at 0.
    pub fn set_frozen_population(&mut self, frozen: bool) {
        self.frozen_population = frozen;
    }

    pub fn frozen_population(&self) -> bool {
        self.frozen_population
    }

    /// Sets the food in the cell containing `(x, y)`, clamped to `[0, cap]`. For
    /// constructed experiments and tests, before or between ticks.
    pub fn set_cell_food(&mut self, x: f32, y: f32, amount: f32) {
        let cell = self.food.cell_at(x, y);
        self.food.set(cell, amount);
    }

    /// Summary of the current state for charts, headless output and CSV export (M-13,
    /// M-17, M-21). Reads state only: it changes nothing and draws no random numbers.
    pub fn summary_metrics(&self) -> SummaryMetrics {
        SummaryMetrics {
            tick: self.tick,
            sim_seconds: self.sim_seconds(),
            prey: SpeciesStats::of(&self.organisms, Species::Prey),
            predator: SpeciesStats::of(&self.organisms, Species::Predator),
            total_food: metrics::sum(self.food.amounts()),
            cumulative_food_consumed: self.food_consumed,
            counters: self.counters,
            prey_extinct_at: self.extinct_at[0],
            predator_extinct_at: self.extinct_at[1],
            population_cap_reached: self.population_cap_reached,
            state_hash: self.state_hash(),
        }
    }

    /// ID of the organism nearest to `(x, y)` within `radius`, through the spatial grids
    /// (M-12 picking, T-INSP-1). Ties go to the lower ID; empty space gives `None`.
    pub fn nearest_organism(&self, x: f32, y: f32, radius: f32) -> Option<u64> {
        let o = &self.organisms;
        let mut best: Option<(f32, u64)> = None;
        let r2 = radius * radius;
        for grid in [&self.prey_grid, &self.predator_grid] {
            grid.for_each_candidate(x, y, radius, |i| {
                let (dx, dy) = (o.x[i] - x, o.y[i] - y);
                let d2 = dx * dx + dy * dy;
                if d2 > r2 {
                    return;
                }
                let candidate = (d2, o.id[i]);
                if best
                    .is_none_or(|b| candidate.0 < b.0 || (candidate.0 == b.0 && candidate.1 < b.1))
                {
                    best = Some(candidate);
                }
            });
        }
        best.map(|(_, id)| id)
    }

    /// Storage index of a living organism by ID, by binary search of the ID order.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.id_order
            .binary_search_by_key(&id, |&(candidate, _)| candidate)
            .ok()
            .map(|k| self.id_order[k].1 as usize)
    }

    /// Inspector view of the organism with `id`, or `None` when it is no longer alive.
    pub fn organism_snapshot(&self, id: u64) -> Option<OrganismSnapshot> {
        let i = self.index_of(id)?;
        let o = &self.organisms;
        let traits = o.traits(i);
        Some(OrganismSnapshot {
            id,
            species: o.species[i],
            x: o.x[i],
            y: o.y[i],
            energy: o.energy[i],
            capacity: energy::capacity(&self.config.energy, traits.size),
            age_ticks: o.age[i],
            age_seconds: f64::from(o.age[i]) / f64::from(self.config.ticks_per_second),
            generation: o.generation[i],
            traits,
            goal: o.goal[i],
            terrain: self.terrain.class_at(o.x[i], o.y[i]),
        })
    }

    /// Deterministic 64-bit FNV-1a hash of the complete simulation state, for regression
    /// and reproducibility checks (not security). Integers are hashed little-endian and
    /// floats as little-endian IEEE bit patterns (`-0.0` written as `0.0`), in this order:
    ///
    /// 1. tick, ticks per second, run seed, resolved terrain seed, terrain checksum
    /// 2. next organism ID, organism count, then every organism column in storage order,
    ///    one column after another (`organisms::organism_columns!` order)
    /// 3. cumulative counters per species, food consumed, extinction ticks, the cap flag
    ///    and the frozen flag
    /// 4. the route queue
    /// 5. food grid width and height, then every cell's food
    /// 6. stream word position of the terrain, initial-population, behavior and mutation
    ///    generators, in that order
    ///
    /// Generator state is covered without reading raw RNG memory: each key is
    /// reconstructable from the hashed seeds and the domain's fixed constant, and the
    /// stream number is always 0. The chart history and per-tick scratch are derived, not
    /// hashed.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv1a64::new();
        h.write_u64(self.tick);
        h.write_u32(self.config.ticks_per_second);
        h.write_u64(self.run_seed);
        h.write_u64(self.terrain_seed);
        h.write_u64(self.terrain.checksum());

        h.write_u64(self.next_id);
        h.write_u64(self.organisms.len() as u64);
        self.organisms.hash_into(&mut h);

        for counters in [self.counters.prey, self.counters.predator] {
            h.write_u64(counters.births);
            h.write_u64(counters.starvation_deaths);
            h.write_u64(counters.predation_deaths);
            h.write_u64(counters.age_deaths);
        }
        h.write_f64(self.food_consumed);
        for extinct in self.extinct_at {
            h.write_u64(extinct.unwrap_or(u64::MAX));
        }
        h.write_bytes(&[
            u8::from(self.population_cap_reached),
            u8::from(self.frozen_population),
            u8::from(self.ever_alive[0]),
            u8::from(self.ever_alive[1]),
        ]);

        h.write_u64(self.route_queue.len() as u64);
        for request in &self.route_queue {
            h.write_u64(request.tick);
            h.write_u64(request.id);
        }

        h.write_u32(self.food.width());
        h.write_u32(self.food.height());
        h.write_f32_slice(self.food.amounts());

        for word_position in self.rng.word_positions() {
            h.write_u128(word_position);
        }
        h.finish()
    }

    // ----- Tick phases, in SCOPE 6.9 order -----
    // Within a phase, organisms are processed in ascending storage index (I-04), except
    // where SCOPE names ID order (predation, births).

    /// Phase 1. Rebuilds both spatial grids and the ID order from current positions.
    fn rebuild_spatial_grid(&mut self) {
        let o = &self.organisms;
        self.prey_grid
            .rebuild(&o.x, &o.y, |i| o.species[i] == Species::Prey);
        self.predator_grid
            .rebuild(&o.x, &o.y, |i| o.species[i] == Species::Predator);
        self.id_order.clear();
        self.id_order
            .extend(o.id.iter().enumerate().map(|(i, &id)| (id, i as u32)));
        self.id_order.sort_unstable();
    }

    /// Nearest organism of the grid's species within `radius` of organism `i` that
    /// `accept` allows, by distance then lower ID.
    fn nearest_in(
        &self,
        grid: &SpatialGrid,
        i: usize,
        radius: f32,
        accept: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let o = &self.organisms;
        let (x, y) = (o.x[i], o.y[i]);
        let r2 = radius * radius;
        let mut best: Option<(f32, u64, usize)> = None;
        grid.for_each_candidate(x, y, radius, |j| {
            let (dx, dy) = (o.x[j] - x, o.y[j] - y);
            let d2 = dx * dx + dy * dy;
            if d2 > r2 || !accept(j) {
                return;
            }
            if best.is_none_or(|(bd, bid, _)| d2 < bd || (d2 == bd && o.id[j] < bid)) {
                best = Some((d2, o.id[j], j));
            }
        });
        best.map(|(_, _, j)| j)
    }

    /// Point the organism is heading for under its current goal, if any.
    fn goal_point(&self, i: usize) -> Option<(f32, f32)> {
        let o = &self.organisms;
        match o.goal[i] {
            Goal::Seek => Some(self.food.cell_center(o.target_cell[i] as usize)),
            Goal::Hunt => self.index_of(o.target_id[i]).map(|j| (o.x[j], o.y[j])),
            Goal::Wander | Goal::Flee => None,
        }
    }

    /// Sets a new goal and resets navigation state. For seeking and hunting, runs the
    /// blocked-path check (SCOPE 6.8 step 1) and queues a route request when the straight
    /// path is blocked.
    fn set_goal(&mut self, i: usize, goal: Goal, target_cell: u32, target_id: u64) {
        let o = &mut self.organisms;
        o.goal[i] = goal;
        o.target_cell[i] = target_cell;
        o.target_id[i] = target_id;
        o.route[i].clear();
        o.route_next[i] = 0;
        o.route_pending[i] = false;
        o.progress_tick[i] = self.tick;
        o.best_distance[i] = f32::INFINITY;
        o.replanned[i] = false;
        if let Some((gx, gy)) = self.goal_point(i) {
            let o = &self.organisms;
            if !self.terrain.segment_clear(o.x[i], o.y[i], gx, gy) {
                self.request_route(i);
            }
        }
    }

    fn request_route(&mut self, i: usize) {
        let o = &mut self.organisms;
        if !o.route_pending[i] {
            o.route_pending[i] = true;
            self.new_requests.push(RouteRequest {
                tick: self.tick,
                id: o.id[i],
            });
        }
    }

    /// Phase 2. Goal selection (SCOPE 6.8). Each organism decides on its first tick and
    /// then every `decision_interval` ticks, staggered by ID; prey also check for threats
    /// every `threat_interval` ticks.
    ///
    /// - Prey: flee the nearest predator within sense range; otherwise keep or choose the
    ///   richest useful food cell in range in the same reachable region and not marked
    ///   unreachable ([`behavior::reconsider_target`]); otherwise wander with a new
    ///   heading (exactly one behavior-domain draw).
    /// - Predators: hunt the nearest eligible prey (size ratio met) within sense range in
    ///   the same region and not marked unreachable; otherwise wander with a new heading.
    ///   A hunt whose prey died is re-decided at once; a moving prey that drifts from the
    ///   route's end triggers a re-plan at most every `hunt_replan_interval` ticks.
    fn sense_and_decide(&mut self) {
        let interval = u64::from(self.config.behavior.decision_interval);
        let threat_interval = u64::from(self.config.behavior.threat_interval);
        let min_useful = behavior::min_useful_food(self.config.food.intake_per_tick);
        let size_ratio = self.config.predation.size_ratio;
        let drift = self.config.navigation.drift_cells * self.food.cell_size();
        let replan_interval = u64::from(self.config.navigation.hunt_replan_interval);
        let tick = self.tick;
        for i in 0..self.organisms.len() {
            let id = self.organisms.id[i];
            let mut due =
                self.organisms.age[i] == 0 || tick.wrapping_add(id).is_multiple_of(interval);
            match self.organisms.species[i] {
                Species::Prey => {
                    let fleeing = self.organisms.goal[i] == Goal::Flee;
                    let threat_due = due || tick.wrapping_add(id).is_multiple_of(threat_interval);
                    if fleeing && self.index_of(self.organisms.target_id[i]).is_none() {
                        due = true;
                    }
                    if threat_due || due {
                        let sense = self.organisms.sense[i];
                        if let Some(j) = self.nearest_in(&self.predator_grid, i, sense, |_| true) {
                            let predator = self.organisms.id[j];
                            if !fleeing || self.organisms.target_id[i] != predator {
                                self.set_goal(i, Goal::Flee, Organisms::NO_TARGET, predator);
                            }
                            continue;
                        } else if fleeing {
                            due = true;
                        }
                    }
                    if self.organisms.goal[i] == Goal::Flee && !due {
                        continue;
                    }
                    if due {
                        self.decide_food(i, min_useful);
                    } else if self.organisms.goal[i] == Goal::Seek
                        && !self.organisms.remaining_route(i).is_empty()
                    {
                        // A seeker without a route walks a segment that was clear when
                        // its goal was set; terrain is static, so only routed seekers can
                        // drift off their path.
                        self.check_route(i);
                    }
                }
                Species::Predator => {
                    if self.organisms.goal[i] == Goal::Hunt
                        && self.index_of(self.organisms.target_id[i]).is_none()
                    {
                        due = true;
                    }
                    if due {
                        self.decide_hunt(i, size_ratio, drift, replan_interval);
                    } else if self.organisms.goal[i] == Goal::Hunt {
                        self.follow_moving_target(i, drift, replan_interval);
                    }
                }
            }
        }
    }

    fn decide_food(&mut self, i: usize, min_useful: f32) {
        let o = &self.organisms;
        let current = (o.goal[i] == Goal::Seek).then_some(o.target_cell[i] as usize);
        let region = self.terrain.region_at(o.x[i], o.y[i]);
        let (avoid, avoid_until) = (o.avoid_target[i], o.avoid_until[i]);
        let tick = self.tick;
        let terrain = &self.terrain;
        let accept = |cell: usize| {
            terrain.region(cell) == region && !(avoid_until > tick && avoid == cell as u64)
        };
        let choice = behavior::reconsider_target(
            &self.food, o.x[i], o.y[i], o.sense[i], current, min_useful, accept,
        );
        match choice {
            Some(cell) if current == Some(cell) => self.check_route(i),
            Some(cell) => self.set_goal(i, Goal::Seek, cell as u32, Organisms::NO_ID),
            None => self.start_wandering(i),
        }
    }

    fn decide_hunt(&mut self, i: usize, size_ratio: f32, drift: f32, replan_interval: u64) {
        let o = &self.organisms;
        let region = self.terrain.region_at(o.x[i], o.y[i]);
        let (avoid, avoid_until) = (o.avoid_target[i], o.avoid_until[i]);
        let predator_size = o.size[i];
        let tick = self.tick;
        let terrain = &self.terrain;
        let prey = self.nearest_in(&self.prey_grid, i, o.sense[i], |j| {
            predator_size >= size_ratio * o.size[j]
                && terrain.region_at(o.x[j], o.y[j]) == region
                && !(avoid_until > tick && avoid == o.id[j])
        });
        match prey {
            Some(j) => {
                let target = self.organisms.id[j];
                if self.organisms.goal[i] == Goal::Hunt && self.organisms.target_id[i] == target {
                    self.follow_moving_target(i, drift, replan_interval);
                } else {
                    self.set_goal(i, Goal::Hunt, Organisms::NO_TARGET, target);
                }
            }
            None => self.start_wandering(i),
        }
    }

    fn start_wandering(&mut self, i: usize) {
        self.set_goal(i, Goal::Wander, Organisms::NO_TARGET, Organisms::NO_ID);
        self.organisms.heading[i] = behavior::wander_heading(&mut self.rng.behavior);
    }

    /// A seeker whose straight path to its next waypoint (or goal) has become blocked has
    /// drifted off its route: request a new one.
    fn check_route(&mut self, i: usize) {
        let o = &self.organisms;
        if o.route_pending[i] {
            return;
        }
        let next = match o.remaining_route(i).first() {
            Some(&cell) => Some(self.food.cell_center(cell as usize)),
            None => self.goal_point(i),
        };
        if let Some((tx, ty)) = next
            && !self.terrain.segment_clear(o.x[i], o.y[i], tx, ty)
        {
            self.request_route(i);
        }
    }

    /// A hunter re-plans when its prey ends up more than `drift` from the route's end
    /// (or, without a route, when the straight path becomes blocked), at most once every
    /// `replan_interval` ticks.
    fn follow_moving_target(&mut self, i: usize, drift: f32, replan_interval: u64) {
        let o = &self.organisms;
        if o.route_pending[i] || self.tick < o.planned_tick[i] + replan_interval {
            return;
        }
        let Some((px, py)) = self.goal_point(i) else {
            return;
        };
        let stale = match o.route[i].last() {
            Some(&end) => {
                let (ex, ey) = self.food.cell_center(end as usize);
                behavior::distance(ex, ey, px, py) > drift
            }
            None => !self.terrain.segment_clear(o.x[i], o.y[i], px, py),
        };
        if stale {
            self.request_route(i);
        }
    }

    /// Moves new requests into the queue, keeping it ordered by (request tick, ID).
    fn flush_route_requests(&mut self) {
        if self.new_requests.is_empty() {
            return;
        }
        self.route_queue.extend(self.new_requests.drain(..));
        self.route_queue.make_contiguous().sort_unstable();
    }

    /// Marks the current target unreachable for `unreachable_ticks` and wanders.
    fn abandon_target(&mut self, i: usize) {
        let until = self.tick + u64::from(self.config.navigation.unreachable_ticks);
        let o = &mut self.organisms;
        o.avoid_target[i] = match o.goal[i] {
            Goal::Seek => u64::from(o.target_cell[i]),
            _ => o.target_id[i],
        };
        o.avoid_until[i] = until;
        self.set_goal(i, Goal::Wander, Organisms::NO_TARGET, Organisms::NO_ID);
    }

    /// Phase 3. Serves queued route requests in (request tick, ID) order until the
    /// per-tick expansion budget cannot fit another full request (SCOPE 6.8 step 4), so
    /// the budget is never exceeded (T-NAV-6). Requests of dead organisms, or whose goal
    /// changed since, are dropped at no cost. A failed search marks the target
    /// unreachable.
    fn serve_route_requests(&mut self) {
        self.flush_route_requests();
        let nav = self.config.navigation.clone();
        let mut used = 0u32;
        while let Some(&request) = self.route_queue.front() {
            if nav.budget_per_tick - used < nav.expansions_per_request {
                break;
            }
            self.route_queue.pop_front();
            let Some(i) = self.index_of(request.id) else {
                continue;
            };
            if !self.organisms.route_pending[i] {
                continue;
            }
            self.organisms.route_pending[i] = false;
            let goal_cell = match self.goal_point(i) {
                Some((gx, gy)) if matches!(self.organisms.goal[i], Goal::Seek | Goal::Hunt) => {
                    self.food.cell_at(gx, gy)
                }
                _ => continue,
            };
            let o = &self.organisms;
            let half_window =
                ((o.sense[i] + nav.window_margin) / self.food.cell_size()).ceil() as u32;
            let (outcome, expansions) = self.route_search.find(
                &self.terrain,
                (o.x[i], o.y[i]),
                goal_cell,
                half_window,
                nav.expansions_per_request,
            );
            used += expansions;
            self.max_request_expansions_this_tick =
                self.max_request_expansions_this_tick.max(expansions);
            match outcome {
                SearchOutcome::Found(route) => {
                    let o = &mut self.organisms;
                    o.route[i] = route;
                    o.route_next[i] = 0;
                    o.planned_tick[i] = self.tick;
                }
                SearchOutcome::NoRoute | SearchOutcome::CapReached => self.abandon_target(i),
            }
        }
        self.route_expansions_this_tick = used;
    }

    /// Phase 4. Movement: steering toward the goal (next waypoint, food cell center or
    /// prey), away from a threat, or along the wandering heading; the terrain speed
    /// multiplier; wall sliding and solid edges ([`behavior::step_through_terrain`]);
    /// realized effort; waypoint advance; and stuck detection with recovery (SCOPE 6.8
    /// step 8).
    fn move_organisms(&mut self) {
        let tps = self.config.ticks_per_second;
        let wander_fraction = self.config.behavior.wander_speed_fraction;
        let nav = self.config.navigation.clone();
        let cell_size = self.food.cell_size();
        self.effort.clear();
        self.effort.resize(self.organisms.len(), 0.0);
        for i in 0..self.organisms.len() {
            let o = &self.organisms;
            let (x, y) = (o.x[i], o.y[i]);
            let available = traits::max_distance_per_tick(o.speed[i], tps)
                * self.terrain.speed_multiplier_at(x, y);
            let goal = o.goal[i];
            let (to_x, to_y, length) = match goal {
                Goal::Wander => {
                    let length = available * wander_fraction;
                    let (tx, ty) = behavior::move_along(x, y, o.heading[i], length);
                    (tx, ty, length)
                }
                Goal::Flee => match self.index_of(o.target_id[i]) {
                    Some(j) => {
                        let away = behavior::heading_of(x - o.x[j], y - o.y[j], o.heading[i]);
                        let (tx, ty) = behavior::move_along(x, y, away, available);
                        (tx, ty, available)
                    }
                    None => (x, y, 0.0),
                },
                Goal::Seek | Goal::Hunt => {
                    let arrived =
                        goal == Goal::Seek && self.food.cell_at(x, y) == o.target_cell[i] as usize;
                    if arrived {
                        (x, y, 0.0)
                    } else {
                        let waypoint = o
                            .remaining_route(i)
                            .first()
                            .map(|&cell| self.food.cell_center(cell as usize));
                        match waypoint.or_else(|| self.goal_point(i)) {
                            Some((tx, ty)) => behavior::move_toward(x, y, tx, ty, available),
                            None => (x, y, 0.0),
                        }
                    }
                }
            };
            let step = behavior::step_through_terrain(&self.terrain, x, y, to_x, to_y, length);
            self.effort[i] = behavior::realized_effort(step.moved, available);

            let o = &mut self.organisms;
            o.x[i] = step.x;
            o.y[i] = step.y;
            let blocked = step.obstructed && step.moved < 0.5 * length;
            if goal == Goal::Wander {
                if blocked {
                    // Turn away from the obstacle by a fixed angle; no random draw.
                    o.heading[i] = (o.heading[i] + 2.0).rem_euclid(core::f32::consts::TAU);
                }
                o.blocked_ticks[i] = 0;
                continue;
            }
            if step.moved > 0.0 {
                o.heading[i] = behavior::heading_of(step.x - x, step.y - y, o.heading[i]);
            }
            if goal == Goal::Flee {
                o.blocked_ticks[i] = 0;
                continue;
            }
            o.blocked_ticks[i] = if blocked { o.blocked_ticks[i] + 1 } else { 0 };

            // Advance past reached waypoints.
            while let Some(&cell) = o.remaining_route(i).first() {
                let (wx, wy) = self.food.cell_center(cell as usize);
                if self.food.cell_at(step.x, step.y) == cell as usize
                    || behavior::distance(step.x, step.y, wx, wy) <= 0.5 * cell_size
                {
                    o.route_next[i] += 1;
                } else {
                    break;
                }
            }

            // Stuck detection: progress along the remaining route to the goal.
            let arrived = goal == Goal::Seek
                && self.food.cell_at(step.x, step.y) == o.target_cell[i] as usize;
            let Some(distance) = self.remaining_distance(i) else {
                continue;
            };
            let o = &mut self.organisms;
            if arrived || distance + nav.stuck_min_progress <= o.best_distance[i] {
                o.best_distance[i] = if arrived { 0.0 } else { distance };
                o.progress_tick[i] = self.tick;
            } else if self.tick - o.progress_tick[i] >= u64::from(nav.stuck_window) {
                if o.replanned[i] {
                    self.abandon_target(i);
                } else {
                    o.replanned[i] = true;
                    o.progress_tick[i] = self.tick;
                    o.best_distance[i] = distance;
                    self.request_route(i);
                }
            }
        }
    }

    /// Length of the path still to walk to the goal: through the remaining route
    /// waypoints, then to the goal point; the straight distance without a route. Walking
    /// a route around an obstacle shortens this steadily even while the straight-line
    /// distance to the goal grows, so stuck detection measures real progress.
    fn remaining_distance(&self, i: usize) -> Option<f32> {
        let (gx, gy) = self.goal_point(i)?;
        let o = &self.organisms;
        let mut at = (o.x[i], o.y[i]);
        let mut total = 0.0;
        for &cell in o.remaining_route(i) {
            let next = self.food.cell_center(cell as usize);
            total += behavior::distance(at.0, at.1, next.0, next.1);
            at = next;
        }
        Some(total + behavior::distance(at.0, at.1, gx, gy))
    }

    /// Phase 5. Prey eat from the cell they stand in, in ascending storage index (the
    /// lower index eats first from a shared cell). Then predators, in ascending ID order,
    /// each capture at most one prey: the nearest eligible prey (predator size ≥ size
    /// ratio × prey size) within the capture radius that has not been captured this tick,
    /// lower ID on a tie (SCOPE 6.5). The predator gains `efficiency × content × prey
    /// size`, capped at capacity. Predators never eat plant food.
    fn feed(&mut self) {
        let food_config = &self.config.food;
        let coefficients = &self.config.energy;
        let n = self.organisms.len();
        self.dying.clear();
        self.dying.resize(n, false);
        let o = &mut self.organisms;
        for i in 0..n {
            if o.species[i] != Species::Prey {
                continue;
            }
            let cell = self.food.cell_at(o.x[i], o.y[i]);
            let capacity = energy::capacity(coefficients, o.size[i]);
            let eaten = food::amount_eaten(
                self.food.food[cell],
                food_config.intake_per_tick,
                capacity - o.energy[i],
                food_config.energy_per_unit,
            );
            if eaten > 0.0 {
                let before = self.food.food[cell];
                let after = before - eaten;
                self.food.food[cell] = after;
                // Count what actually left the cell; the difference is exact in f64.
                self.food_consumed += f64::from(before) - f64::from(after);
                o.energy[i] = (o.energy[i] + eaten * food_config.energy_per_unit).min(capacity);
            }
        }

        // Capture uses positions after movement.
        let o = &self.organisms;
        self.prey_grid
            .rebuild(&o.x, &o.y, |i| o.species[i] == Species::Prey);
        let predation = self.config.predation.clone();
        for k in 0..self.id_order.len() {
            let p = self.id_order[k].1 as usize;
            if self.organisms.species[p] != Species::Predator {
                continue;
            }
            let o = &self.organisms;
            let predator_size = o.size[p];
            let dying = &self.dying;
            let victim = self.nearest_in(&self.prey_grid, p, predation.capture_radius, |j| {
                !dying[j] && predator_size >= predation.size_ratio * o.size[j]
            });
            let Some(j) = victim else {
                continue;
            };
            if self.frozen_population {
                continue;
            }
            let gain = predation.efficiency * predation.content * self.organisms.size[j];
            let capacity = energy::capacity(coefficients, predator_size);
            let o = &mut self.organisms;
            o.energy[p] = (o.energy[p] + gain).min(capacity);
            self.dying[j] = true;
            self.removals.push((j, DeathCause::Predation));
        }
    }

    /// Phase 6. Charges each living organism the energy formula once (SCOPE 6.3) with this
    /// tick's realized effort, and clamps energy at its lower bound of 0.
    fn apply_energy_costs(&mut self) {
        let tps = self.config.ticks_per_second;
        let coefficients = &self.config.energy;
        let o = &mut self.organisms;
        for i in 0..o.len() {
            if self.dying.get(i).copied().unwrap_or(false) {
                continue;
            }
            let effort = self.effort.get(i).copied().unwrap_or(0.0);
            let cost = energy::cost_per_tick(coefficients, o.traits(i), effort, tps);
            let remaining = o.energy[i] - cost;
            o.energy[i] = if remaining > 0.0 { remaining } else { 0.0 };
        }
    }

    /// Phase 7. Ages every organism by one tick, then marks starved organisms (energy at
    /// or below 0 after the energy phase) and, when enabled (S-02), organisms older than
    /// their species' maximum lifespan. Each organism is marked at most once.
    fn age_and_check_deaths(&mut self) {
        let tps = f64::from(self.config.ticks_per_second);
        let max_age = |species: Species| {
            let seconds = self.config.species(species).max_age_seconds;
            (seconds > 0.0).then(|| (f64::from(seconds) * tps).round() as u64)
        };
        let max_ages = [max_age(Species::Prey), max_age(Species::Predator)];
        let o = &mut self.organisms;
        for i in 0..o.len() {
            o.age[i] = o.age[i].saturating_add(1);
            if self.dying.get(i).copied().unwrap_or(false) || self.frozen_population {
                continue;
            }
            let cause = if o.energy[i] <= 0.0 {
                Some(DeathCause::Starvation)
            } else if max_ages[o.species[i].index()].is_some_and(|max| u64::from(o.age[i]) > max) {
                Some(DeathCause::Age)
            } else {
                None
            };
            if let Some(cause) = cause {
                self.dying[i] = true;
                self.removals.push((i, cause));
            }
        }
    }

    /// Phase 8. Reproduction in ascending parent ID (SCOPE 6.6): every living organism with
    /// energy ≥ `threshold × capacity` produces exactly one offspring, whose traits come
    /// from [`reproduction::inherit`]. The parent pays the offspring's starting energy
    /// (`offspring_energy_fraction × offspring capacity`) plus the overhead; a parent that
    /// cannot pay and keep some energy has no offspring. At the population cap births are
    /// suppressed and the warning flag is set (SCOPE 6.7).
    fn collect_births(&mut self) {
        self.births.clear();
        if self.frozen_population {
            return;
        }
        let cap = u64::from(self.config.population_cap);
        let alive_after = (self.organisms.len() - self.removals.len()) as u64;
        let settings = self.config.reproduction.clone();
        let mutation = self.config.mutation.clone();
        for k in 0..self.id_order.len() {
            let i = self.id_order[k].1 as usize;
            if self.dying.get(i).copied().unwrap_or(false) {
                continue;
            }
            let o = &self.organisms;
            let parent = o.traits(i);
            let capacity = energy::capacity(&self.config.energy, parent.size);
            if o.energy[i] < settings.threshold * capacity {
                continue;
            }
            if alive_after + self.births.len() as u64 >= cap {
                self.population_cap_reached = true;
                continue;
            }
            let child = reproduction::inherit(parent, &mutation, &mut self.rng.mutation);
            let position = reproduction::offspring_position(
                (o.x[i], o.y[i]),
                &settings,
                &self.terrain,
                &mut self.rng.mutation,
            );
            let child_energy = settings.offspring_energy_fraction
                * energy::capacity(&self.config.energy, child.size);
            let cost = reproduction::birth_cost(child_energy, &settings);
            if o.energy[i] - cost <= 0.0 {
                continue;
            }
            let species = o.species[i];
            let generation = o.generation[i] + 1;
            self.organisms.energy[i] -= cost;
            self.counters.species_mut(species).births += 1;
            self.births.push(OrganismRow::new(
                0,
                species,
                position,
                child_energy,
                generation,
                child,
            ));
        }
    }

    /// Phase 9. Removes marked organisms in descending index order with swap-remove (I-04),
    /// counting each death once by cause, then appends births with new IDs in birth order,
    /// and records extinctions.
    fn apply_removals_and_births(&mut self) {
        self.removals
            .sort_unstable_by_key(|&(index, _)| std::cmp::Reverse(index));
        for k in 0..self.removals.len() {
            let (index, cause) = self.removals[k];
            debug_assert!(k == 0 || self.removals[k - 1].0 != index, "removed twice");
            let species = self.organisms.species[index];
            let counters = self.counters.species_mut(species);
            match cause {
                DeathCause::Starvation => counters.starvation_deaths += 1,
                DeathCause::Predation => counters.predation_deaths += 1,
                DeathCause::Age => counters.age_deaths += 1,
            }
            self.organisms.swap_remove(index);
        }
        self.removals.clear();
        for mut row in self.births.drain(..) {
            row.id = self.next_id;
            self.next_id += 1;
            self.organisms.push_row(row);
        }
        for species in Species::ALL {
            let alive = self.organisms.count(species) > 0;
            let k = species.index();
            if alive {
                self.ever_alive[k] = true;
            } else if self.ever_alive[k] && self.extinct_at[k].is_none() {
                self.extinct_at[k] = Some(self.tick + 1);
            }
        }
    }

    /// Phase 10. Food regrows linearly toward each cell's cap (SCOPE 6.4), multiplied by
    /// the famine factor from the famine tick on (S-01).
    fn regrow_food(&mut self) {
        let multiplier = match &self.config.famine {
            Some(famine) if self.tick >= famine.at_tick => famine.food_multiplier,
            _ => 1.0,
        };
        self.food
            .regrow(self.config.food.regrowth_per_tick, multiplier);
    }

    /// Phase 11. Records a chart sample when one is due for the tick being completed.
    fn sample_metrics(&mut self) {
        let completed = self.tick + 1;
        if self.metrics.due(completed) {
            let sample = self.metric_sample(completed);
            self.metrics.push(sample);
        }
    }

    fn metric_sample(&self, tick: u64) -> MetricSample {
        MetricSample {
            tick,
            sim_seconds: tick as f64 / f64::from(self.config.ticks_per_second),
            prey: SpeciesStats::of(&self.organisms, Species::Prey),
            predator: SpeciesStats::of(&self.organisms, Species::Predator),
        }
    }

    /// Full consistency check for tests: aligned columns, unique IDs below `next_id`,
    /// positions inside the world on passable cells, finite energy within
    /// `[0, capacity]`, traits within range, valid goals and targets, ages no greater
    /// than the tick, food finite within `[0, cap]`, and no scratch left between ticks.
    #[cfg(test)]
    fn check_invariants(&self) -> Result<(), String> {
        let o = &self.organisms;
        if !o.columns_aligned() {
            return Err("columns have different lengths".into());
        }
        let mut ids = o.id.clone();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != o.len() {
            return Err("duplicate organism IDs".into());
        }
        if ids.last().is_some_and(|&max| max >= self.next_id) {
            return Err("an ID is not below next_id".into());
        }
        if self.id_order.len() != o.len() {
            return Err("ID order is stale".into());
        }
        for i in 0..o.len() {
            let id = o.id[i];
            let (x, y) = (o.x[i], o.y[i]);
            if !self.terrain.passable_at(x, y) {
                return Err(format!(
                    "organism {id} is not on passable terrain at ({x}, {y})"
                ));
            }
            let traits = o.traits(i);
            for trait_id in TraitId::ALL {
                if !trait_id.range().contains(traits.get(trait_id)) {
                    return Err(format!(
                        "organism {id} has {} out of range",
                        trait_id.name()
                    ));
                }
            }
            let capacity = energy::capacity(&self.config.energy, traits.size);
            let e = o.energy[i];
            if !e.is_finite() || e < 0.0 || e > capacity {
                return Err(format!(
                    "organism {id} has energy {e} outside [0, {capacity}]"
                ));
            }
            if u64::from(o.age[i]) > self.tick {
                return Err(format!("organism {id} is older than the simulation"));
            }
            if !(0.0..=core::f32::consts::TAU).contains(&o.heading[i]) {
                return Err(format!("organism {id} has heading {}", o.heading[i]));
            }
            match o.goal[i] {
                Goal::Seek if o.target_cell[i] as usize >= self.food.cell_count() => {
                    return Err(format!("organism {id} targets missing cell"));
                }
                Goal::Seek | Goal::Hunt | Goal::Flee | Goal::Wander => {}
            }
            if (o.goal[i] == Goal::Seek) != (o.target_cell[i] != Organisms::NO_TARGET) {
                return Err(format!("organism {id} has an inconsistent food target"));
            }
            if matches!(o.goal[i], Goal::Hunt | Goal::Flee) != (o.target_id[i] != Organisms::NO_ID)
            {
                return Err(format!("organism {id} has an inconsistent organism target"));
            }
            if o.species[i] == Species::Predator && matches!(o.goal[i], Goal::Seek | Goal::Flee) {
                return Err(format!("predator {id} has a prey goal"));
            }
            if o.species[i] == Species::Prey && o.goal[i] == Goal::Hunt {
                return Err(format!("prey {id} is hunting"));
            }
        }
        for (index, &amount) in self.food.amounts().iter().enumerate() {
            let cap = self.food.cap(index);
            if !amount.is_finite() || amount < 0.0 || amount > cap {
                return Err(format!("cell {index} holds {amount} outside [0, {cap}]"));
            }
        }
        if !self.food_consumed.is_finite() || self.food_consumed < 0.0 {
            return Err(format!("food consumed is {}", self.food_consumed));
        }
        if !self.removals.is_empty() || !self.births.is_empty() || !self.new_requests.is_empty() {
            return Err("per-tick scratch left between ticks".into());
        }
        if self.route_expansions_this_tick > self.config.navigation.budget_per_tick {
            return Err("route budget exceeded".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
