use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;

use hecs::{Entity, World};

use crate::components::{ActiveCamera, Camera, DirectionalLight, Name, Transform, Velocity};
use crate::profile_span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    Client,
    Headless,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    UnknownArgument(String),
    MissingArgument(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeConfig {
    pub mode: EngineMode,
    pub fixed_timestep_seconds: f64,
    pub assets_root: PathBuf,
    pub scene_path: Option<PathBuf>,
}

impl RuntimeConfig {
    pub fn client() -> Self {
        Self {
            mode: EngineMode::Client,
            fixed_timestep_seconds: 1.0 / 60.0,
            assets_root: PathBuf::from("assets"),
            scene_path: None,
        }
    }

    pub fn headless() -> Self {
        Self {
            mode: EngineMode::Headless,
            fixed_timestep_seconds: 1.0 / 60.0,
            assets_root: PathBuf::from("assets"),
            scene_path: None,
        }
    }

    pub fn default_scene_path(&self) -> PathBuf {
        self.scene_path
            .clone()
            .unwrap_or_else(|| self.assets_root.join("sample/scene.gltf"))
    }

    pub fn from_args<I>(args: I) -> Result<Self, RuntimeError>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let mut config = Self::client();
        let mut args = args.into_iter().skip(1).peekable();

        while let Some(arg) = args.next() {
            match arg.as_ref() {
                "--headless" => config.mode = EngineMode::Headless,
                "--assets" => {
                    let value = args.next().ok_or_else(|| {
                        RuntimeError::MissingArgument("expected path after --assets".to_string())
                    })?;
                    config.assets_root = PathBuf::from(value.as_ref());
                }
                "--scene" => {
                    let value = args.next().ok_or_else(|| {
                        RuntimeError::MissingArgument("expected path after --scene".to_string())
                    })?;
                    config.scene_path = Some(PathBuf::from(value.as_ref()));
                }
                unknown => return Err(RuntimeError::UnknownArgument(unknown.to_string())),
            }
        }

        Ok(config)
    }
}

#[derive(Default)]
pub struct SimulationCore {
    frame: u64,
    world: World,
    resources: HashMap<TypeId, Box<dyn Any>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationPhase {
    PreUpdate,
    Update,
    PostUpdate,
}

impl SimulationPhase {
    pub const fn ordered() -> [Self; 3] {
        [Self::PreUpdate, Self::Update, Self::PostUpdate]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootstrapSceneResult {
    pub active_camera: Entity,
}

impl core::fmt::Debug for SimulationCore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SimulationCore")
            .field("frame", &self.frame)
            .finish_non_exhaustive()
    }
}

impl SimulationCore {
    pub fn new() -> Self {
        let mut simulation = Self::default();
        simulation.insert_resource(DirectionalLight::default());
        simulation
    }

    /// Spawns an entity with a `Transform` and `Velocity`, returning its
    /// `hecs::Entity` handle.
    pub fn spawn(&mut self, transform: Transform, velocity: Velocity) -> Entity {
        self.world.spawn((transform, velocity))
    }

    pub fn bootstrap_scene(&mut self) -> Entity {
        self.world.spawn((
            Name::new("MainCamera"),
            ActiveCamera,
            Camera::default(),
            Transform::default(),
        ))
    }

    pub fn bootstrap_scene_with_summary(&mut self) -> BootstrapSceneResult {
        let active_camera = self.bootstrap_scene();

        BootstrapSceneResult { active_camera }
    }

    pub fn active_camera(&self) -> Option<Entity> {
        self.world
            .query::<&ActiveCamera>()
            .iter()
            .map(|(entity, _)| entity)
            .next()
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn insert_resource<T>(&mut self, resource: T) -> Option<T>
    where
        T: 'static,
    {
        self.resources
            .insert(TypeId::of::<T>(), Box::new(resource))
            .and_then(|previous| previous.downcast::<T>().ok())
            .map(|previous| *previous)
    }

    pub fn resource<T>(&self) -> Option<&T>
    where
        T: 'static,
    {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|resource| resource.downcast_ref::<T>())
    }

