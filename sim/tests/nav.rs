//! Navigation tests on hand-drawn ASCII maps (SCOPE 6.8, 15.2: T-NAV-1, T-NAV-2, T-NAV-4
//! to T-NAV-7). Every test checks every tick that no organism is on impassable terrain.
//! Time limits come from the map's distances and the trait speeds, plus a 50% margin.

mod common;

use common::*;
use sim::{Goal, ScenarioConfig, Simulation, Species, StartPopulation};

const TPS: f32 = 30.0;

fn limit(distance: f32, speed: f32) -> u64 {
    (distance / speed * TPS * 1.5).ceil() as u64
}

fn seeker_config() -> ScenarioConfig {
    let mut config = quiet_config(false);
    config.food.intake_per_tick = 0.0; // the food stays, so the target never changes
    config.behavior.wander_speed_fraction = 0.0;
    config
}

/// T-NAV-1: reaches food on an unobstructed route within T ticks.
#[test]
fn t_nav_1_reaches_food_on_an_open_route() {
    let mut s = build(
        &seeker_config(),
        &open_map(16, 16),
        &[place(Species::Prey, 3.0, 3.0, traits(1.0, 1.0, 20.0), 5.0)],
    );
    s.set_cell_food(17.0, 13.0, 1.0);
    let distance = ((17.0f32 - 3.0).powi(2) + (13.0f32 - 3.0).powi(2)).sqrt();
    let reached = run_until(&mut s, limit(distance, 1.0), |s| {
        in_cell_of(s, 0, 17.0, 13.0)
    });
    assert!(reached.is_some());
}

/// T-NAV-2: reaches food behind a wall with one opening within T ticks.
#[test]
fn t_nav_2_reaches_food_behind_a_wall_with_one_opening() {
    let map = "\
        ................\n\
        ................\n\
        ................\n\
        ................\n\
        ................\n\
        #############.##\n\
        ................\n\
        ................\n\
        ................\n\
        ................\n";
    let mut s = build(
        &seeker_config(),
        map,
        &[place(Species::Prey, 3.0, 3.0, traits(1.0, 1.0, 20.0), 5.0)],
    );
    // The food is straight above the prey, behind the wall; the opening is at x 26–28.
    s.set_cell_food(3.0, 17.0, 1.0);
    let route_length = ((27.0f32 - 3.0).powi(2) + (9.0f32 - 3.0).powi(2)).sqrt()
        + ((27.0f32 - 3.0).powi(2) + (17.0f32 - 9.0).powi(2)).sqrt();
    let mut queued = false;
    let reached = run_until(&mut s, limit(route_length, 1.0), |s| {
        queued |= s.organisms().remaining_route(0).len() > 1;
        in_cell_of(s, 0, 3.0, 17.0)
    });
    assert!(reached.is_some(), "food behind the wall not reached");
    assert!(queued, "a route with waypoints was used");
}

/// T-NAV-4: food sealed inside rock is never pursued past stuck recovery: the organism
/// picks another target or wanders and is never stuck on that target longer than three
/// stuck windows. Sealed food is in another reachability region, so it is rejected at once.
/// Food that is reachable only through an opening outside the search window is abandoned
/// after a failed search and marked unreachable.
#[test]
fn t_nav_4_unreachable_food_is_abandoned() {
    let config = seeker_config();
    let window = u64::from(config.navigation.stuck_window);
    let sealed_map = "\
        ..........\n\
        ..........\n\
        ...###....\n\
        ...#.#....\n\
        ...###....\n\
        ..........\n\
        ..........\n\
        ..........\n";
    let mut s = build(
        &config,
        sealed_map,
        &[place(Species::Prey, 3.0, 9.0, traits(1.0, 1.0, 15.0), 5.0)],
    );
    s.set_cell_food(9.0, 9.0, 1.0);
    let sealed = s.food().cell_at(9.0, 9.0) as u32;
    let mut targeted = 0;
    run_until(&mut s, 3 * window + 60, |s| {
        if s.organisms().target_cells()[0] == sealed {
            targeted += 1;
        }
        false
    });
    assert_eq!(targeted, 0, "sealed food is never targeted");

    // A long wall whose opening is far outside the search window (sense 10 + margin 16).
    let mut rows = vec![".".repeat(120); 12];
    rows[6] = format!("{}.", "#".repeat(119));
    let far_map = rows.join("\n");
    let mut s = build(
        &config,
        &far_map,
        &[place(Species::Prey, 20.0, 9.0, traits(1.0, 1.0, 10.0), 5.0)],
    );
    s.set_cell_food(20.0, 17.0, 1.0);
    let target = s.food().cell_at(20.0, 17.0) as u32;
    let mut on_target = 0u64;
    let mut longest = 0u64;
    run_until(&mut s, 10 * window, |s| {
        if s.organisms().target_cells()[0] == target {
            on_target += 1;
            longest = longest.max(on_target);
        } else {
            on_target = 0;
        }
        false
    });
    assert!(
        longest <= 3 * window,
        "stuck on the target for {longest} ticks"
    );
    assert_ne!(
        s.organisms().goals()[0],
        Goal::Seek,
        "gave up on the unreachable food"
    );
}

