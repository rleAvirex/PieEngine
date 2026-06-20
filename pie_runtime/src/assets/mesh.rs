use glam::Vec3;

use crate::assets::MaterialHandle;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshAsset {
    pub name: String,
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub material: MaterialHandle,
}

impl MeshAsset {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.vertices.is_empty() {
            return Err("mesh must contain at least one vertex");
        }

        if self.indices.is_empty() {
            return Err("mesh must contain at least one index");
        }

        Ok(())
    }

    /// Local-space axis-aligned bounding box of the mesh, as `(min, max)`.
    ///
    /// Returns `None` only for an empty mesh; `validate()` already rejects
    /// those, but callers compute bounds defensively.
    pub fn local_aabb(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::INFINITY;
        let mut max = Vec3::NEG_INFINITY;

        for vertex in &self.vertices {
            let position = Vec3::from(vertex.position);
            min = min.min(position);
            max = max.max(position);
        }

        if min.x.is_infinite() {
            None
        } else {
            Some((min, max))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MeshAsset, MeshVertex};
    use glam::Vec3;

    fn vertex(position: [f32; 3]) -> MeshVertex {
        MeshVertex {
            position,
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        }
    }

    #[test]
    fn local_aabb_returns_min_max_over_all_vertices() {
        let mesh = MeshAsset {
            name: "test".to_string(),
            vertices: vec![
                vertex([-1.0, -2.0, -3.0]),
                vertex([1.0, 5.0, 2.0]),
                vertex([0.0, 0.0, 0.0]),
            ],
            indices: vec![0, 1, 2],
            material: crate::assets::MaterialHandle::new(0),
        };

        let (min, max) = mesh.local_aabb().expect("non-empty mesh has bounds");

        assert_eq!(min, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(max, Vec3::new(1.0, 5.0, 2.0));
    }

    #[test]
    fn local_aabb_is_none_for_empty_mesh() {
        let mesh = MeshAsset {
            name: "empty".to_string(),
            vertices: Vec::new(),
            indices: vec![0],
            material: crate::assets::MaterialHandle::new(0),
        };

        assert!(mesh.local_aabb().is_none());
    }

    #[test]
    fn local_aabb_collapses_to_point_for_degenerate_mesh() {
        let mesh = MeshAsset {
            name: "point".to_string(),
            vertices: vec![vertex([2.0, 2.0, 2.0]); 3],
            indices: vec![0, 1, 2],
            material: crate::assets::MaterialHandle::new(0),
        };

        let (min, max) = mesh.local_aabb().expect("point mesh has bounds");

        assert_eq!(min, Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(max, Vec3::new(2.0, 2.0, 2.0));
    }
}
