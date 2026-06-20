use std::env;
use std::num::NonZeroU32;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use egui::{
    CentralPanel, Context, Image, Key, Sense, SidePanel, TextureId, TopBottomPanel, ViewportId,
    load::SizedTexture, vec2, CornerRadius, Stroke, Margin, Frame,
};
use egui_wgpu::WgpuConfiguration;
use egui_wgpu::winit::Painter;
use egui_winit::State as EguiWinitState;
use glam::{Mat3, Mat4, Quat, Vec2, Vec3, Vec4, Vec4Swizzles};
use hecs::Entity;
use pie_runtime::assets::{
    AssetRegistry, MaterialAsset, MaterialHandle, MeshAsset, MeshVertex, load_gltf_scene,
    load_shader_named, spawn_imported_scene,
};
use pie_runtime::components::{DirectionalLight, Name, Transform};
use pie_runtime::core::RuntimeApp;
use pie_runtime::init_logging;
use pie_runtime::rendering::{CameraUniform, camera_view_proj};
use pie_runtime::rendering::sample_scene::bootstrap_fallback_render_scene;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window as WinitWindow, WindowId};

// ---------------------------------------------------------------------------
// Pie Editor Theme
// ---------------------------------------------------------------------------

/// A cohesive dark theme with warm amber accents for Pie Editor.
/// Colors are chosen for readability and a professional game-editor feel.
mod theme {
    use egui::{Color32, CornerRadius, Margin, Stroke, Vec2, Visuals, Style, FontId, FontFamily, TextStyle};

    // -- Palette --
    // Backgrounds
    pub const BG_WINDOW: Color32 = Color32::from_rgb(16, 17, 22);       // near-black
    pub const BG_SIDEBAR: Color32 = Color32::from_rgb(20, 22, 30);      // deep navy
    pub const BG_TOOLBAR: Color32 = Color32::from_rgb(22, 24, 33);      // slightly lighter
    pub const BG_VIEWPORT: Color32 = Color32::from_rgb(12, 14, 20);     // darkest
    pub const BG_WIDGET: Color32 = Color32::from_rgb(28, 30, 40);       // input bg
    pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(36, 38, 52);

    // Accents
    pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(255, 176, 50);    // warm amber
    pub const ACCENT_SECONDARY: Color32 = Color32::from_rgb(80, 160, 255);  // soft blue
    pub const ACCENT_SUCCESS: Color32 = Color32::from_rgb(80, 220, 130);    // green
    pub const ACCENT_PLAY: Color32 = Color32::from_rgb(70, 200, 120);      // play green
    pub const ACCENT_DANGER: Color32 = Color32::from_rgb(255, 90, 90);     // red

    // Text
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(225, 228, 235);     // off-white
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(140, 148, 165);  // muted grey
    pub const TEXT_DIM: Color32 = Color32::from_rgb(80, 86, 100);          // very muted

    // Borders & separators
    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(32, 35, 48);
    pub const BORDER_STRONG: Color32 = Color32::from_rgb(50, 55, 75);
    pub const SEPARATOR: Color32 = Color32::from_rgb(38, 42, 56);

    // Selection
    pub const SELECTION_BG: Color32 = Color32::from_rgb(50, 45, 25);       // dark amber tint
    pub const SELECTION_BORDER: Color32 = ACCENT_PRIMARY;

    // Viewport border
    pub const VIEWPORT_BORDER: Color32 = Color32::from_rgb(40, 44, 60);
    pub const VIEWPORT_BORDER_ACTIVE: Color32 = ACCENT_PRIMARY;

    // -- CornerRadius --
    pub const ROUNDING_SM: CornerRadius = CornerRadius::same(4);
    pub const ROUNDING_MD: CornerRadius = CornerRadius::same(6);

    // -- Spacing --
    pub const SPACING_XS: f32 = 2.0;
    pub const SPACING_SM: f32 = 4.0;
    pub const SPACING_MD: f32 = 8.0;
    pub const SPACING_LG: f32 = 12.0;

    /// Apply the full Pie Editor theme to an egui context.
    pub fn apply(ctx: &egui::Context) {
        let mut visuals = Visuals::dark();

        // Window / panel backgrounds
        visuals.widgets.noninteractive.bg_fill = BG_WIDGET;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
        visuals.widgets.noninteractive.corner_radius = ROUNDING_SM;

        // Interactive widgets
        visuals.widgets.inactive.bg_fill = BG_WIDGET;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
        visuals.widgets.inactive.corner_radius = ROUNDING_SM;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.5, TEXT_SECONDARY);

        visuals.widgets.hovered.bg_fill = BG_WIDGET_HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
        visuals.widgets.hovered.corner_radius = ROUNDING_SM;
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);

        visuals.widgets.active.bg_fill = ACCENT_PRIMARY.linear_multiply(0.15);
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_PRIMARY);
        visuals.widgets.active.corner_radius = ROUNDING_SM;
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, ACCENT_PRIMARY);

        visuals.widgets.open.bg_fill = BG_WIDGET_HOVER;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
        visuals.widgets.open.corner_radius = ROUNDING_SM;

        // Selection highlight
        visuals.selection.bg_fill = SELECTION_BG;
        visuals.selection.stroke = Stroke::new(1.0, SELECTION_BORDER);

        // Window fill
        visuals.window_fill = BG_WINDOW;
        visuals.panel_fill = BG_SIDEBAR;
        visuals.window_stroke = Stroke::new(1.0, BORDER_SUBTLE);
        visuals.window_corner_radius = ROUNDING_MD;
        visuals.window_shadow = egui::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(80),
        };

        // Popup / menu
        visuals.popup_shadow = egui::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(60),
        };

        // Hyperlink / accent
        visuals.hyperlink_color = ACCENT_PRIMARY;

        // Text colors
        visuals.override_text_color = Some(TEXT_PRIMARY);

        // Faint background for striped rows
        visuals.faint_bg_color = Color32::from_rgb(24, 26, 35);

        // Extreme background (very dark areas)
        visuals.extreme_bg_color = BG_VIEWPORT;

        // Clip rectangle rounding
        visuals.clip_rect_margin = 2.0;

        ctx.set_visuals(visuals);

        // -- Font sizes & style overrides --
        let mut style = Style::default();
        style.spacing.item_spacing = Vec2::new(6.0, 4.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.spacing.indent = 14.0;
        style.spacing.interact_size = Vec2::new(0.0, 28.0);
        style.spacing.icon_width = 14.0;
        style.spacing.menu_margin = Margin::same(8);
        style.spacing.window_margin = Margin::same(4);

        // Text styles
        let mut text_styles = [
            (TextStyle::Heading, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        ];
        for (text_style, font_id) in &mut text_styles {
            style.text_styles.insert(text_style.clone(), font_id.clone());
        }

        ctx.set_style(style);
    }

    /// The clear color used for the egui paint pass (behind all UI).
    pub const CLEAR_COLOR: [f32; 4] = [
        BG_WINDOW.r() as f32 / 255.0,
        BG_WINDOW.g() as f32 / 255.0,
        BG_WINDOW.b() as f32 / 255.0,
        1.0,
    ];

    /// The clear color used for the 3D viewport scene.
    pub const VIEWPORT_CLEAR: wgpu::Color = wgpu::Color {
        r: 0.04,
        g: 0.05,
        b: 0.08,
        a: 1.0,
    };
}

// ---------------------------------------------------------------------------
// Gizmo — axis enum, state, vertex generation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// RGBA color for this axis in the viewport gizmo.
    fn color(self) -> [f32; 4] {
        match self {
            Self::X => [0.9, 0.25, 0.25, 1.0],   // red
            Self::Y => [0.25, 0.9, 0.35, 1.0],   // green
            Self::Z => [0.3, 0.5, 1.0, 1.0],     // blue
        }
    }

    /// Unit direction vector for this axis.
    fn direction(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }

    /// All three axes, for iteration.
    const ALL: [Axis; 3] = [Self::X, Self::Y, Self::Z];
}

