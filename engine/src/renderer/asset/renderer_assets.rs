use std::{collections::{hash_map::Values, HashSet}, marker::PhantomData, sync::Arc};
use log::trace;
use parking_lot::{RwLock, RwLockReadGuard}; // The parking lot variant is fair (write-preferring) and consistent across platforms.

use smallvec::SmallVec;
use sourcerenderer_core::{Platform, PlatformPhantomData};

use crate::{asset::*, graphics::{BufferSlice, ComputePipeline, GraphicsPipeline, RayTracingPipeline}};

use super::*;

pub struct RendererAssets<P: Platform> {
    assets: RwLock<RendererAssetMaps<P>>,
    placeholders: AssetPlaceholders<P>,
    shader_manager: ShaderManager<P>,
    integrator: AssetIntegrator<P>,
    _platform: PlatformPhantomData<P>
}

impl<P: Platform> RendererAssets<P> {
    pub(crate) fn new(device: &Arc<crate::graphics::Device<P::GPUBackend>>) -> Self {
        Self {
            assets: RwLock::new(RendererAssetMaps {
                textures: HandleMap::new(),
                materials: HandleMap::new(),
                meshes: HandleMap::new(),
                models: HandleMap::new(),
                shaders: HandleMap::new(),
                graphics_pipelines: SimpleHandleMap::new(),
                compute_pipelines: SimpleHandleMap::new(),
                ray_tracing_pipelines: SimpleHandleMap::new(),
                requested_assets: HashSet::new()
            }),
            placeholders: AssetPlaceholders::new(device),
            shader_manager: ShaderManager::new(device),
            integrator: AssetIntegrator::new(device),
            _platform: Default::default()
        }
    }

    pub(crate) fn integrate(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        path: &str,
        asset_data: &AssetData,
        priority: AssetLoadPriority
    ) {
        self.integrator.integrate(asset_manager, &self.shader_manager, path, asset_data, priority)
    }

    pub(crate) fn reserve_handle(&self, path: &str, asset_type: AssetType) -> AssetHandle {
        let mut assets = self.assets.write();
        assets.reserve_handle(path, asset_type)
    }

    pub(crate) fn reserve_handle_without_path(&self, asset_type: AssetType) -> AssetHandle {
        let mut assets = self.assets.write();
        assets.reserve_handle_without_path(asset_type)
    }

    pub(crate) fn remove_by_handle(&self, asset_type: AssetType, path: &str) -> bool {
        let mut assets = self.assets.write();
        assets.remove_by_key(asset_type, path)
    }

    pub(crate) fn remove_request_by_path(&self, asset_type: AssetType, path: &str) -> bool {
        let mut assets = self.assets.write();
        assets.remove_request_by_path(asset_type, path)
    }

    pub(crate) fn add_asset(&self, asset: AssetWithHandle<P>) -> bool {
        let mut assets = self.assets.write();
        assets.add_asset(asset)
    }

    pub(crate) fn insert_request(&self, request: &(String, AssetType), refresh: bool) -> bool {
        let mut assets = self.assets.write();
        assets.insert_request(request, refresh)
    }

    pub(crate) fn request_graphics_pipeline(&self, asset_manager: &Arc<AssetManager<P>>, info: &GraphicsPipelineInfo) -> GraphicsPipelineHandle {
        self.shader_manager.request_graphics_pipeline(asset_manager, info)
    }

    pub(crate) fn request_compute_pipeline(&self, asset_manager: &Arc<AssetManager<P>>, shader_path: &str) -> ComputePipelineHandle {
        self.shader_manager.request_compute_pipeline(asset_manager, shader_path)
    }

    pub(crate) fn request_ray_tracing_pipeline(&self, asset_manager: &Arc<AssetManager<P>>, info: &RayTracingPipelineInfo) -> RayTracingPipelineHandle {
        self.shader_manager.request_ray_tracing_pipeline(asset_manager, info)
    }

    pub(crate) fn contains(&self, path: &str, asset_type: AssetType) -> bool {
        let assets = self.assets.read();
        assets.contains(path, asset_type)
    }

