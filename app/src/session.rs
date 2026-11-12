//! The running simulation behind the Week 1 window (M-10, M-11).
//!
//! A [`Session`] owns the one authoritative `sim::Simulation`, its run seed and the
//! existing [`SteppingDriver`]. Frames, play, pause and reset all go through the driver,
//! so there is no second timing path. The visuals are a projection of this state: the
//! session only reports when they need refreshing ([`SyncRequest`]) and never takes
//! anything back from them.

use std::time::Duration;

use sim::{ScenarioConfig, Simulation, ValidationError};

use crate::driver::{DriverLimits, FrameReport, SteppingDriver};

/// Fixed provisional run seed of the Week 1 graphical application.
pub const RUN_SEED: u64 = 1;

/// Week 1 graphical demonstration: starting food as a fraction of each cell's cap
/// (shared default 1.0) (P).
pub const DEMO_FOOD_INITIAL_FRACTION: f32 = 0.25;
/// Week 1 graphical demonstration: food regrowth per tick (shared default 0.002) (P).
pub const DEMO_FOOD_REGROWTH_PER_TICK: f32 = 0.0005;

/// Scenario shown by the Week 1 graphical application: `ScenarioConfig::default()` with
/// two app-only demonstration overrides. `headless run` and the shared default are
/// unchanged.
///
/// Why: under the shared default a sated prey's cell regrows (0.002 per tick) faster than
/// the prey eats while standing (about 0.0013 per tick for a size-1 prey), so the cell
/// stays at its cap, remains a worthwhile target, and nearly every prey sits still from
/// about 15 s on. Regrowth of 0.0005 per tick is below that standing demand, so occupied
/// cells slowly deplete and prey move on. Starting food of 0.25 of the cap makes prey
/// graze across several cells to fill up, so the first half-minute shows travel and
/// depletion. Neither override is tuned toward an evolutionary outcome. Provisional,
/// recorded in README.md.
pub fn app_scenario() -> ScenarioConfig {
    let mut config = ScenarioConfig::default();
    config.food.initial_fraction = DEMO_FOOD_INITIAL_FRACTION;
    config.food.regrowth_per_tick = DEMO_FOOD_REGROWTH_PER_TICK;
    config
}

/// What the visuals must do to catch up with the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRequest {
    /// Nothing changed since the last sync.
    None,
    /// Ticks ran: update food colors and prey positions; remove visuals of dead prey.
    Update,
    /// The simulation was replaced (start or reset): rebuild every visual.
    Rebuild,
}

#[derive(Debug, Clone)]
pub struct Session {
    config: ScenarioConfig,
    run_seed: u64,
    simulation: Simulation,
    driver: SteppingDriver,
    initial_hash: u64,
    pending_sync: SyncRequest,
}

impl Session {
    /// Builds the run at tick 0 with a paused driver. Visuals start with a rebuild.
    pub fn new(
        config: ScenarioConfig,
        run_seed: u64,
        limits: DriverLimits,
    ) -> Result<Self, Vec<ValidationError>> {
        let simulation = Simulation::new(&config, run_seed)?;
        let initial_hash = simulation.state_hash();
        Ok(Self {
            config,
            run_seed,
            simulation,
            driver: SteppingDriver::new(limits),
            initial_hash,
            pending_sync: SyncRequest::Rebuild,
        })
    }

    /// One rendered frame: the driver decides how many whole ticks to run from the frame's
    /// real elapsed time. `work_elapsed` reports simulation work done so far this frame.
    pub fn frame(
        &mut self,
        frame_delta: Duration,
        work_elapsed: impl FnMut() -> Duration,
    ) -> FrameReport {
        let report = self
            .driver
            .frame(&mut self.simulation, frame_delta, work_elapsed);
        if report.ticks_run > 0 {
            self.request(SyncRequest::Update);
        }
        report
    }

    pub fn play(&mut self) {
        self.driver.play();
    }

    pub fn pause(&mut self) {
        self.driver.pause();
    }

    /// Rebuilds the simulation from the same scenario and run seed, back to tick 0, and
    /// clears the driver's backlog. The driver keeps its paused or running state (M-02
    /// reset semantics).
    pub fn reset(&mut self) -> Result<(), Vec<ValidationError>> {
        self.simulation = Simulation::new(&self.config, self.run_seed)?;
        self.driver.reset();
        self.request(SyncRequest::Rebuild);
        Ok(())
    }

    /// Returns and clears the pending visual update.
    pub fn take_sync(&mut self) -> SyncRequest {
        std::mem::replace(&mut self.pending_sync, SyncRequest::None)
    }

