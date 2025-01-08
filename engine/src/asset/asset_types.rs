use sourcerenderer_core::Platform;

use crate::renderer::asset::{ComputePipelineHandle, GraphicsPipelineHandle, RayTracingPipelineHandle, RendererComputePipeline, RendererGraphicsPipeline, RendererMaterial, RendererMesh, RendererModel, RendererRayTracingPipeline, RendererShader, RendererTexture};

use super::handle_map::IndexHandle;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct TextureHandle(u64);
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct MaterialHandle(u64);
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct MeshHandle(u64);
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct ModelHandle(u64);
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct SoundHandle(u64);
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct ShaderHandle(u64);

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct LevelHandle(u64);

impl IndexHandle for TextureHandle {
    fn new(index: u64) -> Self { Self(index) }
}
impl IndexHandle for MaterialHandle {
    fn new(index: u64) -> Self { Self(index) }
}
impl IndexHandle for MeshHandle {
    fn new(index: u64) -> Self { Self(index) }
}
impl IndexHandle for ModelHandle {
    fn new(index: u64) -> Self { Self(index) }
}
impl IndexHandle for SoundHandle {
    fn new(index: u64) -> Self { Self(index) }
}
impl IndexHandle for ShaderHandle {
    fn new(index: u64) -> Self { Self(index) }
}
impl IndexHandle for LevelHandle {
    fn new(index: u64) -> Self { Self(index) }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum AssetHandle {
    Texture(TextureHandle),
    Material(MaterialHandle),
    Model(ModelHandle),
    Mesh(MeshHandle),
    Sound(SoundHandle),
    Shader(ShaderHandle),
    GraphicsPipeline(GraphicsPipelineHandle),
    ComputePipeline(ComputePipelineHandle),
    RayTracingPipeline(RayTracingPipelineHandle),
    Level(LevelHandle)
}

impl AssetHandle {
    #[inline]
    pub fn is_renderer_asset(self) -> bool {
        match self {
            AssetHandle::Texture(_) => true,
            AssetHandle::Material(_) => true,
            AssetHandle::Model(_) => true,
            AssetHandle::Shader(_) => true,
            AssetHandle::GraphicsPipeline(_) => true,
            AssetHandle::ComputePipeline(_) => true,
            AssetHandle::RayTracingPipeline(_) => true,
            _ => false
        }
    }

    #[inline]
    pub fn asset_type(self) -> AssetType {
        match self {
            AssetHandle::Texture(_) => AssetType::Texture,
            AssetHandle::Mesh(_) => AssetType::Mesh,
            AssetHandle::Model(_) => AssetType::Model,
            AssetHandle::Sound(_) => AssetType::Sound,
            AssetHandle::Material(_) => AssetType::Material,
            AssetHandle::Shader(_) => AssetType::Shader,
            AssetHandle::GraphicsPipeline(_) => AssetType::GraphicsPipeline,
            AssetHandle::ComputePipeline(_) => AssetType::ComputePipeline,
            AssetHandle::RayTracingPipeline(_) => AssetType::RayTracingPipeline,
            AssetHandle::Level(_) => AssetType::Level
        }
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
            AssetType::ComputePipeline => true,
            AssetType::RayTracingPipeline => true,
            _ => false
        }
    }
}

pub enum AssetWithHandle<P: Platform> {
    Texture(TextureHandle, RendererTexture<P::GPUBackend>),
    Material(MaterialHandle, RendererMaterial),
    Model(ModelHandle, RendererModel),
    Mesh(MeshHandle, RendererMesh<P::GPUBackend>),
    Shader(ShaderHandle, RendererShader<P::GPUBackend>),
    GraphicsPipeline(GraphicsPipelineHandle, RendererGraphicsPipeline<P>),
    ComputePipeline(ComputePipelineHandle, RendererComputePipeline<P>),
    RayTracingPipeline(RayTracingPipelineHandle, RendererRayTracingPipeline<P>),
    Sound(SoundHandle, ())
}

