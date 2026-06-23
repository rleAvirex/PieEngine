//! Picking — screen ray vs. world AABB intersection for entity and gizmo selection.

use glam::{Mat3, Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use hecs::Entity;
use pie_runtime::components::Transform;

/// Local-space bounds of a pickable mesh, cached at scene load so each pick
/// only pays for the world transform, not a full vertex scan.
#[derive(Debug, Clone, Copy)]
pub struct PickableBounds {
    pub entity: Entity,
    pub local_min: Vec3,
    pub local_max: Vec3,
}

/// World-space ray-AABB intersection. Returns the near hit distance `t >= 0`,
/// or `None` if the ray misses. `dir` need not be normalized; the returned `t`
/// is in the same units.
pub fn ray_aabb_hit(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;

    for i in 0..3 {
        let o = origin[i];
        let d = dir[i];
        let lo = min[i];
        let hi = max[i];

        if d.abs() < 1e-8 {
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

    if t_max < 0.0 {
        None
    } else {
        Some(t_min.max(0.0))
    }
}

/// Transform a local-space AABB into its world-space axis-aligned equivalent.
pub fn world_aabb(local_min: Vec3, local_max: Vec3, transform: &Transform) -> (Vec3, Vec3) {
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
///
/// If `view_proj` is non-invertible (e.g. camera has a zero scale component
/// or a degenerate FOV), returns a zero-origin, zero-direction ray so the
/// caller's `ray_aabb_hit` simply reports no hits — silently failing to pick
/// is much easier to diagnose than NaN propagation through the pick logic.
pub fn screen_ray_from_ndc(ndc: Vec2, view_proj: Mat4) -> (Vec3, Vec3) {
    let inv = view_proj.inverse();
    // glam's Mat4::inverse returns a matrix of NaNs if the input is singular.
    // Detect that case by checking one element; if any component is NaN, the
    // whole inverse is corrupt and we should bail out with a no-hit ray.
    if inv.x_axis.x.is_nan() || inv.x_axis.w.is_nan() || inv.w_axis.w.is_nan() {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    let near_point = inv * Vec4::new(ndc.x, ndc.y, -1.0, 1.0);
    let far_point = inv * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);

    // Guard against zero w (degenerate homogeneous coordinate).
    if near_point.w.abs() < 1e-12 || far_point.w.abs() < 1e-12 {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    let near = near_point.xyz() / near_point.w;
    let far = far_point.xyz() / far_point.w;
    let dir = far - near;
    (near, dir)
}

/// Convert a click position within an egui rect to a normalized device coordinate.
pub fn viewport_ndc_from_rect(rect: egui::Rect, click_pos: egui::Pos2) -> Option<Vec2> {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }

    Some(Vec2::new(
        ((click_pos.x - rect.min.x) / rect.width()) * 2.0 - 1.0,
        1.0 - ((click_pos.y - rect.min.y) / rect.height()) * 2.0,
    ))
}
