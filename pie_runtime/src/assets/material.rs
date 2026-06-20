use crate::assets::TextureHandle;

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialAsset {
    pub name: String,
    pub base_color_texture: Option<TextureHandle>,
    pub normal_texture: Option<TextureHandle>,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
}

impl MaterialAsset {
    pub fn pbr(
        name: impl Into<String>,
        color: [f32; 4],
        metallic_factor: f32,
        roughness_factor: f32,
    ) -> Self {
        Self {
            name: name.into(),
            base_color_texture: None,
            normal_texture: None,
            base_color_factor: color,
            metallic_factor,
            roughness_factor,
        }
    }

    pub fn unlit(name: impl Into<String>, color: [f32; 4]) -> Self {
        Self {
            name: name.into(),
            base_color_texture: None,
            normal_texture: None,
            base_color_factor: color,
            metallic_factor: 0.0,
            roughness_factor: 1.0,
        }
    }
}
