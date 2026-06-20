//! Pie Editor — a visual scene editor built on the PieEngine runtime.

mod theme;
mod gizmo;
mod fly_camera;
mod picking;
mod viewport_renderer;
mod ui;

use std::env;
use std::num::NonZeroU32;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use egui::{Context, Key, ViewportId};
use egui_wgpu::WgpuConfiguration;
use egui_wgpu::winit::Painter;
use egui_winit::State as EguiWinitState;
use glam::{Vec2, Vec3};
use hecs::Entity;
use pie_runtime::assets::{
    AssetRegistry, load_fbx_meshes, load_gltf_scene, spawn_imported_scene,
};
use pie_runtime::components::{Camera, Transform};
use pie_runtime::core::RuntimeApp;
use pie_runtime::init_logging;
use pie_runtime::rendering::camera_view_proj;
use pie_runtime::rendering::sample_scene::bootstrap_fallback_render_scene;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window as WinitWindow, WindowId};

use fly_camera::{EditorCamera, SPEED_SCROLL_FACTOR};
use gizmo::{Axis, GizmoState, PickResult, GIZMO_WORLD_SCALE, gizmo_center_aabb, gizmo_shaft_aabb, gizmo_tip_aabb};
use picking::{PickableBounds, ray_aabb_hit, world_aabb, screen_ray_from_ndc, viewport_ndc_from_rect};
use viewport_renderer::{EditorViewportRenderer, EditorViewportTexture};
use ui::{EditorCommands, EditorSceneInfo, EditorUiParams, build_editor_ui};

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    init_logging();

    let args: Vec<String> = env::args().collect();
    let launch = match EditorLaunch::from_args(&args) {
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("pie_editor: failed to parse editor flags: {error}");
            return ExitCode::FAILURE;
        }
    };

    let runtime = match RuntimeApp::from_args(&launch.runtime_args) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("pie_editor: failed to start runtime: {error:?}");
            return ExitCode::FAILURE;
        }
    };

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("pie_editor: failed to create event loop: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut editor = EditorApp::new(runtime, launch);
    match event_loop.run_app(&mut editor) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pie_editor: event loop failed: {error}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// Editor launch configuration
// ---------------------------------------------------------------------------

struct EditorLaunch {
    runtime_args: Vec<String>,
    step_count: Option<u64>,
}

impl EditorLaunch {
    fn from_args(args: &[String]) -> Result<Self, String> {
        let mut runtime_args = vec![
            args.first()
                .cloned()
                .unwrap_or_else(|| "pie_editor".to_string()),
        ];
        let mut step_count = None;
        let mut index = 1;

        while index < args.len() {
            match args[index].as_str() {
                "--step" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "expected step count after --step".to_string())?;
                    step_count = Some(
                        value
                            .parse::<u64>()
                            .map_err(|error| format!("invalid step count '{value}': {error}"))?,
                    );
                    index += 2;
                }
                "--paused" => {
                    index += 1;
                }
                other => {
                    runtime_args.push(other.to_string());
                    index += 1;
                }
            }
        }

        Ok(Self {
            runtime_args,
            step_count,
        })
    }
}

// ---------------------------------------------------------------------------
// Editor scene (asset registry + scene path)
// ---------------------------------------------------------------------------

struct EditorScene {
    registry: AssetRegistry,
    scene_path: std::path::PathBuf,
}

