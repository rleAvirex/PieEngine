use glam::{Quat, Vec3};

/// Position, rotation, and scale of an entity in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Self::default()
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

/// Linear velocity in world units per second.
///
/// Consumed by the movement system each fixed step:
/// `translation += velocity * fixed_timestep_seconds`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Velocity(pub Vec3);

/// Marks the entity used as the active render/gameplay camera.
///
/// A tag rather than data so multiple cameras can exist with only one
/// marked active at a time; later milestones can swap which entity carries
/// this tag instead of mutating camera state directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActiveCamera;

/// Camera parameters that control how the scene is projected.
///
/// Attach this alongside `ActiveCamera` to configure the perspective
/// projection. The `fov` field is the vertical field of view in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Vertical field of view in radians. Default is π/4 (45°).
    pub fov: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov: std::f32::consts::FRAC_PI_4, // 45°
        }
    }
}

impl Camera {
    /// Create a camera with the given vertical FOV in radians.
    pub fn with_fov(fov: f32) -> Self {
        Self { fov: fov.max(0.01) }
    }
}

/// A single directional light used for the baseline lit renderer.
///
/// When `atmosphere_sun_light` is enabled, this light drives the sun
/// direction in the Sky Atmosphere component (UE5-style). The optional
/// `sun_light_index` selects which atmospheric light slot this occupies
/// (0 = primary sun, 1 = secondary e.g. moon).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionalLight {
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    /// Whether this light acts as the atmospheric sun for Sky Atmosphere.
    pub atmosphere_sun_light: bool,
    /// Index into the atmospheric light array (0 or 1).
    pub sun_light_index: u32,
}

impl DirectionalLight {
    pub fn new(direction: Vec3, color: Vec3, intensity: f32) -> Self {
        Self {
            direction: direction.normalize_or_zero(),
            color,
            intensity,
            atmosphere_sun_light: false,
            sun_light_index: 0,
        }
    }

    /// Create a directional light that also acts as the atmospheric sun.
    pub fn new_sun(direction: Vec3, color: Vec3, intensity: f32, index: u32) -> Self {
        Self {
            direction: direction.normalize_or_zero(),
            color,
            intensity,
            atmosphere_sun_light: true,
            sun_light_index: index,
        }
    }
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self::new_sun(Vec3::new(-0.35, -1.0, -0.2), Vec3::splat(1.0), 40.0, 0)
    }
}

/// Sky Light component — captures the sky atmosphere and applies it as
/// indirect (ambient) lighting to objects, similar to UE5's Sky Light actor.
///
/// The viewport renderer manages the actual cubemap capture; this component
/// stores the user-facing settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyLight {
    /// Overall intensity multiplier for the sky light contribution.
    pub intensity: f32,
    /// Whether to capture the sky in real-time each frame (dynamic time-of-day).
    /// When false, the cubemap is only captured on demand (recapture).
    pub real_time_capture: bool,
    /// Resolution of the captured cubemap (per-face). Lower = faster.
    pub capture_resolution: u32,
}

impl Default for SkyLight {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            real_time_capture: true,
            capture_resolution: 64,
        }
    }
}

/// References a mesh/material pair loaded through the asset registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshRenderer {
    pub mesh: crate::assets::MeshHandle,
}

/// A human-readable identifier for debugging, editor display, and lookup.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Name(pub String);

impl Name {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[cfg(test)]
mod tests {
    use super::{ActiveCamera, Camera, DirectionalLight, Name, Transform, Velocity};
    use glam::{Quat, Vec3};

    #[test]
    fn transform_default_is_identity() {
        let transform = Transform::default();

        assert_eq!(transform.translation, Vec3::ZERO);
        assert_eq!(transform.rotation, Quat::IDENTITY);
        assert_eq!(transform.scale, Vec3::ONE);
    }

    #[test]
    fn transform_from_translation_keeps_default_rotation_and_scale() {
        let transform = Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));

        assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(transform.rotation, Quat::IDENTITY);
        assert_eq!(transform.scale, Vec3::ONE);
    }

    #[test]
    fn velocity_default_is_zero() {
        assert_eq!(Velocity::default(), Velocity(Vec3::ZERO));
    }

    #[test]
    fn name_new_accepts_str_and_string() {
        assert_eq!(Name::new("camera"), Name("camera".to_string()));
        assert_eq!(
            Name::new(String::from("camera")),
            Name("camera".to_string())
        );
    }

    #[test]
    fn active_camera_is_a_zero_sized_marker() {
        let _marker = ActiveCamera;
    }

    #[test]
    fn camera_default_fov_is_45_degrees() {
        let cam = Camera::default();
        assert!((cam.fov - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn camera_with_fov_clamps_to_positive() {
        let cam = Camera::with_fov(-1.0);
        assert!(cam.fov > 0.0);
    }

    #[test]
    fn directional_light_normalizes_direction() {
        let light = DirectionalLight::new(Vec3::new(0.0, -4.0, 0.0), Vec3::ONE, 2.0);

        assert!((light.direction.length() - 1.0).abs() < 1e-6);
        assert_eq!(light.color, Vec3::ONE);
        assert_eq!(light.intensity, 2.0);
        assert!(!light.atmosphere_sun_light);
    }

    #[test]
    fn directional_light_sun_variant() {
        let light = DirectionalLight::new_sun(Vec3::new(0.0, -1.0, 0.0), Vec3::ONE, 5.0, 0);
        assert!(light.atmosphere_sun_light);
        assert_eq!(light.sun_light_index, 0);
    }

    #[test]
    fn sky_light_default() {
        let sl = SkyLight::default();
        assert_eq!(sl.intensity, 1.0);
        assert!(sl.real_time_capture);
        assert_eq!(sl.capture_resolution, 64);
    }
}
