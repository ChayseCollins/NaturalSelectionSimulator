//! Unit tests of the tick pipeline that need private state (SCOPE 15.2). Mechanism tests
//! that only need the public API are in `sim/tests/`.

#![allow(clippy::field_reassign_with_default)]

use super::*;

/// A simulation on open grassland (uniform fertility, no barriers), checked after
/// construction.
fn sim(config: &ScenarioConfig, run_seed: u64) -> Simulation {
    let s = Simulation::with_terrain(config, run_seed, Terrain::open(&config.world))
        .expect("test configuration is valid");
    s.check_invariants()
        .expect("invariants hold after construction");
    s
}

/// Prey only, identical traits (spread 0), no births (the cap equals the start count).
fn uniform_config(start_count: u32) -> ScenarioConfig {
    let mut config = ScenarioConfig::default();
    config.species.prey.start = StartPopulation::Count(start_count);
    config.species.predator.start = StartPopulation::Count(0);
    config.species.prey.speed.spread = 0.0;
    config.species.prey.size.spread = 0.0;
    config.species.prey.sense.spread = 0.0;
    config.population_cap = start_count.max(1);
    config
}

fn default_sim() -> Simulation {
    sim(&uniform_config(2_000), 1)
}

/// No food anywhere: every cell's cap is zero.
fn without_food(mut config: ScenarioConfig) -> ScenarioConfig {
    config.food.abundance = 0.0;
    config
}

/// One tick per simulated second, no food, no wandering movement, and a pure base cost of
/// `cost_per_tick`, so energy arithmetic is exact: capacity 10 (size 1), start energy
/// `start_fraction × 10`.
fn exact_energy_config(
    start_count: u32,
    cost_per_tick: f32,
    start_fraction: f32,
) -> ScenarioConfig {
    let mut config = without_food(uniform_config(start_count));
    config.ticks_per_second = 1;
    config.behavior.wander_speed_fraction = 0.0;
    config.species.prey.size.mean = 1.0;
    config.species.prey.start_energy_fraction = start_fraction;
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
    s.rebuild_spatial_grid();
}

fn position(s: &Simulation, index: usize) -> (f32, f32) {
    (s.organisms.x[index], s.organisms.y[index])
}

fn traits(speed: f32, size: f32, sense: f32) -> Traits {
    Traits { speed, size, sense }
}

fn at(species: Species, x: f32, y: f32, traits: Traits, energy: Option<f32>) -> Placement {
    Placement {
        species,
        x,
        y,
        traits,
        energy,
    }
}

/// A constructed run on an open 64 × 64 world with the given organisms and no food.
fn constructed(config: &ScenarioConfig, placements: &[Placement]) -> Simulation {
    let mut config = config.clone();
    config.world.width = 64.0;
    config.world.height = 64.0;
    let s = Simulation::from_placements(&config, 1, Terrain::open(&config.world), placements)
        .expect("valid placements");
    s.check_invariants().expect("consistent");
    s
}

// ----- M-02 core -----