    pub fn resource_mut<T>(&mut self) -> Option<&mut T>
    where
        T: 'static,
    {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|resource| resource.downcast_mut::<T>())
    }

    pub fn run_phase(&mut self, phase: SimulationPhase, fixed_timestep_seconds: f64) {
        profile_span!("sim_phase");
        match phase {
            SimulationPhase::PreUpdate => {}
            SimulationPhase::Update => {
                profile_span!("sim_update_movement");
                for (_entity, (transform, velocity)) in
                    self.world.query_mut::<(&mut Transform, &Velocity)>()
                {
                    transform.translation += velocity.0 * fixed_timestep_seconds as f32;
                }
            }
            SimulationPhase::PostUpdate => {}
        }
    }

    /// Advances the simulation by one fixed step: runs the movement system
    /// over every entity with `Transform` and `Velocity`, then increments
    /// the frame counter.
    pub fn tick(&mut self, fixed_timestep_seconds: f64) {
        profile_span!("sim_tick");
        for phase in SimulationPhase::ordered() {
            self.run_phase(phase, fixed_timestep_seconds);
        }

        self.frame += 1;
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }
}

#[derive(Debug)]
pub struct RuntimeApp {
    config: RuntimeConfig,
    simulation: SimulationCore,
    running: bool,
    accumulated_time_seconds: f64,
    frame_timing_history: crate::profiling::FrameTimingHistory,
    #[cfg(feature = "frame-alloc")]
    frame_allocator: crate::frame_alloc::FrameAllocator,
}

impl RuntimeApp {
    pub fn from_args<I>(args: I) -> Result<Self, RuntimeError>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let config = RuntimeConfig::from_args(args)?;

        Ok(Self {
            config,
            simulation: SimulationCore::new(),
            running: false,
            accumulated_time_seconds: 0.0,
            frame_timing_history: crate::profiling::FrameTimingHistory::default(),
            #[cfg(feature = "frame-alloc")]
            frame_allocator: crate::frame_alloc::FrameAllocator::new(),
        })
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn resume(&mut self) {
        self.start();
    }

    pub fn shutdown(&mut self) {
        self.running = false;
    }

    pub fn pause(&mut self) {
        self.shutdown();
    }

    pub fn run_steps(&mut self, steps: u64) {
        let fixed_timestep_seconds = self.config.fixed_timestep_seconds;
        for _ in 0..steps {
            self.simulation.tick(fixed_timestep_seconds);
        }
    }

    pub fn step(&mut self) {
        self.run_steps(1);
    }

