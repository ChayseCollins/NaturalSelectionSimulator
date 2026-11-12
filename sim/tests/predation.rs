//! Constructed mechanism tests (SCOPE 15.3): differential predation (T-PRED-6) and
//! trait-dependent outcomes (T-SEL-1), on open ASCII maps with fixed seeds.

mod common;

use common::*;
use sim::Species;

/// Time limits: the expected duration from distances and speeds, plus a 50% margin
/// (SCOPE 15.2).
const TICKS_PER_SECOND: f32 = 30.0;

fn expected_ticks(distance: f32, closing_speed: f32) -> u64 {
    (distance / closing_speed * TICKS_PER_SECOND * 1.5).ceil() as u64
}

fn captured(s: &sim::Simulation) -> bool {
    s.counters().predations() > 0
}

/// T-PRED-6 (a): a faster predator catches a slower fleeing prey within T ticks.
#[test]
fn t_pred_6_faster_predator_catches_slower_fleeing_prey() {
    let config = quiet_config(false);
    let mut s = build(
        &config,
        &open_map(128, 128),
        &[
            place(Species::Prey, 128.0, 128.0, traits(1.0, 1.0, 20.0), 5.0),
            place(Species::Predator, 116.0, 128.0, traits(2.0, 1.5, 20.0), 7.0),
        ],
    );
    // 12 units at a closing speed of 1 unit per second (the prey flees at full speed).
    let limit = expected_ticks(12.0, 1.0);
    let caught = run_until(&mut s, limit, captured);
    assert!(caught.is_some(), "not caught within {limit} ticks");
}

/// T-PRED-6 (b): a faster prey escapes a slower predator for T ticks.
#[test]
fn t_pred_6_faster_prey_escapes_slower_predator() {
    let config = quiet_config(false);
    let mut s = build(
        &config,
        &open_map(128, 128),
        &[
            place(Species::Prey, 128.0, 128.0, traits(2.0, 1.0, 20.0), 5.0),
            place(Species::Predator, 116.0, 128.0, traits(1.0, 1.5, 20.0), 7.0),
        ],
    );
    let limit = expected_ticks(12.0, 1.0);
    assert_eq!(
        run_until(&mut s, limit, captured),
        None,
        "escaped for {limit} ticks"
    );
}

/// T-PRED-6 (c): a prey below the size-ratio threshold is captured and an otherwise
/// identical prey above it is not.
#[test]
fn t_pred_6_size_ratio_decides_capture() {
    let config = quiet_config(false);
    let run = |prey_size: f32| {
        let mut s = build(
            &config,
            &open_map(64, 64),
            &[
                place(Species::Prey, 64.0, 64.0, traits(0.5, prey_size, 20.0), 5.0),
                place(Species::Predator, 60.0, 64.0, traits(2.0, 1.5, 20.0), 7.0),
            ],
        );
        run_until(&mut s, expected_ticks(4.0, 1.5), captured).is_some()
    };
    // Predator size 1.5 with ratio 1.2 can eat prey up to size 1.25.
    assert!(run(1.2), "below the threshold: captured");
    assert!(!run(1.3), "above the threshold: never captured");
}

/// T-SEL-1 (a): a prey whose sense range reaches a food patch reproduces within T ticks,
/// while an otherwise identical prey whose range does not reach it does not.
#[test]
fn t_sel_1_sense_range_reaching_food_leads_to_reproduction() {
    let mut config = quiet_config(true);
    config.behavior.wander_speed_fraction = 0.0;
    let run = |sense: f32| {
        let mut s = build(
            &config,
            &open_map(64, 64),
            &[place(
                Species::Prey,
                30.0,
                64.0,
                traits(1.0, 1.0, sense),
                5.0,
            )],
        );
        // A 3 × 3 patch of full cells centered 15 units away.
        for dx in [-2.0, 0.0, 2.0] {
            for dy in [-2.0, 0.0, 2.0] {
                s.set_cell_food(45.0 + dx, 64.0 + dy, 1.0);
            }
        }
        // Walk 15 units at 1 unit/s, then eat 3 energy at 0.05 per tick, plus margin.
        let limit = ((15.0 * TICKS_PER_SECOND + 3.0 / 0.05) * 1.5) as u64;
        run_until(&mut s, limit, |s| s.counters().prey.births > 0)
    };
    assert!(run(20.0).is_some(), "the patch is in range: reproduces");
    assert_eq!(
        run(10.0),
        None,
        "the patch is out of range: no reproduction"
    );
}

/// T-SEL-1 (b): on uniform low food, a prey with higher speed and size starves before an
/// otherwise identical prey with lower speed and size.
#[test]
fn t_sel_1_costly_traits_starve_first_on_low_food() {
    let mut config = quiet_config(false);
    config.food.abundance = 0.001;
    config.food.initial_fraction = 1.0;
    config.energy = sim::EnergyConfig::default();
    let run = |speed: f32, size: f32| {
        let mut s = build(
            &config,
            &open_map(64, 64),
            &[place(
                Species::Prey,
                64.0,
                64.0,
                traits(speed, size, 10.0),
                5.0,
            )],
        );
        run_until(&mut s, 1_000_000, |s| s.organisms().is_empty()).expect("starves eventually")
    };
    let costly = run(2.0, 2.0);
    let cheap = run(1.0, 1.0);
    assert!(
        costly < cheap,
        "costly traits starve at {costly}, cheap at {cheap}"
    );
}
