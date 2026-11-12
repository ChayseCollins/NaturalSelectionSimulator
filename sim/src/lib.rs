//! Natural Selection Simulator core library.
//!
//! Every simulation rule lives in this crate; `app` and `headless` are thin shells
//! over its public API (I-06). This crate never depends on graphics, UI or windowing
//! crates (I-01), performs no file I/O (I-08) and never reads the clock.
//!
//! Implemented so far: the M-02 deterministic core, the M-03 prey foundation, the M-07
//! food grid with regrowth and feeding, and the Week 1 part of M-09 (prey seek the
//! richest useful food cell in sense range in an open world, reconsider that target at
//! each decision, and otherwise wander). Empty phases are placeholders until their
//! milestones land.

#![forbid(unsafe_code)]

mod behavior;
mod config;
pub mod energy;
mod food;
mod hash;
pub mod limits;
mod metrics;
mod organisms;
mod rng;
pub mod traits;

pub use behavior::{MIN_USEFUL_TARGET_BITES, TARGET_SWITCH_FRACTION};
pub use config::{
    BehaviorConfig, EnergyConfig, FoodConfig, ScenarioConfig, SpeciesConfig, TraitStart,
    ValidationError, WorldConfig,
};
pub use food::FoodGrid;
pub use metrics::SummaryMetrics;
pub use organisms::Organisms;

use hash::Fnv1a64;
use rng::RandomDomains;
use traits::{TraitId, Traits};

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
    organisms: Organisms,
    food: FoodGrid,
    /// ID given to the next organism created; IDs are never reused within a run.
    next_id: u64,
    /// Cumulative prey removed by starvation.
    starvation_deaths: u64,
    /// Cumulative food removed from cells by feeding, in food units (not energy).
    food_consumed: f64,
    /// Scratch list of storage indices to remove this tick, in ascending order. Always
    /// empty between ticks, so it is not simulation state and is not hashed.
    pending_removals: Vec<usize>,
    /// Scratch realized movement effort per organism, written in phase 4 and read in
    /// phase 6 of the same tick. Recomputed every tick, so it is not hashed.
    effort: Vec<f32>,
}

impl Simulation {
    /// Validates `config` and builds the run identified by `run_seed` at tick 0: the food
    /// grid (uniform, no randomness) and the starting prey population.
    pub fn new(config: &ScenarioConfig, run_seed: u64) -> Result<Self, Vec<ValidationError>> {
        config.validate()?;
        let terrain_seed = config.resolved_terrain_seed(run_seed);
        let mut simulation = Self {
            rng: RandomDomains::new(terrain_seed, run_seed),
            config: config.clone(),
            run_seed,
            terrain_seed,
            tick: 0,
            organisms: Organisms::with_capacity(config.prey.start_count as usize),
            food: FoodGrid::new(&config.world, &config.food),
            next_id: 0,
            starvation_deaths: 0,
            food_consumed: 0.0,
            pending_removals: Vec::new(),
            effort: Vec::new(),
        };
        simulation.spawn_initial_prey();
        Ok(simulation)
    }

    /// Creates the starting prey from the initial-population random domain only.
    ///
    /// For each organism in ID order the draws are, in this order: x, y (one 64-bit draw
    /// each, uniform over the world), then speed, size and sense (two 64-bit draws each,
    /// normal with the configured mean and spread, clamped to the trait's range). That is
    /// eight 64-bit draws per organism whatever the spreads, so sampling never depends on
    /// parameter values. Starting energy is `start_energy_fraction × capacity(size)`.
    fn spawn_initial_prey(&mut self) {
        let world = &self.config.world;
        let prey = &self.config.prey;
        let rng = &mut self.rng.initial_population;
        for _ in 0..prey.start_count {
            let x = rng::unit_f32(rng) * world.width;
            let y = rng::unit_f32(rng) * world.height;
            let mut sample = |id: TraitId| {
                let start = prey.trait_start(id);
                let range = id.range();
                let value =
                    f64::from(start.mean) + f64::from(start.spread) * rng::standard_normal(rng);
                (value as f32).clamp(range.min, range.max)
            };
            let traits = Traits {
                speed: sample(TraitId::Speed),
                size: sample(TraitId::Size),
                sense: sample(TraitId::Sense),
            };
            let energy =
                prey.start_energy_fraction * energy::capacity(&self.config.energy, traits.size);
            self.organisms.push(self.next_id, (x, y), energy, 0, traits);
            self.next_id += 1;
        }
    }

    /// Advances exactly one fixed tick, running the phases of SCOPE 6.9 in order.
    /// Takes no time argument (I-05).
    pub fn step(&mut self) {
        self.rebuild_spatial_grid();
        self.sense_and_decide();
        self.serve_route_requests();
        self.move_organisms();
        self.feed();
        self.apply_energy_costs();
        self.age_and_check_deaths();
        self.collect_births();
        self.apply_removals_and_births();
        self.regrow_food();
        self.sample_metrics();
        self.tick += 1;
        debug_assert!(self.organisms.columns_aligned());
    }

    /// Advances exactly `n` ticks; identical to calling [`Simulation::step`] `n` times.
    pub fn step_n(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// Number of ticks completed. This integer is the authoritative simulation clock.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Fixed ticks per simulated second, from the configuration.
    pub fn ticks_per_second(&self) -> u32 {
        self.config.ticks_per_second
    }

    /// The run seed this simulation was built with.
    pub fn run_seed(&self) -> u64 {
        self.run_seed
    }

    /// The validated configuration this simulation was built with.
    pub fn config(&self) -> &ScenarioConfig {
        &self.config
    }

    /// Read-only view of the living organisms (currently prey only).
    pub fn organisms(&self) -> &Organisms {
        &self.organisms
    }

    /// Read-only view of the food grid.
    pub fn food(&self) -> &FoodGrid {
        &self.food
    }

    /// Cumulative organisms removed by starvation.
    pub fn starvation_deaths(&self) -> u64 {
        self.starvation_deaths
    }

    /// Cumulative food removed from cells by feeding since tick 0, in food units.
    pub fn cumulative_food_consumed(&self) -> f64 {
        self.food_consumed
    }

    /// Summary of the current state for headless output and CSV export (M-17, M-21).
    /// Reads state only: it changes nothing and draws no random numbers. Averages are
    /// `None` when there are no prey; sums use `f64` in storage order.
    pub fn summary_metrics(&self) -> SummaryMetrics {
        let o = &self.organisms;
        SummaryMetrics {
            tick: self.tick,
            sim_seconds: self.sim_seconds(),
            prey_population: o.len(),
            avg_speed: metrics::mean(&o.speed),
            avg_size: metrics::mean(&o.size),
            avg_sense: metrics::mean(&o.sense),
            avg_energy: metrics::mean(&o.energy),
            total_food: metrics::sum(self.food.amounts()),
            cumulative_food_consumed: self.food_consumed,
            cumulative_starvation_deaths: self.starvation_deaths,
            state_hash: self.state_hash(),
        }
    }

    /// Simulated seconds elapsed, for display only: computed as `tick / ticks_per_second`
    /// in `f64` each call, never accumulated. Exact while `tick` is below 2^53.
    pub fn sim_seconds(&self) -> f64 {
        self.tick as f64 / f64::from(self.config.ticks_per_second)
    }

    /// Deterministic 64-bit FNV-1a hash of the simulation state, for regression and
    /// reproducibility checks (not security). Integers are hashed little-endian and floats
    /// as little-endian IEEE bit patterns (`-0.0` written as `0.0`), in this order:
    ///
    /// 1. tick (`u64`), ticks per second (`u32`), run seed (`u64`), resolved terrain seed
    ///    (`u64`)
    /// 2. next organism ID (`u64`), organism count (`u64`)
    /// 3. each organism column in storage-index order, one column after another: ID
    ///    (`u64`), x, y, energy (`f32`), age, generation (`u32`), speed, size, sense,
    ///    heading (`f32`), target (`u32`)
    /// 4. cumulative starvation deaths (`u64`), cumulative food consumed (`f64`)
    /// 5. food grid width and height (`u32`), then every cell's food (`f32`) in index order
    /// 6. stream word position of the terrain, initial-population, behavior and mutation
    ///    generators, in that order (`u128` each)
    ///
    /// Generator state is covered without reading raw RNG memory: each ChaCha generator's
    /// key is reconstructable from the hashed run or terrain seed plus that domain's fixed
    /// constant (see `rng`), its stream number is always 0, so the word position is the
    /// only remaining state. Derived values (energy capacity, food caps) and per-tick
    /// scratch (movement effort, pending removals) are not hashed.
    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.write_u64(self.tick);
        hasher.write_u32(self.config.ticks_per_second);
        hasher.write_u64(self.run_seed);
        hasher.write_u64(self.terrain_seed);

        let o = &self.organisms;
        hasher.write_u64(self.next_id);
        hasher.write_u64(o.len() as u64);
        hasher.write_u64_slice(&o.id);
        hasher.write_f32_slice(&o.x);
        hasher.write_f32_slice(&o.y);
        hasher.write_f32_slice(&o.energy);
        hasher.write_u32_slice(&o.age);
        hasher.write_u32_slice(&o.generation);
        hasher.write_f32_slice(&o.speed);
        hasher.write_f32_slice(&o.size);
        hasher.write_f32_slice(&o.sense);
        hasher.write_f32_slice(&o.heading);
        hasher.write_u32_slice(&o.target);

        hasher.write_u64(self.starvation_deaths);
        hasher.write_f64(self.food_consumed);

        hasher.write_u32(self.food.width());
        hasher.write_u32(self.food.height());
        hasher.write_f32_slice(self.food.amounts());

        for word_position in self.rng.word_positions() {
            hasher.write_u128(word_position);
        }
        hasher.finish()
    }

