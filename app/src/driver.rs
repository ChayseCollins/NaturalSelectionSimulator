//! Fixed-timestep stepping driver (M-02, SCOPE 7.3).
//!
//! The driver decides only *when* whole ticks run, never *which* ticks run or what they
//! do, so the state at tick N is identical whatever the frame rate (T-DET-4). It reads no
//! clock. The caller passes each frame's real elapsed time, plus a closure reporting how
//! much simulation work has been done so far in the current frame. In the app that closure
//! is `|| work_start.elapsed()` with `work_start` taken just before calling
//! [`SteppingDriver::frame`]; in tests it is a deterministic fake.
//!
//! Semantics:
//! - **Fixed multiplier:** each frame adds `elapsed × ticks_per_second × multiplier` ticks
//!   of owed time to an integer accumulator, then runs whole ticks while at least one is
//!   owed. A tick is never partial and never skipped.
//! - **Per-frame cap:** between ticks, processing stops once the frame has run
//!   `max_ticks_per_frame` ticks or `max_work_per_frame` of work. Unprocessed time stays in
//!   the accumulator for later frames; it is never discarded because of the cap.
//! - **Stall cap:** a single frame contributes at most `stall_cap` (250 ms). This only
//!   affects abnormal stalls (debugger, sleep, window drag); each use is counted.
//! - **Explicit actions clear the backlog:** pause, play, single step, reset and speed
//!   changes set owed time to zero. While paused no time accrues, so resuming never
//!   causes a catch-up burst.
//! - **Max mode:** no accumulator; each frame runs whole ticks while work done in the
//!   frame is below `max_mode_work_per_frame`.

use std::collections::VecDeque;
use std::time::Duration;

use sim::Simulation;

/// Accumulator units per owed tick. The accumulator counts
/// `real nanoseconds × ticks_per_second × multiplier`, so integer frame deltas summing to
/// exactly one second at 1× owe exactly `ticks_per_second` ticks, with no float drift.
const UNITS_PER_TICK: u128 = 1_000_000_000;

/// Requested simulation speed (SCOPE M-11; the list may change after testing, Q5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    X1,
    X2,
    X5,
    X10,
    /// As many whole ticks per frame as fit in the Max-mode work budget.
    Max,
}

impl Speed {
    pub const ALL: [Speed; 5] = [Speed::X1, Speed::X2, Speed::X5, Speed::X10, Speed::Max];

    /// Real-time multiplier, or `None` for Max mode.
    pub fn multiplier(self) -> Option<u32> {
        match self {
            Speed::X1 => Some(1),
            Speed::X2 => Some(2),
            Speed::X5 => Some(5),
            Speed::X10 => Some(10),
            Speed::Max => None,
        }
    }
}

/// Per-frame limits. Defaults are the provisional values of SCOPE 7.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverLimits {
    /// Most ticks run in one frame at a fixed multiplier (600, P).
    pub max_ticks_per_frame: u64,
    /// Most simulation work in one frame at a fixed multiplier (25 ms, P).
    pub max_work_per_frame: Duration,
    /// Simulation work budget per frame in Max mode (12 ms, P).
    pub max_mode_work_per_frame: Duration,
    /// Largest real time one frame may contribute (250 ms, P).
    pub stall_cap: Duration,
    /// Real-time window over which achieved speed is averaged for the status bar (1 s, P).
    /// Not specified by SCOPE; the 60-second overlay statistics are M-18.
    pub achieved_speed_window: Duration,
}

impl Default for DriverLimits {
    fn default() -> Self {
        Self {
            max_ticks_per_frame: 600,
            max_work_per_frame: Duration::from_millis(25),
            max_mode_work_per_frame: Duration::from_millis(12),
            stall_cap: Duration::from_millis(250),
            achieved_speed_window: Duration::from_secs(1),
        }
    }
}

