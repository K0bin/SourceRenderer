use std::{collections::{hash_map::Values, HashMap}, sync::Arc};
use crate::{graphics::GraphicsContext, RwLock, RwLockReadGuard}; // The parking lot variant is fair (write-preferring) and consistent across platforms.

use smallvec::SmallVec;

use crate::{asset::*, graphics::{BufferSlice, ComputePipeline, GraphicsPipeline, RayTracingPipeline}};

use super::*;

pub struct RendererAssets {
    assets: RwLock<RendererAssetMaps>,
    placeholders: AssetPlaceholders,
    shader_manager: ShaderManager,
    integrator: AssetIntegrator,
}

impl RendererAssets {
    pub(crate) fn new(device: &Arc<crate::graphics::Device>) -> Self {
        Self {
            assets: RwLock::new(RendererAssetMaps {
                textures: HashMap::new(),
                materials: HashMap::new(),
                meshes: HashMap::new(),
                models: HashMap::new(),
                shaders: HashMap::new(),
                graphics_pipelines: HashMap::new(),
                compute_pipelines: HashMap::new(),
                ray_tracing_pipelines: HashMap::new(),
            }),
            placeholders: AssetPlaceholders::new(device),
            shader_manager: ShaderManager::new(device),
            integrator: AssetIntegrator::new(device),
        }
    }

    #[inline(always)]
    pub(crate) fn integrate<T: Into<AssetHandle>>(
        &self,
        asset_manager: &Arc<AssetManager>,
        handle: T,
        asset_data: &AssetData,
        priority: AssetLoadPriority
    ) {
        self.integrator.integrate(asset_manager, &self.shader_manager, handle, asset_data, priority)
    }

    #[inline(always)]
    pub(crate) fn add_asset(&self, asset: AssetWithHandle) -> bool {
        let mut assets = self.assets.write();
        match asset {
            AssetWithHandle::Texture(handle, asset) => assets.textures.insert(handle.into(), asset).is_some(),
            AssetWithHandle::Material(handle, asset) => assets.materials.insert(handle.into(), asset).is_some(),
            AssetWithHandle::Model(handle, asset) => assets.models.insert(handle.into(), asset).is_some(),
            AssetWithHandle::Mesh(handle, asset) => assets.meshes.insert(handle.into(), asset).is_some(),
            AssetWithHandle::Shader(handle, asset) => assets.shaders.insert(handle.into(), asset).is_some(),
            AssetWithHandle::GraphicsPipeline(handle, asset) => assets.graphics_pipelines.insert(handle.into(), asset).is_some(),
            AssetWithHandle::ComputePipeline(handle, asset) => assets.compute_pipelines.insert(handle.into(), asset).is_some(),
            AssetWithHandle::RayTracingPipeline(handle, asset) => assets.ray_tracing_pipelines.insert(handle.into(), asset).is_some(),
            _ => panic!("Unsupported asset type {:?}", asset.asset_type()),
        }
    }

    #[inline(always)]
    pub(crate) fn contains<T: Into<AssetHandle>>(&self, handle: T) -> bool {
        let handle: AssetHandle = handle.into();
        let assets = self.assets.read();
        assets.get(handle).is_some()
    }

    #[inline(always)]
    pub(crate) fn request_graphics_pipeline(&self, asset_manager: &Arc<AssetManager>, info: &GraphicsPipelineInfo) -> GraphicsPipelineHandle {
        self.shader_manager.request_graphics_pipeline(asset_manager, info)
    }

    #[inline(always)]
    pub(crate) fn request_compute_pipeline(&self, asset_manager: &Arc<AssetManager>, shader_path: &str) -> ComputePipelineHandle {
        self.shader_manager.request_compute_pipeline(asset_manager, shader_path)
    }

    #[inline(always)]
    pub(crate) fn request_ray_tracing_pipeline(&self, asset_manager: &Arc<AssetManager>, info: &RayTracingPipelineInfo) -> RayTracingPipelineHandle {
        self.shader_manager.request_ray_tracing_pipeline(asset_manager, info)
    }

    pub(crate) fn read<'a>(&'a self) -> RendererAssetsReadOnly<'a> {
        RendererAssetsReadOnly {
            maps: self.assets.read(),
            placeholders: &self.placeholders,
            vertex_buffer: self.integrator.vertex_buffer(),
            index_buffer: self.integrator.index_buffer()
        }
    }

    #[inline(always)]
    pub(crate) fn flush(&self, asset_manager: &Arc<AssetManager>) {
        self.integrator.flush(asset_manager, &self.shader_manager);
    }

    #[inline(always)]
    pub(crate) fn bump_frame(&self, context: &GraphicsContext) {
        self.integrator.bump_frame(context);
    }
}

struct RendererAssetMaps {
    textures: HashMap<TextureHandle, RendererTexture>,
    materials: HashMap<MaterialHandle, RendererMaterial>,
    meshes: HashMap<MeshHandle, RendererMesh>,
    models: HashMap<ModelHandle, RendererModel>,
    shaders: HashMap<ShaderHandle, RendererShader>,
    graphics_pipelines: HashMap<GraphicsPipelineHandle, RendererGraphicsPipeline>,
    compute_pipelines: HashMap<ComputePipelineHandle, RendererComputePipeline>,
    ray_tracing_pipelines: HashMap<RayTracingPipelineHandle, RendererRayTracingPipeline>,
}