/// Interaction state for the viewport transform gizmo.
#[derive(Debug, Clone, Copy, PartialEq)]
enum GizmoState {
    /// Not interacting with the gizmo.
    Idle,
    /// Actively dragging an axis. Stores the axis, the entity's world-space
    /// position when the drag started, and the camera-space axis orientation
    /// at the time of the initial click (so dragging feels locked).
    Dragging {
        axis: Axis,
        entity_start_pos: Vec3,
    },
}

impl Default for GizmoState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Result of a viewport pick test.
enum PickResult {
    Entity(Entity),
    GizmoAxis(Axis),
}

/// Generate gizmo vertex data: 3 colored axis lines + diamond tips.
///
/// Returns `(vertices, counts_per_axis)` where `counts_per_axis[i]`
/// is the vertex count for axis `Axis::ALL[i]`.
fn build_gizmo_vertices(
    origin: Vec3,
    cam_pos: Vec3,
    gizmo_length: f32,
    tip_size: f32,
) -> (Vec<[f32; 3]>, [usize; 3]) {
    let to_cam = (cam_pos - origin).normalize_or_zero();
    let mut vertices = Vec::new();
    let mut counts = [0usize; 3];

    // Compute perpendicular axes for diamond tips (camera-facing).
    let perp_a = if to_cam.cross(Vec3::Y).length_squared() > 0.01 {
        to_cam.cross(Vec3::Y).normalize_or_zero()
    } else {
        to_cam.cross(Vec3::X).normalize_or_zero()
    };
    let perp_b = to_cam.cross(perp_a).normalize_or_zero();

    for (i, axis) in Axis::ALL.iter().enumerate() {
        let start = vertices.len();

        let dir = axis.direction();
        let end = origin + dir * gizmo_length;

        // Main axis line: origin -> end
        vertices.push([origin.x, origin.y, origin.z]);
        vertices.push([end.x, end.y, end.z]);

        // Diamond tip — 4 lines radiating from the endpoint.
        let offsets = [
            perp_a * tip_size,
            perp_b * tip_size,
            -perp_a * tip_size,
            -perp_b * tip_size,
        ];
        for offset in &offsets {
            vertices.push([end.x, end.y, end.z]);
            vertices.push([end.x + offset.x, end.y + offset.y, end.z + offset.z]);
        }

        // Small cross at origin for visual grounding.
        let small = tip_size * 0.5;
        vertices.push([origin.x, origin.y, origin.z]);
        vertices.push([origin.x + perp_a.x * small, origin.y + perp_a.y * small, origin.z + perp_a.z * small]);
        vertices.push([origin.x, origin.y, origin.z]);
        vertices.push([origin.x + perp_b.x * small, origin.y + perp_b.y * small, origin.z + perp_b.z * small]);

        counts[i] = vertices.len() - start;
    }

    (vertices, counts)
}

/// Build a thin world-space AABB around a gizmo axis line for ray-picking.
fn gizmo_axis_aabb(origin: Vec3, axis: Axis, length: f32, thickness: f32) -> (Vec3, Vec3) {
    let dir = axis.direction();
    let end = origin + dir * length;
    let half = Vec3::splat(thickness);
    let min = origin.min(end) - half;
    let max = origin.max(end) + half;
    (min, max)
}

/// Build a thin diamond AABB around the gizmo tip for easier picking.
fn gizmo_tip_aabb(origin: Vec3, axis: Axis, length: f32, thickness: f32) -> (Vec3, Vec3) {
    let tip = origin + axis.direction() * length;
    let ext = Vec3::splat(thickness * 2.5);
    (tip - ext, tip + ext)
}

/// Half-extent of the pickable AABBs around each gizmo axis.
const GIZMO_PICK_THICKNESS: f32 = 0.06;
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
// Editor camera (first-person fly)
// ---------------------------------------------------------------------------

/// Radians below |1e-6| treated as zero so a near-zero yaw/pitch doesn't
/// produce a degenerate forward vector in `Quat::from_rotation_y` lookups.
const YAW_EPSILON: f32 = 1e-6;

/// Maximum pitch magnitude (~85°) so the camera can't flip past straight
/// up/down, which would invert the forward direction.  Keeps a few degrees
/// of margin so the user never feels "stuck" at the pole.
const MAX_PITCH: f32 = 1.48353; // ≈ 85° in radians

/// Scroll-wheel speed adjustment: each notch multiplies/divides speed by this.
const SPEED_SCROLL_FACTOR: f32 = 1.15;
const MIN_MOVE_SPEED: f32 = 0.5;
const MAX_MOVE_SPEED: f32 = 80.0;

/// First-person fly camera. Owns yaw/pitch explicitly so mouse-look is
/// frame-rate independent and free of gimbal flip; the active camera entity's
/// `Transform` is rebuilt from this state every frame.
#[derive(Debug, Clone, Copy)]
struct EditorCamera {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    move_speed: f32,
    look_sensitivity: f32,
}

impl EditorCamera {
    const DEFAULT_MOVE_SPEED: f32 = 4.0;
    const DEFAULT_LOOK_SENSITIVITY: f32 = 0.0025;

    fn new(position: Vec3) -> Self {
        let (yaw, pitch) = Self::yaw_pitch_from_rotation(Quat::IDENTITY);
        Self {
            position,
            yaw,
            pitch,
            move_speed: Self::DEFAULT_MOVE_SPEED,
            look_sensitivity: Self::DEFAULT_LOOK_SENSITIVITY,
        }
    }

    /// Derive an `EditorCamera` from an existing camera `Transform`, so the
    /// fly controls take over from wherever the imported/fallback scene
    /// placed the camera instead of snapping to a default.
    fn from_transform(transform: Transform) -> Self {
        let (yaw, pitch) = Self::yaw_pitch_from_rotation(transform.rotation);
        Self {
            position: transform.translation,
            yaw,
            pitch,
            move_speed: Self::DEFAULT_MOVE_SPEED,
            look_sensitivity: Self::DEFAULT_LOOK_SENSITIVITY,
        }
    }

    /// Decompose a quaternion into yaw (around +Y) and pitch (around -X),
    /// assuming no roll — which holds for a camera built from yaw/pitch.
    fn yaw_pitch_from_rotation(rotation: Quat) -> (f32, f32) {
        // Forward direction the camera is looking, in world space.
        let forward = rotation * Vec3::NEG_Z;
        let yaw = f32::atan2(-forward.x, -forward.z);
        let horizontal = Vec3::new(forward.x, 0.0, forward.z).length();
        let pitch = f32::atan2(forward.y, horizontal);
        (yaw, pitch)
    }

