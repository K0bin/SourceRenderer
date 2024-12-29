use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::hash::Hash;
use std::io::{
    Read,
    Result as IOResult,
    Seek,
    SeekFrom,
};
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{
    AtomicU32,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    RwLock,
};

use bevy_tasks::futures_lite::io::{Cursor, AsyncAsSync};
use bevy_tasks::futures_lite::AsyncSeekExt;
use bevy_tasks::{AsyncComputeTaskPool, IoTaskPool};
use crossbeam_channel::{
    unbounded,
    Receiver,
    Sender,
};
use futures_io::{AsyncRead, AsyncSeek};
use gltf::json::extensions::asset;
use log::{
    error,
    trace,
    warn,
};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{GPUBackend, PackedShader};
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::Vec4;

use crate::math::BoundingBox;
use crate::graphics::TextureInfo;
use crate::renderer::asset::{self as renderer_asset, AssetIntegrator as RendererAssetIntegrator, RendererMaterial, RendererMesh, RendererModel, RendererShader, RendererTexture};

use super::loaded_level::LoadedLevel;
use super::{Asset, AssetData, AssetHandle, AssetType, AssetWithHandle, HandleMap, MaterialData, MaterialHandle, MeshData, MeshHandle, MeshRange, ModelData, ModelHandle, ShaderData, ShaderHandle, SoundHandle, TextureData, TextureHandle};

pub struct AssetLoadRequest {
    pub path: String,
    pub asset_type: AssetType,
    pub progress: Arc<AssetLoaderProgress>,
    pub priority: AssetLoadPriority,
}

pub struct SimpleAssetLoadRequest {
    pub path: String,
    pub asset_type: AssetType,
}

pub struct AssetFile {
    pub path: String,
    pub data: Cursor<Box<[u8]>>,
}

impl AsyncRead for AssetFile {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<IOResult<usize>> {
        AsyncRead::poll_read(Pin::new(&mut self.as_mut().data), cx, buf)
    }
}

impl AsyncSeek for AssetFile {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: SeekFrom,
    ) -> std::task::Poll<IOResult<u64>> {
        AsyncSeek::poll_seek(Pin::new(&mut self.as_mut().data), cx, pos)
    }
}

impl Read for AssetFile {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
        let waker = waker_fn::waker_fn(|| {});
        let mut context = std::task::Context::from_waker(&waker);
        let mut as_sync = AsyncAsSync::new(&mut context, &mut self.data);
        as_sync.read(buf)
    }
}

impl Seek for AssetFile {
    fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
        let waker = waker_fn::waker_fn(|| {});
        let mut context = std::task::Context::from_waker(&waker);
        let mut as_sync = AsyncAsSync::new(&mut context, &mut self.data);
        as_sync.seek(pos)
    }
}

pub trait AssetContainer: Send + Sync + 'static {
    async fn contains(&self, path: &str) -> bool {
        self.load(path).await.is_some()
    }
    async fn load(&self, path: &str) -> Option<AssetFile>;
}

pub trait ErasedAssetContainer: Send + Sync {
    fn contains<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = bool> + 'a>>;
    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<AssetFile>> + 'a>>;
}

impl<T> ErasedAssetContainer for T
    where T: AssetContainer {
    fn contains<'a> (&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = bool> + 'a>> {
        Box::pin(AssetContainer::contains(self, path))
    }

    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<AssetFile>> + 'a>> {
        Box::pin(AssetContainer::load(self, path))
    }
}

pub struct AssetLoaderProgress {
    expected: AtomicU32,
    finished: AtomicU32,
}

impl AssetLoaderProgress {
    pub fn is_done(&self) -> bool {
        self.finished.load(Ordering::SeqCst) == self.expected.load(Ordering::SeqCst)
    }
}

pub enum DirectlyLoadedAsset {
    None,
    Level(LoadedLevel),
}

pub struct AssetLoaderResult {
    pub file_requests: SmallVec<[String; 1]>,
    pub requests: SmallVec<[AssetLoadRequest; 4]>,
    pub primary_asset: DirectlyLoadedAsset,
    pub loaded_assets: SmallVec<[AssetData; 4]>
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AssetLoadPriority {
    High,
    Normal,
    Low,
}

pub trait AssetLoader<P: Platform>: Send + Sync + 'static {
    fn matches(&self, file: &mut AssetFile) -> bool;
    async fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()>;
}