impl RendererAssetMaps {
    #[allow(unused)]
    #[inline]
    fn get<T: Into<AssetHandle>>(&self, handle: T) -> Option<AssetRef> {
        let handle: AssetHandle = handle.into();
        match handle.asset_type() {
            AssetType::Texture => self.textures.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            AssetType::Model => self.models.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            AssetType::Mesh => self.meshes.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            AssetType::Material => self.materials.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            AssetType::Shader => self.shaders.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            AssetType::GraphicsPipeline => self.graphics_pipelines.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            AssetType::ComputePipeline => self.compute_pipelines.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            AssetType::RayTracingPipeline => self.ray_tracing_pipelines.get(&handle.into()).map(|asset| AssetRef::from(asset)),
            _ => panic!("Asset type {:?} is not a renderer asset type", handle.asset_type()),
        }
    }

    #[allow(unused)]
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.textures.is_empty()
            && self.materials.is_empty()
            && self.models.is_empty()
            && self.meshes.is_empty()
            && self.shaders.is_empty()
    }
}

pub struct RendererAssetsReadOnly<'a> {
    maps: RwLockReadGuard<'a, RendererAssetMaps>,
    placeholders: &'a AssetPlaceholders,
    vertex_buffer: &'a Arc<BufferSlice>,
    index_buffer: &'a Arc<BufferSlice>,
}

impl RendererAssetsReadOnly<'_> {
    #[inline(always)]
    pub fn get_model(&self, handle: ModelHandle) -> Option<&RendererModel> {
        self.maps.models.get(&handle)
    }

    #[inline(always)]
    pub fn get_mesh(&self, handle: MeshHandle) -> Option<&RendererMesh> {
        self.maps.meshes.get(&handle)
    }

    #[inline(always)]
    pub fn get_material(&self, handle: MaterialHandle) -> &RendererMaterial {
        self.maps.materials.get(&handle).unwrap_or(self.placeholders.material())
    }

    #[inline(always)]
    pub fn get_placeholder_material(&self) -> &RendererMaterial {
        self.placeholders.material()
    }

    #[inline(always)]
    pub fn get_texture(&self, handle: TextureHandle) -> &RendererTexture {
        self.maps.textures.get(&handle).unwrap_or(self.placeholders.texture_white())
    }

    #[inline(always)]
    pub fn get_texture_opt(&self, handle: TextureHandle) -> Option<&RendererTexture> {
        self.maps.textures.get(&handle)
    }

    #[inline(always)]
    pub fn get_placeholder_texture_black(&self) -> &RendererTexture {
        self.placeholders.texture_black()
    }

    #[inline(always)]
    pub fn get_placeholder_texture_white(&self) -> &RendererTexture {
        self.placeholders.texture_white()
    }

    #[inline(always)]
    pub fn get_shader(&self, handle: ShaderHandle) -> Option<&RendererShader> {
        self.maps.shaders.get(&handle)
    }

    #[inline(always)]
    pub fn get_graphics_pipeline(&self, handle: GraphicsPipelineHandle) -> Option<&Arc<GraphicsPipeline>> {
        self.maps.graphics_pipelines.get(&handle).map(|c| &c.pipeline)
    }

    #[inline(always)]
    pub fn get_compute_pipeline(&self, handle: ComputePipelineHandle) -> Option<&Arc<ComputePipeline>> {
        self.maps.compute_pipelines.get(&handle).map(|c| &c.pipeline)
    }

    #[inline(always)]
    pub fn get_ray_tracing_pipeline(&self, handle: RayTracingPipelineHandle) -> Option<&Arc<RayTracingPipeline>> {
        self.maps.ray_tracing_pipelines.get(&handle).map(|c| &c.pipeline)
    }

    #[inline(always)]
    pub fn all_graphics_pipelines(&self) -> Values<'_, GraphicsPipelineHandle, RendererGraphicsPipeline> {
        self.maps.graphics_pipelines.values()
    }

    #[inline(always)]
    pub fn all_compute_pipelines(&self) -> Values<'_, ComputePipelineHandle, RendererComputePipeline> {
        self.maps.compute_pipelines.values()
    }

    #[inline(always)]
    pub fn all_ray_tracing_pipelines(&self) -> Values<'_, RayTracingPipelineHandle, RendererRayTracingPipeline> {
        self.maps.ray_tracing_pipelines.values()
    }

    pub fn get<T: Into<AssetHandle>>(&self, handle: T) -> Option<AssetRef> {
        self.maps.get(handle)
    }

    pub fn all_pipeline_handles(&self, asset_type: AssetType) -> SmallVec<[AssetHandle; 16]> {
        let mut handles = SmallVec::<[AssetHandle; 16]>::new();
        match asset_type {
            AssetType::GraphicsPipeline => {
                for handle in self.maps.graphics_pipelines.keys() {
                    handles.push((*handle).into());
                }
            },
            AssetType::ComputePipeline => {
                for handle in self.maps.compute_pipelines.keys() {
                    handles.push((*handle).into());
                }
            },
            AssetType::RayTracingPipeline => {
                for handle in self.maps.ray_tracing_pipelines.keys() {
                    handles.push((*handle).into());
                }
            },
            _ => panic!("Asset type is not a pipeline type")
        }
        handles
    }

    #[inline(always)]
    pub fn vertex_buffer(&self) -> &Arc<BufferSlice> {
        self.vertex_buffer
    }

    #[inline(always)]
    pub fn index_buffer(&self) -> &Arc<BufferSlice> {
        self.index_buffer
    }
}
