use glam::{Mat4, Quat, Vec3};

use crate::components::{Camera, Transform};
use crate::core::SimulationCore;

const OPENGL_TO_WGPU_MATRIX: Mat4 = Mat4::from_cols_array(&[
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
]);

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub position: [f32; 4],
    // Camera basis vectors + FOV info for sky atmosphere ray reconstruction
    pub world_right: [f32; 4],    // camera right vector in world space
    pub world_up: [f32; 4],       // camera up vector in world space
    pub world_forward: [f32; 4],  // camera forward vector in world space
    pub tan_half_fov: f32,        // tan(fov / 2)
    pub aspect: f32,              // aspect ratio
    _padding: [f32; 2],           // pad to 16-byte alignment
}

impl CameraUniform {
    /// Create a CameraUniform from a pre-built view-projection matrix and camera position.
    /// Used for cubemap face rendering where we construct view matrices manually.
    ///
    /// The basis vectors are derived from `V⁻¹` (not `(P·V)⁻¹`) so they
    /// represent the camera's world-space orientation, independent of the
    /// projection. The previous implementation extracted them from
    /// `(P·V)⁻¹ = V⁻¹·P⁻¹`, which contaminated the basis with the
    /// projection's z scaling — the resulting `forward` was a multiple of
    /// `cam_pos` rather than the face's actual forward direction, so all six
    /// cubemap faces ended up rendering essentially the same view.
    pub fn from_view_proj(view_proj: Mat4, position: Vec3, aspect_ratio: f32, fov: f32) -> Self {
        // view_proj = P · V ⇒ V = P⁻¹ · view_proj ⇒ V⁻¹ = view_proj⁻¹ · P.
        // The camera basis vectors are the columns of V⁻¹, so we multiply the
        // inverse of view_proj by P on the right to undo the projection before
        // extracting the basis.
        let proj = Mat4::perspective_rh(fov.max(0.01), aspect_ratio.max(0.01), 0.1, 100.0);
        let view = proj.inverse() * view_proj;
        let view_inv = view.inverse();
        let right = view_inv.x_axis.truncate();
        let up = view_inv.y_axis.truncate();
        let forward = -view_inv.z_axis.truncate(); // view space -Z maps to world forward

        Self {
            view_proj: view_proj.to_cols_array_2d(),
            position: [position.x, position.y, position.z, 0.0],
            world_right: [right.x, right.y, right.z, 0.0],
            world_up: [up.x, up.y, up.z, 0.0],
            world_forward: [forward.x, forward.y, forward.z, 0.0],
            tan_half_fov: (fov * 0.5).tan(),
            aspect: aspect_ratio,
            _padding: [0.0; 2],
        }
    }

    pub fn from_simulation(core: &SimulationCore, aspect_ratio: f32) -> Self {
        // Query the active camera once instead of twice — the old code called
        // core.active_camera() twice and world().get twice.
        let active_camera_entity = core.active_camera();
        let (transform, fov) = match active_camera_entity {
            Some(entity) => {
                let transform = core.world().get::<&Transform>(entity).ok().map(|t| *t);
                let fov = core
                    .world()
                    .get::<&Camera>(entity)
                    .ok()
                    .map(|c| c.fov)
                    .unwrap_or_else(|| Camera::default().fov);
                (transform, fov)
            }
            None => (None, Camera::default().fov),
        };
        let transform = transform.unwrap_or_default();

        // Extract camera basis vectors from the rotation quaternion, then
        // RE-ORTHOGONALIZE so world_right is always horizontal (Y=0). This
        // guarantees the sky shader's view-ray reconstruction
        // (forward + right*ndc.x + up*ndc.y) can never produce a tilted
        // horizon, even if the source rotation has tiny floating-point roll
        // or was authored with roll (e.g. from a glTF camera).
        let rot = transform.rotation;
        let forward_raw = rot * Vec3::NEG_Z; // camera looks down -Z in view space
        let forward = forward_raw.normalize_or_zero();
        // right = forward × world_up — always horizontal (Y=0).
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        // Degenerate when looking straight up/down — fall back to +X.
        let right = if right.length_squared() > 1e-10 { right } else { Vec3::X };
        // up = right × forward (re-orthogonalized, guaranteed ⊥ to right and forward).
        let up = right.cross(forward);

        Self {
            view_proj: camera_view_proj(transform, aspect_ratio, fov).to_cols_array_2d(),
            position: [
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
                0.0,
            ],
            world_right: [right.x, right.y, right.z, 0.0],
            world_up: [up.x, up.y, up.z, 0.0],
            world_forward: [forward.x, forward.y, forward.z, 0.0],
            tan_half_fov: (fov * 0.5).tan(),
            aspect: aspect_ratio,
            _padding: [0.0; 2],
        }
    }
}

pub fn camera_view_proj(transform: Transform, aspect_ratio: f32, fov: f32) -> Mat4 {
    let world = Mat4::from_scale_rotation_translation(
        transform.scale,
        transform.rotation,
        transform.translation,
    );
    let view = world.inverse();
    let projection = OPENGL_TO_WGPU_MATRIX
        * Mat4::perspective_rh(fov.max(0.01), aspect_ratio.max(0.01), 0.1, 100.0);
    projection * view
}

pub fn look_at_camera_transform(eye: Vec3, target: Vec3) -> Transform {
    let forward = (target - eye).normalize_or_zero();
    let rotation = if forward.length_squared() > f32::EPSILON {
        Quat::from_rotation_arc(Vec3::NEG_Z, forward)
    } else {
        Quat::IDENTITY
    };

    Transform {
        translation: eye,
        rotation,
        scale: Vec3::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::{CameraUniform, look_at_camera_transform};
    use crate::components::Transform;
    use crate::core::SimulationCore;
    use glam::Vec3;

    #[test]
    fn camera_uniform_uses_active_camera_transform() {
        let mut core = SimulationCore::new();
        let camera = core.bootstrap_scene();

        core.world_mut()
            .insert_one(
                camera,
                Transform::from_translation(Vec3::new(0.0, 1.0, 5.0)),
            )
            .expect("camera entity should exist");

        let uniform = CameraUniform::from_simulation(&core, 16.0 / 9.0);
        let matrix = glam::Mat4::from_cols_array_2d(&uniform.view_proj);

        assert!(matrix.w_axis.w.abs() > f32::EPSILON);
    }

    #[test]
    fn look_at_camera_transform_points_neg_z_at_target() {
        let transform = look_at_camera_transform(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let forward = transform.rotation * Vec3::NEG_Z;

        assert!((forward - Vec3::NEG_Z).length() < 1e-4);
    }
}
