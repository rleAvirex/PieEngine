use std::env;
use std::process::ExitCode;

use pie_runtime::{
    MainLoopLimits, RuntimeApp, StopSignal, init_logging, install_ctrlc_handler, run_main_loop,
};

#[cfg(feature = "rendering")]
use pie_runtime::{EngineMode, run_client_window};

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