impl EditorScene {
    fn load(runtime: &mut RuntimeApp) -> Self {
        let scene_path = runtime.config().default_scene_path();
        let mut registry = AssetRegistry::new();

        match load_gltf_scene(&scene_path, &mut registry) {
            Ok(imported) => {
                spawn_imported_scene(runtime.simulation_mut(), &imported);
            }
            Err(error) => {
                eprintln!(
                    "pie_editor: failed to load scene at {}: {error}; using built-in fallback scene",
                    scene_path.display()
                );
                bootstrap_fallback_render_scene(runtime.simulation_mut(), &mut registry);
            }
        }

        // Load engine gizmo models from FBX into the asset registry.
        // Falls back to pre-converted .bin/.json pie_mesh files if FBX fails.
        // These meshes are NOT spawned as scene entities — they are used by the
        // gizmo overlay renderer via EditorViewportRenderer::load_fbx_gizmos().
        let engine_assets = runtime.config().assets_root.join("Engine/Gizmos");
        if engine_assets.exists() {
            for fbx_name in &["GizmosMoveTool", "GizmosSphere"] {
                let fbx_path = engine_assets.join(format!("{fbx_name}.fbx"));
                if fbx_path.exists() {
                    match load_fbx_meshes(&fbx_path, &mut registry) {
                        Ok(handles) => {
                            eprintln!(
                                "pie_editor: loaded {} FBX gizmo mesh(es) from {}",
                                handles.len(),
                                fbx_path.display()
                            );
                        }
                        Err(error) => {
                            // FBX failed — try the pre-converted .bin file
                            let bin_path = engine_assets.join(format!("{fbx_name}.bin"));
                            if bin_path.exists() {
                                match pie_runtime::assets::load_pie_mesh(&bin_path, &mut registry) {
                                    Ok(_) => eprintln!(
                                        "pie_editor: loaded fallback pie_mesh gizmo from {}",
                                        bin_path.display()
                                    ),
                                    Err(bin_err) => eprintln!(
                                        "pie_editor: warning: FBX and pie_mesh both failed for {}: {error}, {bin_err}",
                                        fbx_name
                                    ),
                                }
                            } else {
                                eprintln!(
                                    "pie_editor: warning: failed to load gizmo {}: {error}",
                                    fbx_name
                                );
                            }
                        }
                    }
                }
            }
        }

        Self {
            registry,
            scene_path,
        }
    }

    fn reload(&mut self, runtime: &mut RuntimeApp) {
        *self = Self::load(runtime);
    }
}

// ---------------------------------------------------------------------------
// Editor application state
// ---------------------------------------------------------------------------

struct EditorApp {
    runtime: RuntimeApp,
    launch: EditorLaunch,
    scene: EditorScene,
    window: Option<Arc<WinitWindow>>,
    egui_ctx: Context,
    egui_state: Option<EguiWinitState>,
    painter: Option<Painter>,
    viewport_renderer: Option<EditorViewportRenderer>,
    viewport_texture: Option<EditorViewportTexture>,
    last_frame_instant: Instant,
    smoothed_delta: f64,
    selected_entity: Option<Entity>,
    editor_camera: EditorCamera,
    pickables: Vec<PickableBounds>,
    viewport_hovered: bool,
    gizmo_state: GizmoState,
    hovered_axis: Option<Axis>,
    hovered_center: bool,
}

impl EditorApp {
    fn new(mut runtime: RuntimeApp, launch: EditorLaunch) -> Self {
        let scene = EditorScene::load(&mut runtime);
        let selected_entity = runtime
            .simulation()
            .world()
            .query::<&Transform>()
            .iter()
            .map(|(entity, _)| entity)
            .next();

        if let Some(step_count) = launch.step_count {
            for _ in 0..step_count {
                runtime.step();
            }
        }

        runtime.pause();

        let (editor_camera, pickables) = Self::init_camera_and_pickables(&runtime, &scene);

        let egui_ctx = Context::default();
        theme::apply(&egui_ctx);

        Self {
            runtime,
            launch,
            scene,
            window: None,
            egui_ctx,
            egui_state: None,
            painter: None,
            viewport_renderer: None,
            viewport_texture: None,
            last_frame_instant: Instant::now(),
            smoothed_delta: 1.0 / 60.0,
            selected_entity,
            editor_camera,
            pickables,
            viewport_hovered: false,
            gizmo_state: GizmoState::default(),
            hovered_axis: None,
            hovered_center: false,
        }
    }

