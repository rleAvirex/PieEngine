//! Gizmo — axis enum, interaction state, solid mesh generation, and pick AABBs.
//!
//! Generates UE5/Unity-style gizmo geometry:
//! - **Axis shafts**: Thick box (6 faces) for each axis, visible from all angles.
//! - **Arrow cones**: Solid cone mesh at each axis tip, 12 segments around.
//! - **Center sphere**: White icosphere for uniform scaling at the origin.
//! - **Hover highlighting**: Hovered or dragged axes are brightened.
//!
//! All geometry uses per-vertex color (no separate color uniform needed).
//! The gizmo uses screen-fixed sizing: it always occupies a consistent number
//! of pixels regardless of camera distance.

use glam::Vec3;

/// Represents one of the three principal axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// RGBA base color for this axis (not hovered).
    pub fn color(self) -> [f32; 4] {
        match self {
            Self::X => [0.85, 0.22, 0.22, 1.0],   // red
            Self::Y => [0.22, 0.85, 0.30, 1.0],   // green
            Self::Z => [0.25, 0.45, 0.95, 1.0],   // blue
        }
    }

    /// RGBA highlight color for this axis when hovered or dragged.
    pub fn highlight_color(self) -> [f32; 4] {
        match self {
            Self::X => [1.0, 0.55, 0.45, 1.0],   // bright red-orange
            Self::Y => [0.45, 1.0, 0.55, 1.0],   // bright green
            Self::Z => [0.50, 0.70, 1.0, 1.0],   // bright blue
        }
    }

    /// Unit direction vector for this axis.
    pub fn direction(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }

    /// All three axes, for iteration.
    pub const ALL: [Axis; 3] = [Self::X, Self::Y, Self::Z];
}

/// Interaction state for the viewport transform gizmo.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum GizmoState {
    /// Not interacting with the gizmo.
    #[default]
    Idle,
    /// Actively dragging an axis to translate.
    Dragging {
        axis: Axis,
        entity_start_pos: Vec3,
        /// Accumulated world-space offset along the drag axis.
        total_world_delta: f32,
    },
    /// Dragging the center sphere to scale uniformly.
    UniformScaling {
        entity_start_scale: Vec3,
        /// Accumulated scale factor delta (0 = no change, positive = bigger).
        total_scale_delta: f32,
    },
}

impl GizmoState {
    /// Returns the axis being dragged, if any.
    pub fn dragged_axis(self) -> Option<Axis> {
        match self {
            Self::Dragging { axis, .. } => Some(axis),
            Self::Idle | Self::UniformScaling { .. } => None,
        }
    }