pub trait ErasedAssetLoader<P: Platform>: Send + Sync {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load<'a>(
        &'a self,
        file: AssetFile,
        manager: &'a Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &'a Arc<AssetLoaderProgress>,
    ) -> Pin<Box<dyn Future<Output = Result<DirectlyLoadedAsset, ()>> + 'a>>;
}

impl<T, P: Platform> ErasedAssetLoader<P> for T
    where T: AssetLoader<P> {
    fn matches(&self, file: &mut AssetFile) -> bool {
        AssetLoader::<P>::matches(self, file)
    }

    fn load<'a>(
        &'a self,
        file: AssetFile,
        manager: &'a Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &'a Arc<AssetLoaderProgress>,
    ) -> Pin<Box<dyn Future<Output = Result<DirectlyLoadedAsset, ()>> + 'a>> {
        Box::pin(AssetLoader::<P>::load(self, file, manager, priority, progress))
    }
}

pub struct AssetManager<P: Platform> {
    device: Arc<crate::graphics::Device<P::GPUBackend>>,
    containers: RwLock<Vec<Box<dyn ErasedAssetContainer>>>,
    loaders: RwLock<Vec<Box<dyn ErasedAssetLoader<P>>>>,

    renderer_assets: RwLock<RendererAssets<P>>,
    renderer_integrator: RendererAssetIntegrator<P>,
}

pub struct RendererAssets<P: Platform> {
    textures: HandleMap<TextureHandle, RendererTexture<P::GPUBackend>>,
    materials: HandleMap<MaterialHandle, RendererMaterial>,
    meshes: HandleMap<MeshHandle, RendererMesh<P::GPUBackend>>,
    models: HandleMap<ModelHandle, RendererModel>,
    shaders: HandleMap<ShaderHandle, RendererShader<P::GPUBackend>>,
    requested_assets: HashSet<(String, AssetType)>,
    _platform: PhantomData<P>
}