    fn ensure_window(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.window.is_some() {
            return Ok(());
        }

        let window_attributes = WinitWindow::default_attributes()
            .with_title("Pie Editor")
            .with_inner_size(winit::dpi::LogicalSize::new(1440, 900));

        let window = Arc::new(event_loop.create_window(window_attributes)?);
        let egui_state = EguiWinitState::new(
            self.egui_ctx.clone(),
            ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let mut painter = pollster::block_on(Painter::new(
            self.egui_ctx.clone(),
            WgpuConfiguration::default(),
            1,
            None,
            true,
            false,
        ));
        pollster::block_on(painter.set_window(ViewportId::ROOT, Some(window.clone())))?;

        self.window = Some(window);
        self.egui_state = Some(egui_state);
        self.painter = Some(painter);
        self.ensure_viewport_renderer()?;
        Ok(())
    }

    fn load_scene(&mut self) {
        self.runtime = RuntimeApp::from_args(&self.launch.runtime_args)
            .expect("editor runtime args should be valid");
        self.scene.reload(&mut self.runtime);
        if let Some(viewport_renderer) = self.viewport_renderer.as_mut() {
            viewport_renderer
                .load_scene(&self.scene.registry, self.runtime.simulation())
                .expect("editor viewport scene should reload");
            viewport_renderer.load_fbx_gizmos(&self.scene.registry);
        }
        let (editor_camera, pickables) =
            Self::init_camera_and_pickables(&self.runtime, &self.scene);
        self.editor_camera = editor_camera;
        self.pickables = pickables;
        self.selected_entity = self
            .runtime
            .simulation()
            .world()
            .query::<&Transform>()
            .iter()
            .map(|(entity, _)| entity)
            .next();
        self.runtime.pause();
        self.request_redraw();
    }

    fn ensure_viewport_renderer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(painter) = self.painter.as_ref() else {
            return Ok(());
        };
        let Some(render_state) = painter.render_state() else {
            return Ok(());
        };

        if self.viewport_renderer.is_none() {
            let mut viewport_renderer =
                EditorViewportRenderer::new(&render_state, &self.runtime.config().assets_root)
                    .map_err(std::io::Error::other)?;
            viewport_renderer
                .load_scene(&self.scene.registry, self.runtime.simulation())
                .map_err(std::io::Error::other)?;
            viewport_renderer.load_fbx_gizmos(&self.scene.registry);
            self.viewport_renderer = Some(viewport_renderer);
        }

        let initial_size = self
            .window
            .as_ref()
            .map(|window| window.inner_size())
            .unwrap_or_default();
        if initial_size.width > 0 && initial_size.height > 0 {
            self.ensure_viewport_texture([initial_size.width, initial_size.height])?;
        }

        Ok(())
    }

    fn ensure_viewport_texture(
        &mut self,
        size: [u32; 2],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(painter) = self.painter.as_ref() else {
            return Ok(());
        };
        let Some(render_state) = painter.render_state() else {
            return Ok(());
        };

        if size[0] == 0 || size[1] == 0 {
            return Ok(());
        }

        if self
            .viewport_texture
            .as_ref()
            .is_some_and(|viewport_texture| viewport_texture.size == size)
        {
            return Ok(());
        }

        let device = &render_state.device;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("editor viewport texture"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        if let Some(existing) = self.viewport_texture.as_mut() {
            render_state
                .renderer
                .write()
                .update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Linear,
                    existing.texture_id,
                );
            existing.texture = texture;
            existing.view = view;
            existing.size = size;
        } else {
            let texture_id = render_state.renderer.write().register_native_texture(
                device,
                &view,
                wgpu::FilterMode::Linear,
            );
            self.viewport_texture = Some(EditorViewportTexture {
                texture,
                view,
                texture_id,
                size,
            });
        }

        Ok(())
    }

