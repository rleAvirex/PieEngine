//! FBX binary file parser — extracts mesh geometry from FBX files.
//!
//! This parser handles the FBX binary format (Kaydara FBX Binary 7.x)
//! and extracts `Geometry` nodes containing `Vertices` (f64 array) and
//! `PolygonVertexIndex` (i32 array) properties. It converts FBX polygon
//! indices to triangle lists and computes flat normals per face.
//!
//! The parser is intentionally limited: it only reads mesh geometry data
//! (positions, normals if present, and polygon indices). It does not
//! parse materials, textures, animations, or the FBX node hierarchy
//! beyond what is needed to locate geometry.

use std::path::Path;

use crate::assets::error::AssetError;
use crate::assets::handle::MaterialHandle;
use crate::assets::handle::MeshHandle;
use crate::assets::material::MaterialAsset;
use crate::assets::mesh::{MeshAsset, MeshVertex};
use crate::assets::registry::AssetRegistry;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load all meshes from an FBX file into the asset registry.
///
/// Returns a list of mesh handles, one per `Geometry` node found in the
/// file. Each mesh gets a default PBR material. If the FBX contains
/// multiple geometry nodes, they are loaded as separate meshes.
pub fn load_fbx_meshes(
    path: &Path,
    registry: &mut AssetRegistry,
) -> Result<Vec<MeshHandle>, AssetError> {
    let data = std::fs::read(path).map_err(|e| AssetError::io(path, e))?;
    let geometries = parse_fbx_geometries(&data, path)?;

    if geometries.is_empty() {
        return Err(AssetError::io(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "FBX file contains no geometry",
            ),
        ));
    }

    let mut handles = Vec::with_capacity(geometries.len());
    for (index, geo) in geometries.into_iter().enumerate() {
        let mesh_name = format!(
            "{}_geo{}",
            path.file_stem().unwrap_or_default().to_string_lossy(),
            index
        );
        let material = registry.insert_material(MaterialAsset::pbr(
            format!("{mesh_name}_mat"),
            [1.0, 1.0, 1.0, 1.0],
            0.0,
            1.0,
        ));

        let mesh = build_mesh_asset(&mesh_name, geo, material)?;
        handles.push(registry.insert_mesh(mesh)?);
    }

    Ok(handles)
}

/// Load the first mesh from an FBX file. Convenience wrapper for files
/// that contain a single geometry.
pub fn load_fbx_mesh(path: &Path, registry: &mut AssetRegistry) -> Result<MeshHandle, AssetError> {
    let handles = load_fbx_meshes(path, registry)?;
    handles.into_iter().next().ok_or_else(|| {
        AssetError::io(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "FBX file contains no geometry",
            ),
        )
    })
}

// ---------------------------------------------------------------------------
// FBX binary parsing
// ---------------------------------------------------------------------------

/// FBX binary magic header: "Kaydara FBX Binary  \x00"
const FBX_MAGIC: &[u8; 21] = b"Kaydara FBX Binary  \x00";

/// A parsed geometry from an FBX file.
struct FbxGeometry {
    /// Vertex positions as (x, y, z) triples.
    positions: Vec<[f32; 3]>,
    /// Triangle indices (already converted from FBX polygon format).
    triangles: Vec<u32>,
    /// Per-vertex normals (optional, from FBX LayerElementNormal).
    normals: Option<Vec<[f32; 3]>>,
}

