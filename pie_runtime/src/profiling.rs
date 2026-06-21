//! Lightweight, always-on frame timing metrics (Milestone 9.1).
//!
//! This is the cheap, always-available layer. For deep-dive profiling with
//! `tracing`/Tracy spans, see the `profiling` Cargo feature (Milestone 9.2) —
//! that layer is feature-gated and zero-cost when disabled. These basic metrics,
//! by contrast, are deliberately always on so the editor overlay and benchmarks
//! always have data.
//!
//! Cost: a handful of [`std::time::Instant::now`] calls per frame (a few
//! nanoseconds each). This is well under the noise floor of any frame budget and
//! does not itself perturb the timings it measures.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The four frame phases tracked by [`FrameTiming`].
///
/// These match the canonical per-frame breakdown used across the engine, the
/// editor overlay, and the benchmark/regression budgets:
///
/// - `Input`   — polling input devices / processing the window event queue.
/// - `Sim`     — fixed-step simulation ticks (the shared `SimulationCore`).
/// - `Render`  — CPU-side render command recording (GPU work is queued, not run).
/// - `Present` — submitting the recorded frame and waiting for the swapchain.
///
/// In headless mode `Render` and `Present` are zero (no GPU work), which is the
/// correct and measurable answer, not a missing measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FramePhase {
    Input,
    Sim,
    Render,
    Present,
}

impl FramePhase {
    /// All phases in canonical order.
    pub const fn ordered() -> [Self; 4] {
        [Self::Input, Self::Sim, Self::Render, Self::Present]
    }

    /// Human-readable label for the editor overlay / logs.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Sim => "sim",
            Self::Render => "render",
            Self::Present => "present",
        }
    }
}

/// Per-phase timing for a single frame.
///
/// Each field is the wall-clock `Duration` spent in that phase. The total frame
/// time is the sum of all phases plus any inter-phase overhead; use [`Self::total`]
/// for the sum (overhead is not tracked separately in v1 — if it becomes
/// significant, add an `Other` phase rather than letting it hide).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameTiming {
    pub input: Duration,
    pub sim: Duration,
    pub render: Duration,
    pub present: Duration,
}

impl FrameTiming {
    /// Sum of all four phases. Note this excludes inter-phase overhead; for a
    /// true wall-clock frame time, measure the whole frame with a single
    /// `Instant` pair instead of summing phases.
    pub fn total(&self) -> Duration {
        self.input + self.sim + self.render + self.present
    }

    /// Returns the duration for a given phase.
    pub fn phase(self, phase: FramePhase) -> Duration {
        match phase {
            FramePhase::Input => self.input,
            FramePhase::Sim => self.sim,
            FramePhase::Render => self.render,
            FramePhase::Present => self.present,
        }
    }

    /// Microseconds per phase, as `(input_us, sim_us, render_us, present_us)`.
    /// Convenience for the editor overlay and benchmark assertions, which work
    /// in integer microseconds to avoid f64 formatting noise.
    pub fn as_micros_tuple(self) -> (u64, u64, u64, u64) {
        (
            self.input.as_micros() as u64,
            self.sim.as_micros() as u64,
            self.render.as_micros() as u64,
            self.present.as_micros() as u64,
        )
    }
}

/// A rolling history of the last `capacity` frames' [`FrameTiming`] samples.
///
/// Used by the editor's profiling overlay (to draw a frame-time graph) and by
/// the benchmark/regression harness (to compute stable averages over a window).
/// The ring buffer is bounded so long-running sessions don't grow unbounded.
#[derive(Debug, Clone)]
pub struct FrameTimingHistory {
    samples: VecDeque<FrameTiming>,
    capacity: usize,
}

impl FrameTimingHistory {
    /// Create a history that retains the last `capacity` samples.
    ///
    /// `capacity == 0` is allowed and discards every push (a no-op history);
    /// useful for "metrics off" configurations without changing call sites.
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Default capacity for the editor overlay: ~1 second of history at 60 FPS,
    /// rounded up to a power of two for cache friendliness.
    pub const DEFAULT_CAPACITY: usize = 128;