    pub fn run_for_seconds(&mut self, delta_seconds: f64) {
        if !self.running || delta_seconds <= 0.0 {
            return;
        }

        self.accumulated_time_seconds += delta_seconds;

        let fixed_timestep_seconds = self.config.fixed_timestep_seconds;
        let epsilon = f64::EPSILON * 16.0;
        while self.accumulated_time_seconds >= fixed_timestep_seconds {
            self.simulation.tick(fixed_timestep_seconds);
            self.accumulated_time_seconds -= fixed_timestep_seconds;
        }

        if self.accumulated_time_seconds.abs() <= epsilon {
            self.accumulated_time_seconds = 0.0;
        }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn simulation(&self) -> &SimulationCore {
        &self.simulation
    }

    pub fn simulation_mut(&mut self) -> &mut SimulationCore {
        &mut self.simulation
    }

    pub fn active_camera(&self) -> Option<Entity> {
        self.simulation.active_camera()
    }

    pub fn bootstrap_scene(&mut self) -> Entity {
        self.simulation.bootstrap_scene()
    }

    pub fn bootstrap_scene_with_summary(&mut self) -> BootstrapSceneResult {
        self.simulation.bootstrap_scene_with_summary()
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn accumulated_time_seconds(&self) -> f64 {
        self.accumulated_time_seconds
    }

    /// The rolling per-frame timing history (input/sim/render/present).
    ///
    /// Always available — the lightweight metrics layer (M9.1) is on by design.
    /// The editor overlay reads this for its frame-time graph; the benchmark
    /// harness reads it for regression checks.
    pub fn frame_timing_history(&self) -> &crate::profiling::FrameTimingHistory {
        &self.frame_timing_history
    }

    /// Mutable access to the timing history, for loop integrators that push
    /// per-frame samples and for tests that want to reset/inspect the buffer.
    pub fn frame_timing_history_mut(&mut self) -> &mut crate::profiling::FrameTimingHistory {
        &mut self.frame_timing_history
    }

    /// The per-frame transient bump allocator (M9.4). Only present when the
    /// `frame-alloc` feature is enabled. Gameplay systems borrow this for
    /// per-frame scratch (transient render commands, query results) instead of
    /// hitting the global heap; the main loop resets it once per frame.
    #[cfg(feature = "frame-alloc")]
    pub fn frame_allocator(&self) -> &crate::frame_alloc::FrameAllocator {
        &self.frame_allocator
    }

    /// Mutable access to the frame allocator, for systems that allocate into it.
    #[cfg(feature = "frame-alloc")]
    pub fn frame_allocator_mut(&mut self) -> &mut crate::frame_alloc::FrameAllocator {
        &mut self.frame_allocator
    }

    /// Reset the frame allocator for a new frame. Called by the main loop at the
    /// top of each iteration; also safe to call manually (e.g. in tests). No-op
    /// when the `frame-alloc` feature is off.
    #[cfg(feature = "frame-alloc")]
    pub fn reset_frame_allocator(&mut self) {
        self.frame_allocator.reset();
    }

    /// No-op when the `frame-alloc` feature is disabled, so loop code doesn't
    /// need cfg guards at every call site.
    #[cfg(not(feature = "frame-alloc"))]
    pub fn reset_frame_allocator(&mut self) {}

    pub fn update(&mut self, delta_seconds: f64) -> u64 {
        if !self.running || delta_seconds <= 0.0 {
            return 0;
        }

        self.accumulated_time_seconds += delta_seconds;

        let fixed_timestep_seconds = self.config.fixed_timestep_seconds;
        let epsilon = f64::EPSILON * 16.0;
        let mut completed_steps = 0;

        while self.accumulated_time_seconds >= fixed_timestep_seconds {
            self.simulation.tick(fixed_timestep_seconds);
            self.accumulated_time_seconds -= fixed_timestep_seconds;
            completed_steps += 1;
        }

        if self.accumulated_time_seconds.abs() <= epsilon {
            self.accumulated_time_seconds = 0.0;
        }

        completed_steps
    }
}

pub fn create_client_app() -> RuntimeApp {
    RuntimeApp::from_args(["pie_runtime"]).expect("client runtime config should be valid")
}

pub fn create_headless_app() -> RuntimeApp {
    RuntimeApp::from_args(["pie_runtime", "--headless"])
        .expect("headless runtime config should be valid")
}

pub fn run_runtime_frame(app: &mut RuntimeApp, delta_seconds: f64) -> u64 {
    app.start();
    let completed_steps = app.update(delta_seconds);
    app.shutdown();
    completed_steps
}

pub fn run_client_frame(delta_seconds: f64) -> RuntimeApp {
    let mut app = create_client_app();
    run_runtime_frame(&mut app, delta_seconds);
    app
}

pub fn run_headless_frame(delta_seconds: f64) -> RuntimeApp {
    let mut app = create_headless_app();
    run_runtime_frame(&mut app, delta_seconds);
    app
}

pub fn run_runtime_frames(app: &mut RuntimeApp, frame_count: u64) -> u64 {
    let delta_seconds = app.config().fixed_timestep_seconds * frame_count as f64;
    run_runtime_frame(app, delta_seconds)
}

pub fn run_client_frames(frame_count: u64) -> RuntimeApp {
    let mut app = create_client_app();
    run_runtime_frames(&mut app, frame_count);
    app
}

pub fn run_headless_frames(frame_count: u64) -> RuntimeApp {
    let mut app = create_headless_app();
    run_runtime_frames(&mut app, frame_count);
    app
}

#[cfg(test)]
mod tests {
    use super::{
        EngineMode, RuntimeApp, RuntimeConfig, RuntimeError, SimulationCore, SimulationPhase,
        create_client_app, create_headless_app, run_client_frame, run_client_frames,
        run_headless_frame, run_headless_frames, run_runtime_frame, run_runtime_frames,
    };
    use crate::components::DirectionalLight;
    use crate::{ActiveCamera, Name, Transform, Velocity};
    use glam::Vec3;
    use std::path::PathBuf;

    #[test]
    fn runtime_config_supports_assets_and_scene_flags() {
        let config = RuntimeConfig::from_args([
            "pie_runtime",
            "--assets",
            "content",
            "--scene",
            "content/demo/scene.gltf",
        ])
        .expect("assets and scene args should parse");

        assert_eq!(config.assets_root, PathBuf::from("content"));
        assert_eq!(
            config.scene_path,
            Some(PathBuf::from("content/demo/scene.gltf"))
        );
        assert_eq!(
            config.default_scene_path(),
            PathBuf::from("content/demo/scene.gltf")
        );
    }

    #[test]
    fn runtime_config_reports_missing_assets_path() {
        let error = RuntimeConfig::from_args(["pie_runtime", "--assets"])
            .expect_err("missing assets path should fail");

        assert_eq!(
            error,
            RuntimeError::MissingArgument("expected path after --assets".to_string())
        );
    }

    #[test]
    fn client_config_uses_client_mode() {
        let config = RuntimeConfig::client();

        assert_eq!(config.mode, EngineMode::Client);
        assert_eq!(config.fixed_timestep_seconds, 1.0 / 60.0);
    }

    #[test]
    fn headless_config_uses_headless_mode() {
        let config = RuntimeConfig::headless();

        assert_eq!(config.mode, EngineMode::Headless);
        assert_eq!(config.fixed_timestep_seconds, 1.0 / 60.0);
    }

    #[test]
    fn runtime_config_defaults_to_client_mode_from_args() {
        let config =
            RuntimeConfig::from_args(["pie_runtime"]).expect("default runtime args should parse");

        assert_eq!(config.mode, EngineMode::Client);
        assert_eq!(config.fixed_timestep_seconds, 1.0 / 60.0);
    }

    #[test]
    fn runtime_config_supports_headless_flag_from_args() {
        let config = RuntimeConfig::from_args(["pie_runtime", "--headless"])
            .expect("headless runtime args should parse");

        assert_eq!(config.mode, EngineMode::Headless);
        assert_eq!(config.fixed_timestep_seconds, 1.0 / 60.0);
    }

    #[test]
    fn runtime_config_rejects_unknown_flags_from_args() {
        let error = RuntimeConfig::from_args(["pie_runtime", "--unknown"])
            .expect_err("unknown runtime args should be rejected");

        assert_eq!(
            error,
            RuntimeError::UnknownArgument("--unknown".to_string())
        );
    }

    #[test]
    fn simulation_core_advances_frames() {
        let mut core = SimulationCore::new();

        core.tick(1.0 / 60.0);
        core.tick(1.0 / 60.0);

        assert_eq!(core.frame(), 2);
    }

    #[test]
    fn simulation_core_moves_entities_with_velocity_each_tick() {
        let mut core = SimulationCore::new();
        let entity = core.spawn(
            Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)),
            Velocity(Vec3::new(2.0, 0.0, -4.0)),
        );

        core.tick(0.5);

        let transform = core
            .world()
            .get::<&Transform>(entity)
            .expect("spawned entity should still have a transform");

        assert_eq!(transform.translation, Vec3::new(2.0, 2.0, 1.0));
        assert_eq!(core.frame(), 1);
    }

