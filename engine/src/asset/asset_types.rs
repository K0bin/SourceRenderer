use std::hash::Hash;

use strum_macros::VariantArray;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct TextureHandle(AssetHandle);

impl From<AssetHandle> for TextureHandle {
    fn from(value: AssetHandle) -> Self {
        assert_eq!(value.asset_type, Self::asset_type());
        Self(value)
    }
}

impl Into<AssetHandle> for TextureHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl TextureHandle {
    #[inline(always)]
    pub fn asset_type() -> AssetType {
        AssetType::Texture
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct MaterialHandle(AssetHandle);

impl From<AssetHandle> for MaterialHandle {
    fn from(value: AssetHandle) -> Self {
        assert_eq!(value.asset_type, Self::asset_type());
        Self(value)
    }
}

impl Into<AssetHandle> for MaterialHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl MaterialHandle {
    #[inline(always)]
    pub fn asset_type() -> AssetType {
        AssetType::Material
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct MeshHandle(AssetHandle);

impl From<AssetHandle> for MeshHandle {
    fn from(value: AssetHandle) -> Self {
        assert_eq!(value.asset_type, Self::asset_type());
        Self(value)
    }
}

impl Into<AssetHandle> for MeshHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl MeshHandle {
    #[inline(always)]
    pub fn asset_type() -> AssetType {
        AssetType::Mesh
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct ModelHandle(AssetHandle);

impl From<AssetHandle> for ModelHandle {
    fn from(value: AssetHandle) -> Self {
        assert_eq!(value.asset_type, Self::asset_type());
        Self(value)
    }
}

impl Into<AssetHandle> for ModelHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl ModelHandle {
    #[inline(always)]
    pub fn asset_type() -> AssetType {
        AssetType::Model
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SoundHandle(AssetHandle);

impl From<AssetHandle> for SoundHandle {
    fn from(value: AssetHandle) -> Self {
        assert_eq!(value.asset_type, Self::asset_type());
        Self(value)
    }
}

impl Into<AssetHandle> for SoundHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl SoundHandle {
    #[inline(always)]
    pub fn asset_type() -> AssetType {
        AssetType::Sound
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct ShaderHandle(AssetHandle);

impl From<AssetHandle> for ShaderHandle {
    fn from(value: AssetHandle) -> Self {
        assert_eq!(value.asset_type, Self::asset_type());
        Self(value)
    }
}

impl Into<AssetHandle> for ShaderHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl ShaderHandle {
    #[inline(always)]
    pub fn asset_type() -> AssetType {
        AssetType::Shader
    }
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct LevelHandle(AssetHandle);

impl From<AssetHandle> for LevelHandle {
    fn from(value: AssetHandle) -> Self {
        assert_eq!(value.asset_type, Self::asset_type());
        Self(value)
    }
}

impl Into<AssetHandle> for LevelHandle {
    fn into(self) -> AssetHandle {
        self.0
    }
}

impl LevelHandle {
    #[inline(always)]
    pub fn asset_type() -> AssetType {
        AssetType::Level
    }
}

#[derive(Eq, Clone, Copy, Debug)]
pub struct AssetHandle {
    index: u64,
    asset_type: AssetType,
}

impl AssetHandle {
    #[inline]
    pub fn new(index: u64, asset_type: AssetType) -> Self {
        Self { index, asset_type }
    }
}

impl PartialEq<AssetHandle> for AssetHandle {
    fn eq(&self, other: &AssetHandle) -> bool {
        if self.index == other.index {
            debug_assert_eq!(self.asset_type, other.asset_type);
        }

        self.index == other.index
    }
}

impl PartialOrd<AssetHandle> for AssetHandle {
    fn partial_cmp(&self, other: &AssetHandle) -> Option<std::cmp::Ordering> {
        if self.index == other.index {
            debug_assert_eq!(self.asset_type, other.asset_type);
        }
        self.index.partial_cmp(&other.index)
    }
}

impl Ord for AssetHandle {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.index == other.index {
            debug_assert_eq!(self.asset_type, other.asset_type);
        }
        self.index.cmp(&other.index)
    }
}

impl Hash for AssetHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl AssetHandle {
    #[inline(always)]
    pub fn asset_type(&self) -> AssetType {
        self.asset_type
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AssetType {
    Texture,
    Model,
    Mesh,
    Material,
    Sound,
    Level,
    Shader,
    GraphicsPipeline,
    MeshGraphicsPipeline,
    ComputePipeline,
    RayTracingPipeline,
}

impl AssetType {
    pub fn group(self) -> AssetTypeGroup {
        match self {
            AssetType::Texture => AssetTypeGroup::Rendering,
            AssetType::Model => AssetTypeGroup::Rendering,
            AssetType::Mesh => AssetTypeGroup::Rendering,
            AssetType::Material => AssetTypeGroup::Rendering,
            AssetType::Shader => AssetTypeGroup::Rendering,
            AssetType::GraphicsPipeline => AssetTypeGroup::Rendering,
            AssetType::MeshGraphicsPipeline => AssetTypeGroup::Rendering,
            AssetType::ComputePipeline => AssetTypeGroup::Rendering,
            AssetType::RayTracingPipeline => AssetTypeGroup::Rendering,
            AssetType::Level => AssetTypeGroup::Level,
            AssetType::Sound => AssetTypeGroup::Audio,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, VariantArray)]
pub enum AssetTypeGroup {
    Rendering,
    Audio,
    Level,
}
