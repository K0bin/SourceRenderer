use std::hash::Hash;

use crate::renderer::asset::{RendererComputePipeline, RendererGraphicsPipeline, RendererMeshGraphicsPipeline, RendererMaterial, RendererMesh, RendererModel, RendererRayTracingPipeline, RendererShader, RendererTexture};

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
    asset_type: AssetType
}

impl AssetHandle {
    #[inline]
    pub fn new(index: u64, asset_type: AssetType) -> Self {
        Self {
            index,
            asset_type
        }
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
    #[inline]
    pub fn is_renderer_asset(self) -> bool {
        match self {
            AssetType::Texture => true,
            AssetType::Model => true,
            AssetType::Mesh => true,
            AssetType::Material => true,
            AssetType::Shader => true,
            AssetType::GraphicsPipeline => true,
            AssetType::MeshGraphicsPipeline => true,
            AssetType::ComputePipeline => true,
            AssetType::RayTracingPipeline => true,
            _ => false
        }
    }
}

pub enum AssetWithHandle {
    Texture(AssetHandle, RendererTexture),
    Material(AssetHandle, RendererMaterial),
    Model(AssetHandle, RendererModel),
    Mesh(AssetHandle, RendererMesh),
    Shader(AssetHandle, RendererShader),
    MeshGraphicsPipeline(AssetHandle, RendererMeshGraphicsPipeline),
    GraphicsPipeline(AssetHandle, RendererGraphicsPipeline),
    ComputePipeline(AssetHandle, RendererComputePipeline),
    RayTracingPipeline(AssetHandle, RendererRayTracingPipeline),
    Sound(AssetHandle, ())
}

impl AssetWithHandle {
    #[inline]
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            AssetWithHandle::Texture(_,_) => true,
            AssetWithHandle::Model(_,_) => true,
            AssetWithHandle::Mesh(_,_) => true,
            AssetWithHandle::Material(_,_) => true,
            AssetWithHandle::Shader(_,_) => true,
            AssetWithHandle::MeshGraphicsPipeline(_, _) => true,
            AssetWithHandle::GraphicsPipeline(_, _) => true,
            AssetWithHandle::ComputePipeline(_, _) => true,
            AssetWithHandle::RayTracingPipeline(_, _) => true,
            _ => false
        }
    }

    #[inline]
    pub fn asset_type(&self) -> AssetType {
        match self {
            AssetWithHandle::Texture(_,_) => AssetType::Texture,
            AssetWithHandle::Mesh(_,_) => AssetType::Mesh,
            AssetWithHandle::Model(_,_) => AssetType::Model,
            AssetWithHandle::Sound(_,_) => AssetType::Sound,
            AssetWithHandle::Material(_,_) => AssetType::Material,
            AssetWithHandle::Shader(_,_) => AssetType::Shader,
            AssetWithHandle::MeshGraphicsPipeline(_, _) => AssetType::MeshGraphicsPipeline,
            AssetWithHandle::GraphicsPipeline(_, _) => AssetType::GraphicsPipeline,
            AssetWithHandle::ComputePipeline(_, _) => AssetType::ComputePipeline,
            AssetWithHandle::RayTracingPipeline(_, _) => AssetType::RayTracingPipeline,
        }
    }

    #[inline]
    pub fn handle(&self) -> AssetHandle {
        match self {
            AssetWithHandle::Texture(handle, _) => *handle,
            AssetWithHandle::Material(handle, _) => *handle,
            AssetWithHandle::Model(handle, _) => *handle,
            AssetWithHandle::Mesh(handle, _) => *handle,
            AssetWithHandle::Shader(handle, _) => *handle,
            AssetWithHandle::Sound(handle, _) => *handle,
            AssetWithHandle::MeshGraphicsPipeline(handle, _) => *handle,
            AssetWithHandle::GraphicsPipeline(handle, _) => *handle,
            AssetWithHandle::ComputePipeline(handle, _) => *handle,
            AssetWithHandle::RayTracingPipeline(handle, _) => *handle,
        }
    }

    #[inline]
    pub fn combine(handle: AssetHandle, asset: Asset) -> AssetWithHandle {
        assert_eq!(handle.asset_type(), asset.asset_type());
        match (handle, asset) {
            (handle, Asset::Texture(texture)) => AssetWithHandle::Texture(handle, texture),
            (handle, Asset::Material(asset)) => AssetWithHandle::Material(handle, asset),
            (handle, Asset::Model(asset)) => AssetWithHandle::Model(handle, asset),
            (handle, Asset::Mesh(asset)) => AssetWithHandle::Mesh(handle, asset),
            (handle, Asset::Shader(asset)) => AssetWithHandle::Shader(handle, asset),
            (handle, Asset::MeshGraphicsPipeline(asset)) => AssetWithHandle::MeshGraphicsPipeline(handle, asset),
            (handle, Asset::GraphicsPipeline(asset)) => AssetWithHandle::GraphicsPipeline(handle, asset),
            (handle, Asset::ComputePipeline(asset)) => AssetWithHandle::ComputePipeline(handle, asset),
            (handle, Asset::RayTracingPipeline(asset)) => AssetWithHandle::RayTracingPipeline(handle, asset),
            (handle, Asset::Sound(asset)) => AssetWithHandle::Sound(handle, asset)
        }
    }
}