    #[test]
    fn simulation_core_bootstrap_scene_spawns_active_camera() {
        let mut core = SimulationCore::new();

        let camera = core.bootstrap_scene();

        let camera_name = core
            .world()
            .get::<&Name>(camera)
            .expect("bootstrapped camera should have a name");
        let _active_camera = core
            .world()
            .get::<&ActiveCamera>(camera)
            .expect("bootstrapped camera should be marked active");
        let camera_transform = core
            .world()
            .get::<&Transform>(camera)
            .expect("bootstrapped camera should have a transform");

        assert_eq!(camera_name.0.as_str(), "MainCamera");
        assert_eq!(camera_transform.translation, Vec3::ZERO);
    }

    #[test]
    fn simulation_core_active_camera_returns_bootstrapped_camera_entity() {
        let mut core = SimulationCore::new();

        let bootstrapped_camera = core.bootstrap_scene();

        assert_eq!(core.active_camera(), Some(bootstrapped_camera));
    }

    #[test]
    fn simulation_core_active_camera_returns_none_when_scene_has_no_camera() {
        let core = SimulationCore::new();

        assert_eq!(core.active_camera(), None);
    }

    #[test]
    fn simulation_phase_order_is_pre_update_update_post_update() {
        assert_eq!(
            SimulationPhase::ordered(),
            [
                SimulationPhase::PreUpdate,
                SimulationPhase::Update,
                SimulationPhase::PostUpdate,
            ]
        );
    }

