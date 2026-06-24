use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::assets::{AssetRegistry, load_gltf_scene, spawn_imported_scene};
use crate::core::RuntimeApp;
use crate::rendering::loading_screen::LoadingScreen;
use crate::rendering::renderer::Renderer;
use crate::rendering::sample_scene::bootstrap_fallback_render_scene;

const MAX_FRAME_DELTA_SECONDS: f64 = 0.25;

pub struct ClientScene {
    pub registry: AssetRegistry,
}

pub fn load_client_scene(app: &mut RuntimeApp) -> Result<ClientScene, String> {
    let scene_path = app.config().default_scene_path();
    let mut registry = AssetRegistry::new();

    match load_gltf_scene(&scene_path, &mut registry) {
        Ok(imported) => {
            spawn_imported_scene(app.simulation_mut(), &imported);
        }
        Err(error) => {
            eprintln!(
                "pie_runtime: failed to load scene at {}: {error}; using built-in fallback scene",
                scene_path.display()
            );
            bootstrap_fallback_render_scene(app.simulation_mut(), &mut registry);
        }
    }

    Ok(ClientScene { registry })
}

/// The client application lifecycle phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientPhase {
    /// Showing the loading screen while the scene and renderer initialize.
    Loading,
    /// Fully initialized — rendering the scene normally.
    Running,
    /// Scene load failed too many times — stop retrying and show the loading
    /// screen indefinitely (so the user sees something rather than a tight
    /// infinite-retry loop burning CPU).
    Fatal,
}

/// Maximum number of consecutive scene-load failures before transitioning to
/// the `Fatal` phase. Without this, a missing scene file would cause an
/// infinite retry loop (load fails → transition_to_running returns early →
/// next RedrawRequested → load fails again → …).
const MAX_SCENE_LOAD_ATTEMPTS: u32 = 3;

pub fn run_client_window(mut app: RuntimeApp) -> Result<(), String> {
    app.start();

    let assets_root = app.config().assets_root.clone();
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    let mut client = ClientApp {
        runtime: app,
        assets_root,
        phase: ClientPhase::Loading,
        window: None,
        loading_screen: None,
        renderer: None,
        scene: None,
        last_frame_instant: Instant::now(),
        scene_load_attempts: 0,
    };

    event_loop
        .run_app(&mut client)
        .map_err(|error| error.to_string())?;

    Ok(())
}

struct ClientApp {
    runtime: RuntimeApp,
    assets_root: std::path::PathBuf,
    phase: ClientPhase,
    window: Option<Arc<Window>>,
    loading_screen: Option<LoadingScreen>,
    renderer: Option<Renderer>,
    scene: Option<ClientScene>,
    last_frame_instant: Instant,
    /// Number of consecutive scene-load failures. Reset to 0 on success.
    /// When it reaches `MAX_SCENE_LOAD_ATTEMPTS`, the phase transitions to
    /// `Fatal` to avoid an infinite retry loop.
    scene_load_attempts: u32,
}

impl ApplicationHandler for ClientApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("Pie Engine")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("pie_runtime: failed to create window: {error}");
                return;
            }
        };

        // Create the loading screen — this sets up the GPU and shows a
        // progress indicator immediately, before loading any scene assets.
        // If GPU initialization fails (no adapter, no device, etc.), log
        // and bail — previously this .expect()ed and tore down the event
        // loop with no diagnostic.
        let loading_screen = match LoadingScreen::new(window.clone()) {
            Ok(ls) => ls,
            Err(error) => {
                eprintln!("pie_runtime: failed to initialize loading screen: {error}");
                return;
            }
        };

        self.window = Some(window);
        self.loading_screen = Some(loading_screen);
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.runtime.shutdown();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                match self.phase {
                    ClientPhase::Loading | ClientPhase::Fatal => {
                        if let Some(loading) = self.loading_screen.as_mut() {
                            loading.resize(size.width, size.height);
                        }
                    }
                    ClientPhase::Running => {
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.resize(size.width, size.height);
                        }
                    }
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta_seconds = (now - self.last_frame_instant)
                    .as_secs_f64()
                    .min(MAX_FRAME_DELTA_SECONDS);
                self.last_frame_instant = now;

                self.runtime.update(delta_seconds);

                match self.phase {
                    ClientPhase::Loading | ClientPhase::Fatal => {
                        self.render_loading_frame(event_loop)
                    }
                    ClientPhase::Running => self.render_scene_frame(event_loop),
                }

                self.request_redraw();
            }
            _ => {}
        }
    }
}