    pub(crate) fn contains_just_path(&self, path: &str) -> Option<AssetType> {
        let assets = self.assets.read();
        assets.contains_just_path(path)
    }

    pub(crate) fn read<'a>(&'a self) -> RendererAssetsReadOnly<'a, P> {
        RendererAssetsReadOnly {
            maps: self.assets.read(),
            placeholders: &self.placeholders,
            shader_manager: &self.shader_manager,
            vertex_buffer: self.integrator.vertex_buffer(),
            index_buffer: self.integrator.index_buffer()
        }
    }

    pub(crate) fn flush(&self, asset_manager: &Arc<AssetManager<P>>) {
        self.integrator.flush(asset_manager, &self.shader_manager);
    }
}

struct RendererAssetMaps<P: Platform> {
    textures: HandleMap<String, TextureHandle, RendererTexture<P::GPUBackend>>,
    materials: HandleMap<String, MaterialHandle, RendererMaterial>,
    meshes: HandleMap<String, MeshHandle, RendererMesh<P::GPUBackend>>,
    models: HandleMap<String, ModelHandle, RendererModel>,
    shaders: HandleMap<String, ShaderHandle, RendererShader<P::GPUBackend>>,
    graphics_pipelines: SimpleHandleMap<GraphicsPipelineHandle, RendererGraphicsPipeline<P>>,
    compute_pipelines: SimpleHandleMap<ComputePipelineHandle, RendererComputePipeline<P>>,
    ray_tracing_pipelines: SimpleHandleMap<RayTracingPipelineHandle, RendererRayTracingPipeline<P>>,
    requested_assets: HashSet<(String, AssetType)>,
}

impl<P: Platform> RendererAssetMaps<P> {
    fn remove_by_key(&mut self, asset_type: AssetType, path: &str) -> bool {
        let mut found = true;
        match asset_type {
            AssetType::Texture => {
                self.textures.remove_by_key(path);
            },
            AssetType::Model => {
                self.models.remove_by_key(path);
            },
            AssetType::Mesh => {
                self.meshes.remove_by_key(path);
            },
            AssetType::Material => {
                self.materials.remove_by_key(path);
            },
            AssetType::Shader => {
                self.shaders.remove_by_key(path);
            },
            _ => {
                found = false;
            }
        }
        found = self.requested_assets.remove(&(path.to_string(), asset_type)) || found;
        found
    }

    fn remove_request_by_path(&mut self, asset_type: AssetType, path: &str) -> bool {
        return self.requested_assets.remove(&(path.to_string(), asset_type));
    }

    fn reserve_handle(&mut self, path: &str, asset_type: AssetType) -> AssetHandle {
        return match asset_type {
            AssetType::Texture => AssetHandle::Texture(self.textures.get_or_create_handle(path)),
            AssetType::Model => AssetHandle::Model(self.models.get_or_create_handle(path)),
            AssetType::Mesh => AssetHandle::Mesh(self.meshes.get_or_create_handle(path)),
            AssetType::Material => AssetHandle::Material(self.materials.get_or_create_handle(path)),
            AssetType::Shader => AssetHandle::Shader(self.shaders.get_or_create_handle(path)),
            AssetType::GraphicsPipeline | AssetType::ComputePipeline | AssetType::RayTracingPipeline => panic!("Asset type {:?} cannot be reserved WITH a path.", asset_type),
            _ => panic!("Unsupported asset type {:?}", asset_type)
        };
    }

    pub(crate) fn reserve_handle_without_path(&mut self, asset_type: AssetType) -> AssetHandle {
        return match asset_type {
            AssetType::GraphicsPipeline => AssetHandle::GraphicsPipeline(self.graphics_pipelines.create_handle()),
            AssetType::ComputePipeline => AssetHandle::ComputePipeline(self.compute_pipelines.create_handle()),
            AssetType::RayTracingPipeline => AssetHandle::RayTracingPipeline(self.ray_tracing_pipelines.create_handle()),
            _ => panic!("Asset type cannot reserve handle without a path: {:?}", asset_type)
        }
    }