    #[test]
    fn simulation_core_runs_requested_phase_without_advancing_frame() {
        let mut core = SimulationCore::new();

        core.run_phase(SimulationPhase::PreUpdate, 1.0 / 60.0);
        core.run_phase(SimulationPhase::Update, 1.0 / 60.0);
        core.run_phase(SimulationPhase::PostUpdate, 1.0 / 60.0);

        assert_eq!(core.frame(), 0);
    }

    #[test]
    fn simulation_core_returns_none_for_missing_resource() {
        let core = SimulationCore::new();

        assert_eq!(core.resource::<u32>(), None);
    }

    #[test]
    fn simulation_core_stores_and_reads_typed_resource() {
        let mut core = SimulationCore::new();

        let previous = core.insert_resource::<u32>(7);

        assert_eq!(previous, None);
        assert_eq!(core.resource::<u32>(), Some(&7));
    }

    #[test]
    fn simulation_core_starts_with_default_directional_light() {
        let core = SimulationCore::new();

        let light = core
            .resource::<DirectionalLight>()
            .expect("default directional light should exist");

        assert!((light.direction.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn simulation_core_insert_resource_returns_previous_value() {
        let mut core = SimulationCore::new();

        core.insert_resource::<u32>(7);
        let previous = core.insert_resource::<u32>(11);

        assert_eq!(previous, Some(7));
        assert_eq!(core.resource::<u32>(), Some(&11));
    }

    #[test]
    fn simulation_core_resource_mut_updates_stored_value() {
        let mut core = SimulationCore::new();
        core.insert_resource::<u32>(7);

        let counter = core
            .resource_mut::<u32>()
            .expect("resource should be present after insertion");
        *counter += 1;

        assert_eq!(core.resource::<u32>(), Some(&8));
    }

    #[test]
    fn simulation_core_bootstrap_scene_summary_reports_active_camera() {
        let mut core = SimulationCore::new();

        let scene = core.bootstrap_scene_with_summary();

        assert_eq!(
            scene.active_camera,
            core.active_camera().expect("camera should exist")
        );
    }

    #[test]
    fn runtime_app_defaults_to_client_mode() {
        let app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        assert_eq!(app.config().mode, EngineMode::Client);
    }

    #[test]
    fn runtime_app_supports_headless_flag() {
        let app = RuntimeApp::from_args(["pie_runtime", "--headless"])
            .expect("headless args should parse");

        assert_eq!(app.config().mode, EngineMode::Headless);
    }

    #[test]
    fn runtime_app_rejects_unknown_flags() {
        let error = RuntimeApp::from_args(["pie_runtime", "--unknown"])
            .expect_err("unknown flags should be rejected");

        assert_eq!(
            error,
            RuntimeError::UnknownArgument("--unknown".to_string())
        );
    }

    #[test]
    fn runtime_app_advances_simulation_by_requested_steps() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.run_steps(3);

        assert_eq!(app.simulation().frame(), 3);
    }

    #[test]
    fn runtime_app_active_camera_returns_bootstrapped_camera_entity() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        let camera = app.simulation_mut().bootstrap_scene();

        assert_eq!(app.active_camera(), Some(camera));
    }

    #[test]
    fn runtime_app_active_camera_returns_none_without_bootstrap() {
        let app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        assert_eq!(app.active_camera(), None);
    }

    #[test]
    fn runtime_app_bootstrap_scene_returns_bootstrapped_camera_entity() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        let camera = app.bootstrap_scene();

        assert_eq!(app.active_camera(), Some(camera));
    }