    // Tick phases, in SCOPE 6.9 order. Empty phases are placeholders for the named milestone.
    // Within a phase, organisms are processed in ascending storage index (I-04).

    /// Phase 1. Placeholder: spatial grid (M-08).
    fn rebuild_spatial_grid(&mut self) {}

    /// Phase 2. Goal selection (Week 1 part of M-09). An organism decides on its first tick
    /// (age 0) and then every `decision_interval` ticks, staggered by ID (SCOPE 6.8). It
    /// keeps its current food target while that target is still worthwhile, and otherwise
    /// targets the richest useful food cell within sense range
    /// ([`behavior::reconsider_target`]). With none, it clears its target and draws a new
    /// wandering heading (exactly one behavior-domain draw); keeping or switching a target
    /// draws nothing. Threat checks arrive with predators (M-05).
    fn sense_and_decide(&mut self) {
        let interval = u64::from(self.config.behavior.decision_interval);
        let min_useful = behavior::min_useful_food(self.config.food.intake_per_tick);
        let o = &mut self.organisms;
        for i in 0..o.len() {
            let due = o.age[i] == 0 || self.tick.wrapping_add(o.id[i]).is_multiple_of(interval);
            if !due {
                continue;
            }
            let current = (o.target[i] != Organisms::NO_TARGET).then_some(o.target[i] as usize);
            match behavior::reconsider_target(
                &self.food, o.x[i], o.y[i], o.sense[i], current, min_useful,
            ) {
                Some(cell) => o.target[i] = cell as u32,
                None => {
                    o.target[i] = Organisms::NO_TARGET;
                    o.heading[i] = behavior::wander_heading(&mut self.rng.behavior);
                }
            }
        }
    }

    /// Phase 3. Placeholder: route-request queue within the per-tick budget (M-09).
    fn serve_route_requests(&mut self) {}

    /// Phase 4. Direct movement in an open world, then realized effort.
    ///
    /// The available distance is `max_distance_per_tick(speed) × terrain multiplier`
    /// (1.0 until M-06). An organism with a target stays put once inside the target cell;
    /// otherwise it moves straight toward the cell center at full available distance,
    /// without overshooting. A wanderer moves `wander_speed_fraction` of the available
    /// distance along its heading. Positions are clamped to the world, and realized effort
    /// is the distance actually moved over the available distance.
    fn move_organisms(&mut self) {
        let ticks_per_second = self.config.ticks_per_second;
        let world = &self.config.world;
        let wander_fraction = self.config.behavior.wander_speed_fraction;
        let o = &mut self.organisms;
        self.effort.clear();
        self.effort.resize(o.len(), 0.0);
        for i in 0..o.len() {
            let (x, y) = (o.x[i], o.y[i]);
            let available = traits::max_distance_per_tick(o.speed[i], ticks_per_second)
                * behavior::terrain_speed_multiplier(x, y);
            let (to_x, to_y, step) = if o.target[i] == Organisms::NO_TARGET {
                let step = available * wander_fraction;
                let (to_x, to_y) = behavior::move_along(x, y, o.heading[i], step);
                (to_x, to_y, step)
            } else {
                let cell = o.target[i] as usize;
                if self.food.cell_at(x, y) == cell {
                    (x, y, 0.0)
                } else {
                    let (target_x, target_y) = self.food.cell_center(cell);
                    behavior::move_toward(x, y, target_x, target_y, available)
                }
            };
            let (new_x, new_y) = behavior::clamp_to_world(to_x, to_y, world.width, world.height);
            // The distance moved is the executed step length; only when a world edge cut
            // the step short is it measured from positions. Measuring every step from f32
            // positions would add rounding noise of up to one position ulp per axis.
            let moved = if (new_x, new_y) == (to_x, to_y) {
                step
            } else {
                behavior::distance(x, y, new_x, new_y)
            };
            self.effort[i] = behavior::realized_effort(moved, available);
            o.x[i] = new_x;
            o.y[i] = new_y;
        }
    }