#[test]
fn new_simulation_starts_at_tick_zero() {
    let s = default_sim();
    assert_eq!(s.tick(), 0);
    assert_eq!(s.ticks_per_second(), 30);
    assert_eq!(s.run_seed(), 1);
    assert_eq!(s.metrics().samples().len(), 1, "tick-0 chart sample");
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
fn starting_populations_above_the_cap_are_rejected() {
    let mut config = ScenarioConfig::default();
    config.population_cap = 100;
    config.species.prey.start = StartPopulation::Density(100.0);
    let errors = Simulation::new(&config, 1).expect_err("too many organisms");
    assert_eq!(errors[0].field(), "species");
}

#[test]
fn step_n_equals_repeated_step() {
    let mut config = ScenarioConfig::default();
    config.world.width = 128.0;
    config.world.height = 128.0;
    let mut batched = Simulation::new(&config, 3).expect("valid");
    let mut single = batched.clone();
    batched.step_n(250);
    for _ in 0..250 {
        single.step();
    }
    assert_eq!(batched.tick(), 250);
    assert_eq!(batched, single);
    assert_eq!(batched.state_hash(), single.state_hash());
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
fn sim_seconds_is_derived_from_tick_and_rate() {
    let mut s = sim(&uniform_config(0), 1);
    s.step_n(45);
    assert_eq!(s.tick(), 45);
    assert!((s.sim_seconds() - 1.5).abs() < 1e-12);
}

#[test]
fn construction_resolves_terrain_seed_deterministically() {
    let derived = uniform_config(0);
    assert_eq!(
        sim(&derived, 1).terrain_seed,
        derived.resolved_terrain_seed(1)
    );
    assert_ne!(sim(&derived, 1).terrain_seed, sim(&derived, 2).terrain_seed);
    let mut explicit = derived.clone();
    explicit.terrain.seed = Some(1001);
    assert_eq!(sim(&explicit, 1).terrain_seed, 1001);
    assert_eq!(sim(&explicit, 2).terrain_seed, 1001);
}

/// Compatibility vectors: these constants were produced by the approved seed-derivation
/// rule (see `rng`) for run seed 1 with no explicit terrain seed. They freeze the
/// convention so a refactor cannot silently change every deterministic run.
#[test]
fn seed_derivation_compatibility_vectors() {
    use rand_core::Rng;

    let terrain_seed = ScenarioConfig::default().resolved_terrain_seed(1);
    assert_eq!(terrain_seed, 0x4DA7_C317_DA78_E193);
    let mut domains = RandomDomains::new(terrain_seed, 1);
    assert_eq!(domains.terrain.next_u64(), 0xFC58_D36D_61C6_5214);
    assert_eq!(domains.initial_population.next_u64(), 0xFD0E_50AA_A9F4_D201);
    assert_eq!(domains.behavior.next_u64(), 0x0674_4D72_B000_A534);
    assert_eq!(domains.mutation.next_u64(), 0x24B9_27D1_05DC_7781);
}

// ----- Initial state (M-03, M-05, M-06, M-07) -----

#[test]
fn initial_population_is_well_formed() {
    let s = default_sim();
    let o = s.organisms();
    assert_eq!(o.len(), 2_000);
    assert_eq!(o.ids(), (0..2_000).collect::<Vec<u64>>());
    assert_eq!(s.next_id, 2_000);
    assert!(o.age().iter().all(|&age| age == 0));
    assert!(o.generation().iter().all(|&g| g == 0));
    assert!(o.goals().iter().all(|&g| g == Goal::Wander));
    assert!(o.target_cells().iter().all(|&t| t == Organisms::NO_TARGET));
    for i in 0..o.len() {
        let capacity = energy::capacity(&s.config.energy, o.traits(i).size);
        assert_eq!(o.energy()[i], 0.5 * capacity);
    }
    let max_x = o.x().iter().copied().fold(0.0_f32, f32::max);
    let min_x = o.x().iter().copied().fold(f32::MAX, f32::min);
    assert!(max_x - min_x > 0.9 * 512.0);
}

#[test]
fn prey_come_first_then_predators_on_passable_cells() {
    let config = ScenarioConfig::default();
    let s = Simulation::new(&config, 7).expect("valid");
    s.check_invariants().expect("consistent");
    let o = s.organisms();
    let prey = o.count(Species::Prey);
    let predators = o.count(Species::Predator);
    let passable = s.terrain().passable_cells().len() as f64;
    assert_eq!(prey as f64, (40.0 * passable / 1_000.0).round());
    assert_eq!(predators as f64, (4.0 * passable / 1_000.0).round());
    assert!(o.species()[..prey].iter().all(|&sp| sp == Species::Prey));
    assert!(
        o.species()[prey..]
            .iter()
            .all(|&sp| sp == Species::Predator)
    );
    let predator_size = metrics::SpeciesStats::of(o, Species::Predator)
        .mean_size
        .expect("predators exist");
    assert!(
        (predator_size - 1.5).abs() < 0.05,
        "predator mean size {predator_size}"
    );
}

#[test]
fn initial_food_on_open_ground_is_uniform_and_full() {
    let s = default_sim();
    let food = s.food();
    assert_eq!((food.width(), food.height()), (256, 256));
    assert!(food.amounts().iter().all(|&a| a == 1.0));
}

#[test]
fn generated_terrain_shapes_food_caps() {
    let s = Simulation::new(&ScenarioConfig::default(), 1).expect("valid");
    let terrain = s.terrain();
    for (cell, &class) in terrain.classes().iter().enumerate() {
        assert_eq!(s.food().cap(cell), class.fertility(), "{}", class.name());
    }
    assert!(terrain.classes().contains(&TerrainClass::Water));
    assert!(terrain.classes().contains(&TerrainClass::Rock));
}

/// Construction uses only the initial-population domain on given terrain: eight 64-bit
/// draws (16 words) per organism whatever the spreads; food draws nothing.
#[test]
fn initialization_consumes_only_the_initial_population_domain() {
    for count in [0, 1, 7, 2_000] {
        let s = sim(&uniform_config(count), 3);
        assert_eq!(s.rng.word_positions(), [0, 16 * u128::from(count), 0, 0]);
    }
    let mut spread = uniform_config(7);
    spread.species.prey.speed.spread = 0.5;
    assert_eq!(sim(&spread, 3).rng.word_positions(), [0, 16 * 7, 0, 0]);
}

#[test]
fn zero_spread_gives_exactly_the_mean() {
    let s = sim(&uniform_config(50), 1);
    for id in TraitId::ALL {
        let mean = s.config.species.prey.trait_start(id).mean;
        assert!(s.organisms().trait_values(id).iter().all(|&v| v == mean));
    }
}

#[test]
fn sampled_traits_are_clamped_to_their_ranges() {
    let mut config = uniform_config(500);
    config.species.prey.speed = TraitStart {
        mean: limits::SPEED.max,
        spread: limits::SPEED_SPREAD.max,
    };
    config.species.prey.size = TraitStart {
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

// ----- Food regrowth and feeding (M-07) -----

#[test]
fn food_regrows_once_per_tick_up_to_cap() {
    let mut config = uniform_config(0);
    config.food.initial_fraction = 0.0;
    config.food.regrowth_per_tick = 0.25;
    let mut s = sim(&config, 1);
    step_checked(&mut s, 1);
    assert!(s.food().amounts().iter().all(|&a| a == 0.25));
    step_checked(&mut s, 7);
    assert!(s.food().amounts().iter().all(|&a| a == 1.0), "capped");
}

/// Feeding configuration with exact binary fractions: capacity 10, 2 energy per food unit,
/// intake 0.25 per tick, no regrowth, one tick per second, stationary prey.
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
    assert_eq!(s.cumulative_food_consumed(), 0.375);
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
    assert!((s.cumulative_food_consumed() - removed).abs() < 1e-6);
}

// ----- Summary metrics (M-13, M-17, M-21) -----

#[test]
fn summary_metrics_report_the_current_state() {
    let mut s = sim(&uniform_config(10), 1);
    let first = s.summary_metrics();
    assert_eq!(first.tick, 0);
    assert_eq!(first.prey.population, 10);
    assert_eq!(
        first.prey.mean_speed,
        Some(1.0),
        "zero spread: exactly the mean"
    );
    assert_eq!(first.prey.mean_energy, Some(5.0));
    assert_eq!(first.prey.max_generation, Some(0));
    assert_eq!(first.predator.population, 0);
    assert_eq!(first.predator.mean_speed, None);
    assert_eq!(first.total_food, 65_536.0, "256 × 256 full cells");
    assert_eq!(first.state_hash, s.state_hash());
    step_checked(&mut s, 45);
    let later = s.summary_metrics();
    assert_eq!(later.tick, 45);
    assert_eq!(later.sim_seconds, 1.5);
    assert!(later.cumulative_food_consumed > 0.0);
}

/// T-CSV-5 (sim part): taking a summary changes no state, hash or random-domain position.
#[test]
fn summary_metrics_are_pure_observations() {
    let mut s = Simulation::new(&ScenarioConfig::default(), 2).expect("valid");
    s.step_n(20);
    let before = s.clone();
    let positions = s.rng.word_positions();
    assert_eq!(s.summary_metrics(), s.summary_metrics());
    assert_eq!(s, before);
    assert_eq!(s.rng.word_positions(), positions);
}

/// T-CHART-1 (unit part): each chart sample's counts and means equal a recount.
#[test]
fn t_chart_1_samples_equal_a_recount() {
    let mut s = Simulation::new(&ScenarioConfig::default(), 4).expect("valid");
    for _ in 0..20 {
        s.step_n(30);
        let sample = *s.metrics().samples().last().expect("sampled");
        assert_eq!(sample.tick, s.tick());
        for species in Species::ALL {
            let recount = SpeciesStats::of(s.organisms(), species);
            let recorded = sample.species(species);
            assert_eq!(recorded.population, s.organisms().count(species));
            for (a, b) in [
                (recorded.mean_speed, recount.mean_speed),
                (recorded.mean_size, recount.mean_size),
                (recorded.mean_sense, recount.mean_sense),
                (recorded.mean_generation, recount.mean_generation),
            ] {
                match (a, b) {
                    (Some(a), Some(b)) => assert!((a - b).abs() <= 1e-6 * b.abs().max(1.0)),
                    (None, None) => {}
                    other => panic!("{other:?}"),
                }
            }
        }
    }
    assert_eq!(s.metrics().samples().len(), 21);
}

// ----- Seeking and movement (M-09) -----

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
    let mut arrived = false;
    for _ in 0..400 {
        let before = position(&s, 0);
        step_checked(&mut s, 1);
        let now_at = position(&s, 0);
        assert_eq!(s.organisms.target_cell[0], target as u32);
        assert!(
            behavior::distance(before.0, before.1, now_at.0, now_at.1) <= available * 1.000_001
        );
        let (cx, cy) = s.food.cell_center(target);
        let now = behavior::distance(now_at.0, now_at.1, cx, cy);
        if !arrived {
            assert!(now < previous, "closer every tick until arrival");
            arrived = s.food.cell_at(now_at.0, now_at.1) == target;
        } else {
            assert_eq!(now_at, before, "stays in the target cell");
        }
        previous = now;
    }
    assert!(arrived, "prey reached the food cell");
}

#[test]
fn realized_effort_follows_actual_movement() {
    let mut s = sim(&exact_energy_config(1, 0.0, 0.5), 1);
    step_checked(&mut s, 1);
    assert_eq!(s.effort, [0.0], "stationary");

    let mut config = without_food(uniform_config(1));
    config.behavior.wander_speed_fraction = 0.5;
    let mut s = sim(&config, 1);
    place(&mut s, 0, 256.0, 256.0);
    step_checked(&mut s, 1);
    assert_eq!(s.effort[0], 0.5, "wandering at half speed");

    config.behavior.wander_speed_fraction = 1.0;
    let mut s = sim(&config, 1);
    place(&mut s, 0, 0.0, 0.0);
    s.organisms.heading[0] = core::f32::consts::PI; // straight into the x = 0 edge
    s.move_organisms();
    assert_eq!(position(&s, 0), (0.0, 0.0));
    assert!(s.effort[0] < 1e-6, "{}", s.effort[0]);
}

#[test]
fn wanderers_stay_inside_the_world() {
    let mut config = without_food(uniform_config(20));
    config.world.width = 16.0;
    config.world.height = 16.0;
    config.behavior.wander_speed_fraction = 1.0;
    config.species.prey.speed.mean = limits::SPEED.max;
    config.ticks_per_second = 1;
    let mut s = sim(&config, 4);
    step_checked(&mut s, 60);
}

/// Prey with no useful food in range wander; wandering draws one behavior value per
/// decision and touches no other domain. Ten prey (IDs 0–9) all decide at tick 0, then
/// ID k decides again at the tick t ≤ 9 where (t + k) % 10 == 0, which is every ID but 0.
#[test]
fn wandering_draws_only_from_the_behavior_domain() {
    let mut s = sim(&without_food(uniform_config(10)), 1);
    let [terrain, initial, behavior, mutation] = s.rng.word_positions();
    step_checked(&mut s, 1);
    assert_eq!(s.rng.word_positions(), [terrain, initial, 2 * 10, mutation]);
    step_checked(&mut s, 9);
    assert_eq!(
        s.rng.word_positions(),
        [terrain, initial, 2 * (10 + 9), mutation]
    );
    assert_eq!(behavior, 0);
}

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
}

/// A hungry prey eats a cell down below one bite, abandons it for the next useful cell in
/// range, and moves again; with no useful food left it wanders (one behavior draw).
#[test]
fn prey_leave_depleted_cells_and_resume_moving() {
    let mut config = uniform_config(1);
    config.food.initial_fraction = 0.0;
    config.food.regrowth_per_tick = 0.0;
    let mut s = sim(&config, 1);
    place(&mut s, 0, 100.0, 100.0);
    let cells = [105.0, 111.0, 117.0].map(|x| s.food.cell_at(x, 100.0));
    for (&cell, amount) in cells.iter().zip([0.3, 0.06, 0.06]) {
        s.food.food[cell] = amount;
    }
    let rng_start = s.rng.word_positions();
    let bite = s.config.food.intake_per_tick;
    let mut visited: Vec<u32> = Vec::new();
    for _ in 0..2_000 {
        step_checked(&mut s, 1);
        if s.organisms.goal[0] == Goal::Wander {
            break;
        }
        let target = s.organisms.target_cell[0];
        if let Some(&previous) = visited.last()
            && previous != target
        {
            assert!(
                s.food.food[previous as usize] < bite,
                "left only below a bite"
            );
        }
        if visited.last() != Some(&target) {
            visited.push(target);
        }
    }
    assert_eq!(visited, cells.map(|cell| cell as u32), "each cell in turn");
    assert_eq!(s.organisms.goal[0], Goal::Wander, "wanders at the end");
    let [_, _, behavior, _] = s.rng.word_positions();
    assert_eq!(
        behavior,
        rng_start[2] + 2,
        "only the wandering decision drew"
    );
}

/// A sated prey whose cell regrows more slowly than it eats moves on from time to time;
/// reconsideration draws no random numbers.
#[test]
fn sated_prey_move_on_when_their_cell_regrows_slower_than_they_eat() {
    let mut config = uniform_config(1);
    config.food.regrowth_per_tick = 0.0005;
    config.species.prey.start_energy_fraction = 1.0;
    let mut s = sim(&config, 1);
    place(&mut s, 0, 101.0, 101.0);
    let rng_start = s.rng.word_positions();
    let mut switches = 0;
    let mut current = u32::MAX;
    for _ in 0..6_000 {
        step_checked(&mut s, 1);
        if s.organisms.target_cell[0] != current {
            switches += 1;
            current = s.organisms.target_cell[0];
        }
    }
    assert!(switches >= 4, "{switches}");
    assert_eq!(s.rng.word_positions(), rng_start);
}

// ----- Energy and death (M-03) -----

/// T-EN-1 (controlled run): energy falls by exactly the formula's cost, charged once per
/// tick with the tick's realized effort.
#[test]
fn t_en_1_energy_is_charged_exactly_once_per_tick() {
    let mut s = sim(&exact_energy_config(1, 0.5, 1.0), 1);
    assert_eq!(s.organisms().energy(), [10.0]);
    step_checked(&mut s, 4);
    assert_eq!(s.organisms().energy(), [8.0]);

    let mut config = without_food(uniform_config(1));
    config.behavior.wander_speed_fraction = 1.0;
    let mut s = sim(&config, 1);
    place(&mut s, 0, 256.0, 256.0);
    let t = s.organisms().traits(0);
    for _ in 0..30 {
        let before = s.organisms().energy()[0];
        s.step();
        let charged = energy::cost_per_tick(&config.energy, t, s.effort[0], 30);
        assert_eq!(s.organisms().energy()[0], before - charged);
    }
}

/// T-EN-3 for both species: without food, energy falls every tick and the organism is
/// removed the tick it reaches 0, counted once as starvation.
#[test]
fn t_en_3_starving_organisms_are_removed_the_tick_energy_reaches_zero() {
    for species in Species::ALL {
        let mut config = exact_energy_config(0, 1.0, 0.5);
        config.population_cap = 10;
        let placements: Vec<Placement> = (0..3)
            .map(|k| {
                at(
                    species,
                    10.0 + 10.0 * k as f32,
                    10.0,
                    traits(1.0, 1.0, 5.0),
                    Some(5.0),
                )
            })
            .collect();
        let mut s = constructed(&config, &placements);
        for tick in 1..=4 {
            let before = s.organisms().energy()[0];
            step_checked(&mut s, 1);
            assert!(s.organisms().energy()[0] < before, "falls every tick");
            assert_eq!(s.organisms().len(), 3, "alive after tick {tick}");
        }
        step_checked(&mut s, 1);
        assert!(s.organisms().is_empty(), "{}", species.name());
        assert_eq!(s.counters().species(species).starvation_deaths, 3);
        assert_eq!(s.extinct_at(species), Some(5));
    }
}

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
    assert_eq!(fed.organisms().energy(), [0.5]);
}