    /// Returns true if the gizmo is actively being interacted with.
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// Result of a viewport pick test.
pub enum PickResult {
    Entity(hecs::Entity),
    GizmoAxis(Axis),
    GizmoCenter,
}

/// A vertex for the gizmo triangle mesh: position + per-vertex color.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GizmoVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// Number of segments around the cone arrowhead.
const CONE_SEGMENTS: usize = 12;

/// Desired gizmo length in pixels on screen (roughly).
const GIZMO_SCREEN_PIXELS: f32 = 90.0;

/// Camera vertical half-FOV used by the runtime (`Mat4::perspective_rh(FRAC_PI_4, ...)`).
/// This is `π/4 / 2 = π/8 ≈ 22.5°`.
const CAM_FOV_HALF: f32 = std::f32::consts::FRAC_PI_4 * 0.5;

/// Calculate a screen-fixed gizmo scale factor.
///
/// This converts a desired pixel size to a world-space size at the given
/// camera distance, using the runtime's actual vertical FOV (45°). The result
/// is that the gizmo always appears `GIZMO_SCREEN_PIXELS` pixels tall
/// regardless of how far the camera is.
pub fn gizmo_screen_scale(cam_distance: f32, viewport_height_pixels: f32) -> f32 {
    let world_height_at_d = 2.0 * cam_distance * CAM_FOV_HALF.tan();
    let pixels_per_world = viewport_height_pixels / world_height_at_d;
    GIZMO_SCREEN_PIXELS / pixels_per_world
}

/// Generate the full gizmo mesh for all three axes plus center sphere.
///
/// Returns a flat list of `GizmoVertex` forming triangles.
///
/// - `hovered_axis`: Which axis the mouse is hovering over (brightens it).
/// - `hovered_center`: Whether the center scale sphere is hovered.
/// - `gizmo_scale`: World-space length of one axis (from `gizmo_screen_scale`).
pub fn build_gizmo_mesh(
    origin: Vec3,
    cam_pos: Vec3,
    gizmo_scale: f32,
    hovered_axis: Option<Axis>,
    hovered_center: bool,
    gizmo_state: GizmoState,
) -> Vec<GizmoVertex> {
    let mut verts = Vec::new();

    // Compute a camera-facing basis perpendicular to the view direction.
    let to_cam = (cam_pos - origin).normalize_or_zero();
    let up = if to_cam.cross(Vec3::Y).length_squared() > 0.01 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = to_cam.cross(up).normalize_or_zero();
    let up_corrected = right.cross(to_cam).normalize_or_zero();

    // Proportions relative to gizmo_scale (like UE5/Unity)
    let shaft_half_width = gizmo_scale * 0.028;   // thick shaft
    let cone_length = gizmo_scale * 0.25;          // prominent arrowhead
    let cone_radius = gizmo_scale * 0.09;          // wide cone
    let center_radius = gizmo_scale * 0.085;        // center scale sphere (larger, easy to grab)

    // -- Center scale sphere (white, highlights when hovered/active) --
    let is_center_active = hovered_center || matches!(gizmo_state, GizmoState::UniformScaling { .. });
    let center_color = if is_center_active {
        [1.0, 1.0, 1.0, 1.0]       // bright white when hovered/active
    } else {
        [0.85, 0.85, 0.85, 1.0]    // light gray normally
    };
    push_icosphere(&mut verts, origin, right, up_corrected, to_cam, center_radius, center_color);

    // -- Per-axis: thick box shaft + cone arrowhead --
    let active_axis = gizmo_state.dragged_axis();
    for axis in Axis::ALL {
        let dir = axis.direction();
        let is_highlighted = hovered_axis == Some(axis) || active_axis == Some(axis);
        let color = if is_highlighted {
            axis.highlight_color()
        } else {
            axis.color()
        };

        // Compute perpendicular frame for this axis
        let perp_a = (right - dir * right.dot(dir)).normalize_or_zero();
        let perp_b = (up_corrected - dir * up_corrected.dot(dir)).normalize_or_zero();
        let perp_a = if perp_a.length_squared() < 0.001 {
            dir.cross(Vec3::Y).normalize_or_zero()
        } else {
            perp_a
        };
        let perp_b = if perp_b.length_squared() < 0.001 {
            dir.cross(perp_a).normalize_or_zero()
        } else {
            perp_b
        };

        // Shaft: thick box from origin to (origin + dir * (scale - cone_length))
        let shaft_end = origin + dir * (gizmo_scale - cone_length);
        push_shaft_box(&mut verts, origin, shaft_end, perp_a, perp_b, shaft_half_width, color);

        // Cone arrowhead
        let cone_tip = origin + dir * gizmo_scale;
        push_cone(&mut verts, shaft_end, cone_tip, perp_a, perp_b, cone_radius, color);
    }

    verts
}

/// Push a thick box (6 faces) for an axis shaft.
///
/// This creates a proper 3D box instead of a flat quad, so the shaft
/// looks solid from every camera angle.
fn push_shaft_box(
    verts: &mut Vec<GizmoVertex>,
    start: Vec3,
    end: Vec3,
    perp_a: Vec3,
    perp_b: Vec3,
    half_width: f32,
    color: [f32; 4],
) {
    // 8 corners: start/end × ±perp_a × ±perp_b
    let s_pp = start + perp_a * half_width + perp_b * half_width;
    let s_pm = start + perp_a * half_width - perp_b * half_width;
    let s_mp = start - perp_a * half_width + perp_b * half_width;
    let s_mm = start - perp_a * half_width - perp_b * half_width;
    let e_pp = end + perp_a * half_width + perp_b * half_width;
    let e_pm = end + perp_a * half_width - perp_b * half_width;
    let e_mp = end - perp_a * half_width + perp_b * half_width;
    let e_mm = end - perp_a * half_width - perp_b * half_width;

    // +perp_a face
    push_tri(verts, s_pm, e_pm, s_pp, color);
    push_tri(verts, s_pp, e_pm, e_pp, color);
    // -perp_a face
    push_tri(verts, s_mp, s_mm, e_mm, color);
    push_tri(verts, s_mp, e_mm, e_mp, color);
    // +perp_b face
    push_tri(verts, s_pp, e_pp, s_mp, color);
    push_tri(verts, s_mp, e_pp, e_mp, color);
    // -perp_b face
    push_tri(verts, s_pm, s_mm, e_mm, color);
    push_tri(verts, s_pm, e_mm, e_pm, color);
    // end cap
    push_tri(verts, e_pp, e_pm, e_mp, color);
    push_tri(verts, e_pm, e_mm, e_mp, color);
    // start cap
    push_tri(verts, s_pp, s_mp, s_mm, color);
    push_tri(verts, s_pp, s_mm, s_pm, color);
}

/// Push a solid cone (arrowhead) as a triangle fan.
fn push_cone(
    verts: &mut Vec<GizmoVertex>,
    base_center: Vec3,
    tip: Vec3,
    perp_a: Vec3,
    perp_b: Vec3,
    radius: f32,
    color: [f32; 4],
) {
    let angle_step = std::f32::consts::TAU / CONE_SEGMENTS as f32;

    for i in 0..CONE_SEGMENTS {
        let angle0 = angle_step * i as f32;
        let angle1 = angle_step * ((i + 1) % CONE_SEGMENTS) as f32;

        let offset0 = (perp_a * angle0.cos() + perp_b * angle0.sin()) * radius;
        let offset1 = (perp_a * angle1.cos() + perp_b * angle1.sin()) * radius;

        let p0 = base_center + offset0;
        let p1 = base_center + offset1;

        // Cone side triangle
        push_tri(verts, p0, p1, tip, color);
        // Cone base cap triangle
        push_tri(verts, p0, base_center, p1, color);
    }
}

/// Push a low-poly icosphere (20 faces) using an icosahedron.
fn push_icosphere(
    verts: &mut Vec<GizmoVertex>,
    center: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    radius: f32,
    color: [f32; 4],
) {
    let phi = 0.5 * (1.0 + (5.0_f32).sqrt()); // golden ratio
    let s = radius / (1.0 + phi * phi).sqrt();

    // 12 vertices of an icosahedron, mapped to world space via the camera basis.
    let v: [Vec3; 12] = [
        center + (-1.0 * right + phi * up) * s,
        center + (1.0 * right + phi * up) * s,
        center + (-1.0 * right - phi * up) * s,
        center + (1.0 * right - phi * up) * s,
        center + (-phi * right + 1.0 * forward) * s,
        center + (phi * right + 1.0 * forward) * s,
        center + (-phi * right - 1.0 * forward) * s,
        center + (phi * right - 1.0 * forward) * s,
        center + (-1.0 * forward + phi * up) * s,
        center + (1.0 * forward + phi * up) * s,
        center + (-1.0 * forward - phi * up) * s,
        center + (1.0 * forward - phi * up) * s,
    ];

    // 20 triangular faces of the icosahedron.
    let faces: [(usize, usize, usize); 20] = [
        (0, 11, 5),  (0, 5, 1),   (0, 1, 7),   (0, 7, 10),  (0, 10, 11),
        (1, 5, 9),   (5, 11, 4),  (11, 10, 2),  (10, 7, 6),  (7, 1, 8),
        (3, 9, 4),   (3, 4, 2),   (3, 2, 6),    (3, 6, 8),   (3, 8, 9),
        (4, 9, 5),   (2, 4, 11),  (6, 2, 10),   (8, 6, 7),   (9, 8, 1),
    ];

    for (a, b, c) in &faces {
        push_tri(verts, v[*a], v[*b], v[*c], color);
    }
}

fn push_tri(verts: &mut Vec<GizmoVertex>, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    verts.push(GizmoVertex { position: [a.x, a.y, a.z], color });
    verts.push(GizmoVertex { position: [b.x, b.y, b.z], color });
    verts.push(GizmoVertex { position: [c.x, c.y, c.z], color });
}

// ---------------------------------------------------------------------------
// Pick AABB helpers — shaft, tip, and center regions
// ---------------------------------------------------------------------------

/// Build a world-space AABB around the gizmo axis **shaft** for ray-picking.
///
/// The shaft starts just past the center sphere (dead zone) and ends where
/// the cone begins. The perpendicular margin is generous so thin shafts
/// are easy to click.
pub fn gizmo_shaft_aabb(origin: Vec3, axis: Axis, gizmo_scale: f32) -> (Vec3, Vec3) {
    let dir = axis.direction();
    // Skip the center sphere area — dead zone so clicking the center
    // sphere doesn't accidentally grab an axis.
    let shaft_start_offset = gizmo_scale * 0.08;
    let shaft_end_offset = gizmo_scale * 0.75; // cone starts at ~0.75
    let start = origin + dir * shaft_start_offset;
    let end = origin + dir * shaft_end_offset;
    // Perpendicular margin: generous for easy clicking on the thin shaft.
    let margin = gizmo_scale * 0.08;
    let half = Vec3::splat(margin);
    let min = start.min(end) - half;
    let max = start.max(end) + half;
    (min, max)
}

/// Build a world-space AABB around the gizmo cone tip for easier picking.
pub fn gizmo_tip_aabb(origin: Vec3, axis: Axis, gizmo_scale: f32) -> (Vec3, Vec3) {
    let tip = origin + axis.direction() * gizmo_scale;
    let ext = Vec3::splat(gizmo_scale * 0.15);
    (tip - ext, tip + ext)
}

/// Build a world-space AABB around the center scale sphere for ray-picking.
pub fn gizmo_center_aabb(origin: Vec3, gizmo_scale: f32) -> (Vec3, Vec3) {
    let ext = Vec3::splat(gizmo_scale * 0.13);
    (origin - ext, origin + ext)
}