impl<P: Platform> RendererAssets<P> {
    fn remove_by_path(&mut self, asset_type: AssetType, path: &str) -> bool {
        let mut found = true;
        match asset_type {
            AssetType::Texture => {
                self.textures.remove_by_path(&path);
            },
            AssetType::Model => {
                self.models.remove_by_path(&path);
            },
            AssetType::Mesh => {
                self.meshes.remove_by_path(&path);
            },
            AssetType::Material => {
                self.materials.remove_by_path(&path);
            },
            AssetType::Shader => {
                self.shaders.remove_by_path(&path);
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

    pub(crate) fn get_handle(&self, path: &str, asset_type: AssetType) -> Option<AssetHandle> {
        match asset {
            AssetWithHandle::Texture(handle, asset) => self.textures.get_handle(path).map(|handle| AssetHandle::Texture(handle)),
            AssetWithHandle::Material(handle, asset) => self.materials.get_handle(path).map(|handle| AssetHandle::Material(handle)),
            AssetWithHandle::Model(handle, asset) => self.models.get_handle(path).map(|handle| AssetHandle::Model(handle)),
            AssetWithHandle::Mesh(handle, asset) => self.meshes.get_handle(path).map(|handle| AssetHandle::Mesh(handle)),
            AssetWithHandle::Shader(handle, asset) => self.shaders.get_handle(path).map(|handle| AssetHandle::Shader(handle)),
            _ => panic!("Unsupported asset type"),
        }
    }

    pub(crate) fn get(&self, handle: AssetHandle) -> Option<&Asset<P>> {
        match asset {
            AssetWithHandle::Texture(handle, asset) => self.textures.get_value(handle).map(|asset| Asset::<P>::Texture(asset)),
            AssetWithHandle::Material(handle, asset) => self.materials.get_value(handle).map(|asset| Asset::<P>::Material(asset)),
            AssetWithHandle::Model(handle, asset) => self.models.get_value(handle).map(|asset| Asset::<P>::Model(asset)),
            AssetWithHandle::Mesh(handle, asset) => self.meshes.get_value(handle).map(|hasset| Asset::<P>::Mesh(asset)),
            AssetWithHandle::Shader(handle, asset) => self.shaders.get_value(handle).map(|asset| Asset::<P>::Shader(asset)),
            _ => panic!("Unsupported asset type"),
        }
    }

    pub(crate) fn contains(&self, path: &str, asset_type: AssetType) -> bool {
        match asset_type {
            AssetType::Texture => self.textures.contains_path(path),
            AssetType::Model => self.models.contains_path(path),
            AssetType::Mesh => self.meshes.contains_path(path),
            AssetType::Material => self.materials.contains_path(path),
            AssetType::Shader => self.shaders.contains_path(path),
            _ => panic!("Unsupported asset type"),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.textures.is_empty()
            && self.materials.is_empty()
            && self.models.is_empty()
            && self.meshes.is_empty()
            && self.shaders.is_empty()
    }
}

impl<P: Platform> AssetManager<P> {
    pub fn new(
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
    ) -> Arc<Self> {
        let manager = Arc::new(Self {
            device: device.clone(),
            loaders: RwLock::new(Vec::new()),
            containers: RwLock::new(Vec::new()),
            renderer_assets: RwLock::new(RendererAssets {
                textures: HandleMap::new(),
                materials: HandleMap::new(),
                meshes: HandleMap::new(),
                models: HandleMap::new(),
                shaders: HandleMap::new(),
                requested_assets: HashSet::new(),
                _platform: PhantomData,
            }),
            renderer_integrator: RendererAssetIntegrator::new(device)
        });

        manager
    }

    pub fn graphics_device(&self) -> &Arc<crate::graphics::Device<P::GPUBackend>> {
        &self.device
    }

    pub fn add_mesh_data(
        self: &Arc<Self>,
        path: &str,
        vertex_buffer_data: Box<[u8]>,
        vertex_count: u32,
        index_buffer_data: Box<[u8]>,
        parts: Box<[MeshRange]>,
        bounding_box: Option<BoundingBox>,
    ) {
        assert_ne!(vertex_count, 0);
        let mesh = MeshData {
            vertices: vertex_buffer_data,
            indices: if !index_buffer_data.is_empty() {
                Some(index_buffer_data)
            } else {
                None
            },
            parts,
            bounding_box,
            vertex_count,
        };
        self.add_asset_data(path, AssetData::Mesh(mesh), AssetLoadPriority::Normal);
    }

    pub fn add_material_data(self: &Arc<Self>, path: &str, albedo: &str, roughness: f32, metalness: f32) {
        let material = MaterialData::new_pbr(albedo, roughness, metalness);
        self.add_asset_data(path, AssetData::Material(material), AssetLoadPriority::Normal);
    }

    pub fn add_material_data_color(self: &Arc<Self>, path: &str, albedo: Vec4, roughness: f32, metalness: f32) {
        let material: MaterialData = MaterialData::new_pbr_color(albedo, roughness, metalness);
        self.add_asset_data(path, AssetData::Material(material), AssetLoadPriority::Normal);
    }

    pub fn add_model_data(self: &Arc<Self>, path: &str, mesh_path: &str, material_paths: &[&str]) {
        let model = ModelData {
            mesh_path: mesh_path.to_string(),
            material_paths: material_paths.iter().map(|mat| (*mat).to_owned()).collect(),
        };
        self.add_asset_data(path, AssetData::Model(model), AssetLoadPriority::Normal);
    }

    pub fn add_texture_data(self: &Arc<Self>, path: &str, info: &TextureInfo, texture_data: Box<[u8]>) {
        self.add_asset_data(
            path,
            AssetData::Texture(TextureData {
                info: info.clone(),
                data: Box::new([texture_data.to_vec().into_boxed_slice()]),
            }),
            AssetLoadPriority::Normal,
        );
    }

    pub fn add_container(self: &Arc<Self>, container: impl AssetContainer) {
        self.add_container_with_progress(container, None)
    }

    pub fn add_container_with_progress(
        self: &Arc<Self>,
        container: impl AssetContainer,
        progress: Option<&Arc<AssetLoaderProgress>>,
    ) {
        let mut containers = self.containers.write().unwrap();
        containers.push(Box::new(container));
        if let Some(progress) = progress {
            progress.finished.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn add_loader(self: &Arc<Self>, loader: impl AssetLoader<P>) {
        let mut loaders = self.loaders.write().unwrap();
        loaders.push(Box::new(loader));
    }

    pub fn add_asset_data(self: &Arc<Self>, path: &str, asset: AssetData, priority: AssetLoadPriority) {
        self.add_asset_data_with_progress(path, asset, None, priority)
    }

    pub fn add_asset_data_with_progress(
        self: &Arc<Self>,
        path: &str,
        asset: AssetData,
        progress: Option<&Arc<AssetLoaderProgress>>,
        priority: AssetLoadPriority,
    ) {
        let asset_type = match &asset {
            AssetData::Texture(_) => AssetType::Texture,
            AssetData::Material(_) => AssetType::Material,
            AssetData::Mesh(_) => AssetType::Mesh,
            AssetData::Model(_) => AssetType::Model,
            AssetData::Sound(_) => AssetType::Sound,
            AssetData::Shader(_) => AssetType::Shader,
        };

        if let Some(progress) = progress {
            progress.finished.fetch_add(1, Ordering::SeqCst);
        }
        if asset.is_renderer_asset() {
            self.renderer_integrator.integrate(self, path, asset_data, priority);
        } else {
            unimplemented!();
        }
    }

    pub fn reserve_handle(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType
    ) -> AssetHandle {
        if asset_type.is_renderer_asset() {
            let mut renderer_assets = self.renderer_assets.write().unwrap();
            return renderer_assets.reserve_handle(path, asset_type);
        } else {
            unimplemented!()
        }
    }

    pub fn add_asset(
        self: &Arc<Self>,
        path: &str,
        asset: Asset<P>
    ) {
        let handle = self.reserve_handle(path, asset.asset_type());
        self.add_asset_with_handle(AssetWithHandle::combine(handle, asset));
    }

    pub fn add_asset_with_handle(
        self: &Arc<Self>,
        asset: AssetWithHandle<P>
    ) {
        if asset.is_renderer_asset() {
            let mut renderer_assets = self.renderer_assets.write().unwrap();
            renderer_assets.add_asset(asset);
        } else {
            unimplemented!();
        }
    }

    pub fn request_asset_update(self: &Arc<Self>, path: &str) {
        log::info!("Reloading: {}", path);
        {
            let mut renderer_assets = self.renderer_assets.write().unwrap();
            let mut asset_type = Option::<AssetType>::None;
            if let Some(handle) = renderer_assets.textures.get_handle(path) {
                asset_type = Some(AssetType::Texture);
            }
            if let Some(handle) = renderer_assets.materials.get_handle(path) {
                asset_type = Some(AssetType::Material);
            }
            if let Some(handle) = renderer_assets.models.get_handle(path) {
                asset_type = Some(AssetType::Model);
            }
            if let Some(handle) = renderer_assets.meshes.get_handle(path) {
                asset_type = Some(AssetType::Mesh);
            }
            if let Some(handle) = renderer_assets.shaders.get_handle(path) {
                asset_type = Some(AssetType::Shader);
            }
        }

        if let Some(asset_type) = asset_type {
            self.request_asset_internal(path, asset_type, AssetLoadPriority::Low, None, true);
        } else {
            warn!("Cannot reload unloaded asset {}", path);
        }
    }

    pub fn request_asset(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
    ) -> Arc<AssetLoaderProgress> {
        self.request_asset_internal(path, asset_type, priority, None, false)
    }

    pub fn request_asset_with_progress(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Arc<AssetLoaderProgress> {
        self.request_asset_internal(path, asset_type, priority, Some(progress), false)
    }

    fn request_asset_internal(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
        progress: Option<&Arc<AssetLoaderProgress>>,
        refresh: bool,
    ) -> Arc<AssetLoaderProgress> {
        let progress = progress.map_or_else(
            || {
                Arc::new(AssetLoaderProgress {
                    expected: AtomicU32::new(0),
                    finished: AtomicU32::new(0),
                })
            },
            |p| p.clone(),
        );
        progress.expected.fetch_add(1, Ordering::SeqCst);

        if asset_type.is_renderer_asset() {
            let request_key = (path.to_string(), asset_type);
            let mut renderer_assets = self.renderer_assets.write().unwrap();
            if (renderer_assets.contains(path, asset_type) && !refresh) || renderer_assets.requested_assets.contains(&request_key) {
                progress.finished.fetch_add(1, Ordering::SeqCst);
                return progress;
            }
            renderer_assets.requested_assets.insert(request_key);
        } else {
            unimplemented!()
        }

        let load_request = AssetLoadRequest {
            asset_type,
            path: path.to_owned(),
            progress: progress.clone(),
            priority,
        };


        let asset_mgr = self.clone();
        IoTaskPool::get().spawn(async move {
            let containers = asset_mgr.containers.read().unwrap();
            let file_opt = asset_mgr.load_file(&load_request.path).await;
            if file_opt.is_none() {
                load_request.progress.finished.fetch_add(1, Ordering::SeqCst);
                return;
            }
            std::mem::drop(containers);
            let file = file_opt.unwrap();
            AsyncComputeTaskPool::get().spawn(async move {
                asset_mgr.load_asset(file, asset_type, priority, &load_request.progress).await;
            });
        });
        progress
    }

    async fn directly_load_asset(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType
    ) -> Result<DirectlyLoadedAsset, ()> {
        assert_eq!(asset_type, AssetType::Level);

        let progress = Arc::new(AssetLoaderProgress {
            expected: AtomicU32::new(1),
            finished: AtomicU32::new(0)
        });
        let file = self.load_file(path).await;
        if file.is_none() {
            return Err(());
        }
        let file: AssetFile = file.unwrap();
        self.load_asset(file, asset_type, AssetLoadPriority::High, &progress).await
    }

    pub async fn load_level(self: &Arc<Self>, path: &str) -> Option<LoadedLevel> {
        let directly_loaded = self.directly_load_asset(path, AssetType::Level).await.ok()?;
        match directly_loaded {
            DirectlyLoadedAsset::Level(level) => Some(level),
            _ => None
        }
    }

    pub async fn load_file(self: &Arc<Self>, path: &str) -> Option<AssetFile> {
        let containers = self.containers.read().unwrap();
        let mut file_opt: Option<AssetFile> = None;
        for container in containers.iter().rev() {
            let container_file_opt = container.load(path).await;
            if container_file_opt.is_some() {
                file_opt = container_file_opt;
                break;
            }
        }
        file_opt
    }

    pub async fn file_exists(&self, path: &str) -> bool {
        let containers = self.containers.read().unwrap();
        for container in containers.iter() {
            if container.contains(path).await {
                return true;
            }
        }
        false
    }

    async fn find_loader<'a>(
        file: &mut AssetFile,
        loaders: &'a [Box<dyn ErasedAssetLoader<P>>],
    ) -> Option<&'a dyn ErasedAssetLoader<P>> {
        let start = AsyncSeekExt::seek(file, SeekFrom::Current(0)).await
            .unwrap_or_else(|_| panic!("Failed to read file: {:?}", file.path));

        let mut loader_opt = Option::<&Box<dyn ErasedAssetLoader<P>>>::None;
        for loader in loaders {
            let loader_matches = loader.matches(file);
            AsyncSeekExt::seek(file, SeekFrom::Start(start)).await.unwrap();
            if loader_matches {
                loader_opt = Some(loader);
                break;
            }
        }
        loader_opt.map(|b| b.as_ref())
    }

    async fn load_asset(
        self: &Arc<Self>,
        mut file: AssetFile,
        asset_type: AssetType,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()> {
        let path = file.path.clone();

        let loaders = self.loaders.read().unwrap();
        let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref()).await;
        if loader_opt.is_none() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            if asset_type.is_renderer_asset() {
                let mut renderer_assets = self.renderer_assets.write().unwrap();
                renderer_assets.remove_request_by_path(asset_type, &path);
            } else {
                unimplemented!();
            }
            error!("Could not find loader for file: {:?}", path.as_str());
            return Err(());
        }
        let loader = loader_opt.unwrap();

        let assets_opt = loader.load(file, self, priority, progress).await;
        if assets_opt.is_err() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            if asset_type.is_renderer_asset() {
                let mut renderer_assets = self.renderer_assets.write().unwrap();
                renderer_assets.remove_request_by_path(asset_type, &path);
            } else {
                unimplemented!();
            }
            error!("Could not load file: {:?}", path.as_str());
            return Err(());
        }
        Ok(assets_opt.unwrap())
    }

    pub fn stop(&self) {
        trace!("Stopping asset manager");
    }
}
