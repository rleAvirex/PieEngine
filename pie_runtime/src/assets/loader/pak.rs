use std::io::Read;
use std::path::Path;

use crate::assets::{
    error::AssetError, material::MaterialAsset, mesh::MeshAsset, registry::AssetRegistry,
    texture::TextureAsset,
};

/// Load a cooked `.pak` file and populate an `AssetRegistry`.
///
/// The pak format is a simple binary container:
///   [magic: "PIE\0"][version: u32 LE][asset_count: u32 LE]
///   For each asset: [kind: u8][name_len: u32 LE][name: [u8]][data_len: u64 LE][data: [u8]]
///
/// Cooked data is already in the runtime's native layout — no image decode or
/// glTF parse is needed at load time.
pub fn load_pak(path: &Path) -> Result<AssetRegistry, AssetError> {
    let mut file = std::fs::File::open(path).map_err(|error| AssetError::io(path, error))?;

    // Read the entire file into memory for parsing.
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|error| AssetError::io(path, error))?;

    // Parse header manually so we control the read cursor.
    let mut offset = 0;

    if data.len() < 12 {
        return Err(AssetError::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, "pak file too short"),
        ));
    }

    let magic = &data[0..4];
    if magic != b"PIE\0" {
        return Err(AssetError::io(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid pak magic: expected PIE\\0, got {:?}", magic),
            ),
        ));
    }
    offset += 4;

    let version = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    offset += 4;
    if version != 1 {
        return Err(AssetError::io(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported pak version: {version}"),
            ),
        ));
    }

    let asset_count = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4;

    // Helper: read a slice of `len` bytes, advancing offset.
    fn read_bytes<'a>(
        data: &'a [u8],
        offset: &mut usize,
        len: usize,
        path: &Path,
    ) -> Result<&'a [u8], AssetError> {
        if *offset + len > data.len() {
            return Err(AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "truncated pak data at offset {} (need {len}, have {})",
                        *offset,
                        data.len() - *offset
                    ),
                ),
            ));
        }
        let slice = &data[*offset..*offset + len];
        *offset += len;
        Ok(slice)
    }

    // First pass: read all asset entries.
    struct RawEntry {
        kind: u8,
        name: String,
        data: Vec<u8>,
    }

    let mut raw_entries = Vec::with_capacity(asset_count);
    for _ in 0..asset_count {
        let kind_slice = read_bytes(&data, &mut offset, 1, path)?;
        let kind = kind_slice[0];

        let name_len =
            u32::from_le_bytes(read_bytes(&data, &mut offset, 4, path)?.try_into().unwrap())
                as usize;
        let name_bytes = read_bytes(&data, &mut offset, name_len, path)?;
        let name = String::from_utf8(name_bytes.to_vec()).map_err(|error| {
            AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid asset name UTF-8: {error}"),
                ),
            )
        })?;

        let data_len =
            u64::from_le_bytes(read_bytes(&data, &mut offset, 8, path)?.try_into().unwrap())
                as usize;
        let entry_data = read_bytes(&data, &mut offset, data_len, path)?.to_vec();

        raw_entries.push(RawEntry {
            kind,
            name,
            data: entry_data,
        });
    }

    // Second pass: load textures first, then materials, then meshes.
    let mut registry = AssetRegistry::new();
    let mut texture_handles = Vec::new();

    // Textures (kind 0x02)
    for entry in &raw_entries {
        if entry.kind != 0x02 {
            continue;
        }
        let texture = decode_texture(&entry.name, &entry.data).map_err(|message| {
            AssetError::io(
                path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, message),
            )
        })?;
        let handle = registry.insert_texture(texture).map_err(|error| {
            AssetError::io(
                path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()),
            )
        })?;
        texture_handles.push(handle);
    }

    // Materials (kind 0x04)
    for entry in &raw_entries {
        if entry.kind != 0x04 {
            continue;
        }
        let material =
            decode_material(&entry.name, &entry.data, &texture_handles).map_err(|message| {
                AssetError::io(
                    path,
                    std::io::Error::new(std::io::ErrorKind::InvalidData, message),
                )
            })?;
        registry.insert_material(material);
    }

    // Meshes (kind 0x01)
    for entry in &raw_entries {
        if entry.kind != 0x01 {
            continue;
        }
        let mesh = decode_mesh(&entry.name, &entry.data).map_err(|message| {
            AssetError::io(
                path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, message),
            )
        })?;
        registry.insert_mesh(mesh).map_err(|error| {
            AssetError::io(
                path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()),
            )
        })?;
    }

    // Shaders (kind 0x03) — no-op for v1; stored for export completeness.

    Ok(registry)
}

fn decode_texture(name: &str, data: &[u8]) -> Result<TextureAsset, String> {
    if data.len() < 8 {
        return Err(format!("texture '{name}' data too short"));
    }
    let width = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let rgba = data[8..].to_vec();

    let texture = TextureAsset {
        name: name.to_string(),
        width,
        height,
        rgba,
    };
    texture
        .validate()
        .map_err(|message| format!("texture '{name}': {message}"))?;
    Ok(texture)
}