#[test]
fn removal_keeps_survivor_rows_aligned_with_their_ids() {
    let mut s = sim(&exact_energy_config(5, 1.0, 1.0), 1);
    step_checked(&mut s, 3);
    s.organisms.heading[4] = 2.5;
    let before = s.organisms().clone();
    s.organisms.energy[1] = 0.5;
    s.organisms.energy[3] = 0.5;
    step_checked(&mut s, 1);
    let after = s.organisms();
    assert_eq!(after.ids(), [0, 4, 2], "descending swap-remove order");
    for (i, &id) in after.ids().iter().enumerate() {
        let j = before.index_of(id).expect("survivor existed before");
        assert_eq!(after.traits(i), before.traits(j));
        assert_eq!(after.headings()[i], before.headings()[j]);
        assert_eq!(after.age()[i], before.age()[j] + 1);
        assert_eq!(after.energy()[i], before.energy()[j] - 1.0);
    }
}

/// S-02: with a maximum lifespan, organisms older than it die of old age.
#[test]
fn old_age_kills_only_when_enabled() {
    let mut config = exact_energy_config(3, 0.0, 0.5);
    let mut s = sim(&config, 1);
    step_checked(&mut s, 20);
    assert_eq!(s.organisms().len(), 3, "off by default");
    config.species.prey.max_age_seconds = 10.0; // 10 ticks at 1 tick per second
    let mut s = sim(&config, 1);
    step_checked(&mut s, 10);
    assert_eq!(s.organisms().len(), 3, "age 10 is not older than 10");
    step_checked(&mut s, 1);
    assert!(s.organisms().is_empty());
    assert_eq!(s.counters().prey.age_deaths, 3);
}