/// What one call to [`SteppingDriver::frame`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameReport {
    /// Whole ticks run this frame.
    pub ticks_run: u64,
    /// The per-frame tick or work cap stopped processing while ticks were still owed.
    pub limited: bool,
    /// The frame delta exceeded the stall cap and was trimmed to it.
    pub stall_capped: bool,
}

/// Decides how many whole simulation ticks to run each rendered frame.
#[derive(Debug, Clone)]
pub struct SteppingDriver {
    limits: DriverLimits,
    speed: Speed,
    paused: bool,
    /// Owed simulation time in accumulator units (see [`UNITS_PER_TICK`]).
    owed: u128,
    /// Tick rate seen on the most recent unpaused frame, for time-based readouts.
    ticks_per_second: u32,
    stall_count: u64,
    /// Recent unpaused frames as `(real nanoseconds, ticks run)`, for achieved speed.
    window: VecDeque<(u128, u64)>,
    window_nanos: u128,
    window_ticks: u64,
}

impl SteppingDriver {
    /// A driver that starts paused at 1× (the app opens paused, SCOPE 8).
    pub fn new(limits: DriverLimits) -> Self {
        Self {
            limits,
            speed: Speed::X1,
            paused: true,
            owed: 0,
            ticks_per_second: 0,
            stall_count: 0,
            window: VecDeque::new(),
            window_nanos: 0,
            window_ticks: 0,
        }
    }

    /// Runs this frame's whole ticks on `sim`. `frame_delta` is the real time since the
    /// previous frame; `work_elapsed` reports simulation work done so far this frame and is
    /// checked before each tick.
    pub fn frame(
        &mut self,
        sim: &mut Simulation,
        frame_delta: Duration,
        mut work_elapsed: impl FnMut() -> Duration,
    ) -> FrameReport {
        if self.paused {
            return FrameReport::default();
        }
        self.ticks_per_second = sim.ticks_per_second();

        let stall_capped = frame_delta > self.limits.stall_cap;
        let delta = if stall_capped {
            self.stall_count += 1;
            self.limits.stall_cap
        } else {
            frame_delta
        };

        let mut ticks_run = 0;
        let limited = match self.speed.multiplier() {
            Some(multiplier) => {
                self.owed +=
                    delta.as_nanos() * u128::from(self.ticks_per_second) * u128::from(multiplier);
                while self.owed >= UNITS_PER_TICK
                    && ticks_run < self.limits.max_ticks_per_frame
                    && work_elapsed() < self.limits.max_work_per_frame
                {
                    sim.step();
                    self.owed -= UNITS_PER_TICK;
                    ticks_run += 1;
                }
                self.owed >= UNITS_PER_TICK
            }
            None => {
                self.owed = 0;
                while work_elapsed() < self.limits.max_mode_work_per_frame {
                    sim.step();
                    ticks_run += 1;
                }
                false
            }
        };

        self.record_frame(delta, ticks_run);
        FrameReport {
            ticks_run,
            limited,
            stall_capped,
        }
    }

    /// Stops stepping and clears the backlog. No time accrues while paused.
    pub fn pause(&mut self) {
        self.paused = true;
        self.clear_backlog();
    }

    /// Resumes stepping. Timing restarts from this moment, so there is no catch-up burst.
    pub fn play(&mut self) {
        self.paused = false;
        self.clear_backlog();
    }

    /// Clears the backlog, leaves the driver paused, and runs exactly one tick.
    pub fn single_step(&mut self, sim: &mut Simulation) {
        self.paused = true;
        self.clear_backlog();
        sim.step();
    }

    /// Clears the backlog. Call after replacing the `Simulation` with a fresh one; the
    /// paused or running state is unchanged.
    pub fn reset(&mut self) {
        self.clear_backlog();
    }

    /// Changes the requested speed and clears the backlog, so a lower speed never has to
    /// process time owed under a higher one.
    pub fn set_speed(&mut self, speed: Speed) {
        self.speed = speed;
        self.clear_backlog();
    }