    #[test]
    fn runtime_app_bootstrap_scene_spawns_camera_components() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        let camera = app.bootstrap_scene();

        let camera_name = app
            .simulation()
            .world()
            .get::<&Name>(camera)
            .expect("bootstrapped camera should have a name");
        let _active_camera = app
            .simulation()
            .world()
            .get::<&ActiveCamera>(camera)
            .expect("bootstrapped camera should be marked active");
        let camera_transform = app
            .simulation()
            .world()
            .get::<&Transform>(camera)
            .expect("bootstrapped camera should have a transform");

        assert_eq!(camera_name.0.as_str(), "MainCamera");
        assert_eq!(camera_transform.translation, Vec3::ZERO);
    }

    #[test]
    fn runtime_app_bootstrap_scene_summary_reports_active_camera() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        let scene = app.bootstrap_scene_with_summary();

        assert_eq!(
            scene.active_camera,
            app.active_camera().expect("camera should exist")
        );
    }

    #[test]
    fn runtime_app_simulation_mut_allows_spawning_entities_into_world() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");
        let entity = app
            .simulation_mut()
            .spawn(Transform::default(), Velocity(Vec3::new(0.0, 3.0, 0.0)));

        app.run_steps(2);

        let transform = app
            .simulation()
            .world()
            .get::<&Transform>(entity)
            .expect("spawned entity should still be present after stepping");

        assert!((transform.translation.y - 0.1).abs() <= f32::EPSILON);
        assert_eq!(transform.translation.x, 0.0);
        assert_eq!(transform.translation.z, 0.0);
    }

    #[test]
    fn runtime_app_starts_and_stops_cleanly() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        assert!(!app.is_running());

        app.start();
        assert!(app.is_running());

        app.shutdown();
        assert!(!app.is_running());
    }

    #[test]
    fn runtime_app_pause_and_resume_toggle_running_state() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.resume();
        assert!(app.is_running());

        app.pause();
        assert!(!app.is_running());
    }

    #[test]
    fn runtime_app_step_advances_a_single_fixed_frame() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.step();

        assert_eq!(app.simulation().frame(), 1);
    }

    #[test]
    fn runtime_app_ignores_time_when_stopped() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.run_for_seconds(0.5);

        assert_eq!(app.accumulated_time_seconds(), 0.0);
        assert_eq!(app.simulation().frame(), 0);
    }

    #[test]
    fn runtime_app_accumulates_partial_frame_time_when_running() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.start();
        app.run_for_seconds(0.5 / 60.0);

        assert_eq!(app.simulation().frame(), 0);
        assert_eq!(app.accumulated_time_seconds(), 0.5 / 60.0);
    }

    #[test]
    fn runtime_app_advances_fixed_steps_from_elapsed_time() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.start();
        app.run_for_seconds((1.0 / 60.0) * 3.0);

        assert_eq!(app.simulation().frame(), 3);
        assert_eq!(app.accumulated_time_seconds(), 0.0);
    }

    #[test]
    fn create_client_app_uses_client_mode() {
        let app = create_client_app();

        assert_eq!(app.config().mode, EngineMode::Client);
    }

    #[test]
    fn create_headless_app_uses_headless_mode() {
        let app = create_headless_app();

        assert_eq!(app.config().mode, EngineMode::Headless);
    }

    #[test]
    fn runtime_app_update_returns_zero_when_not_running() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        let steps = app.update(1.0 / 60.0);

        assert_eq!(steps, 0);
        assert_eq!(app.simulation().frame(), 0);
    }

    #[test]
    fn runtime_app_update_returns_completed_fixed_steps() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.start();
        let steps = app.update((1.0 / 60.0) * 2.0);

        assert_eq!(steps, 2);
        assert_eq!(app.simulation().frame(), 2);
    }

    #[test]
    fn runtime_app_update_preserves_partial_time_remainder() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        app.start();
        let steps = app.update(1.5 / 60.0);

        assert_eq!(steps, 1);
        assert_eq!(app.simulation().frame(), 1);
        assert!((app.accumulated_time_seconds() - (0.5 / 60.0)).abs() <= f64::EPSILON);
    }

    #[test]
    fn run_runtime_frame_starts_updates_and_stops_app() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        let steps = run_runtime_frame(&mut app, (1.0 / 60.0) * 2.0);

        assert_eq!(steps, 2);
        assert_eq!(app.simulation().frame(), 2);
        assert!(!app.is_running());
        assert_eq!(app.accumulated_time_seconds(), 0.0);
    }

    #[test]
    fn run_client_frame_uses_client_mode_and_updates_once() {
        let app = run_client_frame(1.0 / 60.0);

        assert_eq!(app.config().mode, EngineMode::Client);
        assert_eq!(app.simulation().frame(), 1);
        assert!(!app.is_running());
    }

    #[test]
    fn run_headless_frame_uses_headless_mode_and_updates_once() {
        let app = run_headless_frame(1.0 / 60.0);

        assert_eq!(app.config().mode, EngineMode::Headless);
        assert_eq!(app.simulation().frame(), 1);
        assert!(!app.is_running());
    }

    #[test]
    fn run_runtime_frames_advances_requested_frame_count() {
        let mut app = RuntimeApp::from_args(["pie_runtime"]).expect("default args should parse");

        let steps = run_runtime_frames(&mut app, 3);

        assert_eq!(steps, 3);
        assert_eq!(app.simulation().frame(), 3);
        assert!(!app.is_running());
    }

    #[test]
    fn run_client_frames_uses_client_mode_and_updates_requested_frames() {
        let app = run_client_frames(2);

        assert_eq!(app.config().mode, EngineMode::Client);
        assert_eq!(app.simulation().frame(), 2);
        assert!(!app.is_running());
    }

    #[test]
    fn run_headless_frames_uses_headless_mode_and_updates_requested_frames() {
        let app = run_headless_frames(4);

        assert_eq!(app.config().mode, EngineMode::Headless);
        assert_eq!(app.simulation().frame(), 4);
        assert!(!app.is_running());
    }

    #[test]
    fn client_platform_module_creates_client_mode_app() {
        let app = crate::platform::client::create_app();

        assert_eq!(app.config().mode, EngineMode::Client);
    }

    #[test]
    fn client_platform_module_runs_requested_frames() {
        let app = crate::platform::client::run_frames(3);

        assert_eq!(app.config().mode, EngineMode::Client);
        assert_eq!(app.simulation().frame(), 3);
        assert!(!app.is_running());
    }

    #[test]
    fn headless_platform_module_creates_headless_mode_app() {
        let app = crate::platform::headless::create_app();

        assert_eq!(app.config().mode, EngineMode::Headless);
    }

    #[test]
    fn headless_platform_module_runs_requested_frames() {
        let app = crate::platform::headless::run_frames(5);

        assert_eq!(app.config().mode, EngineMode::Headless);
        assert_eq!(app.simulation().frame(), 5);
        assert!(!app.is_running());
    }

    #[test]
    fn logging_init_marks_runtime_logging_as_initialized() {
        let did_initialize = crate::logging::init();

        assert!(did_initialize || crate::logging::is_initialized());
        assert!(crate::logging::is_initialized());
    }

    #[test]
    fn logging_init_is_idempotent() {
        crate::logging::init();
        let did_initialize = crate::logging::init();

        assert!(!did_initialize);
        assert!(crate::logging::is_initialized());
    }
}