// ----- Reproduction (M-04) -----

/// Prey on open ground with a large cap: energy is set by hand, then one tick runs.
fn breeding_sim(species: Species, energy: f32) -> Simulation {
    let mut config = exact_energy_config(0, 0.0, 0.5);
    config.population_cap = 100;
    config.mutation.probability = 0.0;
    config.reproduction = ReproductionConfig {
        threshold: 0.8,
        offspring_energy_fraction: 0.25,
        overhead: 0.5,
        spawn_radius: 3.0,
    };
    constructed(
        &config,
        &[at(species, 32.0, 32.0, traits(1.0, 1.0, 5.0), Some(energy))],
    )
}

/// T-REP-1 for both species: below the threshold no birth; at or above it exactly one
/// birth that tick.
#[test]
fn t_rep_1_threshold_gives_exactly_one_birth() {
    for species in Species::ALL {
        let mut below = breeding_sim(species, 7.99);
        step_checked(&mut below, 1);
        assert_eq!(
            below.organisms().len(),
            1,
            "{}: below threshold",
            species.name()
        );
        for energy in [8.0, 10.0] {
            let mut at_threshold = breeding_sim(species, energy);
            step_checked(&mut at_threshold, 1);
            assert_eq!(
                at_threshold.organisms().len(),
                2,
                "{}: one birth",
                species.name()
            );
            assert_eq!(at_threshold.counters().species(species).births, 1);
        }
    }
}

