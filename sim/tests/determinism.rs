//! Determinism tests through the public `sim` API (SCOPE 15.2).
//!
//! These run on the current state hash (tick, seeds, prey columns including behavior
//! state, starvation count, food grid and generator positions) and are rerun unchanged as
//! the hash grows (SCOPE 6.10, Week 2).

use sim::{ScenarioConfig, Simulation};

/// Small fixed scenario for the long T-DET-1 run, so the must-priority suite stays fast
/// in debug builds: 300 prey seeking food on a 128 × 128 world.
fn small_scenario() -> ScenarioConfig {
    let mut config = ScenarioConfig::default();
    config.world.width = 128.0;
    config.world.height = 128.0;
    config.prey.start_count = 300;
    config
}

fn run(config: &ScenarioConfig, run_seed: u64) -> Simulation {
    Simulation::new(config, run_seed).expect("valid configuration")
}

/// T-DET-1: the same scenario and seed, run twice in-process, give equal state hashes at
/// ticks 100, 1,000 and 10,000, with identical organisms and food.
#[test]
fn t_det_1_same_seed_gives_equal_hashes() {
    let config = small_scenario();
    let mut a = run(&config, 1);
    let mut b = run(&config, 1);
    for checkpoint in [100, 1_000, 10_000] {
        a.step_n(checkpoint - a.tick());
        b.step_n(checkpoint - b.tick());
        assert_eq!(a.tick(), checkpoint);
        assert_eq!(b.tick(), checkpoint);
        assert_eq!(a.organisms(), b.organisms(), "tick {checkpoint}");
        assert_eq!(a.food(), b.food(), "tick {checkpoint}");
        assert_eq!(a.starvation_deaths(), b.starvation_deaths());
        assert_eq!(a.state_hash(), b.state_hash(), "tick {checkpoint}");
    }
    assert!(
        !a.organisms().is_empty(),
        "prey with food survive to tick 10,000"
    );
}

/// T-DET-2: run seeds 1 and 2 of the default scenario give different hashes at tick
/// 1,000, because their organism and food state differ, not only their stored seeds.
#[test]
fn t_det_2_different_seeds_give_different_hashes() {
    let config = ScenarioConfig::default();
    let mut seed_1 = run(&config, 1);
    let mut seed_2 = run(&config, 2);
    seed_1.step_n(1_000);
    seed_2.step_n(1_000);
    assert_ne!(seed_1.organisms().x(), seed_2.organisms().x());
    assert_ne!(seed_1.organisms().energy(), seed_2.organisms().energy());
    assert_ne!(seed_1.food().amounts(), seed_2.food().amounts());
    assert_ne!(seed_1.state_hash(), seed_2.state_hash());
}
