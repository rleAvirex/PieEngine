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
pub use loader::fbx::{load_fbx_mesh, load_fbx_meshes};
pub use loader::gltf::{ImportedNode, ImportedScene, load_gltf_scene};
pub use loader::image::{load_texture_from_path, texture_from_rgba};
#[cfg(feature = "cooked-assets")]
pub use loader::pak::load_pak;
pub use loader::pie_mesh::{load_pie_mesh, load_pie_meshes_from_dir};
pub use loader::shader::{load_shader_named, load_shader_source, shader_asset_path};
pub use material::MaterialAsset;
pub use mesh::{MeshAsset, MeshVertex};
pub use pak::{CookedAssetKind, PakAsset, PakFile};
pub use registry::AssetRegistry;
pub use scene_import::{SpawnedScene, bootstrap_scene_result_from_spawned, spawn_imported_scene};
pub use texture::TextureAsset;
