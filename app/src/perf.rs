//! Rolling frame statistics for the performance overlay (M-18, SCOPE 7.3, 10.1).
//!
//! Over the last 60 seconds of real time: median and p95 frame time, the share of frames
//! over 50 ms, mean simulation milliseconds per frame, ticks per real second, and the
//! achieved speed against the tick rate. Pure logic fed with measured durations; it never
//! touches the simulation.

use std::collections::VecDeque;
use std::time::Duration;

/// Length of the rolling window (SCOPE 7.3: 60 seconds).
pub const WINDOW: Duration = Duration::from_secs(60);
/// A frame longer than this counts as slow (SCOPE 10.2: 50 ms).
pub const SLOW_FRAME: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq)]
struct FrameSample {
    frame: Duration,
    sim: Duration,
    ticks: u64,
}

/// Summary of the window.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PerfSummary {
    pub frames: usize,
    pub window_seconds: f64,
    pub median_frame_ms: f64,
    pub p95_frame_ms: f64,
    pub slow_frame_share: f64,
    pub sim_ms_per_frame: f64,
    pub ticks_per_second: f64,
}

#[derive(Debug, Clone, Default)]
pub struct PerfWindow {
    samples: VecDeque<FrameSample>,
    total: Duration,
}

impl PerfWindow {
    /// Records one frame: its real duration, the simulation work inside it, and ticks run.
    pub fn record(&mut self, frame: Duration, sim: Duration, ticks: u64) {
        self.samples.push_back(FrameSample { frame, sim, ticks });
        self.total += frame;
        while self.total > WINDOW
            && let Some(old) = self.samples.pop_front()
        {
            self.total -= old.frame;
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.total = Duration::ZERO;
    }

    pub fn summary(&self) -> PerfSummary {
        let n = self.samples.len();
        if n == 0 {
            return PerfSummary::default();
        }
        let mut frames: Vec<Duration> = self.samples.iter().map(|s| s.frame).collect();
        frames.sort_unstable();
        let rank = |fraction: f64| frames[(((n - 1) as f64) * fraction).round() as usize];
        let ms = |d: Duration| d.as_secs_f64() * 1_000.0;
        let slow = self.samples.iter().filter(|s| s.frame > SLOW_FRAME).count();
        let sim: Duration = self.samples.iter().map(|s| s.sim).sum();
        let ticks: u64 = self.samples.iter().map(|s| s.ticks).sum();
        let seconds = self.total.as_secs_f64();
        PerfSummary {
            frames: n,
            window_seconds: seconds,
            median_frame_ms: ms(rank(0.5)),
            p95_frame_ms: ms(rank(0.95)),
            slow_frame_share: slow as f64 / n as f64,
            sim_ms_per_frame: ms(sim) / n as f64,
            ticks_per_second: if seconds > 0.0 {
                ticks as f64 / seconds
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_follow_the_recorded_frames() {
        let mut window = PerfWindow::default();
        for k in 0..100u64 {
            let frame = Duration::from_millis(if k % 10 == 0 { 60 } else { 10 });
            window.record(frame, Duration::from_millis(4), 3);
        }
        let s = window.summary();
        assert_eq!(s.frames, 100);
        assert!((s.median_frame_ms - 10.0).abs() < 1e-9);
        assert!((s.p95_frame_ms - 60.0).abs() < 1e-9);
        assert!((s.slow_frame_share - 0.1).abs() < 1e-9);
        assert!((s.sim_ms_per_frame - 4.0).abs() < 1e-9);
        assert!((s.ticks_per_second - 300.0 / 1.5).abs() < 1e-6);
    }

    #[test]
    fn the_window_keeps_only_the_last_sixty_seconds() {
        let mut window = PerfWindow::default();
        for _ in 0..200 {
            window.record(Duration::from_secs(1), Duration::ZERO, 30);
        }
        let s = window.summary();
        assert_eq!(s.frames, 60);
        assert!((s.window_seconds - 60.0).abs() < 1e-9);
        assert!((s.ticks_per_second - 30.0).abs() < 1e-9);
        window.clear();
        assert_eq!(window.summary(), PerfSummary::default());
    }
}
