//! Scenario files and presets through the public API (M-15, M-16): T-SCN-2, T-SCN-4,
//! T-DET-5 and T-INH-5. Tests may read the repository's preset files; `sim` itself never
//! reads files (I-08).

use std::fs;
use std::path::Path;

use sim::{ScenarioConfig, Simulation, StartPopulation, TraitId};

fn small(mut config: ScenarioConfig) -> ScenarioConfig {
    config.world.width = 160.0;
    config.world.height = 160.0;
    config
}

/// T-SCN-2: running from an in-memory configuration and from its saved and reloaded file
/// gives equal hashes.
#[test]
fn t_scn_2_saved_and_reloaded_scenarios_run_identically() {
    let mut config = small(ScenarioConfig::default());
    config.name = "Round trip".to_string();
    config.mutation.magnitude = 0.0731;
    config.terrain.seed = Some(99);
    let text = config.to_toml_string().expect("serializes");
    let reloaded = ScenarioConfig::from_toml_str(&text).expect("reloads");
    assert_eq!(reloaded, config);
    let mut a = Simulation::new(&config, 5).expect("valid");
    let mut b = Simulation::new(&reloaded, 5).expect("valid");
    a.step_n(600);
    b.step_n(600);
    assert_eq!(a.state_hash(), b.state_hash());
}

/// T-SCN-4: every preset in `presets/` (embedded by the app at build time) parses and
/// validates, has a name, description and learning goal, and runs.
#[test]
fn t_scn_4_every_preset_parses_validates_and_runs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let mut count = 0;
    for entry in fs::read_dir(&dir).expect("presets folder") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("readable");
        let config = ScenarioConfig::from_toml_str(&text)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!config.name.is_empty(), "{}", path.display());
        assert!(!config.description.is_empty(), "{}", path.display());
        assert!(!config.learning_goal.is_empty(), "{}", path.display());
        let mut s = Simulation::new(&config, config.seed.unwrap_or(1)).expect("valid preset");
        s.step_n(30);
        count += 1;
    }
    assert!(count >= 1, "at least the baseline preset");
}

/// Benchmark scenarios parse and match SCOPE 10.4's tiers.
#[test]
fn benchmark_scenarios_match_their_tiers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bench/scenarios");
    for (file, size, prey, predators, terrain_seed) in [
        ("b-c.scenario.toml", 512.0, 2_000, 200, 1001),
        ("b-m.scenario.toml", 1024.0, 9_000, 1_000, 1002),
        ("b-g.scenario.toml", 2048.0, 45_000, 5_000, 1003),
    ] {
        let text = fs::read_to_string(dir.join(file)).expect("benchmark file");
        let config = ScenarioConfig::from_toml_str(&text).expect(file);
        assert_eq!(
            (config.world.width, config.world.height),
            (size, size),
            "{file}"
        );
        assert_eq!(config.world.cell_size, 2.0);
        assert_eq!(config.world.spatial_cell_size, 16.0);
        assert_eq!(config.terrain.seed, Some(terrain_seed));
        assert_eq!(config.food.abundance, 1.0);
        assert_eq!(
            config.species.prey.start,
            StartPopulation::Count(prey),
            "{file}"
        );
        assert_eq!(
            config.species.predator.start,
            StartPopulation::Count(predators),
            "{file}"
        );
    }
}

/// T-DET-5 (should): run N ticks, reset (rebuild from the same scenario and seed), run N
/// ticks: hashes equal.
#[test]
fn t_det_5_reset_replays_the_same_run() {
    let config = small(ScenarioConfig::default());
    let mut first = Simulation::new(&config, 8).expect("valid");
    first.step_n(500);
    let mut replay = Simulation::new(&config, 8).expect("valid");
    replay.step_n(500);
    assert_eq!(first.state_hash(), replay.state_hash());
}

/// T-INH-5 (should): with starting spread 0 and mutation off, trait means stay exactly at
/// the start values for 10,000 ticks, through births and deaths.
#[test]
fn t_inh_5_neutral_control_keeps_trait_means() {
    let mut config = small(
        ScenarioConfig::from_toml_str(include_str!("../../presets/baseline.scenario.toml"))
            .expect("baseline parses"),
    );
    config.species.prey.start = StartPopulation::Count(150);
    config.species.predator.start = StartPopulation::Count(10);
    config.mutation.probability = 0.0;
    for species in [&mut config.species.prey, &mut config.species.predator] {
        species.speed.spread = 0.0;
        species.size.spread = 0.0;
        species.sense.spread = 0.0;
    }
    let mut s = Simulation::new(&config, 2).expect("valid");
    let starts = [config.species.prey.clone(), config.species.predator.clone()];
    for _ in 0..100 {
        s.step_n(100);
        let o = s.organisms();
        for i in 0..o.len() {
            let start = &starts[o.species()[i].index()];
            for id in TraitId::ALL {
                assert_eq!(o.trait_values(id)[i], start.trait_start(id).mean);
            }
        }
    }
    assert!(s.counters().prey.births > 0, "births happened");
}
