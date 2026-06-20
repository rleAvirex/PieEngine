mod error;
mod handle;
mod loader;
mod material;
mod mesh;
mod pak;
mod registry;
mod scene_import;
mod texture;

pub use error::AssetError;
pub use handle::{
    Handle, MaterialAssetKind, MaterialHandle, MeshAssetKind, MeshHandle, TextureAssetKind,
    TextureHandle,
};
#[cfg(feature = "cooked-assets")]
pub use loader::pak::load_pak;
pub use loader::gltf::{ImportedNode, ImportedScene, load_gltf_scene};
pub use loader::image::{load_texture_from_path, texture_from_rgba};
pub use loader::shader::{load_shader_named, load_shader_source, shader_asset_path};
pub use material::MaterialAsset;
pub use mesh::{MeshAsset, MeshVertex};
pub use pak::{CookedAssetKind, PakAsset, PakFile};
pub use registry::AssetRegistry;
pub use scene_import::{SpawnedScene, bootstrap_scene_result_from_spawned, spawn_imported_scene};
pub use texture::TextureAsset;