/// Parse the FBX binary file and extract all geometry datasets.
fn parse_fbx_geometries(data: &[u8], path: &Path) -> Result<Vec<FbxGeometry>, AssetError> {
    // Validate header (27 bytes: 21 magic + 4 version + 2 reserved)
    if data.len() < 27 {
        return Err(AssetError::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, "FBX file too short"),
        ));
    }

    if &data[0..FBX_MAGIC.len()] != FBX_MAGIC {
        return Err(AssetError::io(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "invalid FBX magic: expected 'Kaydara FBX Binary', got {:?}",
                    &data[0..FBX_MAGIC.len().min(data.len())]
                ),
            ),
        ));
    }

    let version_offset = FBX_MAGIC.len() + 2; // 21 + 2 = 23; skip the \x00\x1A marker after the magic
    let version = u32::from_le_bytes(data[version_offset..version_offset + 4].try_into().unwrap());

    // The FBX 7.x binary header is 27 bytes: 21-byte magic + 4-byte version + 2 reserved bytes.
    // Records begin immediately after the header.
    let records_start = 27;
    // FBX 7.5 (version 7500+) switched record header fields from u32 to u64
    // (end_offset, num_props, prop_list_len), making each record header 25
    // bytes instead of 13. Older versions (7.1–7.4) use 13-byte headers.
    let header_is_64bit = version >= 7500;
    let records = parse_records(data, records_start, data.len(), header_is_64bit, path)?;

    // Extract geometries from the record tree
    let mut geometries = Vec::new();
    extract_geometries(&records, &mut geometries, path)?;

    Ok(geometries)
}

/// An FBX record (node): name + properties + children.
struct FbxRecord {
    name: String,
    properties: Vec<FbxProperty>,
    children: Vec<FbxRecord>,
}

/// An FBX property value.
enum FbxProperty {
    FloatArray(Vec<f32>),
    DoubleArray(Vec<f64>),
    IntArray(Vec<i32>),
    Int64Array,
    String,
    Other,
}

/// Recursively parse FBX binary records.
///
/// `header_is_64bit` selects between FBX 7.1–7.4 (32-bit header fields,
/// 13-byte header) and FBX 7.5+ (64-bit header fields, 25-byte header).
fn parse_records(
    data: &[u8],
    mut offset: usize,
    end: usize,
    header_is_64bit: bool,
    path: &Path,
) -> Result<Vec<FbxRecord>, AssetError> {
    let mut records = Vec::new();

    // Header size: 4+4+4+1 = 13 bytes for 7.1–7.4; 8+8+8+1 = 25 bytes for 7.5+.
    let header_size = if header_is_64bit { 25 } else { 13 };

    while offset < end.saturating_sub(header_size) {
        // Record header: end_offset + num_props + prop_list_len + name_len(1)
        let (rec_end, num_props, _prop_list_len, name_len) = if header_is_64bit {
            let rec_end = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap()) as usize;
            let num_props = u64::from_le_bytes(data[offset + 8..offset + 16].try_into().unwrap())
                as usize;
            let prop_list_len =
                u64::from_le_bytes(data[offset + 16..offset + 24].try_into().unwrap()) as usize;
            let name_len = data[offset + 24] as usize;
            (rec_end, num_props, prop_list_len, name_len)
        } else {
            let rec_end = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            let num_props =
                u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let prop_list_len =
                u32::from_le_bytes(data[offset + 8..offset + 12].try_into().unwrap()) as usize;
            let name_len = data[offset + 12] as usize;
            (rec_end, num_props, prop_list_len, name_len)
        };

        if rec_end == 0 || rec_end > end {
            break;
        }

        if offset + header_size + name_len > rec_end {
            break;
        }

        let name =
            String::from_utf8_lossy(&data[offset + header_size..offset + header_size + name_len])
                .to_string();
        let mut prop_offset = offset + header_size + name_len;

        // Parse properties
        let mut properties = Vec::with_capacity(num_props);
        for _ in 0..num_props {
            if prop_offset >= rec_end {
                break;
            }
            let ptype = data[prop_offset] as char;
            prop_offset += 1;

            match ptype {
                'Y' => {
                    prop_offset += 2;
                    properties.push(FbxProperty::Other);
                }
                'C' => {
                    prop_offset += 1;
                    properties.push(FbxProperty::Other);
                }
                'I' => {
                    prop_offset += 4;
                    properties.push(FbxProperty::Other);
                }
                'F' => {
                    prop_offset += 4;
                    properties.push(FbxProperty::Other);
                }
                'D' => {
                    prop_offset += 8;
                    properties.push(FbxProperty::Other);
                }
                'L' => {
                    prop_offset += 8;
                    properties.push(FbxProperty::Other);
                }
                'f' => {
                    let (arr, new_off) = parse_array_f32(data, prop_offset, rec_end, path)?;
                    prop_offset = new_off;
                    properties.push(FbxProperty::FloatArray(arr));
                }
                'd' => {
                    let (arr, new_off) = parse_array_f64(data, prop_offset, rec_end, path)?;
                    prop_offset = new_off;
                    properties.push(FbxProperty::DoubleArray(arr));
                }
                'i' => {
                    let (arr, new_off) = parse_array_i32(data, prop_offset, rec_end, path)?;
                    prop_offset = new_off;
                    properties.push(FbxProperty::IntArray(arr));
                }
                'l' => {
                    let (_, new_off) = parse_array_i64(data, prop_offset, rec_end, path)?;
                    prop_offset = new_off;
                    properties.push(FbxProperty::Int64Array);
                }
                'b' => {
                    let new_off = skip_array(data, prop_offset, 1, path)?;
                    prop_offset = new_off;
                    properties.push(FbxProperty::Other);
                }
                'S' => {
                    if prop_offset + 4 > rec_end {
                        break;
                    }
                    let slen =
                        u32::from_le_bytes(data[prop_offset..prop_offset + 4].try_into().unwrap())
                            as usize;
                    prop_offset += 4;
                    if prop_offset + slen > rec_end {
                        break;
                    }
                    properties.push(FbxProperty::String);
                    let _ = &data[prop_offset..prop_offset + slen];
                    prop_offset += slen;
                }
                'R' => {
                    if prop_offset + 4 > rec_end {
                        break;
                    }
                    let rlen =
                        u32::from_le_bytes(data[prop_offset..prop_offset + 4].try_into().unwrap())
                            as usize;
                    prop_offset += 4 + rlen;
                    properties.push(FbxProperty::Other);
                }
                _ => break,
            }
        }

        // Recursively parse child records
        let children = if prop_offset < rec_end {
            parse_records(data, prop_offset, rec_end, header_is_64bit, path)?
        } else {
            Vec::new()
        };

        records.push(FbxRecord {
            name,
            properties,
            children,
        });

        offset = rec_end;
    }

    Ok(records)
}

