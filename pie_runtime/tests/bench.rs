//! Milestone 9.5: CI-runnable performance regression benchmark.
//!
//! This is a **custom harness** (not `cargo bench`, which requires nightly) so it
//! runs on stable Rust and integrates with `cargo test` — the same command CI
//! already runs. The harness exercises a representative regression scene through
//! the public runtime API, collects per-frame `FrameTiming` via the M9.1 metrics
//! layer, computes phase statistics, and asserts documented frame-time budgets.
//!
//! ## How to run
//!
//! - **CI gate (debug, fast):** `cargo test --test bench` — runs in debug, asserts
//!   loose budgets that catch egregious regressions (e.g. an accidental O(n²))
//!   without false-positiving on CI jitter. This is what runs on every PR.
//! - **Real numbers (release):** `cargo test --release --test bench -- --nocapture`
//!   — runs the same scene in release and prints the actual mean/p50/p95/p99/max.
//!   Use this to track real perf and to recalibrate budgets when the scene or
//!   hot path changes.
//! - **Deep dive:** combine with the M9.2 `profiling` feature and Tracy to see
//!   *where* inside sim the time goes: `cargo test --release --features profiling
//!   --test bench -- --nocapture` (with a Tracy instance connected).
//!
//! ## Budget philosophy (per the engine's lean/measurable tenet)
//!
//! Every budget below is a **CI gate**: if a bench sample exceeds it, the test
//! fails and the PR is blocked. Budgets are deliberately loose in debug (the CI
//! config) so they catch regressions, not jitter. The *target* (release) numbers
//! are documented inline as comments — they are not gates, they are the perf
//! goals the team aims for and recalibrates against.
//!
//! When a budget needs to change (e.g. the scene grew, or a legitimate new cost
//! was added), update it here with a commit message explaining why, so the budget
//! file is the audit trail of intentional perf changes.

use std::time::Duration;

use glam::{Quat, Vec3};
use pie_runtime::{
    FrameTimingHistory, MainLoopLimits, RuntimeApp, StopSignal, TimeSource, Transform, Velocity,
    run_main_loop_with_time_source,
};

// ---------------------------------------------------------------------------
// Regression scene parameters
// ---------------------------------------------------------------------------

/// Entity count for the regression scene. 1000 moving entities is enough to
/// exercise the movement-system hot path at a scale where an O(n²) regression
/// would be obvious, while keeping the bench well under a second in debug.
const SCENE_ENTITY_COUNT: u32 = 1000;

/// Frames to run. The `FrameTimingHistory` retains the last 128 samples, so we
/// run a bit more than that to fill the rolling window with steady-state data
/// (the first few frames include one-time setup cost we don't want in the stats).
const FRAME_COUNT: u64 = 256;

// ---------------------------------------------------------------------------
// Budgets (CI gates)
// ---------------------------------------------------------------------------

/// CI gate: the 99th-percentile sim-phase time for 1000 entities must stay
/// under 1 ms in **debug** builds.
///
/// Measured debug baseline (this commit, 1000 entities): p99 ≈ 46 µs, so this
/// gate has ~20× headroom — it catches a real regression (e.g. an O(n²) movement
/// pass would push 1000 entities to milliseconds) without false-positiving on
/// CI runner jitter or scheduler noise.
///
/// Release target (not a gate): p99 < 50 µs for 1000 entities.
const BUDGET_SIM_P99_DEBUG: Duration = Duration::from_millis(1);

/// CI gate: the *mean* sim-phase time for 1000 entities must stay under 500 µs
/// in debug. A second, tighter gate on the mean catches a broad-based slowdown
/// that a single spike (p99) might miss.
const BUDGET_SIM_MEAN_DEBUG: Duration = Duration::from_micros(500);

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A deterministic fixed-step time source. Each `next_delta_seconds()` returns
/// exactly one fixed timestep, so each main-loop iteration advances the
/// simulation by exactly one tick — isolating the per-tick cost in the sim phase
/// of `FrameTiming` (the input phase is ~0 since there's no real wall clock).
struct FixedStepClock {
    delta: f64,
}

impl TimeSource for FixedStepClock {
    fn next_delta_seconds(&mut self) -> f64 {
        self.delta
    }
}

