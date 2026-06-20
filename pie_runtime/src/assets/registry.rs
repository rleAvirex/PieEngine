use crate::assets::error::AssetError;
use crate::assets::handle::AssetKind;
use crate::assets::handle::{Handle, MaterialHandle, MeshHandle, TextureHandle};
use crate::assets::material::MaterialAsset;
use crate::assets::mesh::MeshAsset;
use crate::assets::texture::TextureAsset;

#[derive(Debug, Default)]
pub struct AssetRegistry {
    meshes: Vec<MeshAsset>,
    textures: Vec<TextureAsset>,
    materials: Vec<MaterialAsset>,
}

impl AssetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_mesh(&mut self, mesh: MeshAsset) -> Result<MeshHandle, AssetError> {
        mesh.validate()
            .map_err(|message| AssetError::gltf("mesh", message))?;
        let index = self.meshes.len();
        self.meshes.push(mesh);
        Ok(MeshHandle::new(index as u32))
    }

    pub fn insert_texture(&mut self, texture: TextureAsset) -> Result<TextureHandle, AssetError> {
        texture
            .validate()
            .map_err(|message| AssetError::image("texture", message))?;
        let index = self.textures.len();
        self.textures.push(texture);
        Ok(TextureHandle::new(index as u32))
    }

    pub fn insert_material(&mut self, material: MaterialAsset) -> MaterialHandle {
        let index = self.materials.len();
        self.materials.push(material);
        MaterialHandle::new(index as u32)
    }

    pub fn mesh(&self, handle: MeshHandle) -> Result<&MeshAsset, AssetError> {
        self.get(&self.meshes, handle)
    }

    pub fn texture(&self, handle: TextureHandle) -> Result<&TextureAsset, AssetError> {
        self.get(&self.textures, handle)
    }

    pub fn material(&self, handle: MaterialHandle) -> Result<&MaterialAsset, AssetError> {
        self.get(&self.materials, handle)
    }

    pub fn meshes(&self) -> &[MeshAsset] {
        &self.meshes
    }

    pub fn textures(&self) -> &[TextureAsset] {
        &self.textures
    }

    pub fn materials(&self) -> &[MaterialAsset] {
        &self.materials
    }

    fn get<'a, T, K: AssetKind>(
        &self,
        storage: &'a [T],
        handle: Handle<K>,
    ) -> Result<&'a T, AssetError> {
        storage
            .get(handle.index() as usize)
            .ok_or(AssetError::InvalidHandle {
                kind: K::NAME,
                index: handle.index(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::AssetRegistry;
    use crate::assets::material::MaterialAsset;
    use crate::assets::mesh::{MeshAsset, MeshVertex};
    use crate::assets::texture::TextureAsset;

    #[test]
    fn registry_stores_and_resolves_assets_by_handle() {
        let mut registry = AssetRegistry::new();

        let material =
            registry.insert_material(MaterialAsset::unlit("default", [1.0, 1.0, 1.0, 1.0]));
        let mesh = registry
            .insert_mesh(MeshAsset {
                name: "cube".to_string(),
                vertices: vec![MeshVertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                }],
                indices: vec![0],
                material,
            })
            .expect("mesh should be valid");
        let texture = registry
            .insert_texture(TextureAsset {
                name: "albedo".to_string(),
                width: 1,
                height: 1,
                rgba: vec![255, 128, 64, 255],
            })
            .expect("texture should be valid");

        assert_eq!(registry.mesh(mesh).expect("mesh").name, "cube");
        assert_eq!(registry.texture(texture).expect("texture").width, 1);
        assert_eq!(
            registry.material(material).expect("material").name,
            "default"
        );
    }
}