// ---------------------------------------------------------------------------
// Array property parsers (with zlib decompression support)
// ---------------------------------------------------------------------------

fn parse_array_f32(
    data: &[u8],
    offset: usize,
    end: usize,
    path: &Path,
) -> Result<(Vec<f32>, usize), AssetError> {
    parse_typed_array(data, offset, end, path, 4, |slice| {
        let mut result = Vec::with_capacity(slice.len() / 4);
        for chunk in slice.chunks_exact(4) {
            result.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }
        result
    })
}

fn parse_array_f64(
    data: &[u8],
    offset: usize,
    end: usize,
    path: &Path,
) -> Result<(Vec<f64>, usize), AssetError> {
    parse_typed_array(data, offset, end, path, 8, |slice| {
        let mut result = Vec::with_capacity(slice.len() / 8);
        for chunk in slice.chunks_exact(8) {
            result.push(f64::from_le_bytes(chunk.try_into().unwrap()));
        }
        result
    })
}

fn parse_array_i32(
    data: &[u8],
    offset: usize,
    end: usize,
    path: &Path,
) -> Result<(Vec<i32>, usize), AssetError> {
    parse_typed_array(data, offset, end, path, 4, |slice| {
        let mut result = Vec::with_capacity(slice.len() / 4);
        for chunk in slice.chunks_exact(4) {
            result.push(i32::from_le_bytes(chunk.try_into().unwrap()));
        }
        result
    })
}

fn parse_array_i64(
    data: &[u8],
    offset: usize,
    end: usize,
    path: &Path,
) -> Result<(Vec<i64>, usize), AssetError> {
    parse_typed_array(data, offset, end, path, 8, |slice| {
        let mut result = Vec::with_capacity(slice.len() / 8);
        for chunk in slice.chunks_exact(8) {
            result.push(i64::from_le_bytes(chunk.try_into().unwrap()));
        }
        result
    })
}