    fn render_viewport(&mut self) {
        let selection_aabb = self.selection_world_aabb();
        let gizmo_origin = self.selected_entity.and_then(|entity| {
            self.runtime
                .simulation()
                .world()
                .get::<&Transform>(entity)
                .ok()
                .map(|t| t.translation)
        });
        let Some(viewport_renderer) = self.viewport_renderer.as_mut() else {
            return;
        };
        let Some(viewport_texture) = self.viewport_texture.as_ref() else {
            return;
        };

        viewport_renderer.render_to_view(
            self.runtime.simulation(),
            &viewport_texture.view,
            viewport_texture.size,
            selection_aabb,
            gizmo_origin,
            self.hovered_axis,
            self.hovered_center,
            self.gizmo_state,
        );
    }

    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn init_camera_and_pickables(
        runtime: &RuntimeApp,
        scene: &EditorScene,
    ) -> (EditorCamera, Vec<PickableBounds>) {
        let editor_camera = if let Some(camera_entity) = runtime.simulation().active_camera() {
            let transform = runtime
                .simulation()
                .world()
                .get::<&Transform>(camera_entity)
                .ok()
                .map(|transform| *transform)
                .unwrap_or_default();
            EditorCamera::from_transform(transform)
        } else {
            EditorCamera::new(Vec3::new(0.0, 1.0, 5.0))
        };

        let mut pickables = Vec::new();
        for (entity, mesh_renderer) in runtime
            .simulation()
            .world()
            .query::<&pie_runtime::components::MeshRenderer>()
            .iter()
        {
            if let Ok(mesh) = scene.registry.mesh(mesh_renderer.mesh)
                && let Some((local_min, local_max)) = mesh.local_aabb()
            {
                    pickables.push(PickableBounds {
                        entity,
                        local_min,
                        local_max,
                    });
            }
        }

        (editor_camera, pickables)
    }

    fn pick_viewport(
        &self,
        ndc: Vec2,
        viewport_size: (f32, f32),
        gizmo_origin: Option<Vec3>,
    ) -> Option<PickResult> {
        if viewport_size.0 <= 0.0 || viewport_size.1 <= 0.0 {
            return None;
        }
        let aspect = viewport_size.0 / viewport_size.1;
        let camera_transform = self
            .runtime
            .simulation()
            .active_camera()
            .and_then(|entity| {
                self.runtime
                    .simulation()
                    .world()
                    .get::<&Transform>(entity)
                    .ok()
                    .map(|transform| *transform)
            })
            .unwrap_or_default();
        let fov = self
            .runtime
            .simulation()
            .active_camera()
            .and_then(|e| self.runtime.simulation().world().get::<&Camera>(e).ok())
            .map(|c| c.fov)
            .unwrap_or_else(|| Camera::default().fov);
        let view_proj = camera_view_proj(camera_transform, aspect, fov);
        let (ray_origin, ray_dir) = screen_ray_from_ndc(ndc, view_proj);

        let mut best_t = f32::INFINITY;
        let mut best_result = None;

        // Test gizmo center sphere first (highest priority — uniform scale handle).
        if let Some(origin) = gizmo_origin {
            let (c_min, c_max) = gizmo_center_aabb(origin, GIZMO_WORLD_SCALE);
            if let Some(t) = ray_aabb_hit(ray_origin, ray_dir, c_min, c_max) {
                best_t = t;
                best_result = Some(PickResult::GizmoCenter);
            }
        }

        // Test gizmo axis shafts and tips (lower priority than center).
        if let Some(origin) = gizmo_origin {
            for axis in Axis::ALL {
                // Shaft region — starts past the center sphere, generous perpendicular margin
                let (shaft_min, shaft_max) =
                    gizmo_shaft_aabb(origin, axis, GIZMO_WORLD_SCALE);
                if let Some(t) = ray_aabb_hit(ray_origin, ray_dir, shaft_min, shaft_max)
                    && t < best_t
                {
                        best_t = t;
                        best_result = Some(PickResult::GizmoAxis(axis));
                }
                // Tip region — cone arrowhead
                let (tip_min, tip_max) =
                    gizmo_tip_aabb(origin, axis, GIZMO_WORLD_SCALE);
                if let Some(t) = ray_aabb_hit(ray_origin, ray_dir, tip_min, tip_max)
                    && t < best_t
                {
                        best_t = t;
                        best_result = Some(PickResult::GizmoAxis(axis));
                }
            }
        }

        // Test scene entities.
        for pickable in &self.pickables {
            let world_transform = self
                .runtime
                .simulation()
                .world()
                .get::<&Transform>(pickable.entity)
                .ok()
                .map(|transform| *transform)
                .unwrap_or_default();
            let (world_min, world_max) =
                world_aabb(pickable.local_min, pickable.local_max, &world_transform);

            if let Some(t) = ray_aabb_hit(ray_origin, ray_dir, world_min, world_max)
                && t < best_t
            {
                    best_t = t;
                    best_result = Some(PickResult::Entity(pickable.entity));
            }
        }

        best_result
    }

