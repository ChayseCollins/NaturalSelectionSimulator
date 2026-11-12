//! The running simulation behind the window (M-11, M-14).
//!
//! A [`Session`] owns the one authoritative `sim::Simulation`, the configuration it was
//! built from, its run seed, and the [`SteppingDriver`]. Frames, play, pause, step and
//! reset all go through the driver, so there is no second timing path.
//!
//! Model parameters are edited in a *draft* configuration and draft seed. They apply only
//! on Reset (or New seed, or loading a scenario), never to a running simulation (SCOPE
//! 7.3: "Model parameters apply on Start or Reset").
//!
//! The visuals are a projection of this state: the session only reports when they need
//! refreshing ([`SyncRequest`]) and never takes anything back from them.

use std::time::Duration;

use sim::{ScenarioConfig, Simulation, ValidationError};

use crate::driver::{DriverLimits, FrameReport, Speed, SteppingDriver};

/// What the visuals must do to catch up with the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRequest {
    /// Nothing changed since the last sync.
    None,
    /// Ticks ran: update organism sprites and, on the overlay's schedule, food colors.
    Update,
    /// The simulation was replaced (start, reset, new scenario): rebuild every visual,
    /// including the terrain texture.
    Rebuild,
}

#[derive(Debug, Clone)]
pub struct Session {
    applied: ScenarioConfig,
    /// Edited parameters; applied on the next reset.
    pub draft: ScenarioConfig,
    run_seed: u64,
    /// Edited run seed; applied on the next reset.
    pub draft_seed: u64,
    simulation: Simulation,
    driver: SteppingDriver,
    initial_hash: u64,
    pending_sync: SyncRequest,
    /// `(tick, hash)` of the last hash computed, so the full-state hash runs at most once
    /// per tick however often the UI asks for it.
    hash_cache: Option<(u64, u64)>,
    /// Where the running scenario came from (a preset name or a file path).
    source: String,
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
            draft: config.clone(),
            applied: config,
            run_seed,
            draft_seed: run_seed,
            simulation,
            driver: SteppingDriver::new(limits),
            initial_hash,
            pending_sync: SyncRequest::Rebuild,
            hash_cache: Some((0, initial_hash)),
            source: String::new(),
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

    pub fn toggle_play(&mut self) {
        if self.driver.is_paused() {
            self.play();
        } else {
            self.pause();
        }
    }

    /// Runs exactly one tick and pauses (T-FT-3).
    pub fn step_once(&mut self) {
        self.driver.single_step(&mut self.simulation);
        self.request(SyncRequest::Update);
    }

    /// Changes the requested speed; the driver clears its backlog.
    pub fn set_speed(&mut self, speed: Speed) {
        self.driver.set_speed(speed);
    }

    /// Rebuilds the simulation at tick 0 from the draft configuration and draft seed,
    /// which become the applied ones, and clears the driver's backlog. The driver keeps
    /// its paused or running state. An invalid draft changes nothing and returns its
    /// errors.
    pub fn reset(&mut self) -> Result<(), Vec<ValidationError>> {
        let simulation = Simulation::new(&self.draft, self.draft_seed)?;
        self.applied = self.draft.clone();
        self.run_seed = self.draft_seed;
        self.initial_hash = simulation.state_hash();
        self.hash_cache = Some((0, self.initial_hash));
        self.simulation = simulation;
        self.driver.reset();
        self.request(SyncRequest::Rebuild);
        Ok(())
    }

    /// Resets with a new run seed (and any draft changes).
    pub fn new_seed(&mut self, seed: u64) -> Result<(), Vec<ValidationError>> {
        let previous = self.draft_seed;
        self.draft_seed = seed;
        let result = self.reset();
        if result.is_err() {
            self.draft_seed = previous;
        }
        result
    }

    /// Replaces the scenario (preset or loaded file) and resets to tick 0, paused. The
    /// current simulation is untouched if `config` is invalid.
    pub fn load(
        &mut self,
        config: ScenarioConfig,
        seed: u64,
        source: impl Into<String>,
    ) -> Result<(), Vec<ValidationError>> {
        let (old_draft, old_seed) = (self.draft.clone(), self.draft_seed);
        self.draft = config;
        self.draft_seed = seed;
        match self.reset() {
            Ok(()) => {
                self.source = source.into();
                self.pause();
                Ok(())
            }
            Err(errors) => {
                self.draft = old_draft;
                self.draft_seed = old_seed;
                Err(errors)
            }
        }
    }