    pub fn speed(&self) -> Speed {
        self.speed
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn limits(&self) -> DriverLimits {
        self.limits
    }

    /// Whole ticks owed but not yet run.
    pub fn backlog_ticks(&self) -> u64 {
        u64::try_from(self.owed / UNITS_PER_TICK).unwrap_or(u64::MAX)
    }

    /// Owed simulated time in seconds, including any partial tick, for display.
    pub fn backlog_sim_seconds(&self) -> f64 {
        if self.owed == 0 {
            return 0.0;
        }
        self.owed as f64 / (UNITS_PER_TICK as f64 * f64::from(self.ticks_per_second))
    }

    /// True when at least one whole tick is still owed after the last frame, meaning the
    /// machine is not keeping up with the requested speed.
    pub fn is_behind(&self) -> bool {
        self.owed >= UNITS_PER_TICK
    }

    /// Ticks run ÷ (ticks per second × real seconds) over the recent window, as a multiple
    /// of real time. `None` while paused or before any unpaused frame.
    pub fn achieved_speed(&self) -> Option<f64> {
        if self.paused || self.window_nanos == 0 || self.ticks_per_second == 0 {
            return None;
        }
        let seconds = self.window_nanos as f64 / 1e9;
        Some(self.window_ticks as f64 / (f64::from(self.ticks_per_second) * seconds))
    }

    /// Number of frames whose delta was trimmed by the stall cap.
    pub fn stall_count(&self) -> u64 {
        self.stall_count
    }

    fn clear_backlog(&mut self) {
        self.owed = 0;
        self.window.clear();
        self.window_nanos = 0;
        self.window_ticks = 0;
    }

    fn record_frame(&mut self, delta: Duration, ticks: u64) {
        let nanos = delta.as_nanos();
        self.window.push_back((nanos, ticks));
        self.window_nanos += nanos;
        self.window_ticks += ticks;
        let window = self.limits.achieved_speed_window.as_nanos();
        while let Some(&(oldest_nanos, oldest_ticks)) = self.window.front() {
            if self.window_nanos - oldest_nanos < window {
                break;
            }
            self.window.pop_front();
            self.window_nanos -= oldest_nanos;
            self.window_ticks -= oldest_ticks;
        }
    }
}

#[cfg(test)]
mod tests {
    use sim::ScenarioConfig;

    use super::*;

    fn new_sim() -> Simulation {
        Simulation::new(&ScenarioConfig::default(), 1).expect("default configuration is valid")
    }

    fn running(speed: Speed, limits: DriverLimits) -> SteppingDriver {
        let mut driver = SteppingDriver::new(limits);
        driver.set_speed(speed);
        driver.play();
        driver
    }

    fn tick_cap(max_ticks_per_frame: u64) -> DriverLimits {
        DriverLimits {
            max_ticks_per_frame,
            ..DriverLimits::default()
        }
    }

    /// Work clock for tests where work time never limits a fixed-speed frame.
    fn no_work() -> impl FnMut() -> Duration {
        || Duration::ZERO
    }

    /// Deterministic fake work clock: each tick costs `cost`. The n-th check in a frame
    /// (before tick n) reports `n × cost` of work done.
    fn work_per_tick(cost: Duration) -> impl FnMut() -> Duration {
        let mut checks = 0u32;
        move || {
            let elapsed = cost * checks;
            checks += 1;
            elapsed
        }
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// Thirty frames of 1/30 s, rounded to whole nanoseconds so they sum to exactly 1 s
    /// (ten of 33,333,334 ns and twenty of 33,333,333 ns), as a real frame clock would.
    fn thirtieths_of_a_second() -> Vec<Duration> {
        (0..30)
            .map(|i| Duration::from_nanos(if i % 3 == 0 { 33_333_334 } else { 33_333_333 }))
            .collect()
    }

    fn drain(driver: &mut SteppingDriver, sim: &mut Simulation) {
        while driver.is_behind() {
            driver.frame(sim, Duration::ZERO, no_work());
        }
    }

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    /// T-FT-1: at 1×, frame times summing to 1.000 s produce exactly 30 ticks.
    #[test]
    fn t_ft_1_thirty_frames_of_one_thirtieth_second_run_thirty_ticks() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X1, DriverLimits::default());
        let frames = thirtieths_of_a_second();
        assert_eq!(frames.iter().sum::<Duration>(), Duration::from_secs(1));
        for delta in frames {
            assert_eq!(driver.frame(&mut sim, delta, no_work()).ticks_run, 1);
        }
        assert_eq!(sim.tick(), 30);
        assert_eq!(driver.backlog_ticks(), 0);
    }