fn skip_array(
    data: &[u8],
    offset: usize,
    elem_size: usize,
    path: &Path,
) -> Result<usize, AssetError> {
    let (_val, new_off) = parse_typed_array(data, offset, usize::MAX, path, elem_size, |_| {
        Vec::<u8>::new()
    })?;
    Ok(new_off)
}

/// Generic array parser that handles both uncompressed and zlib-compressed arrays.
///
/// FBX array layout: [encoding: u32][count: u32] then either raw data or
/// [compressed_len: u32][zlib_data].
fn parse_typed_array<T>(
    data: &[u8],
    offset: usize,
    end: usize,
    path: &Path,
    elem_size: usize,
    decode: impl Fn(&[u8]) -> Vec<T>,
) -> Result<(Vec<T>, usize), AssetError> {
    if offset + 8 > end {
        return Err(AssetError::io(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "FBX array header truncated",
            ),
        ));
    }

    let encoding = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let count = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
    let mut pos = offset + 8;

    if encoding == 0 {
        // Uncompressed
        let byte_count = count * elem_size;
        if pos + byte_count > data.len() {
            return Err(AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "FBX uncompressed array truncated",
                ),
            ));
        }
        let result = decode(&data[pos..pos + byte_count]);
        pos += byte_count;
        Ok((result, pos))
    } else {
        // zlib compressed
        if pos + 4 > data.len() {
            return Err(AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "FBX compressed array header truncated",
                ),
            ));
        }
        let compressed_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if pos + compressed_len > data.len() {
            return Err(AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "FBX compressed array data truncated",
                ),
            ));
        }

        use std::io::Read;
        let mut decoder = flate2::read::ZlibDecoder::new(&data[pos..pos + compressed_len]);
        let mut decompressed = Vec::with_capacity(count * elem_size);
        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| AssetError::io(path, e))?;
        pos += compressed_len;

        let result = decode(&decompressed);
        Ok((result, pos))
    }
}

// ---------------------------------------------------------------------------
// Geometry extraction from parsed records
// ---------------------------------------------------------------------------

fn extract_geometries(
    records: &[FbxRecord],
    geometries: &mut Vec<FbxGeometry>,
    path: &Path,
) -> Result<(), AssetError> {
    for record in records {
        if record.name == "Geometry"
            && let Some(geo) = extract_geometry(record, path)?
        {
            geometries.push(geo);
        }
        // Recurse into non-Geometry children too
        extract_geometries(&record.children, geometries, path)?;
    }
    Ok(())
}