/// T-REP-2: parent energy after = before − offspring energy − overhead; the offspring's
/// energy equals the configured fraction of its capacity.
#[test]
fn t_rep_2_parent_pays_offspring_energy_plus_overhead() {
    let mut s = breeding_sim(Species::Prey, 9.0);
    step_checked(&mut s, 1);
    let o = s.organisms();
    assert_eq!(o.energy()[1], 0.25 * 10.0, "offspring: 25% of capacity 10");
    assert_eq!(o.energy()[0], 9.0 - 2.5 - 0.5);
}

/// T-REP-3: offspring generation = parent + 1, age 0 at birth, same species, new unique
/// ID, on a passable cell within the spawn radius.
#[test]
fn t_rep_3_offspring_are_well_formed() {
    for species in Species::ALL {
        let mut s = breeding_sim(species, 10.0);
        s.organisms.generation[0] = 4;
        s.collect_births();
        s.apply_removals_and_births();
        let o = s.organisms();
        assert_eq!(o.ids(), [0, 1]);
        assert_eq!(o.generation()[1], 5);
        assert_eq!(o.age()[1], 0);
        assert_eq!(o.species()[1], species);
        assert_eq!(o.traits(1), o.traits(0), "p = 0: exact copy");
        let d = behavior::distance(o.x()[0], o.y()[0], o.x()[1], o.y()[1]);
        assert!(d <= 3.0 + 1e-5, "within the spawn radius: {d}");
        assert!(s.terrain().passable_at(o.x()[1], o.y()[1]));
        assert_eq!(s.next_id, 2);
    }
}

/// T-REP-4 (should): at the population cap, births are suppressed and the warning flag
/// is set.
#[test]
fn t_rep_4_population_cap_suppresses_births() {
    let mut s = breeding_sim(Species::Prey, 10.0);
    s.config.population_cap = 1;
    step_checked(&mut s, 1);
    assert_eq!(s.organisms().len(), 1);
    assert!(s.population_cap_reached());
    assert_eq!(
        s.rng.word_positions()[3],
        0,
        "suppressed births draw nothing"
    );
}

#[test]
fn births_follow_ascending_parent_id_and_get_ids_in_birth_order() {
    let mut config = exact_energy_config(0, 0.0, 0.5);
    config.population_cap = 100;
    config.mutation.probability = 1.0;
    let placements: Vec<Placement> = (0..3)
        .map(|k| {
            at(
                Species::Prey,
                10.0 + 10.0 * k as f32,
                10.0,
                traits(1.0, 1.0, 5.0),
                Some(10.0),
            )
        })
        .collect();
    let mut s = constructed(&config, &placements);
    // Scramble storage order: IDs [2, 0, 1].
    s.organisms = Organisms::default();
    for (id, x) in [(2, 30.0), (0, 10.0), (1, 20.0)] {
        s.organisms.push_row(OrganismRow::new(
            id,
            Species::Prey,
            (x, 10.0),
            10.0,
            0,
            traits(1.0, 1.0, 5.0),
        ));
    }
    s.rebuild_spatial_grid();
    s.next_id = 3;
    s.dying = vec![false; 3];
    s.collect_births();
    let parents_in_order: Vec<f32> = s.births.iter().map(|row| row.x).collect();
    s.apply_removals_and_births();
    // Children of parents 0, 1, 2 (x near 10, 20, 30) get IDs 3, 4, 5.
    for (k, parent_x) in [10.0, 20.0, 30.0].into_iter().enumerate() {
        assert!((parents_in_order[k] - parent_x).abs() <= 2.0 + 1e-4);
        let child = s.organisms().index_of(3 + k as u64).expect("child");
        assert!((s.organisms().x()[child] - parent_x).abs() <= 2.0 + 1e-4);
    }
}