    /// Whether the draft differs from what is running.
    pub fn has_pending_changes(&self) -> bool {
        self.draft != self.applied || self.draft_seed != self.run_seed
    }

    /// Validation problems of the draft (empty when it can be applied).
    pub fn draft_errors(&self) -> Vec<ValidationError> {
        self.draft.validate().err().unwrap_or_default()
    }

    /// Discards draft edits.
    pub fn revert_draft(&mut self) {
        self.draft = self.applied.clone();
        self.draft_seed = self.run_seed;
    }

    /// The state hash at the current tick, computed at most once per tick.
    pub fn state_hash(&mut self) -> u64 {
        let tick = self.simulation.tick();
        match self.hash_cache {
            Some((cached_tick, hash)) if cached_tick == tick => hash,
            _ => {
                let hash = self.simulation.state_hash();
                self.hash_cache = Some((tick, hash));
                hash
            }
        }
    }

    /// Returns and clears the pending visual update.
    pub fn take_sync(&mut self) -> SyncRequest {
        std::mem::replace(&mut self.pending_sync, SyncRequest::None)
    }

    /// Forces a full visual rebuild (for example after a display option changed).
    pub fn request_rebuild(&mut self) {
        self.pending_sync = SyncRequest::Rebuild;
    }

    pub fn simulation(&self) -> &Simulation {
        &self.simulation
    }

    pub fn driver(&self) -> &SteppingDriver {
        &self.driver
    }

    /// The configuration the running simulation was built from.
    pub fn applied(&self) -> &ScenarioConfig {
        &self.applied
    }

