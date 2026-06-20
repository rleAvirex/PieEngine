use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::assets::{AssetRegistry, load_gltf_scene, spawn_imported_scene};
use crate::core::RuntimeApp;
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

pub fn run_client_window(mut app: RuntimeApp) -> Result<(), String> {
    app.start();

    let scene = load_client_scene(&mut app)?;
    let assets_root = app.config().assets_root.clone();
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    let mut client = ClientApp {
        runtime: app,
        scene,
        assets_root,
        window: None,
        renderer: None,
        last_frame_instant: Instant::now(),
    };

    event_loop
        .run_app(&mut client)
        .map_err(|error| error.to_string())?;

    Ok(())
}

struct ClientApp {
    runtime: RuntimeApp,
    scene: ClientScene,
    assets_root: std::path::PathBuf,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    last_frame_instant: Instant,
}

impl ApplicationHandler for ClientApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("Pie Engine")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("window should be created"),
        );

        let mut renderer =
            Renderer::new(window.clone(), &self.assets_root).expect("renderer should initialize");
        renderer
            .load_scene(&self.scene.registry, self.runtime.simulation())
            .expect("scene assets should upload to GPU");

        self.window = Some(window);
        self.renderer = Some(renderer);
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
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
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
