use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::core::RuntimeApp;

/// Hard ceiling on a single frame's delta time, in seconds.
///
/// Without this, a debugger breakpoint, OS suspend, or any other long stall
/// would hand the fixed-step accumulator a huge delta and force it to "catch
/// up" by running a large number of simulation ticks back-to-back. Clamping
/// here means a stall just costs wall-clock time, not simulation correctness.
const MAX_FRAME_DELTA_SECONDS: f64 = 0.25;

/// Optional limits for [`run_main_loop`].
///
/// Defaults run forever (until the stop signal fires).
#[derive(Debug, Clone, Copy, Default)]
pub struct MainLoopLimits {
    /// Stop after this many fixed simulation steps have completed in total.
    /// `None` means no limit.
    pub max_steps: Option<u64>,
}

/// A clonable, thread-safe stop signal for the main loop.
///
/// `install_ctrlc_handler` wires this up to SIGINT. The same flag can later
/// be flipped by a windowing close event, so Ctrl+C and "user closed the
/// window" both funnel into one shutdown path.
#[derive(Debug, Clone)]
pub struct StopSignal {
    running: Arc<AtomicBool>,
}

impl StopSignal {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn should_continue(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Default for StopSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers a Ctrl+C (SIGINT) handler that flips `signal` to stopped.
///
/// Safe to call once per process. Returns an error if a handler is already
/// installed (e.g. called twice).
pub fn install_ctrlc_handler(signal: StopSignal) -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(move || {
        signal.stop();
    })
}

/// Source of per-iteration delta time for the main loop.
///
/// Production code uses [`WallClock`]. Tests inject a fake source so loop
/// behavior (limits, clamping, shutdown) can be verified without depending
/// on real elapsed time or busy-spinning.
pub trait TimeSource {
    /// Returns the elapsed seconds since the previous call (or since the
    /// time source was created, for the first call).
    fn next_delta_seconds(&mut self) -> f64;
}

/// Real wall-clock time source backed by [`std::time::Instant`].
pub struct WallClock {
    last_instant: Instant,
}

impl WallClock {
    pub fn new() -> Self {
        Self {
            last_instant: Instant::now(),
        }
    }
}

impl Default for WallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeSource for WallClock {
    fn next_delta_seconds(&mut self) -> f64 {
        let now = Instant::now();
        let delta = (now - self.last_instant).as_secs_f64();
        self.last_instant = now;
        delta
    }
}

/// Runs `app` until `stop_signal` is tripped or `limits.max_steps` is
/// reached, whichever comes first, pulling per-iteration delta time from
/// `time_source`.
///
/// Returns the total number of fixed simulation steps completed.
pub fn run_main_loop_with_time_source(
    app: &mut RuntimeApp,
    stop_signal: &StopSignal,
    limits: MainLoopLimits,
    time_source: &mut dyn TimeSource,
) -> u64 {
    app.start();

    let mut total_steps: u64 = 0;

    while stop_signal.should_continue() {
        if let Some(max_steps) = limits.max_steps {
            if total_steps >= max_steps {
                break;
            }
        }

        let raw_delta_seconds = time_source.next_delta_seconds();
        let delta_seconds = raw_delta_seconds.min(MAX_FRAME_DELTA_SECONDS);
        total_steps += app.update(delta_seconds);
    }

    app.shutdown();
    total_steps
}

/// Runs `app` against real wall-clock time. See
/// [`run_main_loop_with_time_source`] for the underlying behavior.
pub fn run_main_loop(
    app: &mut RuntimeApp,
    stop_signal: &StopSignal,
    limits: MainLoopLimits,
) -> u64 {
    let mut wall_clock = WallClock::new();
    run_main_loop_with_time_source(app, stop_signal, limits, &mut wall_clock)
}

#[cfg(test)]
mod tests {
    use super::{
        MainLoopLimits, StopSignal, TimeSource, run_main_loop, run_main_loop_with_time_source,
    };
    use crate::core::RuntimeApp;

    /// Deterministic time source: returns a fixed delta every call.
    /// Used so loop-limit and clamping tests don't depend on real time.
    struct FixedStepClock {
        delta_seconds: f64,
    }

    impl TimeSource for FixedStepClock {
        fn next_delta_seconds(&mut self) -> f64 {
            self.delta_seconds
        }
    }

    #[test]
    fn stop_signal_starts_running() {
        let signal = StopSignal::new();

        assert!(signal.should_continue());
    }

    #[test]
    fn stop_signal_stops_when_told() {
        let signal = StopSignal::new();

        signal.stop();

        assert!(!signal.should_continue());
    }

    #[test]
    fn cloned_stop_signal_shares_state() {
        let signal = StopSignal::new();
        let cloned = signal.clone();

        cloned.stop();

        assert!(!signal.should_continue());
    }

    #[test]
    fn run_main_loop_stops_immediately_when_signal_already_stopped() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");
        let signal = StopSignal::new();
        signal.stop();

        let steps = run_main_loop(&mut app, &signal, MainLoopLimits::default());

        assert_eq!(steps, 0);
        assert!(!app.is_running());
    }

    #[test]
    fn run_main_loop_with_time_source_respects_max_steps_limit() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");
        let signal = StopSignal::new();
        let limits = MainLoopLimits { max_steps: Some(2) };
        let mut clock = FixedStepClock {
            delta_seconds: 1.0 / 60.0,
        };

        let steps = run_main_loop_with_time_source(&mut app, &signal, limits, &mut clock);

        assert_eq!(steps, 2);
        assert_eq!(app.simulation().frame(), 2);
        assert!(!app.is_running());
    }

    #[test]
    fn run_main_loop_with_time_source_clamps_large_deltas() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");
        let signal = StopSignal::new();
        let limits = MainLoopLimits { max_steps: Some(1) };
        // A huge stall (e.g. debugger breakpoint) must be clamped to
        // MAX_FRAME_DELTA_SECONDS before reaching the accumulator. Without
        // the clamp, 600s of delta at a 1/60s timestep would produce 36000
        // ticks in a single update() call instead of a bounded catch-up.
        let mut clock = FixedStepClock {
            delta_seconds: 600.0,
        };

        let steps = run_main_loop_with_time_source(&mut app, &signal, limits, &mut clock);

        // max_steps is only checked between loop iterations, not inside a
        // single update() call, so one iteration with a clamped 0.25s delta
        // at a 1/60s timestep produces floor(0.25 / (1/60)) = 15 steps
        // before the limit check has a chance to stop the loop.
        let expected_steps_from_one_clamped_iteration = 15;
        assert_eq!(steps, expected_steps_from_one_clamped_iteration);
        assert!(!app.is_running());
    }

    #[test]
    fn run_main_loop_with_time_source_starts_and_shuts_down_app() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");
        let signal = StopSignal::new();
        let limits = MainLoopLimits { max_steps: Some(1) };
        let mut clock = FixedStepClock {
            delta_seconds: 1.0 / 60.0,
        };

        assert!(!app.is_running());

        run_main_loop_with_time_source(&mut app, &signal, limits, &mut clock);

        assert!(!app.is_running());
    }
}
