//! Gizmo — axis enum, interaction state, solid mesh generation, and pick AABBs.
//!
//! Generates UE5/Unity-style gizmo geometry:
//! - **Axis shafts**: Camera-facing quads (billboard strips) with controllable
//!   screen-space thickness, so they always look solid regardless of zoom.
//! - **Arrow cones**: Solid cone mesh at each axis tip, 12 segments around.
//! - **Origin cube**: Small cube at the center where all three axes meet.
//!
//! All geometry uses per-vertex color (no separate color uniform needed).

use glam::Vec3;

/// Represents one of the three principal axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// RGBA color for this axis in the viewport gizmo.
    pub fn color(self) -> [f32; 4] {
        match self {
            Self::X => [0.9, 0.25, 0.25, 1.0],   // red
            Self::Y => [0.25, 0.9, 0.35, 1.0],   // green
            Self::Z => [0.3, 0.5, 1.0, 1.0],     // blue
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

/// Generate the full gizmo mesh for all three axes.
///
/// Returns a flat list of `GizmoVertex` forming triangles (3 vertices per
/// triangle). The mesh consists of:
/// - Camera-facing quad for each axis shaft
/// - Solid cone arrowhead for each axis tip
/// - Small cube at the origin
pub fn build_gizmo_mesh(
    origin: Vec3,
    cam_pos: Vec3,
    gizmo_length: f32,
    shaft_half_width: f32,
    cone_length: f32,
    cone_radius: f32,
    cube_half_size: f32,
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

    // -- Origin cube --
    push_cube(&mut verts, origin, right, up_corrected, to_cam, cube_half_size, [0.7, 0.7, 0.7, 1.0]);

    // -- Per-axis: shaft quad + cone arrowhead --
    for axis in Axis::ALL {
        let dir = axis.direction();
        let color = axis.color();

        // Compute a camera-facing frame for this axis:
        // We want two vectors perpendicular to `dir` that also face the camera.
        // Use Gram-Schmidt to orthogonalize `right` and `up_corrected` against `dir`.
        let perp_a = (right - dir * right.dot(dir)).normalize_or_zero();
        let perp_b = (up_corrected - dir * up_corrected.dot(dir)).normalize_or_zero();
        // If either ended up zero (camera aligned with axis), use a fallback.
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

        // Shaft: from origin to (origin + dir * (gizmo_length - cone_length)).
        let shaft_end = origin + dir * (gizmo_length - cone_length);
        push_shaft_quad(&mut verts, origin, shaft_end, dir, perp_a, perp_b, shaft_half_width, color);

        // Cone: from shaft_end to (origin + dir * gizmo_length).
        let cone_tip = origin + dir * gizmo_length;
        push_cone(&mut verts, shaft_end, cone_tip, dir, perp_a, perp_b, cone_radius, color);
    }

    verts
}

/// Push a camera-facing quad (2 triangles = 6 vertices) for an axis shaft.
///
/// The quad is oriented to always face the camera by using the `perp_a` and
/// `perp_b` vectors (which are already computed to face the camera).
#[allow(clippy::too_many_arguments)]
fn push_shaft_quad(
    verts: &mut Vec<GizmoVertex>,
    start: Vec3,
    end: Vec3,
    _dir: Vec3,
    perp_a: Vec3,
    perp_b: Vec3,
    half_width: f32,
    color: [f32; 4],
) {
    // 4 corners of the shaft quad, offset in the camera-facing perpendiculars.
    let s_a = start + perp_a * half_width;
    let s_b = start - perp_a * half_width;
    let e_a = end + perp_a * half_width;
    let e_b = end - perp_a * half_width;

    let s_c = start + perp_b * half_width;
    let s_d = start - perp_b * half_width;
    let e_c = end + perp_b * half_width;
    let e_d = end - perp_b * half_width;

    // First face (perp_a side) — 2 triangles
    push_tri(verts, s_a, e_a, s_b, color);
    push_tri(verts, s_b, e_a, e_b, color);

    // Second face (perp_b side) — 2 triangles
    push_tri(verts, s_c, e_c, s_d, color);
    push_tri(verts, s_d, e_c, e_d, color);
}

/// Push a solid cone (arrowhead) as a triangle fan.
///
/// The cone has `CONE_SEGMENTS` sides, going from the base ring around
/// `base_center` to a single tip at `tip`.
#[allow(clippy::too_many_arguments)]
fn push_cone(
    verts: &mut Vec<GizmoVertex>,
    base_center: Vec3,
    tip: Vec3,
    _dir: Vec3,
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

        // Cone base cap triangle (facing back along -dir)
        push_tri(verts, p0, base_center, p1, color);
    }
}

/// Push a small cube at the origin where all axes meet.
fn push_cube(
    verts: &mut Vec<GizmoVertex>,
    center: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    half_size: f32,
    color: [f32; 4],
) {
    // 8 corners of the cube
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

    // 6 faces × 2 triangles each = 12 triangles = 36 vertices
    let faces: [(usize, usize, usize, usize); 6] = [
        (0, 1, 2, 3), // front
        (5, 4, 7, 6), // back
        (4, 0, 3, 7), // left
        (1, 5, 6, 2), // right
        (3, 2, 6, 7), // top
        (4, 5, 1, 0), // bottom
    ];

    for (a, b, c, d) in &faces {
        push_tri(verts, corners[*a], corners[*b], corners[*c], color);
        push_tri(verts, corners[*a], corners[*c], corners[*d], color);
    }
}

/// Push a single triangle (3 vertices) into the vertex list.
fn push_tri(verts: &mut Vec<GizmoVertex>, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    verts.push(GizmoVertex { position: [a.x, a.y, a.z], color });
    verts.push(GizmoVertex { position: [b.x, b.y, b.z], color });
    verts.push(GizmoVertex { position: [c.x, c.y, c.z], color });
}

// ---------------------------------------------------------------------------
// Pick AABB helpers
// ---------------------------------------------------------------------------

/// Build a world-space AABB around the full gizmo axis (shaft + cone) for ray-picking.
pub fn gizmo_axis_aabb(origin: Vec3, axis: Axis, length: f32, thickness: f32) -> (Vec3, Vec3) {
    let dir = axis.direction();
    let end = origin + dir * length;
    let half = Vec3::splat(thickness);
    let min = origin.min(end) - half;
    let max = origin.max(end) + half;
    (min, max)
}

/// Build a world-space AABB around the gizmo cone tip for easier picking.
pub fn gizmo_tip_aabb(origin: Vec3, axis: Axis, length: f32, thickness: f32) -> (Vec3, Vec3) {
    let tip = origin + axis.direction() * length;
    let ext = Vec3::splat(thickness * 3.0);
    (tip - ext, tip + ext)
}

/// Pick thickness — thicker than before to match the solid mesh geometry.
pub const GIZMO_PICK_THICKNESS: f32 = 0.15;