fn decode_material(
    name: &str,
    data: &[u8],
    texture_handles: &[crate::assets::TextureHandle],
) -> Result<MaterialAsset, String> {
    // Layout: base_color_factor [f32; 4] + metallic f32 + roughness f32 + base_tex_idx u32 + normal_tex_idx u32
    let expected_len = 4 * 4 + 4 + 4 + 4 + 4; // 28 bytes
    if data.len() < expected_len {
        return Err(format!("material '{name}' data too short: {}", data.len()));
    }

    let mut off = 0;
    let read_f32 = |off: &mut usize| -> f32 {
        let value = f32::from_le_bytes(data[*off..*off + 4].try_into().unwrap());
        *off += 4;
        value
    };
    let read_u32 = |off: &mut usize| -> u32 {
        let value = u32::from_le_bytes(data[*off..*off + 4].try_into().unwrap());
        *off += 4;
        value
    };

    let base_color_factor = [
        read_f32(&mut off),
        read_f32(&mut off),
        read_f32(&mut off),
        read_f32(&mut off),
    ];
    let metallic_factor = read_f32(&mut off);
    let roughness_factor = read_f32(&mut off);
    let base_tex_idx = read_u32(&mut off);
    let normal_tex_idx = read_u32(&mut off);

    let base_color_texture = if base_tex_idx != u32::MAX {
        texture_handles.get(base_tex_idx as usize).copied()
    } else {
        None
    };
    let normal_texture = if normal_tex_idx != u32::MAX {
        texture_handles.get(normal_tex_idx as usize).copied()
    } else {
        None
    };

    Ok(MaterialAsset {
        name: name.to_string(),
        base_color_texture,
        normal_texture,
        base_color_factor,
        metallic_factor,
        roughness_factor,
    })
}

fn decode_mesh(name: &str, data: &[u8]) -> Result<MeshAsset, String> {
    let vertex_size = std::mem::size_of::<crate::assets::MeshVertex>(); // 48 bytes

    if data.len() < 8 {
        return Err(format!("mesh '{name}' data too short"));
    }

    let mut off = 0;
    let read_u32 = |off: &mut usize| -> u32 {
        let value = u32::from_le_bytes(data[*off..*off + 4].try_into().unwrap());
        *off += 4;
        value
    };

    // Material handle index — we store it directly since the registry uses
    // insertion-order handles (index 0, 1, 2, ...). The cooker writes material
    // indices in the same order.
    let material_idx = read_u32(&mut off) as usize;
    let vertex_count = read_u32(&mut off) as usize;
    let vertices_data_len = vertex_count * vertex_size;

    if data.len() < off + vertices_data_len {
        return Err(format!(
            "mesh '{name}': vertex data truncated (need {vertices_data_len} bytes, have {})",
            data.len() - off
        ));
    }

    // SAFETY: MeshVertex is repr(C) with bytemuck::Pod — valid for any bit pattern.
    let vertices = {
        let vertex_bytes = &data[off..off + vertices_data_len];
        bytemuck::cast_slice::<u8, crate::assets::MeshVertex>(vertex_bytes).to_vec()
    };
    off += vertices_data_len;

    if data.len() < off + 4 {
        return Err(format!("mesh '{name}': missing index count"));
    }
    let index_count = read_u32(&mut off) as usize;
    let indices_data_len = index_count * 4;

    if data.len() < off + indices_data_len {
        return Err(format!(
            "mesh '{name}': index data truncated (need {indices_data_len} bytes, have {})",
            data.len() - off
        ));
    }

    let indices = {
        let index_bytes = &data[off..off + indices_data_len];
        bytemuck::cast_slice::<u8, u32>(index_bytes).to_vec()
    };

    // Build a MaterialHandle from the stored index.
    let material = crate::assets::MaterialHandle::new(material_idx as u32);

    Ok(MeshAsset {
        name: name.to_string(),
        vertices,
        indices,
        material,
    })
}

#[cfg(test)]
mod tests {
    use super::load_pak;
    use crate::assets::PakFile;