#[test]
fn births_draw_only_from_the_mutation_domain() {
    let mut s = breeding_sim(Species::Prey, 10.0);
    s.dying = vec![false];
    let before = s.rng.word_positions();
    s.collect_births();
    let after = s.rng.word_positions();
    assert_eq!(
        [after[0], after[1], after[2]],
        [before[0], before[1], before[2]]
    );
    assert_eq!(after[3], before[3] + 22, "eleven 64-bit draws per birth");
}

// ----- Predation (M-05) -----

fn predation_config() -> ScenarioConfig {
    let mut config = exact_energy_config(0, 0.0, 0.5);
    config.population_cap = 100;
    config.predation = PredationConfig {
        size_ratio: 1.2,
        capture_radius: 1.0,
        efficiency: 0.5,
        content: 4.0,
    };
    config.reproduction.threshold = 1.0;
    config
}

fn predator_and_prey(prey_x: f32, prey_size: f32, predator_energy: f32) -> Simulation {
    constructed(
        &predation_config(),
        &[
            at(
                Species::Predator,
                30.0,
                30.0,
                traits(0.1, 1.5, 1.0),
                Some(predator_energy),
            ),
            at(
                Species::Prey,
                prey_x,
                30.0,
                traits(0.1, prey_size, 1.0),
                Some(5.0),
            ),
        ],
    )
}

/// T-PRED-1: an eligible predator within the capture radius captures the prey, which is
/// removed at the end of the tick; T-PRED-4 (should): the gain is efficiency × content.
#[test]
fn t_pred_1_eligible_predator_in_radius_captures() {
    let mut s = predator_and_prey(30.8, 1.0, 5.0);
    s.dying = vec![false; 2];
    s.feed();
    assert_eq!(s.removals.len(), 1, "marked during feeding");
    assert_eq!(s.organisms().len(), 2, "still stored until phase 9");
    assert_eq!(s.organisms().energy()[0], 5.0 + 0.5 * 4.0 * 1.0);
    s.apply_removals_and_births();
    assert_eq!(s.organisms().ids(), [0]);
    assert_eq!(s.counters().predations(), 1);
    assert_eq!(s.counters().prey.predation_deaths, 1);
    assert_eq!(s.counters().prey.starvation_deaths, 0);
}

/// T-PRED-2: size ratio not met: no capture even when touching.
#[test]
fn t_pred_2_size_ratio_not_met_means_no_capture() {
    let mut s = predator_and_prey(30.0, 1.26, 5.0); // 1.5 < 1.2 × 1.26
    step_checked(&mut s, 5);
    assert_eq!(s.organisms().len(), 2);
    assert_eq!(s.counters().predations(), 0);
    let mut s = predator_and_prey(30.0, 1.25, 5.0); // 1.5 ≥ 1.2 × 1.25
    step_checked(&mut s, 1);
    assert_eq!(s.counters().predations(), 1);
}

/// T-PRED-3: outside the capture radius: no capture.
#[test]
fn t_pred_3_outside_the_capture_radius_means_no_capture() {
    let mut s = predator_and_prey(31.01, 1.0, 5.0);
    s.dying = vec![false; 2];
    s.feed();
    assert!(s.removals.is_empty());
    let mut s = predator_and_prey(31.0, 1.0, 5.0);
    s.dying = vec![false; 2];
    s.feed();
    assert_eq!(s.removals.len(), 1, "the radius is inclusive");
}

/// T-PRED-4 / T-EN-4 (should): the gain is capped at capacity.
#[test]
fn capture_gain_is_capped_at_capacity() {
    let mut s = predator_and_prey(30.5, 1.0, 14.5);
    s.dying = vec![false; 2];
    s.feed();
    assert_eq!(s.organisms().energy()[0], 15.0, "capacity 10 × 1.5");
}

/// T-PRED-5 (should): two predators touching one prey: only the lower-ID predator
/// captures it, even when it is stored later.
#[test]
fn t_pred_5_lower_id_predator_wins() {
    let mut s = constructed(
        &predation_config(),
        &[
            at(Species::Prey, 30.0, 30.0, traits(0.1, 1.0, 1.0), Some(5.0)),
            at(
                Species::Predator,
                30.5,
                30.0,
                traits(0.1, 1.5, 1.0),
                Some(5.0),
            ),
            at(
                Species::Predator,
                29.5,
                30.0,
                traits(0.1, 1.5, 1.0),
                Some(5.0),
            ),
        ],
    );
    // Store predator 2 before predator 1.
    s.organisms.swap_remove(1);
    s.organisms.push_row(OrganismRow::new(
        1,
        Species::Predator,
        (30.5, 30.0),
        5.0,
        0,
        traits(0.1, 1.5, 1.0),
    ));
    s.rebuild_spatial_grid();
    s.dying = vec![false; 3];
    s.feed();
    let winner = s.organisms().index_of(1).expect("predator 1");
    let loser = s.organisms().index_of(2).expect("predator 2");
    assert_eq!(s.organisms().energy()[winner], 7.0);
    assert_eq!(s.organisms().energy()[loser], 5.0);
    assert_eq!(s.removals.len(), 1, "the prey is captured once");
}

