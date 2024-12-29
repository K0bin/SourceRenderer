use sourcerenderer_core::Platform;

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

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum AssetHandle {
    Texture(TextureHandle),
    Material(MaterialHandle),
    Model(ModelHandle),
    Mesh(MeshHandle),
    Sound(SoundHandle),
    Shader(ShaderHandle)
}

impl AssetHandle {
    pub fn is_renderer_asset(self) -> bool {
        match self {
            AssetHandle::Texture(texture_handle) => true,
            AssetHandle::Material(material_handle) => true,
            AssetHandle::Model(model_handle) => true,
            AssetHandle::Shader(shader_handle) => true,
            _ => false
        }
    }

    pub fn asset_type(self) -> AssetType {
        match self {
            AssetHandle::Texture(_) => AssetType::Texture,
            AssetHandle::Mesh(_) => AssetType::Mesh,
            AssetHandle::Model(_) => AssetType::Model,
            AssetHandle::Sound(_) => AssetType::Sound,
            AssetHandle::Material(_) => AssetType::Material,
            AssetHandle::Shader(_) => AssetType::Shader,
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
    Chunk,
    Container,
    Shader,
}

impl AssetType {
    pub fn is_renderer_asset(self) -> bool {
        match self {
            AssetType::Texture => true,
            AssetType::Model => true,
            AssetType::Mesh => true,
            AssetType::Material => true,
            AssetType::Shader => true,
            _ => false
        }
    }
}

pub enum AssetWithHandle<P: Platform> {
    Texture(TextureHandle, renderer_assets::RendererTexture<P::GPUBackend>),
    Material(MaterialHandle, renderer_assets::RendererMaterial),
    Model(ModelHandle, renderer_assets::RendererModel),
    Mesh(MeshHandle, renderer_assets::RendererMesh<P::GPUBackend>),
    Shader(ShaderHandle, renderer_assets::RendererShader<P::GPUBackend>),
    Sound(SoundHandle, ())
}

impl<P: Platform> AssetWithHandle<P> {
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            AssetWithHandle::Texture(_,_) => true,
            AssetWithHandle::Model(_,_) => true,
            AssetWithHandle::Mesh(_,_) => true,
            AssetWithHandle::Material(_,_) => true,
            AssetWithHandle::Shader(_,_) => true,
            _ => false
        }
    }

    pub fn asset_type(&self) -> AssetType {
        match self {
            AssetWithHandle::Texture(_,_) => AssetType::Texture,
            AssetWithHandle::Mesh(_,_) => AssetType::Mesh,
            AssetWithHandle::Model(_,_) => AssetType::Model,
            AssetWithHandle::Sound(_,_) => AssetType::Sound,
            AssetWithHandle::Material(_,_) => AssetType::Material,
            AssetWithHandle::Shader(_,_) => AssetType::Shader,
        }
    }

    pub fn handle(&self) -> AssetHandle {
        match self {
            AssetWithHandle::Texture(handle, _) => AssetHandle::Texture(*handle),
            AssetWithHandle::Material(handle, _) => AssetHandle::Material(*handle),
            AssetWithHandle::Model(handle, _) => AssetHandle::Model(*handle),
            AssetWithHandle::Mesh(handle, _) => AssetHandle::Mesh(*handle),
            AssetWithHandle::Shader(handle, _) => AssetHandle::Shader(*handle),
            AssetWithHandle::Sound(handle, _) => AssetHandle::Sound(*handle),
        }
    }

    pub fn combine(handle: AssetHandle, asset: Asset<P>) -> AssetWithHandle<P> {
        match (handle, asset) {
            (AssetHandle::Texture(handle), Asset::Texture(texture)) => AssetWithHandle::Texture(handle, texture),
            (AssetHandle::Material(handle), Asset::Material(asset)) => AssetWithHandle::Material(handle, asset),
            (AssetHandle::Model(handle), Asset::Model(asset)) => AssetWithHandle::Model(handle, asset),
            (AssetHandle::Mesh(handle), Asset::Mesh(asset)) => AssetWithHandle::Mesh(handle, asset),
            (AssetHandle::Shader(handle), Asset::Shader(asset)) => AssetWithHandle::Shader(handle, asset),
            _ => panic!("Invalid combination")
        }
    }
}

pub enum Asset<P: Platform> {
    Texture(renderer_assets::RendererTexture<P::GPUBackend>),
    Material(renderer_assets::RendererMaterial),
    Model(renderer_assets::RendererModel),
    Mesh(renderer_assets::RendererMesh<P::GPUBackend>),
    Shader(renderer_assets::RendererShader<P::GPUBackend>),
    Sound(())
}

impl<P: Platform> Asset<P> {
    pub fn is_renderer_asset(&self) -> bool {
        match self {
            Asset::Texture(_) => true,
            Asset::Model(_) => true,
            Asset::Mesh(_) => true,
            Asset::Material(_) => true,
            Asset::Shader(_) => true,
            _ => false
        }
    }

    pub fn asset_type(&self) -> AssetType {
        match self {
            Asset::Texture(_) => AssetType::Texture,
            Asset::Mesh(_) => AssetType::Mesh,
            Asset::Model(_) => AssetType::Model,
            Asset::Sound(_) => AssetType::Sound,
            Asset::Material(_) => AssetType::Material,
            Asset::Shader(_) => AssetType::Shader,
        }
    }
}