pub enum Asset {
    Texture(RendererTexture),
    Material(RendererMaterial),
    Model(RendererModel),
    Mesh(RendererMesh),
    Shader(RendererShader),
    Sound(()),
    GraphicsPipeline(RendererGraphicsPipeline),
    MeshGraphicsPipeline(RendererMeshGraphicsPipeline),
    ComputePipeline(RendererComputePipeline),
    RayTracingPipeline(RendererRayTracingPipeline),
}

impl Asset {
    #[inline]
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            Asset::Texture(_) => true,
            Asset::Model(_) => true,
            Asset::Mesh(_) => true,
            Asset::Material(_) => true,
            Asset::Shader(_) => true,
            Asset::GraphicsPipeline(_) => true,
            Asset::MeshGraphicsPipeline(_) => true,
            Asset::ComputePipeline(_) => true,
            Asset::RayTracingPipeline(_) => true,
            _ => false
        }
    }

    #[inline]
    pub fn asset_type(&self) -> AssetType {
        match self {
            Asset::Texture(_) => AssetType::Texture,
            Asset::Mesh(_) => AssetType::Mesh,
            Asset::Model(_) => AssetType::Model,
            Asset::Sound(_) => AssetType::Sound,
            Asset::Material(_) => AssetType::Material,
            Asset::Shader(_) => AssetType::Shader,
            Asset::GraphicsPipeline(_) => AssetType::GraphicsPipeline,
            Asset::MeshGraphicsPipeline(_) => AssetType::MeshGraphicsPipeline,
            Asset::ComputePipeline(_) => AssetType::ComputePipeline,
            Asset::RayTracingPipeline(_) => AssetType::RayTracingPipeline,
        }
    }
}

pub enum AssetRef<'a> {
    Texture(&'a RendererTexture),
    Material(&'a RendererMaterial),
    Model(&'a RendererModel),
    Mesh(&'a RendererMesh),
    Shader(&'a RendererShader),
    GraphicsPipeline(&'a RendererGraphicsPipeline),
    MeshGraphicsPipeline(&'a RendererMeshGraphicsPipeline),
    ComputePipeline(&'a RendererComputePipeline),
    RayTracingPipeline(&'a RendererRayTracingPipeline),
    Sound(()),
}

impl<'a> From<&'a RendererTexture> for AssetRef<'a> {
    fn from(value: &'a RendererTexture) -> Self {
        Self::Texture(value)
    }
}

impl<'a> From<&'a RendererMaterial> for AssetRef<'a> {
    fn from(value: &'a RendererMaterial) -> Self {
        Self::Material(value)
    }
}

impl<'a> From<&'a RendererModel> for AssetRef<'a> {
    fn from(value: &'a RendererModel) -> Self {
        Self::Model(value)
    }
}

impl<'a> From<&'a RendererMesh> for AssetRef<'a> {
    fn from(value: &'a RendererMesh) -> Self {
        Self::Mesh(value)
    }
}

impl<'a> From<&'a RendererShader> for AssetRef<'a> {
    fn from(value: &'a RendererShader) -> Self {
        Self::Shader(value)
    }
}

impl<'a> From<&'a RendererGraphicsPipeline> for AssetRef<'a> {
    fn from(value: &'a RendererGraphicsPipeline) -> Self {
        Self::GraphicsPipeline(value)
    }
}

impl<'a> From<&'a RendererMeshGraphicsPipeline> for AssetRef<'a> {
    fn from(value: &'a RendererMeshGraphicsPipeline) -> Self {
        Self::MeshGraphicsPipeline(value)
    }
}

impl<'a> From<&'a RendererComputePipeline> for AssetRef<'a> {
    fn from(value: &'a RendererComputePipeline) -> Self {
        Self::ComputePipeline(value)
    }
}

impl<'a> From<&'a RendererRayTracingPipeline> for AssetRef<'a> {
    fn from(value: &'a RendererRayTracingPipeline) -> Self {
        Self::RayTracingPipeline(value)
    }
}

impl AssetRef<'_> {
    #[inline]
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            AssetRef::Texture(_) => true,
            AssetRef::Model(_) => true,
            AssetRef::Mesh(_) => true,
            AssetRef::Material(_) => true,
            AssetRef::Shader(_) => true,
            AssetRef::GraphicsPipeline(_) => true,
            AssetRef::MeshGraphicsPipeline(_) => true,
            AssetRef::ComputePipeline(_) => true,
            AssetRef::RayTracingPipeline(_) => true,
            _ => false
        }
    }

    #[inline]
    pub fn asset_type(&self) -> AssetType {
        match self {
            AssetRef::Texture(_) => AssetType::Texture,
            AssetRef::Mesh(_) => AssetType::Mesh,
            AssetRef::Model(_) => AssetType::Model,
            AssetRef::Sound(_) => AssetType::Sound,
            AssetRef::Material(_) => AssetType::Material,
            AssetRef::Shader(_) => AssetType::Shader,
            AssetRef::GraphicsPipeline(_) => AssetType::GraphicsPipeline,
            AssetRef::MeshGraphicsPipeline(_) => AssetType::MeshGraphicsPipeline,
            AssetRef::ComputePipeline(_) => AssetType::ComputePipeline,
            AssetRef::RayTracingPipeline(_) => AssetType::RayTracingPipeline,
        }
    }
}