/// T-PRED-7 (should): predators never consume plant food.
#[test]
fn t_pred_7_predators_never_eat_plants() {
    let mut config = predation_config();
    config.food.abundance = 1.0;
    let mut s = constructed(
        &config,
        &[at(
            Species::Predator,
            30.0,
            30.0,
            traits(0.1, 1.5, 1.0),
            Some(5.0),
        )],
    );
    let food = s.food().amounts().to_vec();
    step_checked(&mut s, 30);
    assert_eq!(s.food().amounts(), &food[..]);
    assert_eq!(s.cumulative_food_consumed(), 0.0);
}

/// A prey that is captured and would also starve that tick is removed once, as predation.
#[test]
fn captured_and_starving_prey_is_removed_once() {
    let mut s = constructed(
        &predation_config(),
        &[
            at(Species::Prey, 30.0, 30.0, traits(0.1, 1.0, 1.0), Some(0.0)),
            at(Species::Prey, 40.0, 40.0, traits(0.1, 1.0, 1.0), Some(5.0)),
            at(
                Species::Predator,
                30.5,
                30.0,
                traits(0.1, 1.5, 1.0),
                Some(5.0),
            ),
        ],
    );
    step_checked(&mut s, 1);
    assert_eq!(
        s.organisms().ids(),
        [2, 1],
        "only the captured prey is gone"
    );
    assert_eq!(s.counters().prey.predation_deaths, 1);
    assert_eq!(s.counters().prey.starvation_deaths, 0);
}

/// Prey flee the nearest predator in sense range; predators hunt the nearest eligible
/// prey in sense range.
#[test]
fn prey_flee_and_predators_hunt() {
    let mut config = predation_config();
    config.ticks_per_second = 30;
    let mut s = constructed(
        &config,
        &[
            at(Species::Prey, 30.0, 30.0, traits(1.0, 1.0, 8.0), Some(5.0)),
            at(
                Species::Predator,
                36.0,
                30.0,
                traits(1.0, 1.5, 8.0),
                Some(5.0),
            ),
        ],
    );
    step_checked(&mut s, 1);
    assert_eq!(s.organisms().goals()[0], Goal::Flee);
    assert_eq!(s.organisms().target_ids()[0], 1);
    assert_eq!(s.organisms().goals()[1], Goal::Hunt);
    assert_eq!(s.organisms().target_ids()[1], 0);
    assert!(s.organisms().x()[0] < 30.0, "prey moved away");
    assert!(s.organisms().x()[1] < 36.0, "predator closed in");
}

// ----- Terrain movement (M-06) -----

/// T-TM-1: distance per tick on a class with multiplier m equals m × the distance on
/// multiplier-1 terrain (±1e-4); T-TM-2 (should): the per-tick energy cost is the same.
#[test]
fn t_tm_1_terrain_multiplier_scales_distance_but_not_cost() {
    let mut results = Vec::new();
    for symbol in ['.', 'f', '^', ':'] {
        let row: String = std::iter::repeat_n(symbol, 40).collect();
        let map = format!("{row}\n").repeat(8);
        let terrain = Terrain::from_ascii(&map, 2.0).expect("valid map");
        let mut config = without_food(uniform_config(0));
        config.behavior.wander_speed_fraction = 1.0;
        let mut s = Simulation::from_placements(
            &config,
            1,
            terrain,
            &[at(Species::Prey, 10.0, 3.0, traits(2.0, 1.0, 5.0), None)],
        )
        .expect("valid");
        s.organisms.heading[0] = 0.0;
        s.organisms.age[0] = 1; // skip the first decision so the heading stays +x
        let energy_before = s.organisms.energy[0];
        s.move_organisms();
        s.dying = vec![false];
        s.apply_energy_costs();
        let moved = s.organisms.x[0] - 10.0;
        let class = TerrainClass::from_symbol(symbol).expect("known");
        results.push((
            class.speed_multiplier(),
            moved,
            energy_before - s.organisms.energy[0],
        ));
    }
    let (_, base_distance, base_cost) = results[0];
    assert!(base_distance > 0.0);
    for (multiplier, distance, cost) in results {
        assert!(
            (distance - multiplier * base_distance).abs() <= 1e-4,
            "{multiplier}: {distance}"
        );
        assert!(
            (cost - base_cost).abs() <= 1e-7,
            "same cost per tick on every class"
        );
    }
}

#[test]
fn walls_block_movement_and_routes_go_around_them() {
    let terrain = Terrain::from_ascii(
        "..........\n\
         ..........\n\
         ..........\n\
         ....#.....\n\
         ....#.....\n\
         ....#.....\n\
         ..........\n\
         ..........\n",
        2.0,
    )
    .expect("valid map");
    let mut config = uniform_config(0);
    config.food.initial_fraction = 0.0;
    config.food.regrowth_per_tick = 0.0;
    config.food.intake_per_tick = 0.0;
    let mut s = Simulation::from_placements(
        &config,
        1,
        terrain,
        &[at(Species::Prey, 5.0, 5.0, traits(2.0, 1.0, 10.0), None)],
    )
    .expect("valid");
    s.set_cell_food(13.0, 5.0, 1.0);
    let food_cell = s.food().cell_at(13.0, 5.0);
    for _ in 0..300 {
        step_checked(&mut s, 1);
        if s.food().cell_at(s.organisms().x()[0], s.organisms().y()[0]) == food_cell {
            return;
        }
    }
    panic!("never reached the food behind the wall");
}