    fn add_asset(&mut self, asset: AssetWithHandle<P>) -> bool {
        match asset {
            AssetWithHandle::Texture(handle, asset) => self.textures.set(handle, asset),
            AssetWithHandle::Material(handle, asset) => self.materials.set(handle, asset),
            AssetWithHandle::Model(handle, asset) => self.models.set(handle, asset),
            AssetWithHandle::Mesh(handle, asset) => self.meshes.set(handle, asset),
            AssetWithHandle::Shader(handle, asset) => self.shaders.set(handle, asset),
            AssetWithHandle::GraphicsPipeline(handle, asset) => self.graphics_pipelines.set(handle, asset),
            AssetWithHandle::ComputePipeline(handle, asset) => self.compute_pipelines.set(handle, asset),
            AssetWithHandle::RayTracingPipeline(handle, asset) => self.ray_tracing_pipelines.set(handle, asset),
            _ => panic!("Unsupported asset type {:?}", asset.asset_type()),
        }
    }

    fn get_handle(&self, path: &str, asset_type: AssetType) -> Option<AssetHandle> {
        match asset_type {
            AssetType::Texture => self.textures.get_handle(path).map(|handle| AssetHandle::Texture(handle)),
            AssetType::Material => self.materials.get_handle(path).map(|handle| AssetHandle::Material(handle)),
            AssetType::Model => self.models.get_handle(path).map(|handle| AssetHandle::Model(handle)),
            AssetType::Mesh => self.meshes.get_handle(path).map(|handle| AssetHandle::Mesh(handle)),
            AssetType::Shader => self.shaders.get_handle(path).map(|handle| AssetHandle::Shader(handle)),
            AssetType::GraphicsPipeline | AssetType::ComputePipeline | AssetType::RayTracingPipeline => panic!("Asset type {:?} does not use paths.", asset_type),
            _ => panic!("Unsupported asset type {:?}", asset_type),
        }
    }

    fn get(&self, handle: AssetHandle) -> Option<AssetRef<P>> {
        match handle {
            AssetHandle::Texture(handle) => self.textures.get_value(handle).map(|asset| AssetRef::<P>::Texture(asset)),
            AssetHandle::Material(handle) => self.materials.get_value(handle).map(|asset| AssetRef::<P>::Material(asset)),
            AssetHandle::Model(handle) => self.models.get_value(handle).map(|asset| AssetRef::<P>::Model(asset)),
            AssetHandle::Mesh(handle) => self.meshes.get_value(handle).map(|asset| AssetRef::<P>::Mesh(asset)),
            AssetHandle::Shader(handle) => self.shaders.get_value(handle).map(|asset| AssetRef::<P>::Shader(asset)),
            AssetHandle::GraphicsPipeline(handle) => self.graphics_pipelines.get_value(handle).map(|asset| AssetRef::<P>::GraphicsPipeline(asset)),
            AssetHandle::ComputePipeline(handle) => self.compute_pipelines.get_value(handle).map(|asset| AssetRef::<P>::ComputePipeline(asset)),
            AssetHandle::RayTracingPipeline(handle) => self.ray_tracing_pipelines.get_value(handle).map(|asset| AssetRef::<P>::RayTracingPipeline(asset)),
            _ => panic!("Unsupported asset type {:?}", handle.asset_type()),
        }
    }

    fn contains(&self, path: &str, asset_type: AssetType) -> bool {
        match asset_type {
            AssetType::Texture => self.textures.contains_key(path),
            AssetType::Model => self.models.contains_key(path),
            AssetType::Mesh => self.meshes.contains_key(path),
            AssetType::Material => self.materials.contains_key(path),
            AssetType::Shader => self.shaders.contains_key(path),
            AssetType::GraphicsPipeline | AssetType::ComputePipeline | AssetType::RayTracingPipeline => panic!("Asset type {:?} does not use paths.", asset_type),
            _ => panic!("Unsupported asset type {:?}", asset_type),
        }
    }

    fn contains_request(&self, request: &(String, AssetType)) -> bool {
        self.requested_assets.contains(request)
    }

