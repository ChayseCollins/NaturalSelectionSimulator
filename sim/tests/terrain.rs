//! Terrain tests through the public API (M-06): T-TF-1, T-IMP-1, T-TF-4 and T-DET-3.

mod common;

use common::assert_all_on_passable_cells;
use sim::{ScenarioConfig, Simulation, StartPopulation, TerrainClass};

/// T-TF-1: the same terrain seed gives the same terrain grid, whatever the run seed.
#[test]
fn t_tf_1_same_terrain_seed_same_grid() {
    let mut config = ScenarioConfig::default();
    config.terrain.seed = Some(1001);
    let a = Simulation::new(&config, 1).expect("valid");
    let b = Simulation::new(&config, 2).expect("valid");
    assert_eq!(a.terrain(), b.terrain());
    config.terrain.seed = Some(1002);
    let c = Simulation::new(&config, 1).expect("valid");
    assert_ne!(a.terrain().classes(), c.terrain().classes());
}

/// T-DET-3 (should): the same terrain seed with different run seeds gives equal terrain
/// and different state hashes.
#[test]
fn t_det_3_shared_terrain_different_runs() {
    let mut config = ScenarioConfig::default();
    config.terrain.seed = Some(7);
    let mut a = Simulation::new(&config, 1).expect("valid");
    let mut b = Simulation::new(&config, 2).expect("valid");
    a.step_n(100);
    b.step_n(100);
    assert_eq!(a.terrain(), b.terrain());
    assert_ne!(a.state_hash(), b.state_hash());
}

/// T-IMP-1: 2,000 organisms for 10,000 ticks on generated terrain: no organism, including
/// newborns, ends any tick inside an impassable cell or outside the world.
#[test]
fn t_imp_1_no_organism_ever_ends_a_tick_on_impassable_terrain() {
    // The tier-C benchmark world and the tuned baseline model, at 2,000 organisms.
    let mut config =
        ScenarioConfig::from_toml_str(include_str!("../../bench/scenarios/b-c.scenario.toml"))
            .expect("b-c parses");
    config.species.prey.start = StartPopulation::Count(1_800);
    config.species.predator.start = StartPopulation::Count(200);
    let mut s = Simulation::new(&config, 1).expect("valid");
    assert_eq!(s.organisms().len(), 2_000);
    let impassable = s
        .terrain()
        .classes()
        .iter()
        .filter(|class| !class.passable())
        .count();
    assert!(impassable > 10_000, "the map has real barriers");
    for _ in 0..10_000 {
        s.step();
        assert_all_on_passable_cells(&s);
    }
    assert!(s.counters().prey.births > 0, "newborns were checked too");
}

/// T-TF-4 (should): with no consumers, high-fertility cells hold more food than
/// low-fertility cells after warm-up.
#[test]
fn t_tf_4_fertile_cells_hold_more_food() {
    let mut config = ScenarioConfig::default();
    config.species.prey.start = StartPopulation::Count(0);
    config.species.predator.start = StartPopulation::Count(0);
    config.food.initial_fraction = 0.0;
    let mut s = Simulation::new(&config, 5).expect("valid");
    s.step_n(1_000);
    let mean = |class: TerrainClass| {
        let cells: Vec<f32> = s
            .terrain()
            .classes()
            .iter()
            .zip(s.food().amounts())
            .filter(|&(&c, _)| c == class)
            .map(|(_, &food)| food)
            .collect();
        cells.iter().sum::<f32>() / cells.len().max(1) as f32
    };
    assert!(mean(TerrainClass::Grass) > mean(TerrainClass::Forest));
    assert!(mean(TerrainClass::Forest) > mean(TerrainClass::Hills));
    assert!(mean(TerrainClass::Hills) > mean(TerrainClass::Sand));
    assert_eq!(mean(TerrainClass::Water), 0.0);
    assert_eq!(mean(TerrainClass::Rock), 0.0);
}