    /// Helper: build a minimal valid pak with one texture + one material + one mesh.
    fn build_test_pak() -> Vec<u8> {
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice(b"PIE\0");
        buf.extend_from_slice(&1u32.to_le_bytes()); // version
        buf.extend_from_slice(&3u32.to_le_bytes()); // asset count

        // Texture: 2x2 RGBA white
        buf.push(0x02); // kind = texture
        buf.extend_from_slice(&6u32.to_le_bytes()); // name len
        buf.extend_from_slice(b"albedo"); // name
        let mut tex_data = Vec::new();
        tex_data.extend_from_slice(&2u32.to_le_bytes()); // width
        tex_data.extend_from_slice(&2u32.to_le_bytes()); // height
        tex_data.extend_from_slice(&[255u8; 16]); // 2*2*4 = 16 bytes RGBA
        buf.extend_from_slice(&(tex_data.len() as u64).to_le_bytes());
        buf.extend_from_slice(&tex_data);

        // Material
        buf.push(0x04); // kind = material
        buf.extend_from_slice(&7u32.to_le_bytes()); // name len
        buf.extend_from_slice(b"default"); // name
        let mut mat_data = Vec::new();
        mat_data.extend_from_slice(&1.0f32.to_le_bytes()); // base_color_factor r
        mat_data.extend_from_slice(&1.0f32.to_le_bytes()); // g
        mat_data.extend_from_slice(&1.0f32.to_le_bytes()); // b
        mat_data.extend_from_slice(&1.0f32.to_le_bytes()); // a
        mat_data.extend_from_slice(&0.0f32.to_le_bytes()); // metallic
        mat_data.extend_from_slice(&1.0f32.to_le_bytes()); // roughness
        mat_data.extend_from_slice(&0u32.to_le_bytes()); // base_color_texture index = 0
        mat_data.extend_from_slice(&u32::MAX.to_le_bytes()); // normal_texture = none
        buf.extend_from_slice(&(mat_data.len() as u64).to_le_bytes());
        buf.extend_from_slice(&mat_data);

        // Mesh: a single triangle
        buf.push(0x01); // kind = mesh
        buf.extend_from_slice(&4u32.to_le_bytes()); // name len
        buf.extend_from_slice(b"cube"); // name
        let mut mesh_data = Vec::new();
        mesh_data.extend_from_slice(&0u32.to_le_bytes()); // material index = 0
        mesh_data.extend_from_slice(&3u32.to_le_bytes()); // vertex count = 3
        // 3 vertices, each is MeshVertex (48 bytes)
        for _ in 0..3 {
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // pos x
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // pos y
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // pos z
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // normal x
            mesh_data.extend_from_slice(&1.0f32.to_le_bytes()); // normal y
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // normal z
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // uv u
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // uv v
            mesh_data.extend_from_slice(&1.0f32.to_le_bytes()); // tangent x
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // tangent y
            mesh_data.extend_from_slice(&0.0f32.to_le_bytes()); // tangent z
            mesh_data.extend_from_slice(&1.0f32.to_le_bytes()); // tangent w
        }
        mesh_data.extend_from_slice(&3u32.to_le_bytes()); // index count = 3
        mesh_data.extend_from_slice(&0u32.to_le_bytes()); // index 0
        mesh_data.extend_from_slice(&1u32.to_le_bytes()); // index 1
        mesh_data.extend_from_slice(&2u32.to_le_bytes()); // index 2
        buf.extend_from_slice(&(mesh_data.len() as u64).to_le_bytes());
        buf.extend_from_slice(&mesh_data);

        buf
    }

    #[test]
    fn load_pak_round_trip() {
        let data = build_test_pak();
        let pak = PakFile::read(&mut data.as_slice()).expect("read should succeed");

        // Write it back out and reload via the runtime loader.
        let mut rewritten = Vec::new();
        pak.write(&mut rewritten).expect("write should succeed");

        // Write to a temp file and load via load_pak.
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("pie_test_pak.bin");
        std::fs::write(&temp_path, &rewritten).expect("write temp file");

        let registry = load_pak(&temp_path).expect("load_pak should succeed");

        assert_eq!(registry.textures().len(), 1);
        assert_eq!(registry.materials().len(), 1);
        assert_eq!(registry.meshes().len(), 1);

        let texture = registry
            .texture(crate::assets::TextureHandle::new(0))
            .expect("texture handle should be valid");
        assert_eq!(texture.width, 2);
        assert_eq!(texture.height, 2);

        let material = registry
            .material(crate::assets::MaterialHandle::new(0))
            .expect("material handle should be valid");
        assert_eq!(material.name, "default");
        assert!(material.base_color_texture.is_some());

        let mesh = registry
            .mesh(crate::assets::MeshHandle::new(0))
            .expect("mesh handle should be valid");
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.indices.len(), 3);

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn load_pak_rejects_invalid_magic() {
        // Must be at least 12 bytes to reach the magic check (4 magic + 4 version + 4 count).
        let mut data = b"NOTPK".to_vec(); // 5 bytes, not "PIE\0"
        data.extend_from_slice(&0u32.to_le_bytes()); // version
        data.extend_from_slice(&0u32.to_le_bytes()); // asset count

        let temp_path = std::env::temp_dir().join("pie_test_bad_magic.bin");
        std::fs::write(&temp_path, &data).expect("write temp file");

        let error = load_pak(&temp_path).expect_err("should fail");
        assert!(error.to_string().contains("invalid pak magic"));

        let _ = std::fs::remove_file(&temp_path);
    }
}