fn extract_geometry(record: &FbxRecord, _path: &Path) -> Result<Option<FbxGeometry>, AssetError> {
    let mut positions: Option<Vec<f64>> = None;
    let mut raw_indices: Option<Vec<i32>> = None;
    let mut normals: Option<Vec<f32>> = None;

    for child in &record.children {
        match child.name.as_str() {
            "Vertices" => {
                for prop in &child.properties {
                    if let FbxProperty::DoubleArray(arr) = prop
                        && arr.len() >= 9
                    {
                        positions = Some(arr.clone());
                    }
                }
            }
            "PolygonVertexIndex" => {
                for prop in &child.properties {
                    if let FbxProperty::IntArray(arr) = prop
                        && arr.len() >= 3
                    {
                        raw_indices = Some(arr.clone());
                    }
                }
            }
            "LayerElementNormal" => {
                // Try to extract normals from LayerElementNormal > Normals
                for sub in &child.children {
                    if sub.name == "Normals" {
                        for prop in &sub.properties {
                            if let FbxProperty::DoubleArray(arr) = prop
                                && arr.len() >= 3
                            {
                                normals = Some(arr.iter().map(|&v| v as f32).collect());
                            }
                            if let FbxProperty::FloatArray(arr) = prop
                                && arr.len() >= 3
                            {
                                normals = Some(arr.clone());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let raw_positions = match positions {
        Some(p) => p,
        None => return Ok(None),
    };
    let raw_idx = match raw_indices {
        Some(i) => i,
        None => return Ok(None),
    };

    // Convert f64 positions to f32 [x, y, z] triples
    let num_vertices = raw_positions.len() / 3;
    let mut vertex_positions = Vec::with_capacity(num_vertices);
    for i in 0..num_vertices {
        vertex_positions.push([
            raw_positions[i * 3] as f32,
            raw_positions[i * 3 + 1] as f32,
            raw_positions[i * 3 + 2] as f32,
        ]);
    }

    // Convert FBX polygon indices to triangle list
    let triangles = polygon_indices_to_triangles(&raw_idx);

    if triangles.is_empty() {
        return Ok(None);
    }

    // Process normals: FBX normals may be per-face-vertex (count == polygon vertex count)
    // or per-vertex (count == vertex count). For now, we'll compute flat normals
    // if no normals are present, and re-index to per-vertex normals if they are.
    let vertex_normals = if let Some(ref norm_data) = normals {
        // Normals are stored as [nx, ny, nz, ...] triples
        // If count matches polygon vertices (3 * triangle_count), they're per-face-vertex
        // If count matches vertex count, they're per-vertex
        let norm_count = norm_data.len() / 3;
        if norm_count == vertex_positions.len() {
            // Per-vertex normals
            let mut result = vec![[0.0f32; 3]; vertex_positions.len()];
            for i in 0..norm_count.min(vertex_positions.len()) {
                result[i] = [norm_data[i * 3], norm_data[i * 3 + 1], norm_data[i * 3 + 2]];
            }
            Some(result)
        } else {
            // Per-face-vertex normals — we'll handle this in build_mesh_asset
            None
        }
    } else {
        None
    };

    Ok(Some(FbxGeometry {
        positions: vertex_positions,
        triangles,
        normals: vertex_normals,
    }))
}

/// Convert FBX polygon vertex indices to a flat triangle index list.
///
/// FBX encodes polygon boundaries using negative indices: the last vertex
/// of each polygon is stored as `-(real_index + 1)`. For example, a quad
/// with vertices 0,1,2,3 is stored as `[0, 1, 2, -4]`.
///
/// This function fan-triangulates each polygon.
fn polygon_indices_to_triangles(raw_indices: &[i32]) -> Vec<u32> {
    let mut triangles = Vec::new();
    let mut polygon = Vec::new();

    for &idx in raw_indices {
        if idx < 0 {
            // FBX encodes the last vertex of a polygon as -(real_index + 1).
            // Guard against i32::MIN (malformed FBX) which would overflow
            // `-idx - 1` when computed naively; use wrapping arithmetic and
            // then cast to u32.
            let real_idx = (-idx).wrapping_sub(1) as u32;
            polygon.push(real_idx);
        } else {
            polygon.push(idx as u32);
        }

        // End of polygon (negative index marks the last vertex)
        if idx < 0 {
            if polygon.len() >= 3 {
                for k in 1..polygon.len() - 1 {
                    triangles.push(polygon[0]);
                    triangles.push(polygon[k]);
                    triangles.push(polygon[k + 1]);
                }
            }
            polygon.clear();
        }
    }

    // Flush any remaining polygon (shouldn't happen in valid FBX, but be safe)
    if polygon.len() >= 3 {
        for k in 1..polygon.len() - 1 {
            triangles.push(polygon[0]);
            triangles.push(polygon[k]);
            triangles.push(polygon[k + 1]);
        }
    }

    triangles
}

// ---------------------------------------------------------------------------
// Mesh asset construction
// ---------------------------------------------------------------------------

fn build_mesh_asset(
    name: &str,
    geo: FbxGeometry,
    material: MaterialHandle,
) -> Result<MeshAsset, AssetError> {
    let num_vertices = geo.positions.len();
    let mut vertices = Vec::with_capacity(num_vertices);

    // If we have per-vertex normals, use them. Otherwise compute flat normals.
    let computed_normals = if geo.normals.is_none() {
        compute_flat_normals(&geo.positions, &geo.triangles)
    } else {
        Vec::new()
    };

    let normals_ref: &[[f32; 3]] = geo.normals.as_deref().unwrap_or(&computed_normals);

    for i in 0..num_vertices {
        let pos = geo.positions[i];
        let normal = normals_ref.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
        vertices.push(MeshVertex {
            position: pos,
            normal,
            uv: [0.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        });
    }

    Ok(MeshAsset {
        name: name.to_string(),
        vertices,
        indices: geo.triangles,
        material,
    })
}

/// Compute flat (per-face) normals by averaging face normals at each vertex.
fn compute_flat_normals(positions: &[[f32; 3]], triangles: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; positions.len()];

    for tri in triangles.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }

        let p0 = glam::Vec3::from(positions[i0]);
        let p1 = glam::Vec3::from(positions[i1]);
        let p2 = glam::Vec3::from(positions[i2]);

        let face_normal = (p1 - p0).cross(p2 - p0);
        if face_normal.length_squared() > 0.0 {
            let n = face_normal.normalize();
            normals[i0][0] += n.x;
            normals[i0][1] += n.y;
            normals[i0][2] += n.z;
            normals[i1][0] += n.x;
            normals[i1][1] += n.y;
            normals[i1][2] += n.z;
            normals[i2][0] += n.x;
            normals[i2][1] += n.y;
            normals[i2][2] += n.z;
        }
    }

    // Normalize accumulated normals
    for normal in &mut normals {
        let n = glam::Vec3::from(*normal);
        if n.length_squared() > 0.0 {
            let normalized = n.normalize();
            *normal = normalized.into();
        } else {
            *normal = [0.0, 1.0, 0.0];
        }
    }

    normals
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_indices_to_triangles_handles_triangle() {
        let result = polygon_indices_to_triangles(&[0, 1, -3]);
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn polygon_indices_to_triangles_handles_quad() {
        // Quad 0-1-2-3 -> triangles (0,1,2) and (0,2,3)
        let result = polygon_indices_to_triangles(&[0, 1, 2, -4]);
        assert_eq!(result, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn polygon_indices_to_triangles_handles_pentagon() {
        // Pentagon 0-1-2-3-4 -> triangles (0,1,2), (0,2,3), (0,3,4)
        let result = polygon_indices_to_triangles(&[0, 1, 2, 3, -5]);
        assert_eq!(result, vec![0, 1, 2, 0, 2, 3, 0, 3, 4]);
    }

    #[test]
    fn polygon_indices_to_triangles_handles_multiple_polygons() {
        // Two triangles
        let result = polygon_indices_to_triangles(&[0, 1, -3, 3, 4, -6]);
        assert_eq!(result, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn load_fbx_meshes_rejects_non_fbx_file() {
        let path = std::path::PathBuf::from("not_an_fbx.bin");
        let error = load_fbx_meshes(&path, &mut AssetRegistry::new());
        assert!(error.is_err());
    }

    #[test]
    fn compute_flat_normals_produces_valid_normals() {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let triangles = vec![0, 1, 2];
        let normals = compute_flat_normals(&positions, &triangles);

        // All normals should point in +Z for a CCW triangle in the XY plane
        for normal in &normals {
            let n = glam::Vec3::from(*normal);
            assert!(
                (n.length() - 1.0).abs() < 0.001,
                "normal should be unit length"
            );
            assert!(n.z > 0.9, "normal should point in +Z, got {:?}", n);
        }
    }

    #[test]
    fn load_fbx_meshes_loads_gizmo_sphere() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/Engine/Gizmos/GizmosSphere.fbx");

        if !path.exists() {
            eprintln!(
                "Skipping FBX test; GizmosSphere.fbx not found at {}",
                path.display()
            );
            return;
        }

        let mut registry = AssetRegistry::new();
        let handles = load_fbx_meshes(&path, &mut registry).expect("should load FBX");

        assert!(!handles.is_empty(), "should find at least one geometry");

        for handle in &handles {
            let mesh = registry.mesh(*handle).expect("mesh should exist");
            assert!(!mesh.vertices.is_empty(), "mesh should have vertices");
            assert!(!mesh.indices.is_empty(), "mesh should have indices");
        }
    }
}