impl<P: Platform> AssetWithHandle<P> {
    #[inline]
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            AssetWithHandle::Texture(_,_) => true,
            AssetWithHandle::Model(_,_) => true,
            AssetWithHandle::Mesh(_,_) => true,
            AssetWithHandle::Material(_,_) => true,
            AssetWithHandle::Shader(_,_) => true,
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
            AssetWithHandle::GraphicsPipeline(_, _) => AssetType::GraphicsPipeline,
            AssetWithHandle::ComputePipeline(_, _) => AssetType::ComputePipeline,
            AssetWithHandle::RayTracingPipeline(_, _) => AssetType::RayTracingPipeline,
        }
    }

    #[inline]
    pub fn handle(&self) -> AssetHandle {
        match self {
            AssetWithHandle::Texture(handle, _) => AssetHandle::Texture(*handle),
            AssetWithHandle::Material(handle, _) => AssetHandle::Material(*handle),
            AssetWithHandle::Model(handle, _) => AssetHandle::Model(*handle),
            AssetWithHandle::Mesh(handle, _) => AssetHandle::Mesh(*handle),
            AssetWithHandle::Shader(handle, _) => AssetHandle::Shader(*handle),
            AssetWithHandle::Sound(handle, _) => AssetHandle::Sound(*handle),
            AssetWithHandle::GraphicsPipeline(handle, _) => AssetHandle::GraphicsPipeline(*handle),
            AssetWithHandle::ComputePipeline(handle, _) => AssetHandle::ComputePipeline(*handle),
            AssetWithHandle::RayTracingPipeline(handle, _) => AssetHandle::RayTracingPipeline(*handle),
        }
    }

    #[inline]
    pub fn combine(handle: AssetHandle, asset: Asset<P>) -> AssetWithHandle<P> {
        match (handle, asset) {
            (AssetHandle::Texture(handle), Asset::Texture(texture)) => AssetWithHandle::Texture(handle, texture),
            (AssetHandle::Material(handle), Asset::Material(asset)) => AssetWithHandle::Material(handle, asset),
            (AssetHandle::Model(handle), Asset::Model(asset)) => AssetWithHandle::Model(handle, asset),
            (AssetHandle::Mesh(handle), Asset::Mesh(asset)) => AssetWithHandle::Mesh(handle, asset),
            (AssetHandle::Shader(handle), Asset::Shader(asset)) => AssetWithHandle::Shader(handle, asset),
            (AssetHandle::GraphicsPipeline(handle), Asset::GraphicsPipeline(asset)) => AssetWithHandle::GraphicsPipeline(handle, asset),
            (AssetHandle::ComputePipeline(handle), Asset::ComputePipeline(asset)) => AssetWithHandle::ComputePipeline(handle, asset),
            (AssetHandle::RayTracingPipeline(handle), Asset::RayTracingPipeline(asset)) => AssetWithHandle::RayTracingPipeline(handle, asset),
            (handle, asset) => panic!("Invalid combination: Handle type: {:?} + Asset type: {:?}", handle.asset_type(), asset.asset_type())
        }
    }
}

pub enum Asset<P: Platform> {
    Texture(RendererTexture<P::GPUBackend>),
    Material(RendererMaterial),
    Model(RendererModel),
    Mesh(RendererMesh<P::GPUBackend>),
    Shader(RendererShader<P::GPUBackend>),
    Sound(()),
    GraphicsPipeline(RendererGraphicsPipeline<P>),
    ComputePipeline(RendererComputePipeline<P>),
    RayTracingPipeline(RendererRayTracingPipeline<P>),
}

impl<P: Platform> Asset<P> {
    #[inline]
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            Asset::Texture(_) => true,
            Asset::Model(_) => true,
            Asset::Mesh(_) => true,
            Asset::Material(_) => true,
            Asset::Shader(_) => true,
            Asset::GraphicsPipeline(_) => true,
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
            Asset::ComputePipeline(_) => AssetType::ComputePipeline,
            Asset::RayTracingPipeline(_) => AssetType::RayTracingPipeline,
        }
    }
}

pub enum AssetRef<'a, P: Platform> {
    Texture(&'a RendererTexture<P::GPUBackend>),
    Material(&'a RendererMaterial),
    Model(&'a RendererModel),
    Mesh(&'a RendererMesh<P::GPUBackend>),
    Shader(&'a RendererShader<P::GPUBackend>),
    GraphicsPipeline(&'a RendererGraphicsPipeline<P>),
    ComputePipeline(&'a RendererComputePipeline<P>),
    RayTracingPipeline(&'a RendererRayTracingPipeline<P>),
    Sound(()),
}

impl<P: Platform> AssetRef<'_, P> {
    #[inline]
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            AssetRef::Texture(_) => true,
            AssetRef::Model(_) => true,
            AssetRef::Mesh(_) => true,
            AssetRef::Material(_) => true,
            AssetRef::Shader(_) => true,
            AssetRef::GraphicsPipeline(_) => true,
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
            AssetRef::ComputePipeline(_) => AssetType::ComputePipeline,
            AssetRef::RayTracingPipeline(_) => AssetType::RayTracingPipeline,
        }
    }
}