    /// T-FT-1: the remainder of a partial tick is carried, never lost.
    #[test]
    fn t_ft_1_remainder_is_carried_between_frames() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X1, DriverLimits::default());
        // 30 × 33,333,333 ns is 10 ns short of one second: 29 ticks, the 30th not yet owed.
        for _ in 0..30 {
            driver.frame(&mut sim, Duration::from_nanos(33_333_333), no_work());
        }
        assert_eq!(sim.tick(), 29);
        // The missing 10 ns completes the 30th tick.
        let report = driver.frame(&mut sim, Duration::from_nanos(10), no_work());
        assert_eq!(report.ticks_run, 1);
        assert_eq!(sim.tick(), 30);
    }

    /// T-FT-1: only whole ticks run.
    #[test]
    fn t_ft_1_only_whole_ticks_run() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X1, DriverLimits::default());
        let half_tick = Duration::from_nanos(16_666_667);
        assert_eq!(driver.frame(&mut sim, half_tick, no_work()).ticks_run, 0);
        assert_eq!(sim.tick(), 0);
        assert!(driver.backlog_sim_seconds() > 0.0);
        assert_eq!(driver.frame(&mut sim, half_tick, no_work()).ticks_run, 1);
        assert_eq!(sim.tick(), 1);
    }

    /// T-FT-1: the same total time split into different frame sequences gives the same
    /// ticks and state when no cap is reached.
    #[test]
    fn t_ft_1_frame_split_does_not_change_result() {
        let mut even_sim = new_sim();
        let mut even = running(Speed::X10, DriverLimits::default());
        for _ in 0..100 {
            even.frame(&mut even_sim, ms(10), no_work());
        }

        let mut uneven_sim = new_sim();
        let mut uneven = running(Speed::X10, DriverLimits::default());
        for delta in [250, 250, 200, 150, 100, 49, 1].map(ms) {
            let report = uneven.frame(&mut uneven_sim, delta, no_work());
            assert!(!report.limited && !report.stall_capped);
        }

        assert_eq!(even_sim.tick(), 300);
        assert_eq!(uneven_sim.tick(), 300);
        assert_eq!(even_sim.state_hash(), uneven_sim.state_hash());
    }

    /// T-FT-1: under a per-frame tick cap, unprocessed time stays owed and runs later, so
    /// the total equals 30 × multiplier × elapsed once drained.
    #[test]
    fn t_ft_1_tick_cap_retains_backlog_until_drained() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X10, tick_cap(4));
        let report = driver.frame(&mut sim, ms(100), no_work());
        assert_eq!(report.ticks_run, 4);
        assert!(report.limited);
        assert_eq!(driver.backlog_ticks(), 26);
        assert!(close(driver.backlog_sim_seconds(), 26.0 / 30.0));
        assert!(driver.is_behind());

        let mut per_frame = Vec::new();
        while driver.is_behind() {
            per_frame.push(driver.frame(&mut sim, Duration::ZERO, no_work()).ticks_run);
        }
        assert_eq!(per_frame, [4, 4, 4, 4, 4, 4, 2]);
        assert_eq!(sim.tick(), 30, "30 ticks/s × 10 × 0.1 s");
        assert_eq!(driver.backlog_ticks(), 0);
    }

    /// T-FT-1: the per-frame work limit is checked between ticks.
    #[test]
    fn t_ft_1_work_limit_is_respected() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X10, DriverLimits::default());
        // 250 ms at 10× owes 75 ticks; at 5 ms per tick only 5 fit in 25 ms.
        let report = driver.frame(&mut sim, ms(250), work_per_tick(ms(5)));
        assert_eq!(report.ticks_run, 5);
        assert!(report.limited);
        assert_eq!(driver.backlog_ticks(), 70);
    }

    /// T-FT-2: when the cap limits work, achieved speed is reported below requested and the
    /// backlog grows.
    #[test]
    fn t_ft_2_achieved_speed_drops_when_cap_limits_work() {
        let mut sim = new_sim();
        let mut unconstrained = running(Speed::X10, DriverLimits::default());
        for _ in 0..100 {
            unconstrained.frame(&mut sim, ms(20), no_work());
        }
        let full = unconstrained.achieved_speed().expect("running");
        assert!(close(full, 10.0), "{full}");

        let mut sim = new_sim();
        // 20 ms frames at 10× owe 6 ticks each; only 3 may run.
        let mut capped = running(Speed::X10, tick_cap(3));
        for _ in 0..100 {
            capped.frame(&mut sim, ms(20), no_work());
        }
        let achieved = capped.achieved_speed().expect("running");
        assert!(close(achieved, 5.0), "{achieved}");
        assert!(capped.is_behind());
        assert_eq!(capped.backlog_ticks(), 300);
    }

    /// T-FT-2: a frame longer than 250 ms adds only 250 ms and is counted; frames at or
    /// under 250 ms are never trimmed.
    #[test]
    fn t_ft_2_stall_cap_trims_only_abnormal_frames() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X1, DriverLimits::default());

        let report = driver.frame(&mut sim, Duration::from_secs(5), no_work());
        assert!(report.stall_capped);
        assert_eq!(driver.stall_count(), 1);
        assert_eq!(report.ticks_run, 7, "250 ms × 30 = 7.5 ticks");

        let report = driver.frame(&mut sim, ms(250), no_work());
        assert!(!report.stall_capped);
        assert_eq!(driver.stall_count(), 1);
        assert_eq!(report.ticks_run, 8, "7.5 more ticks plus the carried half");
        assert_eq!(sim.tick(), 15);
    }

    /// T-FT-2: Max mode runs only whole ticks within its work budget.
    #[test]
    fn t_ft_2_max_mode_respects_work_budget() {
        let mut sim = new_sim();
        let mut driver = running(Speed::Max, DriverLimits::default());

        let report = driver.frame(&mut sim, ms(16), work_per_tick(ms(1)));
        assert_eq!(report.ticks_run, 12, "12 ms budget at 1 ms per tick");
        let report = driver.frame(&mut sim, ms(16), work_per_tick(ms(5)));
        assert_eq!(report.ticks_run, 3, "ticks start at 0, 5 and 10 ms");
        assert!(!report.limited);
        assert_eq!(sim.tick(), 15);
        assert_eq!(driver.backlog_ticks(), 0);
        assert!(driver.achieved_speed().is_some());
    }

    /// T-FT-3: paused, no ticks run and no time accrues; pause clears the backlog, and
    /// resuming runs no more than one frame's owed time.
    #[test]
    fn t_ft_3_pause_clears_backlog_without_catch_up() {
        let mut sim = new_sim();
        let mut driver = SteppingDriver::new(tick_cap(4));
        assert!(driver.is_paused(), "starts paused");
        assert_eq!(driver.frame(&mut sim, ms(100), no_work()).ticks_run, 0);

        driver.set_speed(Speed::X10);
        driver.play();
        driver.frame(&mut sim, ms(100), no_work());
        assert_eq!(driver.backlog_ticks(), 26);

        driver.pause();
        assert_eq!(driver.backlog_ticks(), 0);
        assert_eq!(driver.achieved_speed(), None);
        let paused_at = sim.tick();
        for _ in 0..50 {
            assert_eq!(driver.frame(&mut sim, ms(100), no_work()).ticks_run, 0);
        }
        assert_eq!(sim.tick(), paused_at);
        assert_eq!(driver.backlog_ticks(), 0);

        driver.set_speed(Speed::X1);
        driver.play();
        let first = thirtieths_of_a_second()[0];
        assert_eq!(driver.frame(&mut sim, first, no_work()).ticks_run, 1);
    }

    /// T-FT-3: Step runs exactly one tick, clears the backlog and leaves the driver paused.
    #[test]
    fn t_ft_3_single_step_runs_exactly_one_tick() {
        let mut sim = new_sim();
        let mut driver = SteppingDriver::new(DriverLimits::default());
        driver.single_step(&mut sim);
        assert_eq!(sim.tick(), 1);
        assert!(driver.is_paused());

        let mut driver = running(Speed::X10, tick_cap(4));
        driver.frame(&mut sim, ms(100), no_work());
        assert!(driver.backlog_ticks() > 0);
        let before = sim.tick();
        driver.single_step(&mut sim);
        assert_eq!(sim.tick(), before + 1);
        assert_eq!(driver.backlog_ticks(), 0);
        assert!(driver.is_paused());
        assert_eq!(driver.frame(&mut sim, ms(100), no_work()).ticks_run, 0);
    }

    /// T-FT-3: Reset clears the backlog and keeps the paused or running state.
    #[test]
    fn t_ft_3_reset_clears_backlog() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X10, tick_cap(4));
        driver.frame(&mut sim, ms(100), no_work());
        assert_eq!(driver.backlog_ticks(), 26);

        sim = new_sim();
        driver.reset();
        assert_eq!(driver.backlog_ticks(), 0);
        assert!(!driver.is_paused());
        assert_eq!(driver.frame(&mut sim, ms(10), no_work()).ticks_run, 3);
        assert_eq!(sim.tick(), 3);
    }

    /// T-FT-3: a speed change clears the backlog accumulated at the previous speed.
    #[test]
    fn t_ft_3_speed_change_clears_backlog() {
        let mut sim = new_sim();
        let mut driver = running(Speed::X10, tick_cap(4));
        driver.frame(&mut sim, ms(100), no_work());
        assert_eq!(driver.backlog_ticks(), 26);

        driver.set_speed(Speed::X1);
        assert_eq!(driver.speed(), Speed::X1);
        assert_eq!(driver.backlog_ticks(), 0);
        let first = thirtieths_of_a_second()[0];
        assert_eq!(driver.frame(&mut sim, first, no_work()).ticks_run, 1);
    }

    /// T-DET-4 (driver portion): two different frame-time sequences, one hitting the
    /// per-frame cap and building a backlog, reach the same tick count for the same elapsed
    /// time once drained, and the same state hash as stepping the simulation directly.
    /// The comparison with `headless run --ticks` is pending M-17.
    #[test]
    fn t_det_4_driver_matches_direct_stepping() {
        let mut capped_sim = new_sim();
        let mut capped = running(Speed::X10, tick_cap(4));
        for _ in 0..50 {
            capped.frame(&mut capped_sim, ms(20), no_work());
        }
        assert!(capped.is_behind());
        drain(&mut capped, &mut capped_sim);

        let mut smooth_sim = new_sim();
        let mut smooth = running(Speed::X10, DriverLimits::default());
        for delta in [ms(250); 4] {
            smooth.frame(&mut smooth_sim, delta, no_work());
        }

        let mut direct = new_sim();
        direct.step_n(300);

        assert_eq!(capped_sim.tick(), 300, "1 s at 10×");
        assert_eq!(smooth_sim.tick(), 300);
        assert_eq!(capped_sim.state_hash(), direct.state_hash());
        assert_eq!(smooth_sim.state_hash(), direct.state_hash());
    }

    #[test]
    fn speed_multipliers_match_scope() {
        let multipliers: Vec<Option<u32>> = Speed::ALL.iter().map(|s| s.multiplier()).collect();
        assert_eq!(multipliers, [Some(1), Some(2), Some(5), Some(10), None]);
    }
}
