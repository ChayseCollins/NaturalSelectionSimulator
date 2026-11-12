//! Shared helpers for `sim` integration tests: constructed scenarios on ASCII maps.

#![allow(dead_code)]

use sim::{Placement, ScenarioConfig, Simulation, Species, StartPopulation, Terrain, Traits};

pub fn traits(speed: f32, size: f32, sense: f32) -> Traits {
    Traits { speed, size, sense }
}

pub fn place(species: Species, x: f32, y: f32, traits: Traits, energy: f32) -> Placement {
    Placement {
        species,
        x,
        y,
        traits,
        energy: Some(energy),
    }
}

/// A quiet base configuration for constructed tests: no starting population, no food,
/// tiny energy costs (organisms do not starve within a test), no mutation, and births only
/// when `allow_births` is set.
pub fn quiet_config(allow_births: bool) -> ScenarioConfig {
    let mut config = ScenarioConfig::default();
    config.species.prey.start = StartPopulation::Count(0);
    config.species.predator.start = StartPopulation::Count(0);
    config.food.initial_fraction = 0.0;
    config.food.regrowth_per_tick = 0.0;
    config.energy.base = 0.001;
    config.energy.size = 0.0;
    config.energy.movement = 0.001;
    config.energy.sense = 0.0;
    config.mutation.probability = 0.0;
    config.population_cap = if allow_births { 1_000 } else { 1 };
    config
}

/// An open grassland map of `columns × rows` cells.
pub fn open_map(columns: usize, rows: usize) -> String {
    format!("{}\n", ".".repeat(columns)).repeat(rows)
}

pub fn build(config: &ScenarioConfig, map: &str, placements: &[Placement]) -> Simulation {
    let terrain = Terrain::from_ascii(map, 2.0).expect("valid map");
    Simulation::from_placements(config, 1, terrain, placements).expect("valid placements")
}

/// Asserts every organism is inside the world on a passable cell (SCOPE 15.2: every
/// navigation test checks this every tick).
pub fn assert_all_on_passable_cells(s: &Simulation) {
    let o = s.organisms();
    for i in 0..o.len() {
        assert!(
            s.terrain().passable_at(o.x()[i], o.y()[i]),
            "tick {}: organism {} at ({}, {}) is not on passable terrain",
            s.tick(),
            o.ids()[i],
            o.x()[i],
            o.y()[i]
        );
    }
}

/// Steps until `done` holds or `limit` ticks pass, checking positions every tick. Returns
/// the tick at which `done` first held.
pub fn run_until(
    s: &mut Simulation,
    limit: u64,
    mut done: impl FnMut(&Simulation) -> bool,
) -> Option<u64> {
    for _ in 0..limit {
        s.step();
        assert_all_on_passable_cells(s);
        if done(s) {
            return Some(s.tick());
        }
    }
    None
}

/// Whether the organism with `id` stands in the food cell containing `(x, y)`.
pub fn in_cell_of(s: &Simulation, id: u64, x: f32, y: f32) -> bool {
    let Some(i) = s.organisms().index_of(id) else {
        return false;
    };
    let food = s.food();
    food.cell_at(s.organisms().x()[i], s.organisms().y()[i]) == food.cell_at(x, y)
}