    /// Camera right vector projected onto the XZ plane and re-normalised.
    /// This is the direction the camera's local +X axis maps to on the
    /// horizontal plane, giving strafes that truly follow the camera.
    fn right_xz(&self) -> Vec3 {
        let rotation = Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch);
        let right = rotation * Vec3::X;
        let projected = Vec3::new(right.x, 0.0, right.z);
        projected.normalize_or_zero()
    }

    /// Apply mouse delta (in pixels) to look, clamping pitch.
    fn apply_look(&mut self, delta_x: f32, delta_y: f32) {
        self.yaw -= delta_x * self.look_sensitivity;
        // Invert Y so dragging the mouse up looks up.
        self.pitch += delta_y * self.look_sensitivity;
        self.pitch = self.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        if self.yaw.abs() < YAW_EPSILON {
            self.yaw = 0.0;
        }
    }

    /// Integrate movement for this frame. `move_axis` is the net (forward,
    /// right, up) input in [-1, 1]; `dt` is frame delta seconds.
    ///
    /// Forward/back follows the **full 3D camera direction** (including pitch),
    /// so looking down and pressing S moves you backward *and* up.  Strafes
    /// stay horizontal, and the explicit up/down axis (Q/E) moves along world Y.
    fn apply_movement(&mut self, move_axis: Vec3, dt: f32) {
        if move_axis == Vec3::ZERO {
            return;
        }
        // Full camera forward in 3D — retains the pitch (Y) component so
        // that WASD movement truly follows where the camera is looking.
        let rotation = Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch);
        let forward_3d = (rotation * Vec3::NEG_Z).normalize_or_zero();
        // Right stays purely horizontal so strafes don't drift vertically.
        let right_horizontal = self.right_xz();
        // Q / E move along world up/down.
        let vertical = Vec3::Y * move_axis.z;

        let displacement = (forward_3d * move_axis.x
            + right_horizontal * move_axis.y
            + vertical)
            .normalize_or_zero()
            * self.move_speed
            * dt;
        self.position += displacement;
    }

    /// Multiply move speed by `factor`, clamped to [MIN_MOVE_SPEED, MAX_MOVE_SPEED].
    fn adjust_speed(&mut self, factor: f32) {
        self.move_speed = (self.move_speed * factor).clamp(MIN_MOVE_SPEED, MAX_MOVE_SPEED);
    }

    /// Rebuild the runtime `Transform` from yaw/pitch/position. Roll is zero.
    fn to_transform(&self) -> Transform {
        let rotation = Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch);
        Transform {
            translation: self.position,
            rotation,
            scale: Vec3::ONE,
        }
    }
}

// ---------------------------------------------------------------------------
// Picking (screen ray vs. world AABB)
// ---------------------------------------------------------------------------

/// Local-space bounds of a pickable mesh, cached at scene load so each pick
/// only pays for the world transform, not a full vertex scan.
#[derive(Debug, Clone, Copy)]
struct PickableBounds {
    entity: Entity,
    local_min: Vec3,
    local_max: Vec3,
}

/// World-space ray-AABB intersection. Returns the near hit distance `t >= 0`,
/// or `None` if the ray misses. `dir` need not be normalized; the returned `t`
/// is in the same units.
fn ray_aabb_hit(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;

    for i in 0..3 {
        let o = origin[i];
        let d = dir[i];
        let lo = min[i];
        let hi = max[i];

        if d.abs() < 1e-8 {
            // Parallel to this axis slab: must already be inside it.
            if o < lo || o > hi {
                return None;
            }
        } else {
            let mut t1 = (lo - o) / d;
            let mut t2 = (hi - o) / d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_min = t_min.max(t1);
            t_max = t_max.min(t2);
            if t_min > t_max {
                return None;
            }
        }
    }

    // Origin inside the box counts as a hit at t=0.
    if t_max < 0.0 {
        None
    } else {
        Some(t_min.max(0.0))
    }
}

/// Transform a local-space AABB into its world-space axis-aligned equivalent.
/// This is a conservative (potentially larger) fit when rotation is present;
/// exact for translation+scale only. Good enough for picking.
fn world_aabb(local_min: Vec3, local_max: Vec3, transform: &Transform) -> (Vec3, Vec3) {
    // 8 corners of the local box, transformed to world space.
    let corners = [
        Vec3::new(local_min.x, local_min.y, local_min.z),
        Vec3::new(local_max.x, local_min.y, local_min.z),
        Vec3::new(local_min.x, local_max.y, local_min.z),
        Vec3::new(local_max.x, local_max.y, local_min.z),
        Vec3::new(local_min.x, local_min.y, local_max.z),
        Vec3::new(local_max.x, local_min.y, local_max.z),
        Vec3::new(local_min.x, local_max.y, local_max.z),
        Vec3::new(local_max.x, local_max.y, local_max.z),
    ];

    let basis = Mat3::from_quat(transform.rotation);
    let mut world_min = Vec3::INFINITY;
    let mut world_max = Vec3::NEG_INFINITY;
    for corner in corners {
        let world = basis * (corner * transform.scale) + transform.translation;
        world_min = world_min.min(world);
        world_max = world_max.max(world);
    }
    (world_min, world_max)
}

/// Build a world-space screen ray from a normalized device coordinate in
/// [-1, 1] (Y up) by inverting `view_proj`.
fn screen_ray_from_ndc(ndc: Vec2, view_proj: Mat4) -> (Vec3, Vec3) {
    let near_point = view_proj.inverse() * Vec4::new(ndc.x, ndc.y, -1.0, 1.0);
    let far_point = view_proj.inverse() * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);

    let near = near_point.xyz() / near_point.w;
    let far = far_point.xyz() / far_point.w;
    let dir = far - near;
    (near, dir)
}

struct EditorViewportTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    texture_id: TextureId,
    size: [u32; 2],
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn create_editor_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("editor viewport depth texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

struct EditorViewportRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    model_buffer: wgpu::Buffer,
    model_bind_group: wgpu::BindGroup,
    light_buffer: wgpu::Buffer,
    light_bind_group: wgpu::BindGroup,
    material_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    #[allow(dead_code)]
    fallback_texture: wgpu::Texture,
    fallback_texture_view: wgpu::TextureView,
    materials: std::collections::HashMap<MaterialHandle, EditorGpuMaterial>,
    drawables: Vec<EditorGpuDrawable>,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    depth_size: [u32; 2],
    line_pipeline: wgpu::RenderPipeline,
    line_camera_buffer: wgpu::Buffer,
    line_camera_bind_group: wgpu::BindGroup,
    line_color_buffer: wgpu::Buffer,
    line_color_bind_group: wgpu::BindGroup,
    line_vertex_buffer: wgpu::Buffer,
    // Gizmo vertex buffer — large enough for 3 axis lines + diamond tips + origin cross.
    gizmo_vertex_buffer: wgpu::Buffer,
    gizmo_vertex_capacity: usize,
}

struct EditorGpuMaterial {
    _buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    _texture: Option<wgpu::Texture>,
    _normal_texture: Option<wgpu::Texture>,
}

struct EditorGpuDrawable {
    entity: Entity,
    mesh: EditorGpuMesh,
    material: MaterialHandle,
}

