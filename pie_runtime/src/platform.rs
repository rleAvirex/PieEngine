use crate::core::{
    RuntimeApp, create_client_app, create_headless_app, run_client_frame, run_client_frames,
    run_headless_frame, run_headless_frames,
};

pub mod client {
    use super::{RuntimeApp, create_client_app, run_client_frame, run_client_frames};

    pub fn create_app() -> RuntimeApp {
        create_client_app()
    }

    pub fn run_frame(delta_seconds: f64) -> RuntimeApp {
        run_client_frame(delta_seconds)
    }

    pub fn run_frames(frame_count: u64) -> RuntimeApp {
        run_client_frames(frame_count)
    }
}

pub mod headless {
    use super::{RuntimeApp, create_headless_app, run_headless_frame, run_headless_frames};

    pub fn create_app() -> RuntimeApp {
        create_headless_app()
    }

    pub fn run_frame(delta_seconds: f64) -> RuntimeApp {
        run_headless_frame(delta_seconds)
    }

    pub fn run_frames(frame_count: u64) -> RuntimeApp {
        run_headless_frames(frame_count)
    }
}