    /// Push a new frame's timing, evicting the oldest sample if at capacity.
    pub fn push(&mut self, timing: FrameTiming) {
        if self.capacity == 0 {
            return;
        }
        if self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(timing);
    }

    /// All retained samples, oldest first.
    pub fn samples(&self) -> &VecDeque<FrameTiming> {
        &self.samples
    }

    /// Number of retained samples (≤ `capacity`).
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The most recently pushed timing, if any.
    pub fn last(&self) -> Option<&FrameTiming> {
        self.samples.back()
    }

    /// Arithmetic mean of each phase over the retained window.
    ///
    /// Returns [`FrameTiming::default()`] (all zeros) if the history is empty,
    /// so callers can render the overlay without a special case on startup.
    pub fn average(&self) -> FrameTiming {
        if self.samples.is_empty() {
            return FrameTiming::default();
        }
        let n = self.samples.len() as u32;
        let sum = self
            .samples
            .iter()
            .fold(FrameTiming::default(), |acc, t| FrameTiming {
                input: acc.input + t.input,
                sim: acc.sim + t.sim,
                render: acc.render + t.render,
                present: acc.present + t.present,
            });
        FrameTiming {
            input: sum.input / n,
            sim: sum.sim / n,
            render: sum.render / n,
            present: sum.present / n,
        }
    }

    /// The worst (max) total frame time across the retained window. Used by
    /// benchmark regression checks that care about spikes, not just the mean.
    pub fn max_total(&self) -> Duration {
        self.samples
            .iter()
            .map(FrameTiming::total)
            .max()
            .unwrap_or_default()
    }

    /// The retained capacity (not the current sample count).
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for FrameTimingHistory {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY)
    }
}

/// RAII guard that accumulates elapsed wall-clock time into a target `Duration`
/// when dropped.
///
/// Used to time a phase ergonomically without a manual `Instant::now()` pair:
///
/// ```
/// use pie_runtime::profiling::{FrameTiming, PhaseTimer};
/// let mut timing = FrameTiming::default();
/// {
///     let _t = PhaseTimer::new(&mut timing.sim);
///     // ... simulation work ...
/// } // drop: timing.sim += elapsed
/// ```
///
/// The guard *adds* to the target on drop (not overwrites), so a phase split
/// across multiple spans accumulates correctly. Each `FrameTiming` field starts
/// at zero per frame, so add-vs-overwrite is equivalent for the common case.
pub struct PhaseTimer<'a> {
    start: Instant,
    target: &'a mut Duration,
}

impl<'a> PhaseTimer<'a> {
    /// Begin timing; the elapsed duration will be added to `target` on drop.
    pub fn new(target: &'a mut Duration) -> Self {
        Self {
            start: Instant::now(),
            target,
        }
    }
}

impl<'a> Drop for PhaseTimer<'a> {
    fn drop(&mut self) {
        *self.target += self.start.elapsed();
    }
}

#[cfg(test)]
mod tests {
    use super::{FramePhase, FrameTiming, FrameTimingHistory, PhaseTimer};
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn frame_timing_default_is_all_zero() {
        let t = FrameTiming::default();
        assert_eq!(t.input, Duration::ZERO);
        assert_eq!(t.sim, Duration::ZERO);
        assert_eq!(t.render, Duration::ZERO);
        assert_eq!(t.present, Duration::ZERO);
        assert_eq!(t.total(), Duration::ZERO);
    }

    #[test]
    fn frame_timing_total_sums_all_phases() {
        let t = FrameTiming {
            input: Duration::from_micros(10),
            sim: Duration::from_micros(20),
            render: Duration::from_micros(30),
            present: Duration::from_micros(40),
        };
        assert_eq!(t.total(), Duration::from_micros(100));
    }

