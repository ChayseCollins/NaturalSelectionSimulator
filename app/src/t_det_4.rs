//! T-DET-4: the app's stepping driver, the `headless run` path and direct stepping reach
//! the same state at tick 1,000 for the same scenario and seed. The driver is fed two
//! different frame-time sequences, one hitting the per-frame cap and building a backlog.
//! No window is opened. The binary-level check of `headless` is in
//! `headless/tests/cli.rs`.
//!
//! This is a unit test of the `app` library rather than an integration test in
//! `app/tests/`: integration tests make Cargo also link the Bevy-based `app` binary
//! (about 1 GB with debug info), and two such links at once exhausted memory in WSL2.

use std::time::Duration;

use crate::driver::{DriverLimits, Speed, SteppingDriver};
use headless::cli::RunArgs;
use sim::{ScenarioConfig, Simulation};

const SEED: u64 = 1;
const TICKS: u64 = 1_000;

fn new_sim() -> Simulation {
    Simulation::new(&ScenarioConfig::default(), SEED).expect("valid default scenario")
}

/// Runs the driver at 10× (300 ticks per real second) through `frames`, then drains any
/// backlog with zero-length frames.
fn drive(limits: DriverLimits, frames: &[Duration]) -> Simulation {
    let mut sim = new_sim();
    let mut driver = SteppingDriver::new(limits);
    driver.set_speed(Speed::X10);
    driver.play();
    for &frame in frames {
        driver.frame(&mut sim, frame, || Duration::ZERO);
    }
    while driver.is_behind() {
        driver.frame(&mut sim, Duration::ZERO, || Duration::ZERO);
    }
    sim
}

#[test]
fn t_det_4_driver_headless_and_direct_stepping_agree() {
    // 1,000 ticks at 10× need 3.3333... s of real time; each sequence sums to just over
    // that (by 2 ns), which owes exactly 1,000 whole ticks.
    let mut capped_frames = vec![Duration::from_millis(100); 33];
    capped_frames.push(Duration::from_nanos(33_333_334));
    let capped = drive(
        DriverLimits {
            max_ticks_per_frame: 7,
            ..DriverLimits::default()
        },
        &capped_frames,
    );

    let mut smooth_frames = vec![Duration::from_millis(250); 13];
    smooth_frames.push(Duration::from_nanos(83_333_334));
    let smooth = drive(DriverLimits::default(), &smooth_frames);

    let mut direct = new_sim();
    direct.step_n(TICKS);

    let headless_run = headless::run::execute(&RunArgs {
        seed: SEED,
        ticks: TICKS,
        sample_every: None,
        csv: None,
        overwrite: false,
    })
    .expect("headless run");

    assert_eq!(capped.tick(), TICKS);
    assert_eq!(smooth.tick(), TICKS);
    assert_eq!(headless_run.simulation.tick(), TICKS);
    assert_eq!(capped.state_hash(), direct.state_hash());
    assert_eq!(smooth.state_hash(), direct.state_hash());
    assert_eq!(headless_run.simulation.state_hash(), direct.state_hash());
    assert_eq!(
        headless_run.simulation, direct,
        "identical state, not just hash"
    );
}