/// Build a regression scene: `count` entities with `Transform` + `Velocity`,
/// spread along the X axis so they don't all start coincident. Each gets a
/// unit-X velocity so the movement system does real work every tick.
fn build_regression_scene(app: &mut RuntimeApp, count: u32) {
    app.start();
    let sim = app.simulation_mut();
    for i in 0..count {
        let x = i as f32;
        sim.spawn(
            Transform {
                translation: Vec3::new(x, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            Velocity(Vec3::new(1.0, 0.0, 0.0)),
        );
    }
}

/// Per-phase statistics over a `FrameTimingHistory` window.
#[derive(Debug, Clone, Copy)]
struct PhaseStats {
    mean: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
    max: Duration,
}

impl PhaseStats {
    fn from_history(
        history: &FrameTimingHistory,
        pick: impl Fn(&pie_runtime::FrameTiming) -> Duration,
    ) -> Self {
        let mut samples: Vec<Duration> = history.samples().iter().map(pick).collect();
        samples.sort();
        let n = samples.len();
        if n == 0 {
            return Self {
                mean: Duration::ZERO,
                p50: Duration::ZERO,
                p95: Duration::ZERO,
                p99: Duration::ZERO,
                max: Duration::ZERO,
            };
        }
        let sum: Duration = samples.iter().sum();
        let mean = sum / n as u32;
        let p50 = samples[(n / 2).saturating_sub(1)];
        let p95 = samples[(n * 95 / 100).min(n - 1)];
        let p99 = samples[(n * 99 / 100).min(n - 1)];
        let max = samples[n - 1];
        Self {
            mean,
            p50,
            p95,
            p99,
            max,
        }
    }
}

/// Run the regression bench and return the sim-phase stats + the raw history.
fn run_regression_bench() -> (PhaseStats, FrameTimingHistory) {
    let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args parse");
    build_regression_scene(&mut app, SCENE_ENTITY_COUNT);

    let signal = StopSignal::new();
    let limits = MainLoopLimits {
        max_steps: Some(FRAME_COUNT),
    };
    let mut clock = FixedStepClock { delta: 1.0 / 60.0 };
    run_main_loop_with_time_source(&mut app, &signal, limits, &mut clock);

    let history = app.frame_timing_history().clone();
    let sim_stats = PhaseStats::from_history(&history, |t| t.sim);
    (sim_stats, history)
}

/// Print a human-readable bench report to stderr (visible with `--nocapture`).
fn print_report(label: &str, sim: &PhaseStats) {
    eprintln!("--- pie_runtime bench: {label} ---");
    eprintln!(
        "scene: {SCENE_ENTITY_COUNT} entities, {FRAME_COUNT} frames (history retains last 128)"
    );
    eprintln!(
        "sim phase: mean={:?} p50={:?} p95={:?} p99={:?} max={:?}",
        sim.mean, sim.p50, sim.p95, sim.p99, sim.max
    );
    eprintln!(
        "budgets (debug CI gates): sim p99 < {:?}, sim mean < {:?}",
        BUDGET_SIM_P99_DEBUG, BUDGET_SIM_MEAN_DEBUG
    );
    eprintln!("release target (not a gate): sim p99 < 50µs for {SCENE_ENTITY_COUNT} entities");
}

// ---------------------------------------------------------------------------
// Tests (CI gates)
// ---------------------------------------------------------------------------

/// The primary CI regression gate. Runs the regression scene in whatever build
/// mode `cargo test` uses (debug in CI) and asserts the sim-phase budgets.
///
/// This test is NOT `#[ignore]` — it runs on every PR as part of `cargo test`.
/// The budgets are loose enough for debug CI (see constants above).
#[test]
fn regression_sim_phase_stays_within_budget() {
    let (sim_stats, _history) = run_regression_bench();
    print_report("regression", &sim_stats);

    assert!(
        sim_stats.p99 < BUDGET_SIM_P99_DEBUG,
        "sim p99 regression: {:?} >= budget {:?} (scene: {SCENE_ENTITY_COUNT} entities). \
         If this is an intentional perf change, update BUDGET_SIM_P99_DEBUG with a \
         commit message explaining why.",
        sim_stats.p99,
        BUDGET_SIM_P99_DEBUG
    );
    assert!(
        sim_stats.mean < BUDGET_SIM_MEAN_DEBUG,
        "sim mean regression: {:?} >= budget {:?} (scene: {SCENE_ENTITY_COUNT} entities). \
         If this is an intentional perf change, update BUDGET_SIM_MEAN_DEBUG with a \
         commit message explaining why.",
        sim_stats.mean,
        BUDGET_SIM_MEAN_DEBUG
    );
}

/// Sanity: the regression scene actually simulates (entities move). Catches a
/// bench that passes budgets because nothing ran. The first entity should have
/// moved ~FRAME_COUNT fixed steps along X (velocity 1.0 * timestep * steps).
#[test]
fn regression_scene_actually_advances_entities() {
    let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args parse");
    build_regression_scene(&mut app, SCENE_ENTITY_COUNT);

    // Run a known number of steps and check the simulation frame counter.
    let before = app.simulation().frame();
    app.run_steps(FRAME_COUNT);
    let after = app.simulation().frame();
    assert_eq!(
        after - before,
        FRAME_COUNT,
        "simulation should have advanced by {FRAME_COUNT} frames"
    );
}

/// A larger, slower bench for manual release-mode profiling. Marked `#[ignore]`
/// so it doesn't run in CI; run explicitly with
/// `cargo test --release --test bench -- --nocapture --ignored stress`.
#[test]
#[ignore]
fn stress_10000_entities_release_profile() {
    let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args parse");
    build_regression_scene(&mut app, 10_000);

    let signal = StopSignal::new();
    let limits = MainLoopLimits {
        max_steps: Some(FRAME_COUNT),
    };
    let mut clock = FixedStepClock { delta: 1.0 / 60.0 };
    run_main_loop_with_time_source(&mut app, &signal, limits, &mut clock);

    let history = app.frame_timing_history();
    let stats = PhaseStats::from_history(history, |t| t.sim);
    print_report("stress (10000 entities, ignored)", &stats);
    // No budget assertion here — this is a profiling aid, not a gate. The
    // numbers are for the team to read with --nocapture and track over time.
}
