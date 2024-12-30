use std::{collections::HashSet, marker::PhantomData, sync::{Arc, RwLock, RwLockReadGuard}};

use sourcerenderer_core::Platform;

use crate::asset::*;

use super::*;

pub struct RendererAssets<P: Platform> {
    assets: RwLock<RendererAssetMaps<P>>,
    placeholders: AssetPlaceholders<P>,
    shader_manager: ShaderManager<P>,
    integrator: AssetIntegrator<P>,
    _platform: PhantomData<P>
}

impl<P: Platform> RendererAssets<P> {
    pub fn new(device: &Arc<crate::graphics::Device<P::GPUBackend>>) -> Self {
        Self {
            assets: RwLock::new(RendererAssetMaps {
                textures: HandleMap::new(),
                materials: HandleMap::new(),
                meshes: HandleMap::new(),
                models: HandleMap::new(),
                shaders: HandleMap::new(),
                requested_assets: HashSet::new()
            }),
            placeholders: AssetPlaceholders::new(device),
            shader_manager: ShaderManager::new(device),
            integrator: AssetIntegrator::new(device),
            _platform: PhantomData
        }
    }

    pub fn integrate(
        &self,
        asset_manager: &Arc<AssetManager<P>>,
        path: &str,
        asset_data: &AssetData,
        priority: AssetLoadPriority
    ) {
        self.integrator.integrate(asset_manager, &self.shader_manager, path, asset_data, priority)
    }

    pub(crate) fn reserve_handle(&self, path: &str, asset_type: AssetType) -> AssetHandle {
        let mut assets = self.assets.write().unwrap();
        assets.reserve_handle(path, asset_type)
    }

    pub(crate) fn remove_by_key(&self, asset_type: AssetType, path: &str) -> bool {
        let mut assets = self.assets.write().unwrap();
        assets.remove_by_key(asset_type, path)
    }

    pub(crate) fn remove_request_by_path(&self, asset_type: AssetType, path: &str) -> bool {
        let mut assets = self.assets.write().unwrap();
        assets.remove_request_by_path(asset_type, path)
    }

    pub(crate) fn add_asset(&self, asset: AssetWithHandle<P>) -> bool {
        let mut assets = self.assets.write().unwrap();
        assets.add_asset(asset)
    }

    pub(crate) fn insert_request(&self, request: &(String, AssetType)) -> bool {
        let mut assets = self.assets.write().unwrap();
        assets.insert_request(request)
    }

    pub fn read<'a>(&'a self) -> RendererAssetsReadOnly<'a, P> {
        RendererAssetsReadOnly {
            maps: self.assets.read().unwrap(),
            placeholders: &self.placeholders,
            shader_manager: &self.shader_manager
        }
    }
}

struct RendererAssetMaps<P: Platform> {
    textures: HandleMap<String, TextureHandle, RendererTexture<P::GPUBackend>>,
    materials: HandleMap<String, MaterialHandle, RendererMaterial>,
    meshes: HandleMap<String, MeshHandle, RendererMesh<P::GPUBackend>>,
    models: HandleMap<String, ModelHandle, RendererModel>,
    shaders: HandleMap<String, ShaderHandle, RendererShader<P::GPUBackend>>,
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
            _ => panic!("Unsupported asset type")
        };
    }

    fn add_asset(&mut self, asset: AssetWithHandle<P>) -> bool {
        match asset {
            AssetWithHandle::Texture(handle, asset) => self.textures.set(handle, asset),
            AssetWithHandle::Material(handle, asset) => self.materials.set(handle, asset),
            AssetWithHandle::Model(handle, asset) => self.models.set(handle, asset),
            AssetWithHandle::Mesh(handle, asset) => self.meshes.set(handle, asset),
            AssetWithHandle::Shader(handle, asset) => self.shaders.set(handle, asset),
            _ => panic!("Unsupported asset type"),
        }
    }

    fn get_handle(&self, path: &str, asset_type: AssetType) -> Option<AssetHandle> {
        match asset_type {
            AssetType::Texture => self.textures.get_handle(path).map(|handle| AssetHandle::Texture(handle)),
            AssetType::Material => self.materials.get_handle(path).map(|handle| AssetHandle::Material(handle)),
            AssetType::Model => self.models.get_handle(path).map(|handle| AssetHandle::Model(handle)),
            AssetType::Mesh => self.meshes.get_handle(path).map(|handle| AssetHandle::Mesh(handle)),
            AssetType::Shader => self.shaders.get_handle(path).map(|handle| AssetHandle::Shader(handle)),
            _ => panic!("Unsupported asset type"),
        }
    }

    fn get(&self, handle: AssetHandle) -> Option<AssetRef<P>> {
        match handle {
            AssetHandle::Texture(handle) => self.textures.get_value(handle).map(|asset| AssetRef::<P>::Texture(asset)),
            AssetHandle::Material(handle) => self.materials.get_value(handle).map(|asset| AssetRef::<P>::Material(asset)),
            AssetHandle::Model(handle) => self.models.get_value(handle).map(|asset| AssetRef::<P>::Model(asset)),
            AssetHandle::Mesh(handle) => self.meshes.get_value(handle).map(|asset| AssetRef::<P>::Mesh(asset)),
            AssetHandle::Shader(handle) => self.shaders.get_value(handle).map(|asset| AssetRef::<P>::Shader(asset)),
            _ => panic!("Unsupported asset type"),
        }
    }

    fn contains(&self, path: &str, asset_type: AssetType) -> bool {
        match asset_type {
            AssetType::Texture => self.textures.contains_key(path),
            AssetType::Model => self.models.contains_key(path),
            AssetType::Mesh => self.meshes.contains_key(path),
            AssetType::Material => self.materials.contains_key(path),
            AssetType::Shader => self.shaders.contains_key(path),
            _ => panic!("Unsupported asset type"),
        }
    }

    fn contains_request(&self, request: &(String, AssetType)) -> bool {
        self.requested_assets.contains(request)
    }

    fn insert_request(&mut self, request: &(String, AssetType)) -> bool {
        if self.contains(&request.0, request.1) || self.requested_assets.contains(request) {
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
    shader_manager: &'a ShaderManager<P>
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

    pub fn get_texture(&self, handle: TextureHandle, ) -> &RendererTexture<P::GPUBackend> {
        self.maps.textures.get_value(handle).unwrap_or(self.placeholders.texture_white())
    }

    pub fn get_shader(&self, handle: ShaderHandle) -> Option<&RendererShader<P::GPUBackend>> {
        self.maps.shaders.get_value(handle)
    }

    pub fn get_shader_by_path(&self, path: &str) -> Option<&RendererShader<P::GPUBackend>> {
        self.maps.shaders.get_value_by_path(path)
    }

    pub fn contains_shader_by_path(&self, path: &str) -> bool {
        self.maps.shaders.contains_value_for_key(path)
    }
}
