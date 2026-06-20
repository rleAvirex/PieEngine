use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

pub trait AssetKind {
    const NAME: &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshAssetKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureAssetKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialAssetKind;

impl AssetKind for MeshAssetKind {
    const NAME: &'static str = "mesh";
}

impl AssetKind for TextureAssetKind {
    const NAME: &'static str = "texture";
}

impl AssetKind for MaterialAssetKind {
    const NAME: &'static str = "material";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle<K: AssetKind> {
    id: u32,
    _kind: PhantomData<K>,
}

impl<K: AssetKind> Handle<K> {
    pub const INVALID: Self = Self {
        id: u32::MAX,
        _kind: PhantomData,
    };

    pub(crate) fn new(id: u32) -> Self {
        Self {
            id,
            _kind: PhantomData,
        }
    }

    pub fn index(&self) -> u32 {
        self.id
    }

    pub fn is_valid(&self) -> bool {
        self.id != u32::MAX
    }
}

impl<K: AssetKind> fmt::Display for Handle<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", K::NAME, self.id)
    }
}

pub type MeshHandle = Handle<MeshAssetKind>;
pub type TextureHandle = Handle<TextureAssetKind>;
pub type MaterialHandle = Handle<MaterialAssetKind>;