struct EditorGpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl EditorViewportRenderer {
    fn new(render_state: &egui_wgpu::RenderState, assets_root: &Path) -> Result<Self, String> {
        let device = Arc::new(render_state.device.clone());
        let queue = Arc::new(render_state.queue.clone());
        let target_format = wgpu::TextureFormat::Rgba8UnormSrgb;

        let shader_source =
            load_shader_named(assets_root, "textured_mesh").map_err(|error| error.to_string())?;
        let shader = device.as_ref().create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("editor viewport shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let camera_bind_group_layout =
            device.as_ref().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor viewport camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let model_bind_group_layout =
            device.as_ref().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor viewport model bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout =
            device.as_ref().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor viewport material bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let light_bind_group_layout =
            device.as_ref().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor viewport light bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.as_ref().create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("editor viewport pipeline layout"),
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &model_bind_group_layout,
                &light_bind_group_layout,
                &texture_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.as_ref().create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("editor viewport pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MeshVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: (std::mem::size_of::<[f32; 3]>() * 2 + std::mem::size_of::<[f32; 2]>()) as wgpu::BufferAddress,
                            shader_location: 3,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let camera_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport camera buffer"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = device.as_ref().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor viewport camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let model_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport model buffer"),
            size: std::mem::size_of::<EditorModelUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let model_bind_group = device.as_ref().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor viewport model bind group"),
            layout: &model_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: model_buffer.as_entire_binding(),
            }],
        });

        let light_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport light buffer"),
            size: std::mem::size_of::<EditorLightUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let light_bind_group = device.as_ref().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor viewport light bind group"),
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
        });

        let sampler = device.as_ref().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("editor viewport sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let fallback_texture = device.as_ref().create_texture(&wgpu::TextureDescriptor {
            label: Some("editor viewport fallback texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let fallback_texture_view =
            fallback_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (depth_texture, depth_texture_view) =
            create_editor_depth_texture(&device, 1, 1);

        let line_shader_source =
            load_shader_named(assets_root, "debug_line").map_err(|error| error.to_string())?;
        let line_shader = device.as_ref().create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("editor viewport line shader"),
            source: wgpu::ShaderSource::Wgsl(line_shader_source.into()),
        });

        let line_camera_bind_group_layout =
            device.as_ref().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor viewport line camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let line_color_bind_group_layout =
            device.as_ref().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor viewport line color bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let line_pipeline_layout =
            device.as_ref().create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("editor viewport line pipeline layout"),
                bind_group_layouts: &[&line_camera_bind_group_layout, &line_color_bind_group_layout],
                push_constant_ranges: &[],
            });

        let line_pipeline = device.as_ref().create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("editor viewport line pipeline"),
            layout: Some(&line_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &line_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &line_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let line_camera_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport line camera buffer"),
            size: std::mem::size_of::<LineCameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let line_camera_bind_group = device.as_ref().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor viewport line camera bind group"),
            layout: &line_camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: line_camera_buffer.as_entire_binding(),
            }],
        });

        let line_color_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport line color buffer"),
            size: std::mem::size_of::<LineColorUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let line_color_bind_group = device.as_ref().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor viewport line color bind group"),
            layout: &line_color_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: line_color_buffer.as_entire_binding(),
            }],
        });

        // 24 vertices = 12 edges of an AABB. Reused for every selection box.
        let line_vertex_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport line vertex buffer"),
            size: (std::mem::size_of::<[f32; 3]>() * LINE_VERTEX_COUNT) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Gizmo vertex buffer — holds 3 axis lines + diamond tips + origin cross.
        // Each axis: 2 (line) + 8 (diamond: 4 pairs) + 4 (origin cross: 2 pairs) = 14 vertices.
        // 3 axes: 42 vertices. Allocate 64 for headroom.
        const GIZMO_MAX_VERTICES: usize = 64;
        let gizmo_vertex_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport gizmo vertex buffer"),
            size: (std::mem::size_of::<[f32; 3]>() * GIZMO_MAX_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            camera_buffer,
            camera_bind_group,
            model_buffer,
            model_bind_group,
            light_buffer,
            light_bind_group,
            material_bind_group_layout: texture_bind_group_layout,
            sampler,
            fallback_texture,
            fallback_texture_view,
            materials: std::collections::HashMap::new(),
            drawables: Vec::new(),
            depth_texture,
            depth_texture_view,
            depth_size: [1, 1],
            line_pipeline,
            line_camera_buffer,
            line_camera_bind_group,
            line_color_buffer,
            line_color_bind_group,
            line_vertex_buffer,
            gizmo_vertex_buffer,
            gizmo_vertex_capacity: GIZMO_MAX_VERTICES,
        })
    }

    fn load_scene(
        &mut self,
        registry: &AssetRegistry,
        simulation: &pie_runtime::core::SimulationCore,
    ) -> Result<(), String> {
        self.materials.clear();
        let mut drawables = Vec::new();

        for (entity, mesh_renderer) in simulation
            .world()
            .query::<&pie_runtime::components::MeshRenderer>()
            .iter()
        {
            let mesh = registry
                .mesh(mesh_renderer.mesh)
                .map_err(|error| error.to_string())?;
            let material = registry
                .material(mesh.material)
                .map_err(|error| error.to_string())?;

            if !self.materials.contains_key(&mesh.material) {
                self.materials
                    .insert(mesh.material, self.upload_material(registry, material)?);
            }

            drawables.push(EditorGpuDrawable {
                entity,
                mesh: upload_editor_mesh(self.device.as_ref(), mesh),
                material: mesh.material,
            });
        }

        self.drawables = drawables;
        Ok(())
    }

    fn upload_material(
        &self,
        registry: &AssetRegistry,
        material: &MaterialAsset,
    ) -> Result<EditorGpuMaterial, String> {
        let uniform = EditorMaterialUniform {
            base_color_factor: material.base_color_factor,
            parameters: [
                material.metallic_factor,
                material.roughness_factor,
                0.0,
                0.0,
            ],
        };

        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("editor viewport material buffer"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let base_color_view = if let Some(texture_handle) = material.base_color_texture {
            let texture = registry
                .texture(texture_handle)
                .map_err(|error| error.to_string())?;
            Some(upload_editor_texture(self.device.as_ref(), self.queue.as_ref(), texture))
        } else {
            None
        };

        let (normal_gpu, normal_view) = if let Some(normal_handle) = material.normal_texture {
            let texture = registry
                .texture(normal_handle)
                .map_err(|error| error.to_string())?;
            let uploaded = upload_editor_texture(self.device.as_ref(), self.queue.as_ref(), texture);
            (Some(uploaded.texture), Some(uploaded.view))
        } else {
            (None, None)
        };

        let base_color_view_ref = base_color_view
            .as_ref()
            .map(|t| &t.view)
            .unwrap_or(&self.fallback_texture_view);
        let normal_view_ref = normal_view
            .as_ref()
            .unwrap_or(&self.fallback_texture_view);

        let bind_group = self.device.as_ref().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor viewport material bind group"),
            layout: &self.material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(base_color_view_ref),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(normal_view_ref),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        Ok(EditorGpuMaterial {
            _buffer: buffer.clone(),
            bind_group,
            _texture: base_color_view.map(|t| t.texture),
            _normal_texture: normal_gpu,
        })
    }

    fn render_to_view(
        &mut self,
        simulation: &pie_runtime::core::SimulationCore,
        view: &wgpu::TextureView,
        size: [u32; 2],
        selection_aabb: Option<(Vec3, Vec3)>,
        gizmo_origin: Option<Vec3>,
    ) {
        if size[0] == 0 || size[1] == 0 {
            return;
        }

        if self.depth_size != size {
            let (depth_texture, depth_texture_view) =
                create_editor_depth_texture(self.device.as_ref(), size[0], size[1]);
            self.depth_texture = depth_texture;
            self.depth_texture_view = depth_texture_view;
            self.depth_size = size;
        }

        let aspect_ratio = size[0] as f32 / size[1] as f32;
        let camera_uniform = CameraUniform::from_simulation(simulation, aspect_ratio);
        self.queue
            .as_ref()
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));

        let view_proj = camera_view_proj(
            simulation
                .active_camera()
                .and_then(|entity| simulation.world().get::<&Transform>(entity).ok())
                .map(|transform| *transform)
                .unwrap_or_default(),
            aspect_ratio,
        );
        let line_camera_uniform = LineCameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
        };
        self.queue.as_ref().write_buffer(
            &self.line_camera_buffer,
            0,
            bytemuck::bytes_of(&line_camera_uniform),
        );

        let directional_light = simulation
            .resource::<DirectionalLight>()
            .copied()
            .unwrap_or_default();
        let light_uniform = EditorLightUniform {
            direction: [
                directional_light.direction.x,
                directional_light.direction.y,
                directional_light.direction.z,
                0.0,
            ],
            color: [
                directional_light.color.x,
                directional_light.color.y,
                directional_light.color.z,
                directional_light.intensity,
            ],
        };
        self.queue
            .as_ref()
            .write_buffer(&self.light_buffer, 0, bytemuck::bytes_of(&light_uniform));

        if let Some((min, max)) = selection_aabb {
            let vertices = aabb_line_vertices(min, max);
            let bytes: &[u8] = bytemuck::cast_slice(&vertices);
            self.queue
                .as_ref()
                .write_buffer(&self.line_vertex_buffer, 0, bytes);
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("editor viewport encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("editor viewport render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(theme::VIEWPORT_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_bind_group(2, &self.light_bind_group, &[]);

            for drawable in &self.drawables {
                let transform = simulation
                    .world()
                    .get::<&Transform>(drawable.entity)
                    .ok()
                    .map(|transform| *transform)
                    .unwrap_or_default();
                let model = Mat4::from_scale_rotation_translation(
                    transform.scale,
                    transform.rotation,
                    transform.translation,
                );
                let model_uniform = EditorModelUniform {
                    model: model.to_cols_array_2d(),
                    normal_matrix: model.inverse().transpose().to_cols_array_2d(),
                };
                self.queue
                    .write_buffer(&self.model_buffer, 0, bytemuck::bytes_of(&model_uniform));

                render_pass.set_bind_group(1, &self.model_bind_group, &[]);
                let material = self
                    .materials
                    .get(&drawable.material)
                    .expect("scene materials should be uploaded before rendering");
                render_pass.set_bind_group(3, &material.bind_group, &[]);
                render_pass.set_vertex_buffer(0, drawable.mesh.vertex_buffer.slice(..));
                render_pass.set_index_buffer(
                    drawable.mesh.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(0..drawable.mesh.index_count, 0, 0..1);
            }

            if selection_aabb.is_some() {
                let line_color = LineColorUniform {
                    color: [1.0, 0.85, 0.1, 1.0],
                };
                self.queue.as_ref().write_buffer(
                    &self.line_color_buffer,
                    0,
                    bytemuck::bytes_of(&line_color),
                );

                render_pass.set_pipeline(&self.line_pipeline);
                render_pass.set_bind_group(0, &self.line_camera_bind_group, &[]);
                render_pass.set_bind_group(1, &self.line_color_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.line_vertex_buffer.slice(..));
                render_pass.draw(0..LINE_VERTEX_COUNT as u32, 0..1);
            }

            // ---- Gizmo drawing ----
            if let Some(origin) = gizmo_origin {
                let cam_pos = simulation
                    .active_camera()
                    .and_then(|entity| simulation.world().get::<&Transform>(entity).ok())
                    .map(|t| t.translation)
                    .unwrap_or(Vec3::new(0.0, 1.0, 5.0));

                // Scale gizmo with distance so it stays readable.
                let dist = (cam_pos - origin).length();
                let scale = (dist * 0.15).clamp(0.4, 4.0);
                let tip = scale * 0.18;

                let (gizmo_verts, counts) = build_gizmo_vertices(origin, cam_pos, scale, tip);
                if !gizmo_verts.is_empty() && gizmo_verts.len() <= self.gizmo_vertex_capacity {
                    let all_bytes: &[u8] = bytemuck::cast_slice(&gizmo_verts);
                    self.queue
                        .as_ref()
                        .write_buffer(&self.gizmo_vertex_buffer, 0, all_bytes);

                    render_pass.set_pipeline(&self.line_pipeline);
                    render_pass.set_bind_group(0, &self.line_camera_bind_group, &[]);
                    render_pass.set_bind_group(1, &self.line_color_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.gizmo_vertex_buffer.slice(..));

                    let mut offset = 0u32;
                    for (i, axis) in Axis::ALL.iter().enumerate() {
                        let color = LineColorUniform {
                            color: axis.color(),
                        };
                        self.queue.as_ref().write_buffer(
                            &self.line_color_buffer,
                            0,
                            bytemuck::bytes_of(&color),
                        );
                        let verts_in_axis = counts[i] as u32;
                        render_pass.draw(offset..offset + verts_in_axis, 0..1);
                        offset += verts_in_axis;
                    }
                }
            }
        }

        self.queue.as_ref().submit(std::iter::once(encoder.finish()));
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EditorModelUniform {
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EditorMaterialUniform {
    base_color_factor: [f32; 4],
    parameters: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EditorLightUniform {
    direction: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineCameraUniform {
    view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineColorUniform {
    color: [f32; 4],
}

/// Vertex count for the line vertex buffer: 12 AABB edges * 2 endpoints.
const LINE_VERTEX_COUNT: usize = 24;

/// Build the 24-vertex line list for an AABB's 12 edges.
fn aabb_line_vertices(min: Vec3, max: Vec3) -> [[f32; 3]; LINE_VERTEX_COUNT] {
    let a = min;
    let g = max;
    let b = Vec3::new(g.x, a.y, a.z);
    let c = Vec3::new(g.x, g.y, a.z);
    let d = Vec3::new(a.x, g.y, a.z);
    let e = Vec3::new(a.x, a.y, g.z);
    let f = Vec3::new(g.x, a.y, g.z);
    let h = Vec3::new(a.x, g.y, g.z);

    // 12 edges: bottom square, top square, four verticals.
    [
        [a.x, a.y, a.z], [b.x, b.y, b.z], // a-b
        [b.x, b.y, b.z], [c.x, c.y, c.z], // b-c
        [c.x, c.y, c.z], [d.x, d.y, d.z], // c-d
        [d.x, d.y, d.z], [a.x, a.y, a.z], // d-a
        [e.x, e.y, e.z], [f.x, f.y, f.z], // e-f
        [f.x, f.y, f.z], [g.x, g.y, g.z], // f-g
        [g.x, g.y, g.z], [h.x, h.y, h.z], // g-h
        [h.x, h.y, h.z], [e.x, e.y, e.z], // h-e
        [a.x, a.y, a.z], [e.x, e.y, e.z], // a-e
        [b.x, b.y, b.z], [f.x, f.y, f.z], // b-f
        [c.x, c.y, c.z], [g.x, g.y, g.z], // c-g
        [d.x, d.y, d.z], [h.x, h.y, h.z], // d-h
    ]
}

fn upload_editor_mesh(device: &wgpu::Device, mesh: &MeshAsset) -> EditorGpuMesh {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("editor viewport mesh vertex buffer"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("editor viewport mesh index buffer"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    EditorGpuMesh {
        vertex_buffer,
        index_buffer,
        index_count: mesh.indices.len() as u32,
    }
}

struct UploadedEditorTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

fn upload_editor_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &pie_runtime::assets::TextureAsset,
) -> UploadedEditorTexture {
    let gpu_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("editor viewport loaded texture"),
        size: wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texture.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * texture.width),
            rows_per_image: Some(texture.height),
        },
        wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
    );

    let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
    UploadedEditorTexture {
        texture: gpu_texture,
        view,
    }
}

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
}

#[derive(Default)]
struct EditorCommands {
    reload_scene: bool,
    viewport_size: Option<[u32; 2]>,
    viewport_hovered: bool,
    viewport_look_delta: Option<(f32, f32)>,
    viewport_rect: Option<egui::Rect>,
    viewport_click_pos: Option<egui::Pos2>,
    gizmo_drag_delta: Option<(f32, f32)>,
    gizmo_drag_end: bool,
}

fn viewport_ndc_from_rect(rect: egui::Rect, click_pos: egui::Pos2) -> Option<Vec2> {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }

    Some(Vec2::new(
        ((click_pos.x - rect.min.x) / rect.width()) * 2.0 - 1.0,
        1.0 - ((click_pos.y - rect.min.y) / rect.height()) * 2.0,
    ))
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
                .map_err(|error| std::io::Error::other(error))?;
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
            if let Ok(mesh) = scene.registry.mesh(mesh_renderer.mesh) {
                if let Some((local_min, local_max)) = mesh.local_aabb() {
                    pickables.push(PickableBounds {
                        entity,
                        local_min,
                        local_max,
                    });
                }
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
        let view_proj = camera_view_proj(camera_transform, aspect);
        let (ray_origin, ray_dir) = screen_ray_from_ndc(ndc, view_proj);

        let mut best_t = f32::INFINITY;
        let mut best_result = None;

        // Test gizmo axes first (they take priority).
        if let Some(origin) = gizmo_origin {
            let dist = (camera_transform.translation - origin).length();
            let scale = (dist * 0.15).clamp(0.4, 4.0);
            for axis in Axis::ALL {
                let (axis_min, axis_max) =
                    gizmo_axis_aabb(origin, axis, scale, GIZMO_PICK_THICKNESS);
                if let Some(t) = ray_aabb_hit(ray_origin, ray_dir, axis_min, axis_max) {
                    if t < best_t {
                        best_t = t;
                        best_result = Some(PickResult::GizmoAxis(axis));
                    }
                }
                let (tip_min, tip_max) =
                    gizmo_tip_aabb(origin, axis, scale, GIZMO_PICK_THICKNESS);
                if let Some(t) = ray_aabb_hit(ray_origin, ray_dir, tip_min, tip_max) {
                    if t < best_t {
                        best_t = t;
                        best_result = Some(PickResult::GizmoAxis(axis));
                    }
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

            if let Some(t) = ray_aabb_hit(ray_origin, ray_dir, world_min, world_max) {
                if t < best_t {
                    best_t = t;
                    best_result = Some(PickResult::Entity(pickable.entity));
                }
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
        let transform = self.editor_camera.to_transform();
        if let Some(camera_entity) = self.runtime.simulation().active_camera() {
            let _ = self
                .runtime
                .simulation_mut()
                .world_mut()
                .insert_one(camera_entity, transform);
        }
    }
}

fn build_editor_ui(
    ctx: &Context,
    runtime: &mut RuntimeApp,
    scene: &EditorScene,
    selected_entity: &mut Option<Entity>,
    viewport_texture_id: Option<TextureId>,
    commands: &mut EditorCommands,
    gizmo_state: GizmoState,
    smoothed_delta: f64,
    cam_pos: glam::Vec3,
    cam_speed: f32,
) {
    use egui::RichText;
    use theme::*;

    let toggle_running = ctx.input(|input| input.key_pressed(Key::Space));
    let step_requested = ctx.input(|input| input.key_pressed(Key::N));
    let reload_requested = ctx.input(|input| input.key_pressed(Key::R));

    let scene_path = scene.scene_path.display().to_string();
    let mesh_count = scene.registry.meshes().len();
    let texture_count = scene.registry.textures().len();
    let material_count = scene.registry.materials().len();
    let frame = runtime.simulation().frame();
    let is_running = runtime.is_running();
    let entity_count = runtime.simulation().world().iter().count();

    // ---- Toolbar ----
    TopBottomPanel::top("toolbar")
        .frame(Frame {
            inner_margin: Margin::symmetric(8, 4),
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_TOOLBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Logo / title
                let title = RichText::new("◆  PIE EDITOR")
                    .color(ACCENT_PRIMARY)
                    .size(15.0)
                    .strong();
                ui.label(title);

                ui.add_space(SPACING_LG);

                // Thin vertical separator
                let sep = egui::Separator::default().spacing(0.0).vertical();
                ui.add(sep);
                ui.add_space(SPACING_MD);

                // Scene info badge
                let scene_label = RichText::new(format!("⬡  {scene_path}"))
                    .color(TEXT_SECONDARY)
                    .size(11.0);
                ui.label(scene_label);

                ui.add_space(SPACING_LG);

                // Vertical separator
                let sep = egui::Separator::default().spacing(0.0).vertical();
                ui.add(sep);
                ui.add_space(SPACING_MD);

                // Playback controls
                // Reload
                let reload_btn = egui::Button::new(
                    RichText::new("↻  Reload").color(TEXT_PRIMARY).size(12.0),
                )
                .corner_radius(ROUNDING_SM)
                .fill(BG_WIDGET)
                .stroke(Stroke::new(1.0, BORDER_SUBTLE));
                if ui.add(reload_btn).on_hover_text("Reload scene (R)").clicked() {
                    commands.reload_scene = true;
                }

                ui.add_space(SPACING_XS);

                // Play / Pause toggle
                if is_running {
                    let pause_btn = egui::Button::new(
                        RichText::new("⏸  Pause").color(TEXT_PRIMARY).size(12.0),
                    )
                    .corner_radius(ROUNDING_SM)
                    .fill(BG_WIDGET)
                    .stroke(Stroke::new(1.0, BORDER_SUBTLE));
                    if ui.add(pause_btn).clicked() {
                        runtime.pause();
                    }
                } else {
                    let play_btn = egui::Button::new(
                        RichText::new("▶  Play").color(ACCENT_PLAY).size(12.0),
                    )
                    .corner_radius(ROUNDING_SM)
                    .fill(ACCENT_PLAY.linear_multiply(0.12))
                    .stroke(Stroke::new(1.0, ACCENT_PLAY.linear_multiply(0.4)));
                    if ui.add(play_btn).clicked() {
                        runtime.resume();
                    }
                }

                ui.add_space(SPACING_XS);

                // Step
                let step_btn = egui::Button::new(
                    RichText::new("⏭  Step").color(TEXT_PRIMARY).size(12.0),
                )
                .corner_radius(ROUNDING_SM)
                .fill(BG_WIDGET)
                .stroke(Stroke::new(1.0, BORDER_SUBTLE));
                if ui.add(step_btn).on_hover_text("Step one frame (N)").clicked() {
                    runtime.pause();
                    runtime.step();
                }

                // Push status to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Running indicator
                    let (dot_color, state_text) = if is_running {
                        (ACCENT_SUCCESS, "Running")
                    } else {
                        (TEXT_DIM, "Paused")
                    };
                    let dot = RichText::new("●").color(dot_color).size(10.0);
                    let state = RichText::new(state_text).color(TEXT_SECONDARY).size(11.0);
                    ui.label(state);
                    ui.add_space(SPACING_XS);
                    ui.label(dot);

                    ui.add_space(SPACING_LG);

                    // Frame counter
                    let frame_text = RichText::new(format!("Frame {frame}"))
                        .color(TEXT_SECONDARY)
                        .size(11.0)
                        .monospace();
                    ui.label(frame_text);

                    ui.add_space(SPACING_LG);

                    // Shortcuts hint
                    let hint = RichText::new("Space: toggle  ·  N: step  ·  R: reload")
                        .color(TEXT_DIM)
                        .size(10.0);
                    ui.label(hint);
                });
            });
        });

    // ---- Hierarchy Panel ----
    SidePanel::left("hierarchy")
        .resizable(true)
        .default_width(280.0)
        .frame(Frame {
            inner_margin: Margin::same(8),
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_SIDEBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            // Section header
            let header = RichText::new("SCENE")
                .color(ACCENT_PRIMARY)
                .size(11.0)
                .strong();
            ui.label(header);
            ui.add_space(SPACING_SM);

            // Scene stats in a compact row
            ui.horizontal(|ui| {
                let stat = |label: &str, value: usize| {
                    RichText::new(format!("{label}: {value}"))
                        .color(TEXT_SECONDARY)
                        .size(10.0)
                        .monospace()
                };
                ui.label(stat("entities", entity_count));
                ui.add_space(SPACING_MD);
                ui.label(stat("meshes", mesh_count));
                ui.add_space(SPACING_MD);
                ui.label(stat("materials", material_count));
                ui.add_space(SPACING_MD);
                ui.label(stat("textures", texture_count));
            });

            // Scene path
            ui.add_space(SPACING_XS);
            let path_text = RichText::new(format!("⬡  {scene_path}"))
                .color(TEXT_DIM)
                .size(10.0);
            ui.label(path_text);

            ui.add_space(SPACING_MD);
            ui.separator();
            ui.add_space(SPACING_SM);

            // Hierarchy header
            let hier_header = RichText::new("HIERARCHY")
                .color(TEXT_SECONDARY)
                .size(10.0)
                .strong();
            ui.label(hier_header);
            ui.add_space(SPACING_SM);

            // Entity list
            let mut entities = Vec::new();
            for (entity, _) in runtime.simulation().world().query::<&Transform>().iter() {
                entities.push(entity);
            }

            for entity in entities {
                let name = runtime
                    .simulation()
                    .world()
                    .get::<&Name>(entity)
                    .map(|name| name.0.clone())
                    .unwrap_or_else(|_| format!("{entity:?}"));
                let selected = *selected_entity == Some(entity);

                let text = if selected {
                    RichText::new(format!("  ▸  {name}"))
                        .color(ACCENT_PRIMARY)
                        .size(12.0)
                } else {
                    RichText::new(format!("  ○  {name}"))
                        .color(TEXT_PRIMARY)
                        .size(12.0)
                };

                let response = ui.selectable_label(selected, text);
                if response.clicked() {
                    *selected_entity = Some(entity);
                }
            }
        });

    // ---- Inspector Panel ----
    SidePanel::right("inspector")
        .resizable(true)
        .default_width(320.0)
        .frame(Frame {
            inner_margin: Margin::same(8),
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_SIDEBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            // Section header
            let header = RichText::new("INSPECTOR")
                .color(ACCENT_PRIMARY)
                .size(11.0)
                .strong();
            ui.label(header);
            ui.add_space(SPACING_SM);
            ui.separator();
            ui.add_space(SPACING_MD);

            // Lighting section
            if let Some(light) = runtime.simulation_mut().resource_mut::<DirectionalLight>() {
                let light_header = RichText::new("☀  Lighting")
                    .color(TEXT_PRIMARY)
                    .size(12.0)
                    .strong();
                ui.collapsing(light_header, |ui| {
                    ui.add_space(SPACING_SM);

                    // Intensity
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Intensity").color(TEXT_SECONDARY).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut light.intensity)
                                    .speed(0.1)
                                    .min_decimals(2),
                            );
                        });
                    });

                    ui.add_space(SPACING_XS);

                    // Direction
                    ui.label(RichText::new("Direction").color(TEXT_SECONDARY).size(11.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut light.direction.x).speed(0.01));
                        ui.label(RichText::new("Y").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut light.direction.y).speed(0.01));
                        ui.label(RichText::new("Z").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut light.direction.z).speed(0.01));
                    });
                    if light.direction.length_squared() > f32::EPSILON {
                        light.direction = light.direction.normalize();
                    }

                    ui.add_space(SPACING_XS);

                    // Color
                    ui.label(RichText::new("Color").color(TEXT_SECONDARY).size(11.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("R").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut light.color.x).speed(0.05));
                        ui.label(RichText::new("G").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut light.color.y).speed(0.05));
                        ui.label(RichText::new("B").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut light.color.z).speed(0.05));
                    });
                });
            }

            ui.add_space(SPACING_SM);
            ui.separator();
            ui.add_space(SPACING_MD);

            // Selected entity
            if let Some(entity) = *selected_entity {
                let entity_name = runtime
                    .simulation()
                    .world()
                    .get::<&Name>(entity)
                    .map(|name| name.0.clone())
                    .unwrap_or_else(|_| format!("{entity:?}"));

                let sel_header = RichText::new(format!("◆  {entity_name}"))
                    .color(ACCENT_SECONDARY)
                    .size(12.0)
                    .strong();
                ui.label(sel_header);

                let id_text = RichText::new(format!("{entity:?}"))
                    .color(TEXT_DIM)
                    .size(10.0)
                    .monospace();
                ui.label(id_text);

                ui.add_space(SPACING_SM);

                if let Ok(mut transform) = runtime
                    .simulation_mut()
                    .world_mut()
                    .get::<&mut Transform>(entity)
                {
                    // Position
                    let pos_header = RichText::new("Position")
                        .color(TEXT_SECONDARY)
                        .size(11.0)
                        .strong();
                    ui.label(pos_header);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut transform.translation.x).speed(0.05));
                        ui.label(RichText::new("Y").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut transform.translation.y).speed(0.05));
                        ui.label(RichText::new("Z").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut transform.translation.z).speed(0.05));
                    });

                    ui.add_space(SPACING_XS);

                    // Scale
                    let scale_header = RichText::new("Scale")
                        .color(TEXT_SECONDARY)
                        .size(11.0)
                        .strong();
                    ui.label(scale_header);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut transform.scale.x).speed(0.05));
                        ui.label(RichText::new("Y").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut transform.scale.y).speed(0.05));
                        ui.label(RichText::new("Z").color(TEXT_DIM).size(10.0).monospace());
                        ui.add(egui::DragValue::new(&mut transform.scale.z).speed(0.05));
                    });
                } else {
                    ui.label(
                        RichText::new("Selected entity has no editable transform.")
                            .color(TEXT_DIM)
                            .size(11.0),
                    );
                }
            } else {
                let empty_text = RichText::new("No entity selected.")
                    .color(TEXT_DIM)
                    .size(12.0);
                ui.label(empty_text);
                ui.add_space(SPACING_XS);
                let hint = RichText::new("Click an entity in the viewport to select it.")
                    .color(TEXT_DIM)
                    .size(10.0);
                ui.label(hint);
            }
        });

    // ---- Viewport (Central Panel) ----
    CentralPanel::default()
        .frame(Frame {
            inner_margin: Margin::ZERO,
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_VIEWPORT,
            stroke: Stroke::NONE,
        })
        .show(ctx, |ui| {
            let available = ui.available_size();
            let viewport_size = [
                available.x.max(1.0).round() as u32,
                available.y.max(1.0).round() as u32,
            ];
            commands.viewport_size = Some(viewport_size);

            if let Some(texture_id) = viewport_texture_id {
                // Viewport border frame
                let border_stroke = if commands.viewport_hovered {
                    Stroke::new(1.5, VIEWPORT_BORDER_ACTIVE)
                } else {
                    Stroke::new(1.0, VIEWPORT_BORDER)
                };

                let viewport_frame = Frame {
                    inner_margin: Margin::ZERO,
                    outer_margin: Margin::same(0),
                    corner_radius: ROUNDING_SM,
                    shadow: egui::Shadow::NONE,
                    fill: BG_VIEWPORT,
                    stroke: border_stroke,
                };

                viewport_frame.show(ui, |ui| {
                    let response = ui.add(
                        Image::from_texture(SizedTexture::new(
                            texture_id,
                            vec2(viewport_size[0] as f32, viewport_size[1] as f32),
                        ))
                        .sense(Sense::click_and_drag()),
                    );

                    commands.viewport_hovered = response.hovered() || response.dragged();
                    commands.viewport_rect = Some(response.rect);

                    // Right-click drag → camera look.
                    if response.dragged_by(egui::PointerButton::Secondary) {
                        let delta = response.drag_delta();
                        commands.viewport_look_delta = Some((delta.x, delta.y));
                    }

                    // Left-click drag → gizmo drag (if active) or camera look.
                    if response.dragged_by(egui::PointerButton::Primary) {
                        let delta = response.drag_delta();
                        if matches!(gizmo_state, GizmoState::Dragging { .. }) {
                            commands.gizmo_drag_delta = Some((delta.x, delta.y));
                        }
                    }

                    // Left-click release → end gizmo drag.
                    if response.drag_stopped_by(egui::PointerButton::Primary)
                        && matches!(gizmo_state, GizmoState::Dragging { .. })
                    {
                        commands.gizmo_drag_end = true;
                    }

                    if response.clicked() {
                        commands.viewport_click_pos = response.interact_pointer_pos();
                    }
                });
            } else {
                // Placeholder
                Frame {
                    inner_margin: Margin::same(16),
                    outer_margin: Margin::ZERO,
                    corner_radius: CornerRadius::ZERO,
                    shadow: egui::Shadow::NONE,
                    fill: BG_VIEWPORT,
                    stroke: Stroke::new(1.0, VIEWPORT_BORDER),
                }
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        let vp_icon = RichText::new("◆")
                            .color(ACCENT_PRIMARY)
                            .size(28.0);
                        ui.label(vp_icon);
                        ui.add_space(SPACING_SM);
                        let vp_title = RichText::new("Viewport")
                            .color(TEXT_PRIMARY)
                            .size(16.0)
                            .strong();
                        ui.label(vp_title);
                        ui.add_space(SPACING_SM);
                        let vp_desc = RichText::new(
                            "The runtime preview comes next.\nRight-click drag to look, WASD to move.",
                        )
                        .color(TEXT_SECONDARY)
                        .size(11.0);
                        ui.label(vp_desc);
                    });
                });
            }
        });

    // ---- Status Bar ----
    TopBottomPanel::bottom("status_bar")
        .frame(Frame {
            inner_margin: Margin::symmetric(8, 2),
            outer_margin: Margin::ZERO,
            corner_radius: CornerRadius::ZERO,
            shadow: egui::Shadow::NONE,
            fill: BG_TOOLBAR,
            stroke: Stroke::new(1.0, SEPARATOR),
        })
        .show(ctx, |ui| {
            let fps = if smoothed_delta > 0.0 {
                (1.0 / smoothed_delta) as i32
            } else {
                0
            };
            let (dot_color, state_text) = if is_running {
                (ACCENT_SUCCESS, "Running")
            } else {
                (TEXT_DIM, "Paused")
            };

            ui.horizontal(|ui| {
                // Status dot + state
                let dot = RichText::new("●").color(dot_color).size(9.0);
                ui.label(dot);
                ui.add_space(4.0);
                ui.label(RichText::new(state_text).color(TEXT_SECONDARY).size(10.0));

                ui.add_space(SPACING_MD);

                // FPS
                let fps_color = if fps >= 55 {
                    ACCENT_SUCCESS
                } else if fps >= 30 {
                    ACCENT_PRIMARY
                } else {
                    ACCENT_PLAY
                };
                ui.label(
                    RichText::new(format!("{fps} FPS"))
                        .color(fps_color)
                        .size(10.0)
                        .monospace(),
                );

                ui.add_space(SPACING_MD);

                // Frame
                ui.label(
                    RichText::new(format!("Frame {frame}"))
                        .color(TEXT_SECONDARY)
                        .size(10.0)
                        .monospace(),
                );

                ui.add_space(SPACING_MD);

                // Entity / mesh counts
                ui.label(
                    RichText::new(format!("{entity_count} entities"))
                        .color(TEXT_DIM)
                        .size(10.0),
                );
                ui.add_space(SPACING_SM);
                ui.label(
                    RichText::new(format!("{mesh_count} meshes"))
                        .color(TEXT_DIM)
                        .size(10.0),
                );

                ui.add_space(SPACING_MD);

                // Camera position
                ui.label(
                    RichText::new(format!(
                        "Cam: ({:.1}, {:.1}, {:.1})",
                        cam_pos.x, cam_pos.y, cam_pos.z
                    ))
                    .color(TEXT_DIM)
                    .size(10.0)
                    .monospace(),
                );

                ui.add_space(SPACING_SM);

                // Camera speed
                ui.label(
                    RichText::new(format!("Speed: {:.1}", cam_speed))
                        .color(TEXT_DIM)
                        .size(10.0)
                        .monospace(),
                );

                // Gizmo state indicator
                if let GizmoState::Dragging { axis, .. } = gizmo_state {
                    ui.add_space(SPACING_MD);
                    let (axis_label, axis_color) = match axis {
                        Axis::X => ("X", theme::ACCENT_DANGER),
                        Axis::Y => ("Y", ACCENT_SUCCESS),
                        Axis::Z => ("Z", theme::ACCENT_SECONDARY),
                    };
                    ui.label(
                        RichText::new(format!("Dragging {axis_label}"))
                            .color(axis_color)
                            .size(10.0)
                            .strong(),
                    );
                }

                // Right side: fixed timestep info
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("60 Hz fixed step")
                            .color(TEXT_DIM)
                            .size(9.0),
                    );
                });
            });
        });

    if toggle_running {
        if is_running {
            runtime.pause();
        } else {
            runtime.resume();
        }
    }

    if step_requested {
        runtime.pause();
        runtime.step();
    }

    if reload_requested {
        commands.reload_scene = true;
    }
}

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
        if let Some(egui_state) = self.egui_state.as_mut() {
            if egui_state.on_window_event(window.as_ref(), &event).repaint {
                needs_redraw = true;
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(painter) = self.painter.as_mut() {
                    if let (Some(width), Some(height)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                    {
                        painter.on_window_resized(ViewportId::ROOT, width, height);
                    }
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
                let full_output = egui_ctx.run(raw_input, |ctx| {
                    build_editor_ui(
                        ctx,
                        &mut self.runtime,
                        &self.scene,
                        &mut selected_entity,
                        viewport_texture_id,
                        &mut commands,
                        gizmo_state,
                        smoothed_delta,
                        cam_pos,
                        cam_speed,
                    );
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
                        // Positive scroll = speed up, negative = speed down.
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
                    // E = up, Q = down.  Space is intentionally left unbound so
                    // it can be used by other editor shortcuts.
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

                // ---- Gizmo drag update ----
                if let GizmoState::Dragging { axis, entity_start_pos } = self.gizmo_state {
                    if let Some((dx, dy)) = commands.gizmo_drag_delta {
                        // Project mouse delta onto the gizmo axis in world space.
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
                        // Convert mouse delta to a world-space displacement along the axis.
                        let axis_dir = axis.direction();
                        let cam_right = cam_transform.rotation * Vec3::X;
                        let cam_up = cam_transform.rotation * Vec3::Y;

                        // How much of the axis projects onto the screen's right and up.
                        let axis_screen_x = cam_right.dot(axis_dir);
                        let axis_screen_y = cam_up.dot(axis_dir);

                        // Scale: pixels → world units. Approximate using gizmo length.
                        let dist = (cam_transform.translation - entity_start_pos).length();
                        let sensitivity = dist * 0.003;
                        let world_delta =
                            (dx * axis_screen_x + dy * axis_screen_y) * sensitivity;

                        if let Some(entity) = self.selected_entity {
                            let new_translation = entity_start_pos + axis_dir * world_delta;
                            if let Ok(mut transform) = self
                                .runtime
                                .simulation_mut()
                                .world_mut()
                                .get::<&mut Transform>(entity)
                            {
                                transform.translation = new_translation;
                            }
                            self.apply_camera_to_runtime();
                            needs_redraw = true;
                        }
                    }
                    if commands.gizmo_drag_end {
                        self.gizmo_state = GizmoState::Idle;
                    }
                }

                // ---- Viewport picking (entity or gizmo) ----
                if let (Some(rect), Some(click_pos)) =
                    (commands.viewport_rect, commands.viewport_click_pos)
                {
                    if let Some(ndc) = viewport_ndc_from_rect(rect, click_pos) {
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
                            Some(PickResult::GizmoAxis(axis)) => {
                                if let Some(entity) = self.selected_entity {
                                    if let Ok(transform) = self
                                        .runtime
                                        .simulation()
                                        .world()
                                        .get::<&Transform>(entity)
                                    {
                                        self.gizmo_state = GizmoState::Dragging {
                                            axis,
                                            entity_start_pos: transform.translation,
                                        };
                                        needs_redraw = true;
                                    }
                                }
                            }
                            None => {}
                        }
                    }
                }

                if let Some(viewport_size) = commands.viewport_size {
                    if let Err(error) = self.ensure_viewport_texture(viewport_size) {
                        eprintln!("pie_editor: failed to resize viewport texture: {error}");
                    }
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


#[cfg(test)]
mod tests {
    use super::{ray_aabb_hit, viewport_ndc_from_rect};
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