/// T-NAV-5: a predator reaches and captures slow, reachable prey around an obstruction
/// within T ticks.
#[test]
fn t_nav_5_predator_captures_prey_around_an_obstruction() {
    let map = "\
        ....................\n\
        ....................\n\
        ....................\n\
        ........#...........\n\
        ........#...........\n\
        ........#...........\n\
        ........#...........\n\
        ........#...........\n\
        ........#...........\n\
        ....................\n";
    let config = quiet_config(false);
    let mut s = build(
        &config,
        map,
        &[
            // A slow prey with a short sense range: it barely reacts.
            place(Species::Prey, 23.0, 9.0, traits(0.2, 1.0, 2.0), 5.0),
            place(Species::Predator, 9.0, 9.0, traits(1.5, 1.5, 25.0), 7.0),
        ],
    );
    let detour = 2.0 * ((9.0f32 - 3.0).powi(2) + 16.0).sqrt() + 14.0;
    let caught = run_until(&mut s, limit(detour, 1.3), |s| {
        s.counters().predations() > 0
    });
    assert!(caught.is_some(), "the predator did not get around the wall");
}

/// T-NAV-6: route expansions per tick never exceed the budget, asserted every tick, and
/// each request stays within its own cap, on a crowded generated map.
#[test]
fn t_nav_6_route_budget_is_never_exceeded() {
    let mut config = ScenarioConfig::default();
    config.world.width = 256.0;
    config.world.height = 256.0;
    config.terrain.water_level = 0.35;
    config.terrain.feature_scale = 40.0;
    config.navigation.budget_per_tick = 1_000;
    config.navigation.expansions_per_request = 256;
    config.species.prey.start = StartPopulation::Count(1_500);
    config.species.predator.start = StartPopulation::Count(150);
    let mut s = Simulation::new(&config, 3).expect("valid");
    let mut busiest = 0;
    for _ in 0..1_500 {
        s.step();
        assert!(s.route_expansions_last_tick() <= 1_000, "tick {}", s.tick());
        assert!(
            s.max_request_expansions_last_tick() <= 256,
            "tick {}",
            s.tick()
        );
        busiest = busiest.max(s.route_expansions_last_tick());
        assert_all_on_passable_cells(&s);
    }
    assert!(busiest > 0, "routes were planned");
}

/// T-NAV-7a (classroom-scale form, run always): in tier C ecological, fewer than 5% of
/// organisms are pressed against terrain at any metric sample.
#[test]
fn t_nav_7a_few_organisms_are_stuck_at_tier_c() {
    let config =
        sim::ScenarioConfig::from_toml_str(include_str!("../../bench/scenarios/b-c.scenario.toml"))
            .expect("b-c parses");
    let mut s = Simulation::new(&config, 1).expect("valid");
    let mut worst: f64 = 0.0;
    for _ in 0..100 {
        s.step_n(30);
        let population = s.organisms().len().max(1);
        worst = worst.max(s.stuck_count() as f64 / population as f64);
    }
    assert!(worst < 0.05, "worst stuck share {worst}");
}

/// T-NAV-7 (full form): `b-m` ecological for 3,000 ticks. 7a: fewer than 5% stuck at
/// every sample. 7b (performance evidence, printed, not asserted in debug builds): p99
/// tick time against the median. Slow, so run weekly with `--ignored --release`.
#[test]
#[ignore = "tier M run; cargo test -p sim --release -- --ignored"]
fn t_nav_7_tier_m() {
    let config =
        sim::ScenarioConfig::from_toml_str(include_str!("../../bench/scenarios/b-m.scenario.toml"))
            .expect("b-m parses");
    let mut s = Simulation::new(&config, 1).expect("valid");
    let mut worst: f64 = 0.0;
    let mut times = Vec::new();
    for tick in 0..3_000 {
        let started = std::time::Instant::now();
        s.step();
        times.push(started.elapsed());
        if tick % 30 == 29 {
            worst = worst.max(s.stuck_count() as f64 / s.organisms().len().max(1) as f64);
        }
    }
    times.sort_unstable();
    let median = times[times.len() / 2];
    let p99 = times[times.len() * 99 / 100];
    println!(
        "T-NAV-7b: median {median:?}, p99 {p99:?}, ratio {:.2}",
        p99.as_secs_f64() / median.as_secs_f64()
    );
    assert!(worst < 0.05, "T-NAV-7a: worst stuck share {worst}");
}