    #[test]
    fn frame_phase_ordered_and_labels_are_stable() {
        // The canonical order is part of the overlay/benchmark contract;
        // lock it down so a reorder doesn't silently shift columns.
        assert_eq!(
            FramePhase::ordered(),
            [
                FramePhase::Input,
                FramePhase::Sim,
                FramePhase::Render,
                FramePhase::Present
            ]
        );
        assert_eq!(FramePhase::Input.label(), "input");
        assert_eq!(FramePhase::Sim.label(), "sim");
        assert_eq!(FramePhase::Render.label(), "render");
        assert_eq!(FramePhase::Present.label(), "present");
    }

    #[test]
    fn phase_timer_accumulates_elapsed_into_target() {
        let mut timing = FrameTiming::default();
        {
            let _t = PhaseTimer::new(&mut timing.sim);
            sleep(Duration::from_millis(2));
        }
        // At least *some* time was recorded. We assert a loose lower bound
        // (1ms) rather than an exact value to stay robust to scheduler jitter.
        assert!(
            timing.sim >= Duration::from_millis(1),
            "sim timing should be >= 1ms, was {:?}",
            timing.sim
        );
        // Other phases untouched.
        assert_eq!(timing.input, Duration::ZERO);
        assert_eq!(timing.render, Duration::ZERO);
        assert_eq!(timing.present, Duration::ZERO);
    }

    #[test]
    fn phase_timer_adds_not_overwrites() {
        let mut timing = FrameTiming::default();
        let pre = Duration::from_micros(100);
        timing.sim = pre;
        {
            let _t = PhaseTimer::new(&mut timing.sim);
            sleep(Duration::from_millis(1));
        }
        assert!(
            timing.sim > pre,
            "timer should add to the existing value, not overwrite it"
        );
    }

    #[test]
    fn history_push_retains_up_to_capacity() {
        let mut h = FrameTimingHistory::new(3);
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);

        for i in 0u64..5 {
            h.push(FrameTiming {
                input: Duration::from_micros(i),
                ..Default::default()
            });
        }
        assert_eq!(h.len(), 3, "capacity 3 should retain only the last 3");
        // Oldest retained should be the i=2 sample (0,1 were evicted).
        assert_eq!(h.samples()[0].input, Duration::from_micros(2));
        assert_eq!(h.samples()[2].input, Duration::from_micros(4));
        assert_eq!(h.last().unwrap().input, Duration::from_micros(4));
    }

    #[test]
    fn history_capacity_zero_is_a_noop_sink() {
        let mut h = FrameTimingHistory::new(0);
        h.push(FrameTiming::default());
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert_eq!(h.capacity(), 0);
    }

    #[test]
    fn history_average_is_zero_when_empty() {
        let h = FrameTimingHistory::new(16);
        let avg = h.average();
        assert_eq!(avg, FrameTiming::default());
    }

    #[test]
    fn history_average_is_arithmetic_mean_per_phase() {
        let mut h = FrameTimingHistory::new(16);
        h.push(FrameTiming {
            sim: Duration::from_micros(100),
            ..Default::default()
        });
        h.push(FrameTiming {
            sim: Duration::from_micros(300),
            ..Default::default()
        });
        let avg = h.average();
        assert_eq!(avg.sim, Duration::from_micros(200));
        assert_eq!(avg.input, Duration::ZERO);
    }

    #[test]
    fn history_max_total_picks_the_worst_frame() {
        let mut h = FrameTimingHistory::new(16);
        h.push(FrameTiming {
            sim: Duration::from_micros(100),
            ..Default::default()
        });
        h.push(FrameTiming {
            sim: Duration::from_micros(500),
            render: Duration::from_micros(200),
            ..Default::default()
        }); // total 700
        h.push(FrameTiming {
            sim: Duration::from_micros(60),
            ..Default::default()
        });
        assert_eq!(h.max_total(), Duration::from_micros(700));
    }

    #[test]
    fn as_micros_tuple_round_trips_durations() {
        let t = FrameTiming {
            input: Duration::from_micros(1),
            sim: Duration::from_micros(2),
            render: Duration::from_micros(3),
            present: Duration::from_micros(4),
        };
        assert_eq!(t.as_micros_tuple(), (1, 2, 3, 4));
    }
}
