pub mod assets;
pub mod components;
pub mod core;
pub mod logging;
pub mod loop_runner;
pub mod platform;
#[cfg(feature = "rendering")]
pub mod rendering;

pub use assets::{
    AssetError, AssetRegistry, Handle, ImportedScene, MaterialAsset, MaterialHandle, MeshAsset,
    MeshHandle, MeshVertex, SpawnedScene, TextureAsset, TextureHandle, load_gltf_scene,
    load_shader_named, load_shader_source, load_texture_from_path, spawn_imported_scene,
};
pub use components::{ActiveCamera, MeshRenderer, Name, Transform, Velocity};
pub use core::{
    BootstrapSceneResult, EngineMode, RuntimeApp, RuntimeConfig, RuntimeError, SimulationCore,
    SimulationPhase, create_client_app, create_headless_app, run_client_frame, run_client_frames,
    run_headless_frame, run_headless_frames, run_runtime_frame, run_runtime_frames,
};
pub use logging::{init as init_logging, is_initialized as is_logging_initialized};
pub use loop_runner::{
    MainLoopLimits, StopSignal, TimeSource, WallClock, install_ctrlc_handler, run_main_loop,
    run_main_loop_with_time_source,
};
#[cfg(feature = "rendering")]
pub use rendering::run_client_window;