    fn selection_world_aabb(&self) -> Option<(Vec3, Vec3)> {
        let entity = self.selected_entity?;
        let pickable = self.pickables.iter().find(|p| p.entity == entity)?;
        let world_transform = self
            .runtime
            .simulation()
            .world()
            .get::<&Transform>(entity)
            .ok()
            .map(|transform| *transform)
            .unwrap_or_default();
        Some(world_aabb(
            pickable.local_min,
            pickable.local_max,
            &world_transform,
        ))
    }

    fn apply_camera_to_runtime(&mut self) {
        let transform = self.editor_camera.into_transform();
        if let Some(camera_entity) = self.runtime.simulation().active_camera() {
            let _ = self
                .runtime
                .simulation_mut()
                .world_mut()
                .insert_one(camera_entity, transform);
        }
    }
}

// ---------------------------------------------------------------------------
// ApplicationHandler — main event loop
// ---------------------------------------------------------------------------

impl ApplicationHandler for EditorApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        if let Err(error) = self.ensure_window(event_loop) {
            eprintln!("pie_editor: failed to create window: {error}");
            event_loop.exit();
            return;
        }

        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };

        let mut needs_redraw = false;
        if let Some(egui_state) = self.egui_state.as_mut()
            && egui_state.on_window_event(window.as_ref(), &event).repaint
        {
                needs_redraw = true;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(painter) = self.painter.as_mut()
                    && let (Some(width), Some(height)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                        painter.on_window_resized(ViewportId::ROOT, width, height);
                }
                needs_redraw = true;
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                needs_redraw = true;
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta_seconds = (now - self.last_frame_instant).as_secs_f64().min(0.25);
                self.last_frame_instant = now;
                self.smoothed_delta = self.smoothed_delta * 0.9 + delta_seconds * 0.1;

                if self.runtime.is_running() {
                    self.runtime.update(delta_seconds);
                }

                let raw_input = if let Some(egui_state) = self.egui_state.as_mut() {
                    egui_state.take_egui_input(window.as_ref())
                } else {
                    return;
                };

                let egui_ctx = self.egui_ctx.clone();
                let mut commands = EditorCommands::default();
                let mut selected_entity = self.selected_entity;
                let viewport_texture_id = self
                    .viewport_texture
                    .as_ref()
                    .map(|texture| texture.texture_id);
                let gizmo_state = self.gizmo_state;
                let smoothed_delta = self.smoothed_delta;
                let cam_pos = self.editor_camera.position;
                let cam_speed = self.editor_camera.move_speed;

                let scene_info = EditorSceneInfo {
                    scene_path: self.scene.scene_path.display().to_string(),
                    mesh_count: self.scene.registry.meshes().len(),
                    texture_count: self.scene.registry.textures().len(),
                    material_count: self.scene.registry.materials().len(),
                };

                let full_output = egui_ctx.run(raw_input, |ctx| {
                    build_editor_ui(EditorUiParams {
                        ctx,
                        runtime: &mut self.runtime,
                        scene_info: &scene_info,
                        selected_entity: &mut selected_entity,
                        viewport_texture_id,
                        commands: &mut commands,
                        gizmo_state,
                        smoothed_delta,
                        cam_pos,
                        cam_speed,
                    });
                });
                self.selected_entity = selected_entity;
                self.viewport_hovered = commands.viewport_hovered;

                if let Some(egui_state) = self.egui_state.as_mut() {
                    egui_state.handle_platform_output(window.as_ref(), full_output.platform_output);
                }

                // ---- Camera input processing ----

                // Scroll wheel adjusts movement speed (only when viewport hovered).
                if commands.viewport_hovered {
                    let scroll_delta = egui_ctx.input(|i| i.raw_scroll_delta.y);
                    if scroll_delta != 0.0 {
                        let factor = if scroll_delta > 0.0 {
                            SPEED_SCROLL_FACTOR
                        } else {
                            1.0 / SPEED_SCROLL_FACTOR
                        };
                        self.editor_camera.adjust_speed(factor);
                        needs_redraw = true;
                    }
                }

                // Mouse look
                if let Some((dx, dy)) = commands.viewport_look_delta {
                    self.editor_camera.apply_look(dx, dy);
                    needs_redraw = true;
                }

                // WASD movement — only while the right mouse button is held
                // so that camera control doesn't interfere with other input.
                let right_mouse_held = egui_ctx.input(|i| {
                    i.pointer.button_down(egui::PointerButton::Secondary)
                });
                if right_mouse_held {
                    let egui_ctx_ref = &egui_ctx;
                    let forward = if egui_ctx_ref.input(|i| i.key_pressed(Key::W)) || egui_ctx_ref.input(|i| i.key_down(Key::W)) {
                        1.0
                    } else if egui_ctx_ref.input(|i| i.key_down(Key::S)) {
                        -1.0
                    } else {
                        0.0
                    };
                    let right = if egui_ctx_ref.input(|i| i.key_down(Key::D)) {
                        1.0
                    } else if egui_ctx_ref.input(|i| i.key_down(Key::A)) {
                        -1.0
                    } else {
                        0.0
                    };
                    let up = if egui_ctx_ref.input(|i| i.key_down(Key::E)) {
                        1.0
                    } else if egui_ctx_ref.input(|i| i.key_down(Key::Q)) {
                        -1.0
                    } else {
                        0.0
                    };
                    self.editor_camera.apply_movement(
                        Vec3::new(forward, right, up),
                        delta_seconds as f32,
                    );
                    if forward != 0.0 || right != 0.0 || up != 0.0 {
                        needs_redraw = true;
                    }
                }

                self.apply_camera_to_runtime();

                // ---- Gizmo drag update: axis translation ----
                if let GizmoState::Dragging { axis, entity_start_pos, total_world_delta } = self.gizmo_state {
                    if let Some((dx, dy)) = commands.gizmo_drag_delta {
                        let cam_transform = self
                            .runtime
                            .simulation()
                            .active_camera()
                            .and_then(|e| {
                                self.runtime
                                    .simulation()
                                    .world()
                                    .get::<&Transform>(e)
                                    .ok()
                                    .map(|t| *t)
                            })
                            .unwrap_or_default();
                        let axis_dir = axis.direction();
                        let cam_right = cam_transform.rotation * Vec3::X;
                        let cam_up = cam_transform.rotation * Vec3::Y;

                        // Project the axis direction onto the camera's screen-space axes.
                        let axis_screen_x = cam_right.dot(axis_dir);
                        let axis_screen_y = cam_up.dot(axis_dir);

                        // Compute world units per pixel at the object's distance.
                        // This makes the object move 1:1 with the mouse cursor.
                        let dist = (cam_transform.translation - entity_start_pos).length();
                        let viewport_h = commands.viewport_rect
                            .map(|r| r.height())
                            .unwrap_or(900.0);
                        let fov = self
                            .runtime
                            .simulation()
                            .active_camera()
                            .and_then(|e| self.runtime.simulation().world().get::<&Camera>(e).ok())
                            .map(|c| c.fov)
                            .unwrap_or_else(|| Camera::default().fov);
                        let fov_half = fov * 0.5;
                        let world_height_at_d = 2.0 * dist * fov_half.tan();
                        let world_per_pixel = world_height_at_d / viewport_h;

                        // NOTE: screen Y (dy) is positive downward, but cam_up points
                        // upward in world space. Negate dy so dragging up moves along
                        // the axis if it points upward on screen.
                        let frame_delta =
                            (dx * axis_screen_x + (-dy) * axis_screen_y) * world_per_pixel;

                        let new_total = total_world_delta + frame_delta;

                        if let Some(entity) = self.selected_entity {
                            let new_translation = entity_start_pos + axis_dir * new_total;
                            if let Ok(mut transform) = self
                                .runtime
                                .simulation_mut()
                                .world_mut()
                                .get::<&mut Transform>(entity)
                            {
                                transform.translation = new_translation;
                            }
                            self.gizmo_state = GizmoState::Dragging {
                                axis,
                                entity_start_pos,
                                total_world_delta: new_total,
                            };
                            self.apply_camera_to_runtime();
                            needs_redraw = true;
                        }
                    }
                    if commands.gizmo_drag_end {
                        self.gizmo_state = GizmoState::Idle;
                    }
                }

                // ---- Gizmo drag update: uniform scaling ----
                if let GizmoState::UniformScaling { entity_start_scale, total_scale_delta } = self.gizmo_state {
                    if let Some((_, dy)) = commands.gizmo_drag_delta {
                        // Drag up (negative dy in screen) → scale up.
                        let scale_speed = 0.004;
                        let frame_delta = -dy * scale_speed;
                        let new_total = total_scale_delta + frame_delta;
                        let factor = (1.0 + new_total).max(0.01);
                        let new_scale = entity_start_scale * factor;

                        if let Some(entity) = self.selected_entity {
                            if let Ok(mut transform) = self
                                .runtime
                                .simulation_mut()
                                .world_mut()
                                .get::<&mut Transform>(entity)
                            {
                                transform.scale = new_scale;
                            }
                            self.gizmo_state = GizmoState::UniformScaling {
                                entity_start_scale,
                                total_scale_delta: new_total,
                            };
                            self.apply_camera_to_runtime();
                            needs_redraw = true;
                        }
                    }
                    if commands.gizmo_drag_end {
                        self.gizmo_state = GizmoState::Idle;
                    }
                }

                // ---- Gizmo hover detection (every frame) ----
                self.hovered_axis = None;
                self.hovered_center = false;
                if let (Some(rect), Some(hover_pos)) =
                    (commands.viewport_rect, commands.viewport_hover_pos)
                    && !self.gizmo_state.is_active()
                    && let Some(ndc) = viewport_ndc_from_rect(rect, hover_pos)
                {
                        let gizmo_origin = self.selected_entity.and_then(|entity| {
                            self.runtime
                                .simulation()
                                .world()
                                .get::<&Transform>(entity)
                                .ok()
                                .map(|t| t.translation)
                        });
                        match self.pick_viewport(ndc, (rect.width(), rect.height()), gizmo_origin) {
                            Some(PickResult::GizmoAxis(axis)) => {
                                self.hovered_axis = Some(axis);
                            }
                            Some(PickResult::GizmoCenter) => {
                                self.hovered_center = true;
                            }
                            Some(PickResult::Entity(_)) | None => {}
                        }
                }
                // While dragging, keep the dragged element highlighted.
                if let Some(axis) = self.gizmo_state.dragged_axis() {
                    self.hovered_axis = Some(axis);
                }
                if matches!(self.gizmo_state, GizmoState::UniformScaling { .. }) {
                    self.hovered_center = true;
                }

                // ---- Viewport picking: gizmo drag start (on primary drag-start) ----
                if commands.viewport_primary_drag_started
                    && !self.gizmo_state.is_active()
                    && let Some(rect) = commands.viewport_rect
                    && let Some(drag_start_pos) = commands.viewport_primary_drag_start_pos
                    && let Some(ndc) = viewport_ndc_from_rect(rect, drag_start_pos)
                {
                        let gizmo_origin = self.selected_entity.and_then(|entity| {
                            self.runtime
                                .simulation()
                                .world()
                                .get::<&Transform>(entity)
                                .ok()
                                .map(|t| t.translation)
                        });
                        match self.pick_viewport(ndc, (rect.width(), rect.height()), gizmo_origin) {
                            Some(PickResult::GizmoAxis(axis)) => {
                                if let Some(entity) = self.selected_entity
                                    && let Ok(transform) = self
                                        .runtime
                                        .simulation()
                                        .world()
                                        .get::<&Transform>(entity)
                                {
                                        self.gizmo_state = GizmoState::Dragging {
                                            axis,
                                            entity_start_pos: transform.translation,
                                            total_world_delta: 0.0,
                                        };
                                        needs_redraw = true;
                                }
                            }
                            Some(PickResult::GizmoCenter) => {
                                if let Some(entity) = self.selected_entity
                                    && let Ok(transform) = self
                                        .runtime
                                        .simulation()
                                        .world()
                                        .get::<&Transform>(entity)
                                {
                                        self.gizmo_state = GizmoState::UniformScaling {
                                            entity_start_scale: transform.scale,
                                            total_scale_delta: 0.0,
                                        };
                                        needs_redraw = true;
                                }
                            }
                            Some(PickResult::Entity(_)) | None => {}
                        }
                }

                // ---- Viewport picking: entity selection (on click-release) ----
                if let (Some(rect), Some(click_pos)) =
                    (commands.viewport_rect, commands.viewport_click_pos)
                    && !self.gizmo_state.is_active()
                    && let Some(ndc) = viewport_ndc_from_rect(rect, click_pos)
                {
                        let gizmo_origin = self.selected_entity.and_then(|entity| {
                            self.runtime
                                .simulation()
                                .world()
                                .get::<&Transform>(entity)
                                .ok()
                                .map(|t| t.translation)
                        });
                        match self.pick_viewport(ndc, (rect.width(), rect.height()), gizmo_origin) {
                            Some(PickResult::Entity(entity)) => {
                                self.selected_entity = Some(entity);
                                needs_redraw = true;
                            }
                            Some(PickResult::GizmoAxis(_)) | Some(PickResult::GizmoCenter) => {
                                // Handled above via drag-start.
                            }
                            None => {}
                        }
                }

                if let Some(viewport_size) = commands.viewport_size
                    && let Err(error) = self.ensure_viewport_texture(viewport_size)
                {
                        eprintln!("pie_editor: failed to resize viewport texture: {error}");
                }

                self.render_viewport();

                let paint_jobs =
                    egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

                if let Some(painter) = self.painter.as_mut() {
                    painter.paint_and_update_textures(
                        ViewportId::ROOT,
                        full_output.pixels_per_point,
                        theme::CLEAR_COLOR,
                        &paint_jobs,
                        &full_output.textures_delta,
                        Vec::new(),
                    );
                }

                if commands.reload_scene {
                    self.load_scene();
                    needs_redraw = true;
                }

                if self.runtime.is_running() {
                    needs_redraw = true;
                }
            }
            _ => {}
        }

        if needs_redraw {
            self.request_redraw();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::picking::{ray_aabb_hit, viewport_ndc_from_rect};
    use egui::{Pos2, Rect, pos2};
    use glam::{Vec2, Vec3};

    #[test]
    fn viewport_ndc_maps_center_to_origin() {
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(210.0, 120.0));
        let ndc = viewport_ndc_from_rect(rect, Pos2::new(110.0, 70.0))
            .expect("non-empty rect should map to ndc");

        assert_eq!(ndc, Vec2::ZERO);
    }

    #[test]
    fn viewport_ndc_maps_corners_correctly() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 100.0));

        let top_left = viewport_ndc_from_rect(rect, pos2(0.0, 0.0)).expect("top-left maps");
        let bottom_right =
            viewport_ndc_from_rect(rect, pos2(200.0, 100.0)).expect("bottom-right maps");

        assert_eq!(top_left, Vec2::new(-1.0, 1.0));
        assert_eq!(bottom_right, Vec2::new(1.0, -1.0));
    }

    #[test]
    fn viewport_ndc_rejects_empty_rect() {
        let rect = Rect::from_min_max(pos2(5.0, 5.0), pos2(5.0, 10.0));

        assert!(viewport_ndc_from_rect(rect, pos2(5.0, 7.0)).is_none());
    }

    #[test]
    fn ray_aabb_hit_returns_nearest_positive_distance() {
        let hit = ray_aabb_hit(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, -2.0),
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        )
        .expect("ray should hit box");

        assert_eq!(hit, 2.0);
    }
}