    pub fn run_seed(&self) -> u64 {
        self.run_seed
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn set_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
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
    use sim::StartPopulation;

    use crate::camera::CameraView;
    use crate::presets;

    use super::*;

    /// A small scenario so these tests stay fast: the baseline model on a 96 × 96 world.
    fn small() -> ScenarioConfig {
        let mut config = presets::baseline().config;
        config.world.width = 96.0;
        config.world.height = 96.0;
        config.species.prey.start = StartPopulation::Count(100);
        config.species.predator.start = StartPopulation::Count(8);
        config
    }

    fn session() -> Session {
        Session::new(small(), 1, DriverLimits::default()).expect("valid scenario")
    }

    fn no_work() -> Duration {
        Duration::ZERO
    }

    /// One thirtieth of a second, rounded up so 30 of them owe at least 30 ticks at 1×.
    const THIRTIETH: Duration = Duration::from_nanos(33_333_334);

    #[test]
    fn startup_builds_the_baseline_paused_at_tick_zero() {
        let baseline = presets::baseline();
        let seed = baseline.config.seed.unwrap_or(1);
        let s = Session::new(baseline.config.clone(), seed, DriverLimits::default())
            .expect("the baseline preset is valid");
        assert_eq!(s.simulation().tick(), 0);
        assert!(s.driver().is_paused());
        assert_eq!(s.driver().speed(), Speed::X1);
        let direct = Simulation::new(&baseline.config, seed).expect("valid");
        assert_eq!(s.initial_hash(), direct.state_hash());
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
    fn step_runs_exactly_one_tick_and_pauses() {
        let mut s = session();
        s.play();
        s.step_once();
        assert_eq!(s.simulation().tick(), 1);
        assert!(s.driver().is_paused());
        s.step_once();
        assert_eq!(s.simulation().tick(), 2);
    }

    #[test]
    fn pause_clears_backlog_and_resume_has_no_catch_up_burst() {
        let limits = DriverLimits {
            max_ticks_per_frame: 2,
            ..DriverLimits::default()
        };
        let mut s = Session::new(small(), 1, limits).expect("valid");
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

    /// M-11: Reset returns to the tick-0 hash and keeps the run state.
    #[test]
    fn reset_returns_to_the_tick_zero_state_and_keeps_run_state() {
        let mut s = session();
        let initial = s.simulation().clone();
        s.play();
        for _ in 0..90 {
            s.frame(THIRTIETH, no_work);
        }
        assert_eq!(s.simulation().tick(), 90);
        assert_ne!(s.state_hash(), s.initial_hash());
        s.take_sync();
        s.reset().expect("same valid scenario");
        assert_eq!(s.simulation().tick(), 0);
        assert_eq!(s.state_hash(), s.initial_hash());
        assert_eq!(s.simulation(), &initial, "identical tick-0 state");
        assert_eq!(s.driver().backlog_ticks(), 0);
        assert!(!s.driver().is_paused(), "reset keeps the running state");
        assert_eq!(s.take_sync(), SyncRequest::Rebuild);
    }

    /// M-14: edits apply only on reset; an invalid draft changes nothing.
    #[test]
    fn draft_edits_apply_only_on_reset() {
        let mut s = session();
        s.play();
        for _ in 0..30 {
            s.frame(THIRTIETH, no_work);
        }
        s.draft.food.abundance = 0.5;
        assert!(s.has_pending_changes());
        assert_eq!(
            s.simulation().config().food.abundance,
            1.0,
            "not applied while running"
        );
        s.reset().expect("valid draft");
        assert_eq!(s.simulation().config().food.abundance, 0.5);
        assert!(!s.has_pending_changes());

        let before = s.simulation().clone();
        s.draft.mutation.probability = 7.0;
        assert_eq!(s.draft_errors()[0].field(), "mutation.probability");
        assert!(s.reset().is_err());
        assert_eq!(
            s.simulation(),
            &before,
            "an invalid draft never changes the run"
        );
        s.revert_draft();
        assert!(!s.has_pending_changes());
    }

    #[test]
    fn new_seed_and_load_replace_the_run() {
        let mut s = session();
        let first = s.initial_hash();
        s.new_seed(99).expect("valid");
        assert_eq!(s.run_seed(), 99);
        assert_ne!(s.initial_hash(), first);
        let mut other = small();
        other.name = "Other".to_string();
        s.play();
        s.load(other.clone(), 5, "file.toml").expect("valid");
        assert_eq!(s.applied().name, "Other");
        assert_eq!(s.source(), "file.toml");
        assert!(s.driver().is_paused(), "a loaded scenario opens paused");
        let mut invalid = other;
        invalid.world.width = -1.0;
        let hash = s.state_hash();
        assert!(s.load(invalid, 5, "bad.toml").is_err());
        assert_eq!(
            s.state_hash(),
            hash,
            "an invalid file never changes the simulation"
        );
        assert_eq!(s.source(), "file.toml");
    }

    /// The session runs exactly the ticks the driver selects and ends in the same state
    /// as stepping directly.
    #[test]
    fn session_frames_match_direct_stepping() {
        let mut s = session();
        s.set_speed(Speed::X5);
        s.play();
        let mut total = 0;
        for frame in [100, 7, 250, 33, 1, 16].map(Duration::from_millis) {
            total += s.frame(frame, no_work).ticks_run;
        }
        assert_eq!(s.simulation().tick(), total);
        let mut direct = Simulation::new(&small(), 1).expect("valid");
        direct.step_n(total);
        assert_eq!(s.simulation(), &direct);
    }

    /// T-CAM-3 (automated part): camera changes leave the simulation, its random-domain
    /// positions (compared through `==`) and its hash untouched.
    #[test]
    fn camera_changes_do_not_touch_the_simulation() {
        let mut s = session();
        s.play();
        for _ in 0..10 {
            s.frame(THIRTIETH, no_work);
        }
        s.pause();
        let before = s.simulation().clone();
        let (width, height) = (96.0, 96.0);
        let mut view = CameraView::fit(width, height, 1280.0, 720.0);
        for step in 0..50 {
            view.drag(13.0, -7.0, width, height);
            view.zoom_at(if step % 2 == 0 { 3.0 } else { -2.0 }, (40.0, 30.0));
            s.frame(THIRTIETH, no_work);
        }
        assert_eq!(s.simulation(), &before);
        assert_eq!(s.state_hash(), before.state_hash());
    }
}