    /// Phase 5. Prey eat from the cell they stand in, in ascending storage index, so with
    /// several prey in one cell the lower index eats first (a provisional, deterministic
    /// order advantage). Each eats [`food::amount_eaten`]; the cell loses exactly that
    /// amount and the prey gains `amount × energy_per_unit`, capped at its capacity.
    /// Predator capture arrives with M-05.
    fn feed(&mut self) {
        let food_config = &self.config.food;
        let coefficients = &self.config.energy;
        let o = &mut self.organisms;
        for i in 0..o.len() {
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
                // Count what actually left the cell; the difference of two f32 values is
                // exact in f64.
                self.food_consumed += f64::from(before) - f64::from(after);
                o.energy[i] = (o.energy[i] + eaten * food_config.energy_per_unit).min(capacity);
            }
        }
    }

    /// Phase 6. Charges each organism the energy formula once (SCOPE 6.3) with this tick's
    /// realized effort, and clamps energy at its lower bound of 0.
    fn apply_energy_costs(&mut self) {
        let ticks_per_second = self.config.ticks_per_second;
        let coefficients = &self.config.energy;
        let organisms = &mut self.organisms;
        for i in 0..organisms.len() {
            let effort = self.effort.get(i).copied().unwrap_or(0.0);
            let cost =
                energy::cost_per_tick(coefficients, organisms.traits(i), effort, ticks_per_second);
            let remaining = organisms.energy[i] - cost;
            organisms.energy[i] = if remaining > 0.0 { remaining } else { 0.0 };
        }
    }

    /// Phase 7. Ages every organism by one tick, then marks starved organisms (energy at
    /// or below 0 after the energy phase, SCOPE 6.7) for removal in phase 9. Death from old
    /// age is S-02 and not implemented.
    fn age_and_check_deaths(&mut self) {
        let organisms = &mut self.organisms;
        for i in 0..organisms.len() {
            organisms.age[i] += 1;
            if organisms.energy[i] <= 0.0 {
                self.pending_removals.push(i);
            }
        }
    }

    /// Phase 8. Placeholder: births in ascending parent ID (M-04).
    fn collect_births(&mut self) {}

    /// Phase 9. Removes marked organisms in descending index order with swap-remove
    /// (I-04), counting each starvation once. Appending births arrives with M-04.
    fn apply_removals_and_births(&mut self) {
        for &index in self.pending_removals.iter().rev() {
            self.organisms.swap_remove(index);
            self.starvation_deaths += 1;
        }
        self.pending_removals.clear();
    }

    /// Phase 10. Food regrows linearly toward each cell's cap (SCOPE 6.4). Famine (S-01)
    /// would change the cap here.
    fn regrow_food(&mut self) {
        self.food.regrow(self.config.food.regrowth_per_tick);
    }

    /// Phase 11. Placeholder: metrics sampling every 30 ticks (M-13).
    fn sample_metrics(&mut self) {}

    /// Full consistency check for tests: aligned columns, unique IDs below `next_id`,
    /// positions inside the world, finite energy within `[0, capacity]`, traits within
    /// range, finite headings, valid targets, ages no greater than the tick, food finite
    /// within `[0, cap]`, and no pending removals between ticks.
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
        let world = &self.config.world;
        for i in 0..o.len() {
            let id = o.id[i];
            let (x, y) = (o.x[i], o.y[i]);
            if !(0.0..=world.width).contains(&x) || !(0.0..=world.height).contains(&y) {
                return Err(format!("organism {id} is outside the world at ({x}, {y})"));
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
            let target = o.target[i];
            if target != Organisms::NO_TARGET && target as usize >= self.food.cell_count() {
                return Err(format!("organism {id} targets missing cell {target}"));
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
        if !self.pending_removals.is_empty() {
            return Err("pending removals left between ticks".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(config: &ScenarioConfig, run_seed: u64) -> Simulation {
        let s = Simulation::new(config, run_seed).expect("test configuration is valid");
        s.check_invariants()
            .expect("invariants hold after construction");
        s
    }

    fn default_sim() -> Simulation {
        sim(&ScenarioConfig::default(), 1)
    }

    /// Small population with identical traits (spread 0) and the given start count.
    fn uniform_config(start_count: u32) -> ScenarioConfig {
        let mut config = ScenarioConfig::default();
        config.prey.start_count = start_count;
        config.prey.speed.spread = 0.0;
        config.prey.size.spread = 0.0;
        config.prey.sense.spread = 0.0;
        config
    }

    /// No food anywhere: every cell's cap is zero.
    fn without_food(mut config: ScenarioConfig) -> ScenarioConfig {
        config.food.abundance = 0.0;
        config
    }

    /// One tick per simulated second, no food, no wandering movement, and a pure base cost
    /// of `cost_per_tick`, so energy arithmetic is exact: capacity 10 (size 1), start
    /// energy `start_fraction × 10`.
    fn exact_energy_config(
        start_count: u32,
        cost_per_tick: f32,
        start_fraction: f32,
    ) -> ScenarioConfig {
        let mut config = without_food(uniform_config(start_count));
        config.ticks_per_second = 1;
        config.behavior.wander_speed_fraction = 0.0;
        config.prey.size.mean = 1.0;
        config.prey.start_energy_fraction = start_fraction;
        config.energy = EnergyConfig {
            capacity: 10.0,
            base: cost_per_tick,
            size: 0.0,
            movement: 0.0,
            sense: 0.0,
        };
        config
    }

    fn step_checked(s: &mut Simulation, n: u64) {
        for _ in 0..n {
            s.step();
            s.check_invariants()
                .expect("invariants hold after every tick");
        }
    }

    fn place(s: &mut Simulation, index: usize, x: f32, y: f32) {
        s.organisms.x[index] = x;
        s.organisms.y[index] = y;
    }

    // ----- M-02 behavior kept -----

    #[test]
    fn new_simulation_starts_at_tick_zero() {
        let s = default_sim();
        assert_eq!(s.tick(), 0);
        assert_eq!(s.ticks_per_second(), 30);
        assert_eq!(s.run_seed(), 1);
        assert_eq!(s.config(), &ScenarioConfig::default());
    }

    #[test]
    fn new_rejects_invalid_configuration() {
        let config = ScenarioConfig {
            ticks_per_second: 0,
            ..ScenarioConfig::default()
        };
        let errors = Simulation::new(&config, 1).expect_err("zero tick rate must be rejected");
        assert_eq!(errors[0].field(), "ticks_per_second");
    }

    #[test]
    fn step_advances_exactly_one_tick_each_call() {
        let mut s = default_sim();
        for expected in 1..=5 {
            s.step();
            assert_eq!(s.tick(), expected);
        }
    }

    #[test]
    fn step_n_zero_is_a_no_op() {
        let mut s = default_sim();
        s.step_n(3);
        let before = s.clone();
        s.step_n(0);
        assert_eq!(s, before);
    }

    #[test]
    fn step_n_equals_repeated_step() {
        let mut batched = default_sim();
        let mut single = default_sim();
        batched.step_n(250);
        for _ in 0..250 {
            single.step();
        }
        assert_eq!(batched.tick(), 250);
        assert_eq!(batched, single);
        assert_eq!(batched.state_hash(), single.state_hash());
    }

    #[test]
    fn sim_seconds_is_derived_from_tick_and_rate() {
        let mut s = sim(&uniform_config(0), 1);
        let close = |actual: f64, expected: f64| (actual - expected).abs() < 1e-12;
        assert!(close(s.sim_seconds(), 0.0));
        s.step_n(30);
        assert!(close(s.sim_seconds(), 1.0));
        s.step_n(15);
        assert_eq!(s.tick(), 45);
        assert!(close(s.sim_seconds(), 1.5));
    }

    #[test]
    fn construction_resolves_terrain_seed_deterministically() {
        let derived = uniform_config(0);
        assert_eq!(sim(&derived, 1).terrain_seed, sim(&derived, 1).terrain_seed);
        assert_eq!(
            sim(&derived, 1).terrain_seed,
            derived.resolved_terrain_seed(1)
        );
        assert_ne!(sim(&derived, 1).terrain_seed, sim(&derived, 2).terrain_seed);

        let explicit = ScenarioConfig {
            terrain_seed: Some(1001),
            ..derived
        };
        assert_eq!(sim(&explicit, 1).terrain_seed, 1001);
        assert_eq!(sim(&explicit, 2).terrain_seed, 1001);
    }

    /// Compatibility vectors: these constants were produced by the approved seed-derivation
    /// rule (see `rng`) for run seed 1 with no explicit terrain seed. They have no
    /// statistical meaning; they freeze the convention so a refactor cannot silently change
    /// every deterministic run. Update them only with a recorded decision.
    #[test]
    fn seed_derivation_compatibility_vectors() {
        use rand_core::Rng;

        let terrain_seed = ScenarioConfig::default().resolved_terrain_seed(1);
        assert_eq!(terrain_seed, TERRAIN_SEED_FOR_RUN_SEED_1);
        let mut domains = RandomDomains::new(terrain_seed, 1);
        assert_eq!(domains.terrain.next_u64(), FIRST_TERRAIN_OUTPUT);
        assert_eq!(
            domains.initial_population.next_u64(),
            FIRST_INITIAL_POPULATION_OUTPUT
        );
        assert_eq!(domains.behavior.next_u64(), FIRST_BEHAVIOR_OUTPUT);
        assert_eq!(domains.mutation.next_u64(), FIRST_MUTATION_OUTPUT);
    }

    const TERRAIN_SEED_FOR_RUN_SEED_1: u64 = 0x4DA7_C317_DA78_E193;
    const FIRST_TERRAIN_OUTPUT: u64 = 0xFC58_D36D_61C6_5214;
    const FIRST_INITIAL_POPULATION_OUTPUT: u64 = 0xFD0E_50AA_A9F4_D201;
    const FIRST_BEHAVIOR_OUTPUT: u64 = 0x0674_4D72_B000_A534;
    const FIRST_MUTATION_OUTPUT: u64 = 0x24B9_27D1_05DC_7781;

    // ----- Initial state (M-03, M-07) -----

    #[test]
    fn initial_population_is_well_formed() {
        let s = default_sim();
        let o = s.organisms();
        let config = s.config();
        assert_eq!(o.len(), 2_000);
        assert!(o.columns_aligned());
        assert_eq!(o.ids(), (0..2_000).collect::<Vec<u64>>());
        assert_eq!(s.next_id, 2_000);
        assert!(o.age().iter().all(|&age| age == 0));
        assert!(o.generation().iter().all(|&g| g == 0));
        assert!(o.targets().iter().all(|&t| t == Organisms::NO_TARGET));
        assert!(o.headings().iter().all(|&h| h == 0.0));
        for i in 0..o.len() {
            let capacity = energy::capacity(&config.energy, o.traits(i).size);
            assert_eq!(o.energy()[i], config.prey.start_energy_fraction * capacity);
        }
        let max_x = o.x().iter().copied().fold(0.0_f32, f32::max);
        let min_x = o.x().iter().copied().fold(f32::MAX, f32::min);
        assert!(max_x - min_x > 0.9 * config.world.width);
    }

    #[test]
    fn initial_food_is_uniform_and_full() {
        let s = default_sim();
        let food = s.food();
        assert_eq!((food.width(), food.height()), (256, 256));
        assert!(food.amounts().iter().all(|&a| a == 1.0));
        assert_eq!(
            food,
            sim(&ScenarioConfig::default(), 2).food(),
            "seed-independent"
        );
    }

    #[test]
    fn same_scenario_and_seed_give_identical_prey() {
        assert_eq!(
            default_sim().organisms(),
            sim(&ScenarioConfig::default(), 1).organisms()
        );
    }

    #[test]
    fn different_run_seeds_give_different_prey() {
        let a = sim(&ScenarioConfig::default(), 1);
        let b = sim(&ScenarioConfig::default(), 2);
        assert_ne!(a.organisms().x(), b.organisms().x());
        assert_ne!(a.organisms().y(), b.organisms().y());
        for id in TraitId::ALL {
            assert_ne!(
                a.organisms().trait_values(id),
                b.organisms().trait_values(id)
            );
        }
    }

    /// Construction uses only the initial-population domain: eight 64-bit draws (16 words)
    /// per organism, whatever the spreads; food initialization draws nothing.
    #[test]
    fn initialization_consumes_only_the_initial_population_domain() {
        for count in [0, 1, 7, 2_000] {
            let mut config = ScenarioConfig::default();
            config.prey.start_count = count;
            let s = sim(&config, 3);
            assert_eq!(s.rng.word_positions(), [0, 16 * u128::from(count), 0, 0]);
        }
        assert_eq!(
            sim(&uniform_config(7), 3).rng.word_positions(),
            [0, 16 * 7, 0, 0],
            "zero spreads draw the same number of values"
        );
        assert_eq!(
            sim(&uniform_config(0), 3).rng.word_positions(),
            [0; 4],
            "food alone draws nothing"
        );
    }

    #[test]
    fn zero_spread_gives_exactly_the_mean() {
        let s = sim(&uniform_config(50), 1);
        let config = s.config();
        for id in TraitId::ALL {
            let mean = config.prey.trait_start(id).mean;
            assert!(s.organisms().trait_values(id).iter().all(|&v| v == mean));
        }
    }

    #[test]
    fn sampled_traits_are_clamped_to_their_ranges() {
        let mut config = ScenarioConfig::default();
        config.prey.start_count = 500;
        config.prey.speed = TraitStart {
            mean: limits::SPEED.max,
            spread: limits::SPEED_SPREAD.max,
        };
        config.prey.size = TraitStart {
            mean: limits::SIZE.min,
            spread: limits::SIZE_SPREAD.max,
        };
        let s = sim(&config, 1);
        assert!(
            s.organisms()
                .trait_values(TraitId::Speed)
                .contains(&limits::SPEED.max)
        );
        assert!(
            s.organisms()
                .trait_values(TraitId::Size)
                .contains(&limits::SIZE.min)
        );
    }

    #[test]
    fn zero_starting_prey_is_a_valid_empty_run() {
        let mut s = sim(&uniform_config(0), 1);
        assert!(s.organisms().is_empty());
        step_checked(&mut s, 10);
        assert_eq!(s.starvation_deaths(), 0);
    }

    // ----- Food regrowth and feeding (M-07) -----

    #[test]
    fn food_regrows_once_per_tick_up_to_cap() {
        let mut config = uniform_config(0);
        config.food.initial_fraction = 0.0;
        config.food.regrowth_per_tick = 0.25;
        let mut s = sim(&config, 1);
        step_checked(&mut s, 1);
        assert!(s.food().amounts().iter().all(|&a| a == 0.25));
        step_checked(&mut s, 2);
        assert!(s.food().amounts().iter().all(|&a| a == 0.75));
        step_checked(&mut s, 5);
        assert!(s.food().amounts().iter().all(|&a| a == 1.0), "capped");

        config.food.regrowth_per_tick = 0.0;
        let mut s = sim(&config, 1);
        step_checked(&mut s, 5);
        assert!(
            s.food().amounts().iter().all(|&a| a == 0.0),
            "zero regrowth"
        );
    }

    /// Feeding configuration with exact binary fractions: capacity 10, 2 energy per food
    /// unit, intake 0.25 per tick, no regrowth, one tick per second, stationary prey.
    fn feeding_sim(start_count: u32) -> Simulation {
        let mut config = exact_energy_config(start_count, 0.0, 0.5);
        config.food.abundance = 1.0;
        config.food.initial_fraction = 0.0;
        config.food.regrowth_per_tick = 0.0;
        config.food.intake_per_tick = 0.25;
        config.food.energy_per_unit = 2.0;
        sim(&config, 1)
    }

    #[test]
    fn feeding_moves_exact_amounts_from_cell_to_energy() {
        let mut s = feeding_sim(1);
        place(&mut s, 0, 3.0, 3.0);
        let cell = s.food.cell_at(3.0, 3.0);
        s.food.food[cell] = 1.0;
        s.feed();
        assert_eq!(s.food.amounts()[cell], 0.75, "cell loses the bite");
        assert_eq!(s.organisms.energy()[0], 5.5, "prey gains 0.25 × 2");
    }

    #[test]
    fn feeding_never_exceeds_capacity_or_empties_below_zero() {
        let mut s = feeding_sim(1);
        place(&mut s, 0, 3.0, 3.0);
        let cell = s.food.cell_at(3.0, 3.0);

        s.food.food[cell] = 1.0;
        s.organisms.energy[0] = 9.75;
        s.feed();
        assert_eq!(s.organisms.energy()[0], 10.0, "filled exactly to capacity");
        assert_eq!(s.food.amounts()[cell], 0.875, "only 0.125 food was needed");

        s.feed();
        assert_eq!(s.food.amounts()[cell], 0.875, "a full prey eats nothing");

        s.organisms.energy[0] = 2.0;
        s.food.food[cell] = 0.125;
        s.feed();
        assert_eq!(s.food.amounts()[cell], 0.0, "cell emptied, not negative");
        assert_eq!(s.organisms.energy()[0], 2.25);
    }

    #[test]
    fn prey_sharing_a_cell_eat_in_storage_order() {
        let mut s = feeding_sim(2);
        place(&mut s, 0, 3.0, 3.0);
        place(&mut s, 1, 3.5, 3.5);
        let cell = s.food.cell_at(3.0, 3.0);
        s.food.food[cell] = 0.375;
        s.feed();
        assert_eq!(s.organisms.energy(), [5.5, 5.25], "index 0 eats first");
        assert_eq!(s.food.amounts()[cell], 0.0);
    }

    #[test]
    fn food_consumed_counts_exactly_the_food_removed() {
        let mut s = feeding_sim(2);
        place(&mut s, 0, 3.0, 3.0);
        place(&mut s, 1, 3.5, 3.5);
        let cell = s.food.cell_at(3.0, 3.0);
        s.food.food[cell] = 0.375;
        s.feed();
        assert_eq!(
            s.cumulative_food_consumed(),
            0.375,
            "0.25 + 0.125, in food units"
        );
        s.feed();
        assert_eq!(
            s.cumulative_food_consumed(),
            0.375,
            "an empty cell adds nothing"
        );

        s.food.food[cell] = 1.0;
        s.organisms.energy[0] = 10.0;
        s.organisms.energy[1] = 10.0;
        s.feed();
        assert_eq!(s.cumulative_food_consumed(), 0.375, "full prey eat nothing");
        assert_eq!(s.food.amounts()[cell], 1.0);
    }

    #[test]
    fn food_consumed_matches_the_food_that_left_the_grid() {
        let mut config = uniform_config(200);
        config.food.regrowth_per_tick = 0.0;
        config.world.width = 64.0;
        config.world.height = 64.0;
        let mut s = sim(&config, 1);
        let start = metrics::sum(s.food().amounts());
        step_checked(&mut s, 300);
        let removed = start - metrics::sum(s.food().amounts());
        assert!(s.cumulative_food_consumed() > 0.0);
        assert!(
            (s.cumulative_food_consumed() - removed).abs() < 1e-6,
            "{} vs {removed}",
            s.cumulative_food_consumed()
        );
    }

    // ----- Summary metrics (M-17, M-21) -----

    #[test]
    fn summary_metrics_report_the_current_state() {
        let mut s = sim(&uniform_config(10), 1);
        let first = s.summary_metrics();
        assert_eq!(first.tick, 0);
        assert_eq!(first.sim_seconds, 0.0);
        assert_eq!(first.prey_population, 10);
        assert_eq!(first.avg_speed, Some(1.0), "zero spread: exactly the mean");
        assert_eq!(first.avg_size, Some(1.0));
        assert_eq!(first.avg_sense, Some(10.0));
        assert_eq!(first.avg_energy, Some(5.0));
        assert_eq!(first.total_food, 65_536.0, "256 × 256 full cells");
        assert_eq!(first.cumulative_food_consumed, 0.0);
        assert_eq!(first.cumulative_starvation_deaths, 0);
        assert_eq!(first.state_hash, s.state_hash());

        step_checked(&mut s, 45);
        let later = s.summary_metrics();
        assert_eq!(later.tick, 45);
        assert_eq!(later.sim_seconds, 1.5);
        assert_eq!(later.cumulative_food_consumed, s.cumulative_food_consumed());
        assert!(later.cumulative_food_consumed > 0.0);
    }

    #[test]
    fn summary_metrics_without_prey_have_no_averages() {
        let s = sim(&uniform_config(0), 1);
        let m = s.summary_metrics();
        assert_eq!(m.prey_population, 0);
        assert_eq!(
            (m.avg_speed, m.avg_size, m.avg_sense, m.avg_energy),
            (None, None, None, None)
        );
        assert!(m.total_food.is_finite() && m.sim_seconds.is_finite());
    }

    /// T-CSV-5 (sim part): taking a summary changes no state, hash or random-domain
    /// position, however often it is taken.
    #[test]
    fn summary_metrics_are_pure_observations() {
        let mut s = default_sim();
        step_checked(&mut s, 20);
        let before = s.clone();
        let positions = s.rng.word_positions();
        let first = s.summary_metrics();
        let second = s.summary_metrics();
        assert_eq!(first, second);
        assert_eq!(s, before);
        assert_eq!(s.rng.word_positions(), positions);
        assert_eq!(s.state_hash(), before.state_hash());
    }

    // ----- Seeking and movement (Week 1 part of M-09) -----

    #[test]
    fn prey_move_toward_detected_food_without_overshooting() {
        let mut config = exact_energy_config(1, 0.0, 0.5);
        config.food.abundance = 1.0;
        config.food.initial_fraction = 0.0;
        config.food.regrowth_per_tick = 0.0;
        config.food.intake_per_tick = 0.0; // keep the food so the target never changes
        config.ticks_per_second = 30;
        let mut s = sim(&config, 1);
        place(&mut s, 0, 100.0, 100.0);
        let target = s.food.cell_at(107.0, 101.0);
        s.food.food[target] = 1.0;
        let available = traits::max_distance_per_tick(s.organisms.speed[0], 30);

        let mut previous = behavior::distance(100.0, 100.0, 107.0, 101.0);
        let mut arrived_at = None;
        for tick in 0..400 {
            let (x0, y0) = (s.organisms.x[0], s.organisms.y[0]);
            step_checked(&mut s, 1);
            let (x1, y1) = (s.organisms.x[0], s.organisms.y[0]);
            assert_eq!(s.organisms.targets()[0], target as u32);
            assert!(behavior::distance(x0, y0, x1, y1) <= available * 1.000_001);
            let (cx, cy) = s.food.cell_center(target);
            let now = behavior::distance(x1, y1, cx, cy);
            if arrived_at.is_none() {
                assert!(now < previous, "closer every tick until arrival");
                if s.food.cell_at(x1, y1) == target {
                    arrived_at = Some(tick);
                }
            } else {
                assert_eq!((x1, y1), (x0, y0), "stays in the target cell");
            }
            previous = now;
        }
        assert!(arrived_at.is_some(), "prey reached the food cell");
    }

    #[test]
    fn realized_effort_follows_actual_movement() {
        // Stationary: no food and no wandering movement.
        let mut s = sim(&exact_energy_config(1, 0.0, 0.5), 1);
        step_checked(&mut s, 1);
        assert_eq!(s.effort, [0.0]);

        // Wandering at half speed in open space.
        let mut config = without_food(uniform_config(1));
        config.behavior.wander_speed_fraction = 0.5;
        let mut s = sim(&config, 1);
        place(&mut s, 0, 256.0, 256.0);
        step_checked(&mut s, 1);
        assert_eq!(s.effort[0], 0.5);

        // Full speed (wandering at fraction 1).
        config.behavior.wander_speed_fraction = 1.0;
        let mut s = sim(&config, 1);
        place(&mut s, 0, 256.0, 256.0);
        step_checked(&mut s, 1);
        assert_eq!(s.effort[0], 1.0);

        // Cut short by the world edge: effort is the fraction actually moved.
        let mut s = sim(&config, 1);
        place(&mut s, 0, 0.0, 0.0);
        s.organisms.heading[0] = core::f32::consts::PI; // straight into the x = 0 edge
        s.move_organisms();
        assert_eq!((s.organisms.x[0], s.organisms.y[0]), (0.0, 0.0));
        assert!(s.effort[0] < 1e-6, "{}", s.effort[0]);
    }

    #[test]
    fn wanderers_stay_inside_the_world() {
        let mut config = without_food(uniform_config(20));
        config.world.width = 16.0;
        config.world.height = 16.0;
        config.behavior.wander_speed_fraction = 1.0;
        config.prey.speed.mean = limits::SPEED.max;
        config.ticks_per_second = 1;
        let mut s = sim(&config, 4);
        step_checked(&mut s, 60);
    }

    // ----- Target reconsideration (Week 1 behavior correction) -----

    /// Identical-trait prey (speed 1, size 1, sense 10) at 30 ticks per second with the
    /// default energy coefficients, no starting food and no regrowth: food is placed by
    /// hand.
    fn hand_placed_food_config(start_count: u32) -> ScenarioConfig {
        let mut config = uniform_config(start_count);
        config.food.initial_fraction = 0.0;
        config.food.regrowth_per_tick = 0.0;
        config
    }

    fn position(s: &Simulation, index: usize) -> (f32, f32) {
        (s.organisms.x[index], s.organisms.y[index])
    }

    /// A hungry prey eats a cell down below one bite, abandons it for the next useful cell
    /// in range, and moves again, three times over; with no useful food left it wanders.
    /// The amounts (0.3, 0.06, 0.06) are chosen so each switch is due to the one-bite rule
    /// alone: a partly eaten first cell never falls below 75% of 0.06 while it still holds
    /// a bite. Keeping and switching targets draw no random numbers; only the final switch
    /// to wandering draws one behavior value (two 32-bit words).
    #[test]
    fn prey_leave_depleted_cells_and_resume_moving() {
        let mut s = sim(&hand_placed_food_config(1), 1);
        place(&mut s, 0, 100.0, 100.0);
        // Cell centers (105, 101), (111, 101), (117, 101): each next one is in range only
        // once the prey has reached the previous one.
        let cells = [105.0, 111.0, 117.0].map(|x| s.food.cell_at(x, 100.0));
        for (&cell, amount) in cells.iter().zip([0.3, 0.06, 0.06]) {
            s.food.food[cell] = amount;
        }
        let rng_start = s.rng.word_positions();

        let bite = s.config.food.intake_per_tick;
        let mut visited = Vec::new();
        let mut departures = 0;
        let mut awaiting_departure = false;
        let mut fed_in_place = 0;
        for _ in 0..2_000 {
            let (before, energy_before) = (position(&s, 0), s.organisms.energy[0]);
            step_checked(&mut s, 1);
            let target = s.organisms.target[0];
            if target == Organisms::NO_TARGET {
                break;
            }
            if let Some(&previous) = visited.last()
                && previous != target
            {
                assert!(
                    s.food.food[previous as usize] < bite,
                    "left cell {previous} only once it held less than a bite"
                );
                awaiting_departure = true;
            }
            if visited.last() != Some(&target) {
                visited.push(target);
            }
            let moved = position(&s, 0) != before;
            fed_in_place += usize::from(!moved && s.organisms.energy[0] > energy_before);
            if moved && awaiting_departure {
                departures += 1;
                awaiting_departure = false;
            }
        }
        assert_eq!(visited, cells.map(|cell| cell as u32), "each cell in turn");
        assert_eq!(departures, 2, "moved on from the first and second cells");
        assert!(
            fed_in_place >= 4,
            "stationary feeding happened: {fed_in_place}"
        );
        assert_eq!(
            s.organisms.target[0],
            Organisms::NO_TARGET,
            "wanders at the end"
        );
        for cell in cells {
            assert!(s.food.food[cell] < bite, "cell {cell} depleted");
        }
        let [terrain, initial, behavior, mutation] = s.rng.word_positions();
        assert_eq!(
            [terrain, initial, mutation],
            [rng_start[0], rng_start[1], rng_start[3]]
        );
        assert_eq!(
            behavior,
            rng_start[2] + 2,
            "only the wandering decision drew"
        );
    }

    /// One sated prey at the center of cell (50, 50) with every cell full (cap 1.0).
    fn sated_prey_on_full_food(regrowth_per_tick: f32) -> Simulation {
        let mut config = uniform_config(1);
        config.food.regrowth_per_tick = regrowth_per_tick;
        config.prey.start_energy_fraction = 1.0;
        let mut s = sim(&config, 1);
        place(&mut s, 0, 101.0, 101.0);
        s
    }

    /// Food a stationary sated prey of this test's traits eats per tick: exactly its
    /// standing energy cost.
    fn standing_demand(s: &Simulation) -> f32 {
        energy::cost_per_tick(&s.config.energy, s.organisms.traits(0), 0.0, 30)
            / s.config.food.energy_per_unit
    }

    /// Diagnostic for the Week 1 defect (prey stop moving about 15 s after Play). A sated
    /// prey eats only its standing cost. When its cell regrows more slowly than that, the
    /// cell slowly depletes: the prey feeds in place for a while (valid), then its cell
    /// falls below `TARGET_SWITCH_FRACTION` of a full neighbor, it retargets and moves on,
    /// and this repeats. No random numbers are drawn: useful food is always in range.
    #[test]
    fn sated_prey_move_on_when_their_cell_regrows_slower_than_they_eat() {
        let regrowth = 0.0005;
        let mut s = sated_prey_on_full_food(regrowth);
        assert!(regrowth < standing_demand(&s), "{}", standing_demand(&s));
        let rng_start = s.rng.word_positions();
        let cap = s.food.cap(0);
        let first = s.food.cell_at(101.0, 101.0);

        step_checked(&mut s, 30);
        assert_eq!(s.organisms.target[0], first as u32, "feeds where it stands");
        assert_eq!(position(&s, 0), (101.0, 101.0), "stationary while feeding");
        assert!(s.food.food[first] < cap, "it ate");

        let mut current = first as u32;
        let mut switches = Vec::new();
        let mut moving_ticks = 0;
        for _ in 30..6_000 {
            let before = position(&s, 0);
            step_checked(&mut s, 1);
            let target = s.organisms.target[0];
            if target != current {
                let left_behind = s.food.food[current as usize];
                assert!(
                    left_behind < TARGET_SWITCH_FRACTION * cap + regrowth,
                    "switched away from {left_behind}, no longer worthwhile"
                );
                assert_eq!(s.food.food[target as usize], cap, "to a full cell");
                switches.push(s.tick());
                current = target;
            }
            if position(&s, 0) != before {
                moving_ticks += 1;
            }
        }
        assert!(switches.len() >= 3, "repeated retargeting: {switches:?}");
        assert!(
            switches[0] > 100,
            "first fed in place for a while: {switches:?}"
        );
        assert!(moving_ticks > 0, "movement resumed");
        assert_eq!(
            s.rng.word_positions(),
            rng_start,
            "reconsideration drew nothing"
        );
    }

    /// The boundary of the rule: when a cell regrows at least as fast as its sated
    /// occupant eats (the shared default, 0.002 per tick), the cell stays at its cap and
    /// remains worthwhile, so the prey feeds there indefinitely. This is valid stationary
    /// feeding, not a stuck target; the app's demonstration scenario lowers regrowth.
    #[test]
    fn sated_prey_stay_where_regrowth_covers_their_demand() {
        let mut s = sated_prey_on_full_food(0.002);
        assert!(0.002 >= standing_demand(&s));
        let first = s.food.cell_at(101.0, 101.0) as u32;
        for _ in 0..6_000 {
            step_checked(&mut s, 1);
            assert_eq!(s.organisms.target[0], first);
        }
        assert_eq!(position(&s, 0), (101.0, 101.0));
    }

    /// Controlled multi-cell, multi-prey run in the slow-regrowth regime: in every
    /// 20-second window prey move, feed, deplete cells below a bite and retarget, rather
    /// than settling into permanent stationary feeding.
    #[test]
    fn grazing_population_keeps_moving_feeding_and_retargeting() {
        let mut config = ScenarioConfig::default();
        config.world.width = 64.0;
        config.world.height = 64.0;
        config.prey.start_count = 100;
        config.food.initial_fraction = 0.25;
        config.food.regrowth_per_tick = 0.0005;
        let mut s = sim(&config, 1);
        let bite = config.food.intake_per_tick;

        for window in 0..6 {
            let (mut moved, mut fed, mut retargeted, mut depleted) = (0, 0, 0, 0);
            for _ in 0..600 {
                let before = s.organisms.clone();
                let food_before = s.food.food.clone();
                s.step();
                depleted += food_before
                    .iter()
                    .zip(&s.food.food)
                    .filter(|&(&was, &now)| was >= bite && now < bite)
                    .count();
                if s.organisms.ids() != before.ids() {
                    continue; // a death reordered storage this tick
                }
                for i in 0..s.organisms.len() {
                    moved += usize::from(position(&s, i) != (before.x[i], before.y[i]));
                    fed += usize::from(s.organisms.energy[i] > before.energy[i]);
                    retargeted += usize::from(s.organisms.target[i] != before.target[i]);
                }
            }
            s.check_invariants().expect("consistent");
            // Prey-ticks of movement and feeding, target changes, and cells eaten below a
            // bite in this window. Floors sit well under the observed values (at least
            // 31,000, 1,000, 400 and 36) and far above a stationary population's (0).
            let counts =
                format!("moved {moved}, fed {fed}, retargeted {retargeted}, depleted {depleted}");
            assert!(moved >= 10_000, "window {window}: {counts}");
            assert!(fed >= 500, "window {window}: {counts}");
            assert!(retargeted >= 100, "window {window}: {counts}");
            assert!(depleted >= 10, "window {window}: {counts}");
        }
    }

    /// T-EN-1 (controlled run): energy falls by exactly the formula's cost, charged once
    /// per tick with the tick's realized effort, and by nothing else.
    #[test]
    fn t_en_1_energy_is_charged_exactly_once_per_tick() {
        // Exact arithmetic: 0.5 per tick from 10.0.
        let mut s = sim(&exact_energy_config(1, 0.5, 1.0), 1);
        assert_eq!(s.organisms().energy(), [10.0]);
        step_checked(&mut s, 4);
        assert_eq!(s.organisms().energy(), [8.0]);

        // Default coefficients at 30 ticks per second, stationary: the non-movement terms.
        let mut config = without_food(uniform_config(1));
        config.behavior.wander_speed_fraction = 0.0;
        let mut s = sim(&config, 1);
        let traits = s.organisms().traits(0);
        let stationary = energy::cost_per_tick(&config.energy, traits, 0.0, 30);
        let mut expected = s.organisms().energy()[0];
        for _ in 0..100 {
            s.step();
            expected -= stationary;
            assert_eq!(s.organisms().energy()[0], expected);
        }

        // Moving: each tick costs exactly the formula at that tick's realized effort.
        config.behavior.wander_speed_fraction = 1.0;
        let mut s = sim(&config, 1);
        place(&mut s, 0, 256.0, 256.0);
        for _ in 0..30 {
            let before = s.organisms().energy()[0];
            s.step();
            let charged = energy::cost_per_tick(&config.energy, traits, s.effort[0], 30);
            assert!(s.effort[0] > 0.99);
            assert_eq!(s.organisms().energy()[0], before - charged);
            assert!(charged > stationary, "moving costs more than standing");
        }
    }

    /// T-EN-3: without food, energy falls every tick; an organism just above 0 survives
    /// until the cost brings it to 0, and is removed in the tick that happens.
    #[test]
    fn t_en_3_starving_prey_are_removed_the_tick_energy_reaches_zero() {
        let mut s = sim(&exact_energy_config(3, 1.0, 0.5), 1);
        let mut previous = s.organisms().energy()[0];
        for tick in 1..=4 {
            step_checked(&mut s, 1);
            let now = s.organisms().energy()[0];
            assert!(now < previous, "energy falls every tick");
            previous = now;
            assert_eq!(s.organisms().len(), 3, "alive after tick {tick}");
        }
        assert_eq!(s.organisms().energy(), [1.0, 1.0, 1.0]);
        assert_eq!(s.starvation_deaths(), 0);

        step_checked(&mut s, 1);
        assert!(s.organisms().is_empty());
        assert_eq!(s.starvation_deaths(), 3, "each starvation counted once");
        step_checked(&mut s, 5);
        assert_eq!(s.starvation_deaths(), 3);
    }

    /// T-EN-3: energy clamps at 0 rather than going negative, and the organism is marked in
    /// the aging-and-death phase (7) but removed only in phase 9.
    #[test]
    fn t_en_3_starvation_is_detected_in_phase_7_and_removed_in_phase_9() {
        let mut s = sim(&exact_energy_config(2, 1.0, 0.5), 1);
        s.organisms.energy[0] = 0.3;

        s.apply_energy_costs();
        assert_eq!(s.organisms.energy[0], 0.0, "clamped at the lower bound");
        assert_eq!(
            s.organisms.energy[0].to_bits(),
            0.0_f32.to_bits(),
            "not -0.0"
        );
        s.age_and_check_deaths();
        assert_eq!(s.pending_removals, [0]);
        assert_eq!(s.organisms.len(), 2, "still stored until phase 9");
        s.apply_removals_and_births();
        assert_eq!(s.organisms.ids(), [1]);
        assert_eq!(s.starvation_deaths, 1);
    }

    /// Feeding (phase 5) happens before the energy charge (6) and the starvation check (7):
    /// a prey that would starve this tick survives if it eats enough first.
    #[test]
    fn feeding_before_the_starvation_check_prevents_death() {
        let mut config = exact_energy_config(1, 1.0, 0.05); // 0.5 energy, costs 1.0 per tick
        let mut hungry = sim(&config, 1);
        step_checked(&mut hungry, 1);
        assert_eq!(hungry.starvation_deaths(), 1, "dies without food");

        config.food.abundance = 1.0;
        config.food.intake_per_tick = 1.0;
        let mut fed = sim(&config, 1);
        step_checked(&mut fed, 1);
        assert_eq!(fed.starvation_deaths(), 0);
        assert_eq!(
            fed.organisms().energy(),
            [0.5],
            "0.5 + 1.0 eaten - 1.0 cost"
        );
    }

    /// Week 1 vertical slice: a prey with food in sense range approaches it, enters its
    /// cell, eats, gains energy, and outlives an identical prey without that food.
    #[test]
    fn vertical_slice_prey_seek_eat_and_outlive_unfed_twin() {
        let mut config = without_food(uniform_config(1));
        config.food.abundance = 1.0;
        config.food.initial_fraction = 0.0;
        config.food.regrowth_per_tick = 0.0;
        let food_cell_point = (106.0, 100.0);

        let run = |with_food: bool| {
            let mut s = sim(&config, 1);
            place(&mut s, 0, 100.0, 100.0);
            let cell = s.food.cell_at(food_cell_point.0, food_cell_point.1);
            if with_food {
                s.food.food[cell] = 1.0;
            }
            let mut ate = false;
            let mut entered = false;
            while !s.organisms().is_empty() {
                let before = s.organisms().energy()[0];
                s.step();
                if let Some(&x) = s.organisms().x().first() {
                    entered |= s.food.cell_at(x, s.organisms().y()[0]) == cell;
                    ate |= s.organisms().energy()[0] > before;
                }
                assert!(s.tick() < 1_000_000);
            }
            (s.tick(), entered, ate, s.food.amounts()[cell])
        };

        let (fed_death, entered, ate, food_left) = run(true);
        let (unfed_death, _, unfed_ate, _) = run(false);
        assert!(entered, "the prey reached the food cell");
        assert!(ate, "the prey gained energy by eating");
        assert!(food_left < 1.0, "the cell lost food");
        assert!(!unfed_ate);
        assert!(
            fed_death > unfed_death,
            "fed {fed_death} vs unfed {unfed_death}"
        );
    }

    #[test]
    fn every_prey_eventually_starves_without_food() {
        let mut config = without_food(ScenarioConfig::default());
        config.prey.start_count = 50;
        let mut s = sim(&config, 1);
        let mut ticks = 0;
        while !s.organisms().is_empty() {
            s.step();
            ticks += 1;
            assert!(ticks < 1_000_000, "prey should starve");
        }
        assert_eq!(s.starvation_deaths(), 50);
        s.check_invariants().expect("consistent when empty");
    }

    #[test]
    fn age_increments_once_per_tick() {
        let mut s = sim(&uniform_config(4), 1);
        step_checked(&mut s, 1);
        assert_eq!(s.organisms().age(), [1; 4]);
        step_checked(&mut s, 36);
        assert_eq!(s.organisms().age(), [37; 4]);
    }

    /// Removal keeps every surviving row intact, including the behavior columns.
    #[test]
    fn removal_keeps_survivor_rows_aligned_with_their_ids() {
        let mut s = sim(&exact_energy_config(5, 1.0, 1.0), 1);
        step_checked(&mut s, 3);
        s.organisms.heading[4] = 2.5;
        // Give ID 2 its own cell as target, so it stays put while targeting.
        let own_cell = s.food.cell_at(s.organisms.x[2], s.organisms.y[2]);
        s.organisms.target[2] = own_cell as u32;
        let before = s.organisms().clone();
        s.organisms.energy[1] = 0.5;
        s.organisms.energy[3] = 0.5;
        step_checked(&mut s, 1);

        let after = s.organisms();
        assert_eq!(after.ids(), [0, 4, 2], "descending swap-remove order");
        assert_eq!(s.starvation_deaths(), 2);
        for (i, &id) in after.ids().iter().enumerate() {
            let j = before.index_of(id).expect("survivor existed before");
            assert_eq!(after.x()[i], before.x()[j]);
            assert_eq!(after.y()[i], before.y()[j]);
            assert_eq!(after.traits(i), before.traits(j));
            assert_eq!(after.generation()[i], before.generation()[j]);
            assert_eq!(after.headings()[i], before.headings()[j]);
            assert_eq!(after.age()[i], before.age()[j] + 1);
            assert_eq!(after.energy()[i], before.energy()[j] - 1.0);
        }
        assert_eq!(
            after.targets()[after.index_of(2).expect("alive")],
            own_cell as u32
        );
    }

    // ----- Random-domain use while stepping -----

    #[test]
    fn food_seeking_draws_no_random_numbers() {
        let mut s = default_sim();
        let at_start = s.rng.word_positions();
        step_checked(&mut s, 200);
        assert_eq!(
            s.rng.word_positions(),
            at_start,
            "food is everywhere in range"
        );
        assert_eq!(at_start, [0, 16 * 2_000, 0, 0]);
    }

    /// Wandering draws one behavior-domain value per wandering decision and touches no
    /// other domain. Ten prey (IDs 0–9) with no food: all decide at tick 0, then ID k
    /// decides again at the tick t ≤ 9 where (t + k) % 10 == 0, which is every ID but 0.
    #[test]
    fn wandering_draws_only_from_the_behavior_domain() {
        let mut s = sim(&without_food(uniform_config(10)), 1);
        let [terrain, initial, behavior, mutation] = s.rng.word_positions();
        assert_eq!(behavior, 0);
        step_checked(&mut s, 1);
        assert_eq!(s.rng.word_positions(), [terrain, initial, 2 * 10, mutation]);
        step_checked(&mut s, 9);
        assert_eq!(
            s.rng.word_positions(),
            [terrain, initial, 2 * (10 + 9), mutation]
        );
    }

    // ----- Determinism and state hash -----

    /// T-DET-2 on real state: different run seeds give different prey and food, not just
    /// different stored seeds, through tick 1,000.
    #[test]
    fn t_det_2_run_seeds_diverge_in_organism_state() {
        let config = ScenarioConfig::default();
        let mut seed_1 = sim(&config, 1);
        let mut seed_2 = sim(&config, 2);
        assert_ne!(seed_1.organisms(), seed_2.organisms());
        seed_1.step_n(1_000);
        seed_2.step_n(1_000);
        assert!(!seed_1.organisms().is_empty() && !seed_2.organisms().is_empty());
        assert_ne!(seed_1.organisms().x(), seed_2.organisms().x());
        assert_ne!(seed_1.organisms().energy(), seed_2.organisms().energy());
        assert_ne!(seed_1.food().amounts(), seed_2.food().amounts());
        assert_ne!(seed_1.state_hash(), seed_2.state_hash());
    }

    #[test]
    fn identical_state_gives_identical_hash() {
        assert_eq!(default_sim().state_hash(), default_sim().state_hash());
    }

    #[test]
    fn stepping_changes_the_hash() {
        let mut s = default_sim();
        let mut previous = s.state_hash();
        for _ in 0..10 {
            s.step();
            let current = s.state_hash();
            assert_ne!(current, previous);
            previous = current;
        }
    }

    #[test]
    fn hashing_does_not_change_state() {
        let mut s = default_sim();
        s.step_n(10);
        let snapshot = s.clone();
        let first = s.state_hash();
        let second = s.state_hash();
        assert_eq!(first, second);
        assert_eq!(s, snapshot);
    }

    /// A named change to one piece of simulation state.
    type Mutation = (&'static str, fn(&mut Simulation));

    #[test]
    fn hash_covers_every_organism_field_food_and_counter() {
        let mut base = sim(&uniform_config(3), 1);
        base.step_n(2);
        let base_hash = base.state_hash();
        let mutations: [Mutation; 16] = [
            ("id", |s| s.organisms.id[1] = 99),
            ("x", |s| s.organisms.x[1] += 0.5),
            ("y", |s| s.organisms.y[1] += 0.5),
            ("energy", |s| s.organisms.energy[1] -= 0.25),
            ("age", |s| s.organisms.age[1] += 1),
            ("generation", |s| s.organisms.generation[1] += 1),
            ("speed", |s| s.organisms.speed[1] += 0.5),
            ("size", |s| s.organisms.size[1] += 0.5),
            ("sense", |s| s.organisms.sense[1] += 0.5),
            ("heading", |s| s.organisms.heading[1] += 0.5),
            ("target", |s| s.organisms.target[1] ^= 1),
            ("next_id", |s| s.next_id += 1),
            ("starvation_deaths", |s| s.starvation_deaths += 1),
            ("food_consumed", |s| s.food_consumed += 0.125),
            ("removal", |s| s.organisms.swap_remove(2)),
            ("food", |s| s.food.food[17] -= 0.125),
        ];
        for (name, mutate) in mutations {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert_ne!(changed.state_hash(), base_hash, "{name} is not covered");
        }
        let mut scratch = base.clone();
        scratch.effort = vec![0.75; 3];
        assert_eq!(
            scratch.state_hash(),
            base_hash,
            "per-tick effort is not state"
        );
    }

    #[test]
    fn hash_covers_seeds_tick_rate_and_grid_size() {
        let base = uniform_config(0);
        let base_hash = sim(&base, 1).state_hash();
        let faster = ScenarioConfig {
            ticks_per_second: 60,
            ..base.clone()
        };
        let other_terrain = ScenarioConfig {
            terrain_seed: Some(base.resolved_terrain_seed(1) ^ 1),
            ..base.clone()
        };
        let mut coarser = base.clone();
        coarser.world.cell_size = 4.0;
        assert_ne!(sim(&faster, 1).state_hash(), base_hash);
        assert_ne!(sim(&other_terrain, 1).state_hash(), base_hash);
        assert_ne!(sim(&base, 2).state_hash(), base_hash);
        assert_ne!(sim(&coarser, 1).state_hash(), base_hash);
    }
}