impl ClientApp {
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Render a loading screen frame. On the first call, this transitions
    /// to the running phase after loading the scene.
    fn render_loading_frame(&mut self, event_loop: &ActiveEventLoop) {
        // If we've already given up, just keep rendering the loading screen
        // without re-attempting the load (avoids an infinite retry loop).
        if self.phase == ClientPhase::Fatal {
            if let Some(loading) = self.loading_screen.as_mut() {
                loading.set_progress(0.0, "Scene load failed — check logs");
                let _ = loading.render();
            }
            return;
        }

        // Update loading screen progress
        if let Some(loading) = self.loading_screen.as_mut() {
            loading.set_progress(0.2, "Loading scene...");
            match loading.render() {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost) => {
                    if let Some(window) = self.window.as_ref() {
                        let size = window.inner_size();
                        loading.resize(size.width, size.height);
                    }
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    self.runtime.shutdown();
                    event_loop.exit();
                }
                Err(error) => {
                    eprintln!("pie_runtime: loading screen render error: {error:?}");
                }
            }
        }

        // Transition to running phase: load the scene and create the full renderer.
        // This happens after the first loading frame so the user sees the loading screen.
        self.transition_to_running();
    }

    /// Load the scene and create the full renderer, transitioning from the
    /// loading screen to the running phase.
    fn transition_to_running(&mut self) {
        // Load the scene
        let scene = match load_client_scene(&mut self.runtime) {
            Ok(scene) => scene,
            Err(error) => {
                self.scene_load_attempts += 1;
                eprintln!(
                    "pie_runtime: failed to load scene (attempt {}/{}): {error}",
                    self.scene_load_attempts, MAX_SCENE_LOAD_ATTEMPTS
                );
                if self.scene_load_attempts >= MAX_SCENE_LOAD_ATTEMPTS {
                    eprintln!(
                        "pie_runtime: giving up after {} failed scene-load attempts; \
                         transitioning to Fatal phase",
                        self.scene_load_attempts
                    );
                    self.phase = ClientPhase::Fatal;
                }
                return;
            }
        };

        // Update loading screen to show we're creating the renderer
        if let Some(loading) = self.loading_screen.as_mut() {
            loading.set_progress(0.8, "Initializing renderer...");
            let _ = loading.render();
        }

        // Create the full renderer (reuses the same window)
        let window = self
            .window
            .clone()
            .expect("window should exist during transition");
        let mut renderer = match Renderer::new(window, &self.assets_root) {
            Ok(r) => r,
            Err(error) => {
                self.scene_load_attempts += 1;
                eprintln!(
                    "pie_runtime: failed to create renderer (attempt {}/{}): {error}",
                    self.scene_load_attempts, MAX_SCENE_LOAD_ATTEMPTS
                );
                if self.scene_load_attempts >= MAX_SCENE_LOAD_ATTEMPTS {
                    self.phase = ClientPhase::Fatal;
                }
                return;
            }
        };

        if let Err(error) = renderer.load_scene(&scene.registry, self.runtime.simulation()) {
            eprintln!("pie_runtime: failed to load scene into renderer: {error}");
            self.scene_load_attempts += 1;
            if self.scene_load_attempts >= MAX_SCENE_LOAD_ATTEMPTS {
                self.phase = ClientPhase::Fatal;
            }
            return;
        }

        // Transition complete — reset the failure counter and switch to Running.
        self.scene_load_attempts = 0;
        self.loading_screen = None;
        self.renderer = Some(renderer);
        self.scene = Some(scene);
        self.phase = ClientPhase::Running;
    }

    /// Render a normal scene frame using the full renderer.
    fn render_scene_frame(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(renderer) = self.renderer.as_mut() {
            match renderer.render(self.runtime.simulation()) {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost) => {
                    if let Some(window) = self.window.as_ref() {
                        let size = window.inner_size();
                        renderer.resize(size.width, size.height);
                    }
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    self.runtime.shutdown();
                    event_loop.exit();
                }
                Err(error) => {
                    eprintln!("pie_runtime: render error: {error:?}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::load_client_scene;
    use crate::components::MeshRenderer;
    use crate::core::RuntimeApp;

    #[test]
    fn load_client_scene_falls_back_when_scene_file_is_missing() {
        let mut app =
            RuntimeApp::from_args(["pie_runtime", "--scene", "missing/sample/scene.gltf"])
                .expect("args should parse");

        let scene = load_client_scene(&mut app).expect("fallback scene should load");

        assert_eq!(scene.registry.meshes().len(), 1);
        assert!(app.active_camera().is_some());
        assert_eq!(
            app.simulation()
                .world()
                .query::<&MeshRenderer>()
                .iter()
                .count(),
            1
        );
    }
}