    fn insert_request(&mut self, request: &(String, AssetType), refresh: bool) -> bool {
        if (self.contains(&request.0, request.1) && !refresh) || self.requested_assets.contains(request) {
            return false;
        }
        self.requested_assets.insert(request.clone());
        true
    }

    fn is_empty(&self) -> bool {
        self.textures.is_empty()
            && self.materials.is_empty()
            && self.models.is_empty()
            && self.meshes.is_empty()
            && self.shaders.is_empty()
    }

    fn contains_just_path(&self, path: &str) -> Option<AssetType> {
        let mut asset_type = Option::<AssetType>::None;
        if let Some(_) = self.textures.get_handle(path) {
            asset_type = Some(AssetType::Texture);
        }
        if let Some(_) = self.materials.get_handle(path) {
            asset_type = Some(AssetType::Material);
        }
        if let Some(_) = self.models.get_handle(path) {
            asset_type = Some(AssetType::Model);
        }
        if let Some(_) = self.meshes.get_handle(path) {
            asset_type = Some(AssetType::Mesh);
        }
        if let Some(_) = self.shaders.get_handle(path) {
            asset_type = Some(AssetType::Shader);
        }
        asset_type
    }
}

pub struct RendererAssetsReadOnly<'a, P: Platform> {
    maps: RwLockReadGuard<'a, RendererAssetMaps<P>>,
    placeholders: &'a AssetPlaceholders<P>,
    shader_manager: &'a ShaderManager<P>,
    vertex_buffer: &'a Arc<BufferSlice<P::GPUBackend>>,
    index_buffer: &'a Arc<BufferSlice<P::GPUBackend>>,
}

