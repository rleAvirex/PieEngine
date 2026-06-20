use glam::{Mat4, Quat, Vec3};

use crate::components::Transform;
use crate::core::SimulationCore;

const OPENGL_TO_WGPU_MATRIX: Mat4 = Mat4::from_cols_array(&[
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
]);

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub position: [f32; 4],
}

impl CameraUniform {
    pub fn from_simulation(core: &SimulationCore, aspect_ratio: f32) -> Self {
        let transform = core
            .active_camera()
            .and_then(|entity| core.world().get::<&Transform>(entity).ok())
            .map(|transform| *transform)
            .unwrap_or_default();

        Self {
            view_proj: camera_view_proj(transform, aspect_ratio).to_cols_array_2d(),
            position: [
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
                0.0,
            ],
        }
    }
}

pub fn camera_view_proj(transform: Transform, aspect_ratio: f32) -> Mat4 {
    let world = Mat4::from_scale_rotation_translation(
        transform.scale,
        transform.rotation,
        transform.translation,
    );
    let view = world.inverse();
    let projection = OPENGL_TO_WGPU_MATRIX
        * Mat4::perspective_rh(
            std::f32::consts::FRAC_PI_4,
            aspect_ratio.max(0.01),
            0.1,
            100.0,
        );
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
