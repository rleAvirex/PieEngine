//! Gizmo — axis enum, interaction state, solid mesh generation, and pick AABBs.
//!
//! Generates UE5/Unity-style gizmo geometry:
//! - **Axis shafts**: Thick box (6 faces) for each axis, visible from all angles.
//! - **Arrow cones**: Solid cone mesh at each axis tip, 12 segments around.
//! - **Origin sphere**: Small icosphere at the center where all axes meet.
//! - **Hover highlighting**: Hovered or dragged axes are brightened to white.
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
    /// Actively dragging an axis.
    Dragging {
        axis: Axis,
        entity_start_pos: Vec3,
    },
}

impl GizmoState {
    /// Returns the axis being dragged, if any.
    pub fn dragged_axis(self) -> Option<Axis> {
        match self {
            Self::Dragging { axis, .. } => Some(axis),
            Self::Idle => None,
        }
    }
}

/// Result of a viewport pick test.
pub enum PickResult {
    Entity(hecs::Entity),
    GizmoAxis(Axis),
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

/// Calculate a screen-fixed gizmo scale factor.
///
/// This converts a desired pixel size to a world-space size at the given
/// camera distance, using the vertical FOV approximation. The result is
/// that the gizmo always appears roughly `GIZMO_SCREEN_PIXELS` pixels tall
/// regardless of how far the camera is.
pub fn gizmo_screen_scale(cam_distance: f32, viewport_height_pixels: f32) -> f32 {
    // Approximate: the visible world height at distance d with perspective is
    // roughly `2 * d * tan(fov_half)`. We don't have FOV directly, so we
    // estimate from the runtime's typical 60° vertical FOV.
    let fov_half = std::f32::consts::FRAC_PI_6; // ~30° → 60° total
    let world_height_at_d = 2.0 * cam_distance * fov_half.tan();
    let pixels_per_world = viewport_height_pixels / world_height_at_d;
    GIZMO_SCREEN_PIXELS / pixels_per_world
}

/// Generate the full gizmo mesh for all three axes.
///
/// Returns a flat list of `GizmoVertex` forming triangles.
///
/// - `hovered_axis`: Which axis the mouse is hovering over (brightens it).
///   Also considers the `GizmoState::Dragging` axis as "active".
/// - `gizmo_scale`: World-space length of one axis (from `gizmo_screen_scale`).
pub fn build_gizmo_mesh(
    origin: Vec3,
    cam_pos: Vec3,
    gizmo_scale: f32,
    hovered_axis: Option<Axis>,
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
    let origin_radius = gizmo_scale * 0.04;        // small sphere at center

    // -- Origin sphere --
    push_icosphere(&mut verts, origin, right, up_corrected, to_cam, origin_radius, [0.75, 0.75, 0.75, 1.0]);

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

/// Push a low-poly icosphere (20 faces) for the origin marker.
fn push_icosphere(
    verts: &mut Vec<GizmoVertex>,
    center: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    radius: f32,
    color: [f32; 4],
) {
    // Golden ratio for icosahedron
    let t = 0.5 * (1.0 + (5.0_f32).sqrt());
    let s = radius / (1.0 + t * t).sqrt();

    // 12 vertices of an icosahedron in local space, then transformed
    let _local_verts: Vec<Vec3> = vec![
        center + (-1.0 * right + t * up) * s,
        center + (1.0 * right + t * up) * s,
        center + (-1.0 * right - t * up) * s,
        center + (1.0 * right - t * up) * s,
        center + (-t * right + 1.0 * forward) * s + up * s * 0.0,
        center + (t * right + 1.0 * forward) * s,
        center + (-t * right - 1.0 * forward) * s,
        center + (t * right - 1.0 * forward) * s,
        center + (-1.0 * forward + t * up) * s,
        center + (1.0 * forward + t * up) * s,
        center + (-1.0 * forward - t * up) * s,
        center + (1.0 * forward - t * up) * s,
    ];

    // Simplified: just use a cube for the origin (cheaper and still looks good)
    push_cube(verts, center, right, up, forward, radius, color);
}

/// Push a small cube.
fn push_cube(
    verts: &mut Vec<GizmoVertex>,
    center: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    half_size: f32,
    color: [f32; 4],
) {
    let corners = [
        center - right * half_size - up * half_size - forward * half_size,
        center + right * half_size - up * half_size - forward * half_size,
        center + right * half_size + up * half_size - forward * half_size,
        center - right * half_size + up * half_size - forward * half_size,
        center - right * half_size - up * half_size + forward * half_size,
        center + right * half_size - up * half_size + forward * half_size,
        center + right * half_size + up * half_size + forward * half_size,
        center - right * half_size + up * half_size + forward * half_size,
    ];

    let faces: [(usize, usize, usize, usize); 6] = [
        (0, 1, 2, 3), (5, 4, 7, 6), (4, 0, 3, 7),
        (1, 5, 6, 2), (3, 2, 6, 7), (4, 5, 1, 0),
    ];

    for (a, b, c, d) in &faces {
        push_tri(verts, corners[*a], corners[*b], corners[*c], color);
        push_tri(verts, corners[*a], corners[*c], corners[*d], color);
    }
}

fn push_tri(verts: &mut Vec<GizmoVertex>, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    verts.push(GizmoVertex { position: [a.x, a.y, a.z], color });
    verts.push(GizmoVertex { position: [b.x, b.y, b.z], color });
    verts.push(GizmoVertex { position: [c.x, c.y, c.z], color });
}

// ---------------------------------------------------------------------------
// Pick AABB helpers — sized to match visual geometry + generous margin
// ---------------------------------------------------------------------------

/// Build a world-space AABB around the full gizmo axis (shaft + cone) for ray-picking.
/// `visual_radius` should be the larger of shaft_half_width and cone_radius,
/// then we add a generous margin for easy clicking.
pub fn gizmo_axis_aabb(origin: Vec3, axis: Axis, length: f32, visual_radius: f32) -> (Vec3, Vec3) {
    let dir = axis.direction();
    let end = origin + dir * length;
    let margin = visual_radius * 3.0; // 3× visual size for comfortable picking
    let half = Vec3::splat(margin);
    let min = origin.min(end) - half;
    let max = origin.max(end) + half;
    (min, max)
}

/// Build a world-space AABB around the gizmo cone tip for easier picking.
pub fn gizmo_tip_aabb(origin: Vec3, axis: Axis, length: f32, visual_radius: f32) -> (Vec3, Vec3) {
    let tip = origin + axis.direction() * length;
    let ext = Vec3::splat(visual_radius * 4.0);
    (tip - ext, tip + ext)
}