    pub fn simulation(&self) -> &Simulation {
        &self.simulation
    }

    pub fn driver(&self) -> &SteppingDriver {
        &self.driver
    }

    pub fn run_seed(&self) -> u64 {
        self.run_seed
    }

    /// State hash of this run at tick 0; a reset returns to it.
    pub fn initial_hash(&self) -> u64 {
        self.initial_hash
    }

    fn request(&mut self, request: SyncRequest) {
        if self.pending_sync != SyncRequest::Rebuild {
            self.pending_sync = request;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::camera::CameraView;
    use crate::driver::Speed;

    use super::*;

    /// A small scenario so these tests stay fast: 100 prey on a 64 × 64 world.
    fn small() -> ScenarioConfig {
        let mut config = ScenarioConfig::default();
        config.world.width = 64.0;
        config.world.height = 64.0;
        config.prey.start_count = 100;
        config
    }

    fn session() -> Session {
        Session::new(small(), RUN_SEED, DriverLimits::default()).expect("valid scenario")
    }

    fn no_work() -> Duration {
        Duration::ZERO
    }

    /// One thirtieth of a second, rounded up so 30 of them owe at least 30 ticks at 1×.
    const THIRTIETH: Duration = Duration::from_nanos(33_333_334);

    #[test]
    fn startup_builds_the_demo_scenario_paused_at_tick_zero() {
        let s = Session::new(app_scenario(), RUN_SEED, DriverLimits::default())
            .expect("demo scenario is valid");
        assert_eq!(s.run_seed(), 1);
        assert_eq!(s.simulation().run_seed(), 1);
        assert_eq!(s.simulation().tick(), 0);
        assert!(s.driver().is_paused());
        assert_eq!(s.driver().speed(), Speed::X1);
        let direct = Simulation::new(&app_scenario(), 1).expect("valid");
        assert_eq!(s.initial_hash(), direct.state_hash());
        assert_eq!(s.simulation().state_hash(), s.initial_hash());
    }

    /// The demo differs from the shared default in exactly the two recorded food fields,
    /// and the shared default (used by `headless run`) keeps its own values.
    #[test]
    fn demo_scenario_overrides_only_the_recorded_fields() {
        let demo = app_scenario();
        assert!(demo.validate().is_ok());
        assert_eq!(demo.food.initial_fraction, DEMO_FOOD_INITIAL_FRACTION);
        assert_eq!(demo.food.regrowth_per_tick, DEMO_FOOD_REGROWTH_PER_TICK);
        let mut restored = demo.clone();
        let default = ScenarioConfig::default();
        restored.food.initial_fraction = default.food.initial_fraction;
        restored.food.regrowth_per_tick = default.food.regrowth_per_tick;
        assert_eq!(restored, default, "no other overrides");
        assert_eq!(
            default.food.initial_fraction,
            sim::limits::FOOD_INITIAL_FRACTION.default
        );
        assert_eq!(
            default.food.regrowth_per_tick,
            sim::limits::FOOD_REGROWTH_PER_TICK.default
        );
        assert_eq!(
            default.prey.start_count,
            sim::limits::PREY_START_COUNT_DEFAULT
        );
    }

    /// Reset of the real demo session rebuilds exactly the demo's tick-zero state and hash.
    #[test]
    fn reset_reproduces_the_demo_tick_zero_state_and_hash() {
        let mut s = Session::new(app_scenario(), RUN_SEED, DriverLimits::default()).expect("valid");
        let initial = s.simulation().clone();
        let fresh = Simulation::new(&app_scenario(), RUN_SEED).expect("valid");
        assert_eq!(s.initial_hash(), fresh.state_hash());
        s.play();
        for _ in 0..45 {
            s.frame(THIRTIETH, no_work);
        }
        assert_ne!(s.simulation().state_hash(), s.initial_hash());
        s.reset().expect("valid");
        assert_eq!(s.simulation().tick(), 0);
        assert_eq!(s.simulation(), &initial, "identical tick-0 state");
        assert_eq!(s.simulation().state_hash(), s.initial_hash());
        assert_eq!(s.simulation().state_hash(), fresh.state_hash());
    }

    #[test]
    fn paused_frames_run_no_ticks_and_play_runs_them() {
        let mut s = session();
        assert_eq!(
            s.take_sync(),
            SyncRequest::Rebuild,
            "first frame builds visuals"
        );
        for _ in 0..30 {
            assert_eq!(s.frame(THIRTIETH, no_work).ticks_run, 0);
        }
        assert_eq!(s.simulation().tick(), 0);
        assert_eq!(
            s.take_sync(),
            SyncRequest::None,
            "nothing changed while paused"
        );

        s.play();
        for _ in 0..30 {
            assert_eq!(s.frame(THIRTIETH, no_work).ticks_run, 1);
        }
        assert_eq!(s.simulation().tick(), 30);
        assert_eq!(s.take_sync(), SyncRequest::Update);
    }

    #[test]
    fn pause_clears_backlog_and_resume_has_no_catch_up_burst() {
        let limits = DriverLimits {
            max_ticks_per_frame: 2,
            ..DriverLimits::default()
        };
        let mut s = Session::new(small(), RUN_SEED, limits).expect("valid");
        s.play();
        s.frame(Duration::from_millis(200), no_work); // owes 6 ticks, runs 2
        assert!(s.driver().backlog_ticks() > 0);
        s.pause();
        assert_eq!(s.driver().backlog_ticks(), 0);
        let paused_at = s.simulation().tick();
        for _ in 0..100 {
            s.frame(Duration::from_millis(100), no_work);
        }
        assert_eq!(
            s.simulation().tick(),
            paused_at,
            "a long pause accrues nothing"
        );
        s.play();
        assert_eq!(
            s.frame(THIRTIETH, no_work).ticks_run,
            1,
            "one frame's worth only"
        );
    }

    #[test]
    fn reset_returns_to_the_tick_zero_state_and_keeps_run_state() {
        let mut s = session();
        let initial = s.simulation().clone();
        s.play();
        for _ in 0..90 {
            s.frame(THIRTIETH, no_work);
        }
        assert_eq!(s.simulation().tick(), 90);
        assert_ne!(s.simulation().state_hash(), s.initial_hash());
        s.take_sync();

        s.reset().expect("same valid scenario");
        assert_eq!(s.simulation().tick(), 0);
        assert_eq!(s.simulation().state_hash(), s.initial_hash());
        assert_eq!(s.simulation(), &initial, "identical tick-0 state");
        assert_eq!(s.driver().backlog_ticks(), 0);
        assert!(!s.driver().is_paused(), "reset keeps the running state");
        assert_eq!(s.take_sync(), SyncRequest::Rebuild);

        s.pause();
        s.reset().expect("valid");
        assert!(s.driver().is_paused(), "reset keeps the paused state");
    }

    #[test]
    fn reset_clears_a_pending_backlog() {
        let limits = DriverLimits {
            max_ticks_per_frame: 2,
            ..DriverLimits::default()
        };
        let mut s = Session::new(small(), RUN_SEED, limits).expect("valid");
        s.play();
        s.frame(Duration::from_millis(200), no_work);
        assert!(s.driver().backlog_ticks() > 0);
        s.reset().expect("valid");
        assert_eq!(s.driver().backlog_ticks(), 0);
    }

    /// The session runs exactly the ticks the driver selects, through the one driver, and
    /// ends in the same state as stepping directly.
    #[test]
    fn session_frames_match_direct_stepping() {
        let mut s = session();
        s.play();
        let mut total = 0;
        for frame in [100, 7, 250, 33, 1, 16].map(Duration::from_millis) {
            total += s.frame(frame, no_work).ticks_run;
        }
        assert_eq!(s.simulation().tick(), total);
        let mut direct = Simulation::new(&small(), RUN_SEED).expect("valid");
        direct.step_n(total);
        assert_eq!(s.simulation(), &direct);
        assert_eq!(s.simulation().state_hash(), direct.state_hash());
    }

    /// Camera changes are separate state: panning and zooming leave the simulation, its
    /// random-domain positions (compared through `==`) and its hash untouched.
    #[test]
    fn camera_changes_do_not_touch_the_simulation() {
        let mut s = session();
        s.play();
        for _ in 0..10 {
            s.frame(THIRTIETH, no_work);
        }
        s.pause();
        let before = s.simulation().clone();
        let (width, height) = {
            let world = &s.simulation().config().world;
            (world.width, world.height)
        };
        let mut view = CameraView::fit(width, height, 1280.0, 720.0);
        for step in 0..50 {
            view.drag(13.0, -7.0, width, height);
            view.zoom_steps(if step % 2 == 0 { 3.0 } else { -2.0 });
            s.frame(THIRTIETH, no_work);
        }
        assert_eq!(s.simulation(), &before);
        assert_eq!(s.simulation().tick(), before.tick());
        assert_eq!(s.simulation().state_hash(), before.state_hash());
    }
}
