//! Gizmo — axis enum, interaction state, vertex generation, and pick AABBs.

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
    /// Actively dragging an axis. Stores the axis, the entity's world-space
    /// position when the drag started, and the camera-space axis orientation
    /// at the time of the initial click (so dragging feels locked).
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

/// Generate gizmo vertex data: 3 colored axis lines + diamond tips.
///
/// Returns `(vertices, counts_per_axis)` where `counts_per_axis[i]`
/// is the vertex count for axis `Axis::ALL[i]`.
pub fn build_gizmo_vertices(
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
pub fn gizmo_axis_aabb(origin: Vec3, axis: Axis, length: f32, thickness: f32) -> (Vec3, Vec3) {
    let dir = axis.direction();
    let end = origin + dir * length;
    let half = Vec3::splat(thickness);
    let min = origin.min(end) - half;
    let max = origin.max(end) + half;
    (min, max)
}

/// Build a thin diamond AABB around the gizmo tip for easier picking.
pub fn gizmo_tip_aabb(origin: Vec3, axis: Axis, length: f32, thickness: f32) -> (Vec3, Vec3) {
    let tip = origin + axis.direction() * length;
    let ext = Vec3::splat(thickness * 2.5);
    (tip - ext, tip + ext)
}

/// Half-extent of the pickable AABBs around each gizmo axis.
pub const GIZMO_PICK_THICKNESS: f32 = 0.06;
