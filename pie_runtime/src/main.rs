use std::env;
use std::process::ExitCode;

use pie_runtime::{
    MainLoopLimits, RuntimeApp, StopSignal, init_logging, install_ctrlc_handler, run_main_loop,
};

#[cfg(feature = "rendering")]
use pie_runtime::{EngineMode, run_client_window};

// M9.3: mimalloc as the global allocator. Gated behind the `mimalloc` feature so
// it stays toggleable per the engine's "every system must be toggleable /
// measurable" philosophy. When the feature is off, the system allocator is used
// (no behavior change, no extra dep). When on, mimalloc replaces the global
// allocator for the whole process — generally lower fragmentation and better
// multi-threaded throughput than system malloc, at the cost of a slightly larger
// binary and a one-time init. Measure with the M9.1 frame-timing overlay and the
// M9.2 tracing layer before/after toggling to confirm the tradeoff is worth it
// for your workload.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    init_logging();

    let args: Vec<String> = env::args().collect();
    let mut app = match RuntimeApp::from_args(&args) {
        Ok(app) => app,
        Err(error) => {
            eprintln!("pie_runtime: failed to start: {error:?}");
            return ExitCode::FAILURE;
        }
    };

    #[cfg(feature = "rendering")]
    if app.config().mode == EngineMode::Client {
        match run_client_window(app) {
            Ok(()) => return ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("pie_runtime: client window failed: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    let stop_signal = StopSignal::new();
    if let Err(error) = install_ctrlc_handler(stop_signal.clone()) {
        eprintln!("pie_runtime: failed to install Ctrl+C handler: {error}");
        return ExitCode::FAILURE;
    }

    println!(
        "Pie Runtime starting: mode={:?}, fixed_timestep_seconds={}",
        app.config().mode,
        app.config().fixed_timestep_seconds
    );
    println!("Press Ctrl+C to stop.");

    let total_steps = run_main_loop(&mut app, &stop_signal, MainLoopLimits::default());

    println!("Pie Runtime stopped after {total_steps} simulation steps.");

    ExitCode::SUCCESS
}
