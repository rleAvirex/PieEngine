//! First-person fly camera for the editor viewport.

use glam::{Mat3, Quat, Vec3};
use pie_runtime::components::Transform;

/// Scroll-wheel speed adjustment: each notch multiplies/divides speed by this.
pub const SPEED_SCROLL_FACTOR: f32 = 1.15;
const MIN_MOVE_SPEED: f32 = 0.5;
const MAX_MOVE_SPEED: f32 = 80.0;

/// Radians below |1e-6| treated as zero so a near-zero yaw/pitch doesn't
/// produce a degenerate forward vector in `Quat::from_rotation_y` lookups.
const YAW_EPSILON: f32 = 1e-6;

/// Maximum pitch magnitude (~85°) so the camera can't flip past straight
/// up/down, which would invert the forward direction.
const MAX_PITCH: f32 = 1.48353; // ≈ 85° in radians

/// First-person fly camera. Owns yaw/pitch explicitly so mouse-look is
/// frame-rate independent and free of gimbal flip; the active camera entity's
/// `Transform` is rebuilt from this state every frame.
#[derive(Debug, Clone, Copy)]
pub struct EditorCamera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub move_speed: f32,
    pub look_sensitivity: f32,
}

impl EditorCamera {
    pub const DEFAULT_MOVE_SPEED: f32 = 4.0;
    pub const DEFAULT_LOOK_SENSITIVITY: f32 = 0.0025;

    pub fn new(position: Vec3) -> Self {
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
    pub fn from_transform(transform: Transform) -> Self {
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
        let forward = rotation * Vec3::NEG_Z;
        let yaw = f32::atan2(-forward.x, -forward.z);
        let horizontal = Vec3::new(forward.x, 0.0, forward.z).length();
        let pitch = f32::atan2(forward.y, horizontal);
        (yaw, pitch)
    }

    /// Camera right vector projected onto the XZ plane and re-normalised.
    pub fn right_xz(&self) -> Vec3 {
        // Right is always horizontal with the basis-vector construction, so
        // we can derive it directly from yaw.
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        right.normalize_or_zero()
    }

    /// Apply mouse delta (in pixels) to look, clamping pitch.
    pub fn apply_look(&mut self, delta_x: f32, delta_y: f32) {
        self.yaw -= delta_x * self.look_sensitivity;
        self.pitch += delta_y * self.look_sensitivity;
        self.pitch = self.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        if self.yaw.abs() < YAW_EPSILON {
            self.yaw = 0.0;
        }
    }

    /// Integrate movement for this frame. `move_axis` is the net (forward,
    /// right, up) input in [-1, 1]; `dt` is frame delta seconds.
    pub fn apply_movement(&mut self, move_axis: Vec3, dt: f32) {
        if move_axis == Vec3::ZERO {
            return;
        }
        let cos_pitch = self.pitch.cos();
        let forward_3d = Vec3::new(
            self.yaw.sin() * cos_pitch,
            self.pitch.sin(),
            -self.yaw.cos() * cos_pitch,
        ).normalize_or_zero();
        let right_horizontal = self.right_xz();
        let vertical = Vec3::Y * move_axis.z;

        let displacement = (forward_3d * move_axis.x + right_horizontal * move_axis.y + vertical)
            .normalize_or_zero()
            * self.move_speed
            * dt;
        self.position += displacement;
    }

    /// Multiply move speed by `factor`, clamped to [MIN_MOVE_SPEED, MAX_MOVE_SPEED].
    pub fn adjust_speed(&mut self, factor: f32) {
        self.move_speed = (self.move_speed * factor).clamp(MIN_MOVE_SPEED, MAX_MOVE_SPEED);
    }

    /// Rebuild the runtime `Transform` from yaw/pitch/position. Roll is zero.
    ///
    /// Constructs the rotation from basis vectors rather than
    /// `Ry(yaw) * Rx(pitch)` to guarantee zero roll regardless of quaternion
    /// multiplication conventions. The right vector is always horizontal
    /// (cross product of world-up and forward), so the horizon never tilts.
    pub fn into_transform(self) -> Transform {
        let cos_pitch = self.pitch.cos();
        // Forward from yaw/pitch. With yaw=0, pitch=0: forward = -Z.
        let forward = Vec3::new(
            self.yaw.sin() * cos_pitch,
            self.pitch.sin(),
            -self.yaw.cos() * cos_pitch,
        ).normalize_or_zero();
        // Right = world_up × forward, always horizontal (Y=0).
        let right = Vec3::Y.cross(forward).normalize_or_zero();
        // If forward is parallel to world-up (looking straight up/down),
        // fall back to world-right.
        let right = if right.length_squared() > 1e-10 { right } else { Vec3::X };
        // Up = forward × right.
        let up = forward.cross(right);

        // Build rotation matrix from basis vectors (columns = right, up, -forward).
        // glam's from_mat3 takes column-major: [right, up, backward] where
        // backward = -forward (because camera looks down -Z).
        let backward = -forward;
        let rotation = Quat::from_mat3(&glam::Mat3::from_cols(right, up, backward));
        Transform {
            translation: self.position,
            rotation,
            scale: Vec3::ONE,
        }
    }
}