// ----- Picking and inspection (M-12) -----

/// T-INSP-1: picking returns the nearest organism within the pick radius, lowest ID on a
/// tie, none on empty space; T-INSP-2 (should): the snapshot equals the columns.
#[test]
fn t_insp_1_picking_and_snapshots() {
    let s = constructed(
        &predation_config(),
        &[
            at(Species::Prey, 10.0, 10.0, traits(1.0, 1.0, 5.0), Some(3.0)),
            at(
                Species::Predator,
                12.0,
                10.0,
                traits(1.5, 2.0, 7.0),
                Some(4.0),
            ),
            at(Species::Prey, 14.0, 10.0, traits(1.0, 1.0, 5.0), Some(3.0)),
        ],
    );
    assert_eq!(s.nearest_organism(10.4, 10.0, 3.0), Some(0));
    assert_eq!(s.nearest_organism(11.9, 10.0, 3.0), Some(1));
    assert_eq!(
        s.nearest_organism(13.0, 10.0, 3.0),
        Some(1),
        "tie: lower ID"
    );
    assert_eq!(s.nearest_organism(40.0, 40.0, 3.0), None, "empty space");
    assert_eq!(
        s.nearest_organism(16.0, 10.0, 1.0),
        None,
        "outside the radius"
    );

    let snapshot = s.organism_snapshot(1).expect("alive");
    assert_eq!(snapshot.species, Species::Predator);
    assert_eq!((snapshot.x, snapshot.y), (12.0, 10.0));
    assert_eq!(snapshot.energy, 4.0);
    assert_eq!(snapshot.capacity, 20.0);
    assert_eq!(snapshot.traits, traits(1.5, 2.0, 7.0));
    assert_eq!(snapshot.generation, 0);
    assert_eq!(s.organism_snapshot(99), None, "unknown or dead");
}

// ----- State hash -----

#[test]
fn hash_covers_counters_food_queue_and_flags() {
    let mut base = Simulation::new(&ScenarioConfig::default(), 1).expect("valid");
    base.step_n(2);
    let base_hash = base.state_hash();
    type Change = (&'static str, fn(&mut Simulation));
    let changes: [Change; 12] = [
        ("next_id", |s| s.next_id += 1),
        ("births", |s| s.counters.prey.births += 1),
        ("starvation", |s| s.counters.predator.starvation_deaths += 1),
        ("predation", |s| s.counters.prey.predation_deaths += 1),
        ("age", |s| s.counters.prey.age_deaths += 1),
        ("food_consumed", |s| s.food_consumed += 0.125),
        ("extinction", |s| s.extinct_at[1] = Some(3)),
        ("cap flag", |s| s.population_cap_reached = true),
        ("frozen", |s| s.frozen_population = true),
        ("queue", |s| {
            s.route_queue.push_back(RouteRequest { tick: 1, id: 2 })
        }),
        ("removal", |s| s.organisms.swap_remove(2)),
        ("food", |s| s.food.food[17] = 0.5),
    ];
    for (name, change) in changes {
        let mut changed = base.clone();
        change(&mut changed);
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
fn hash_covers_seeds_tick_rate_and_terrain() {
    let base = ScenarioConfig::default();
    let base_hash = Simulation::new(&base, 1).expect("valid").state_hash();
    let mut faster = base.clone();
    faster.ticks_per_second = 60;
    let mut other_terrain = base.clone();
    other_terrain.terrain.seed = Some(base.resolved_terrain_seed(1) ^ 1);
    let mut wetter = base.clone();
    wetter.terrain.water_level = 0.4;
    for config in [faster, other_terrain, wetter] {
        assert_ne!(
            Simulation::new(&config, 1).expect("valid").state_hash(),
            base_hash
        );
    }
    assert_ne!(
        Simulation::new(&base, 2).expect("valid").state_hash(),
        base_hash
    );
}

#[test]
fn frozen_population_suppresses_births_and_deaths() {
    let mut config = ScenarioConfig::default();
    config.world.width = 128.0;
    config.world.height = 128.0;
    config.food.abundance = 0.0;
    let mut s = Simulation::new(&config, 1).expect("valid");
    s.set_frozen_population(true);
    let count = s.organisms().len();
    s.step_n(3_000);
    assert_eq!(s.organisms().len(), count);
    assert_eq!(
        s.counters().prey.deaths() + s.counters().predator.deaths(),
        0
    );
    assert_eq!(s.counters().prey.births + s.counters().predator.births, 0);
}

/// The full model runs on generated terrain with every invariant holding every tick.
#[test]
fn ecological_run_on_generated_terrain_stays_consistent() {
    let mut config = ScenarioConfig::default();
    config.world.width = 192.0;
    config.world.height = 192.0;
    let mut s = Simulation::new(&config, 11).expect("valid");
    s.check_invariants().expect("consistent at start");
    for _ in 0..1_500 {
        s.step();
        s.check_invariants().expect("consistent");
        assert!(s.route_expansions_last_tick() <= config.navigation.budget_per_tick);
        assert!(s.max_request_expansions_last_tick() <= config.navigation.expansions_per_request);
    }
    let counters = s.counters();
    assert!(counters.prey.births > 0, "prey reproduced");
    assert!(counters.prey.deaths() > 0, "prey died");
}