impl<P: Platform> RendererAssetsReadOnly<'_, P> {
    pub fn get_model(&self, handle: ModelHandle) -> Option<&RendererModel> {
        self.maps.models.get_value(handle)
    }

    pub fn get_mesh(&self, handle: MeshHandle) -> Option<&RendererMesh<P::GPUBackend>> {
        self.maps.meshes.get_value(handle)
    }

    pub fn get_material(&self, handle: MaterialHandle) -> &RendererMaterial {
        self.maps.materials.get_value(handle).unwrap_or(self.placeholders.material())
    }

    pub fn get_placeholder_material(&self) -> &RendererMaterial {
        self.placeholders.material()
    }

    pub fn get_texture(&self, handle: TextureHandle, ) -> &RendererTexture<P::GPUBackend> {
        self.maps.textures.get_value(handle).unwrap_or(self.placeholders.texture_white())
    }

    pub fn get_texture_opt(&self, handle: TextureHandle, ) -> Option<&RendererTexture<P::GPUBackend>> {
        self.maps.textures.get_value(handle)
    }

    pub fn get_placeholder_texture_black(&self) -> &RendererTexture<P::GPUBackend> {
        self.placeholders.texture_black()
    }

    pub fn get_placeholder_texture_white(&self) -> &RendererTexture<P::GPUBackend> {
        self.placeholders.texture_white()
    }

    pub fn get_shader(&self, handle: ShaderHandle) -> Option<&RendererShader<P::GPUBackend>> {
        self.maps.shaders.get_value(handle)
    }

    pub fn get_shader_by_path(&self, path: &str) -> Option<&RendererShader<P::GPUBackend>> {
        self.maps.shaders.get_value_by_key(path)
    }

    pub fn get_graphics_pipeline(&self, handle: GraphicsPipelineHandle) -> Option<&Arc<GraphicsPipeline<P::GPUBackend>>> {
        self.maps.graphics_pipelines.get_value(handle).map(|c| &c.pipeline)
    }

    pub fn get_compute_pipeline(&self, handle: ComputePipelineHandle) -> Option<&Arc<ComputePipeline<P::GPUBackend>>> {
        self.maps.compute_pipelines.get_value(handle).map(|c| &c.pipeline)
    }

    pub fn get_ray_tracing_pipeline(&self, handle: RayTracingPipelineHandle) -> Option<&Arc<RayTracingPipeline<P::GPUBackend>>> {
        self.maps.ray_tracing_pipelines.get_value(handle).map(|c| &c.pipeline)
    }

    pub fn contains_shader_by_path(&self, path: &str) -> bool {
        self.maps.shaders.contains_value_for_key(path)
    }

    pub(crate) fn contains_just_path(&self, path: &str) -> Option<AssetType> {
        if self.maps.textures.contains_value_for_key(path) {
            return Some(AssetType::Texture);
        }
        if self.maps.materials.contains_value_for_key(path) {
            return Some(AssetType::Material);
        }
        if self.maps.meshes.contains_value_for_key(path) {
            return Some(AssetType::Mesh);
        }
        if self.maps.models.contains_value_for_key(path) {
            return Some(AssetType::Model);
        }
        if self.maps.shaders.contains_value_for_key(path) {
            return Some(AssetType::Shader);
        }
        None
    }

    pub fn all_graphics_pipelines(&self) -> Values<'_, GraphicsPipelineHandle, RendererGraphicsPipeline<P>> {
        self.maps.graphics_pipelines.values()
    }

    pub fn all_compute_pipelines(&self) -> Values<'_, ComputePipelineHandle, RendererComputePipeline<P>> {
        self.maps.compute_pipelines.values()
    }

    pub fn all_ray_tracing_pipelines(&self) -> Values<'_, RayTracingPipelineHandle, RendererRayTracingPipeline<P>> {
        self.maps.ray_tracing_pipelines.values()
    }

    pub fn get(&self, handle: AssetHandle) -> Option<AssetRef<P>> {
        match handle {
            AssetHandle::Texture(texture_handle) => Some(AssetRef::Texture(self.get_texture(texture_handle))),
            AssetHandle::Material(material_handle) => Some(AssetRef::Material(self.get_material(material_handle))),
            AssetHandle::Model(model_handle) => self.get_model(model_handle).map(|model| AssetRef::Model(model)),
            AssetHandle::Mesh(mesh_handle) => self.get_mesh(mesh_handle).map(|mesh| AssetRef::Mesh(mesh)),
            AssetHandle::Shader(shader_handle) => self.get_shader(shader_handle).map(|shader| AssetRef::Shader(shader)),
            AssetHandle::GraphicsPipeline(graphics_pipeline_handle) => self.maps.graphics_pipelines.get_value(graphics_pipeline_handle).map(|pipeline| AssetRef::GraphicsPipeline(pipeline)),
            AssetHandle::ComputePipeline(compute_pipeline_handle) => self.maps.compute_pipelines.get_value(compute_pipeline_handle).map(|pipeline| AssetRef::ComputePipeline(pipeline)),
            AssetHandle::RayTracingPipeline(ray_tracing_pipeline_handle) => self.maps.ray_tracing_pipelines.get_value(ray_tracing_pipeline_handle).map(|pipeline| AssetRef::RayTracingPipeline(pipeline)),
            _ => panic!("Asset type is not a renderer asset")
        }
    }

    pub fn all_pipeline_handles(&self, asset_type: AssetType) -> SmallVec<[AssetHandle; 16]> {
        let mut handles = SmallVec::<[AssetHandle; 16]>::new();
        match asset_type {
            AssetType::GraphicsPipeline => {
                for handle in self.maps.graphics_pipelines.handles() {
                    handles.push(AssetHandle::GraphicsPipeline(*handle));
                }
            },
            AssetType::ComputePipeline => {
                for handle in self.maps.compute_pipelines.handles() {
                    handles.push(AssetHandle::ComputePipeline(*handle));
                }
            },
            AssetType::RayTracingPipeline => {
                for handle in self.maps.ray_tracing_pipelines.handles() {
                    handles.push(AssetHandle::RayTracingPipeline(*handle));
                }
            },
            _ => panic!("Asset type is not a pipeline type")
        }
        handles
    }

    pub fn vertex_buffer(&self) -> &Arc<BufferSlice<P::GPUBackend>> {
        self.vertex_buffer
    }

    pub fn index_buffer(&self) -> &Arc<BufferSlice<P::GPUBackend>> {
        self.index_buffer
    }
}
