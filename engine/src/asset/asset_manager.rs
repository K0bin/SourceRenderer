use std::collections::{
    HashMap,
    VecDeque,
};
use std::future::Future;
use std::hash::Hash;
use std::io::{
    Read,
    Result as IOResult,
    Seek,
    SeekFrom,
};
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::pin::Pin;
use std::process::Output;
use std::sync::atomic::{
    AtomicBool,
    AtomicU32,
    Ordering,
};
use std::sync::{
    Arc,
    Condvar,
    Mutex,
    RwLock,
    Weak,
};

use bevy_ecs::bundle::Bundle;
use bevy_ecs::query::Has;
use bevy_ecs::schedule::Condition;
use bevy_ecs::system::Resource;
use bevy_ecs::world::World;
use bevy_tasks::futures_lite::io::{Cursor, AsyncAsSync};
use bevy_tasks::futures_lite::{AsyncReadExt, AsyncSeekExt, FutureExt};
use bevy_tasks::{AsyncComputeTaskPool, IoTaskPool};
use crossbeam_channel::{
    unbounded,
    Receiver,
    Sender,
};
use futures_io::{AsyncRead, AsyncSeek};
use log::{
    error,
    trace,
    warn,
};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::PackedShader;
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::Vec4;

use crate::math::BoundingBox;
use crate::graphics::TextureInfo;

use super::loaded_level::LoadedLevel;
use super::loaders::{BspLevelLoader, CSGODirectoryContainer, FSContainer, GltfContainer, GltfLoader, ImageLoader, MDLModelLoader, PakFileContainer, ShaderLoader, VMTMaterialLoader, VPKContainer, VPKContainerLoader, VTFTextureLoader};

pub struct AssetLoadRequest {
    pub path: String,
    pub asset_type: AssetType,
    pub progress: Arc<AssetLoaderProgress>,
    pub priority: AssetLoadPriority,
}

pub struct LoadedAsset {
    pub path: String,
    pub asset: Asset,
    pub priority: AssetLoadPriority,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Clone)]
pub struct MeshRange {
    pub start: u32,
    pub count: u32,
}

pub struct Texture {
    pub info: TextureInfo,
    pub data: Box<[Box<[u8]>]>,
}

pub struct Mesh {
    pub indices: Option<Box<[u8]>>,
    pub vertices: Box<[u8]>,
    pub parts: Box<[MeshRange]>,
    pub bounding_box: Option<BoundingBox>,
    pub vertex_count: u32,
}

#[derive(Clone)]
pub struct Model {
    pub mesh_path: String,
    pub material_paths: Vec<String>,
}

#[derive(Clone)]
pub struct Material {
    pub shader_name: String,
    pub properties: HashMap<String, MaterialValue>,
}

impl Material {
    pub fn new_pbr(albedo_texture_path: &str, roughness: f32, metalness: f32) -> Self {
        let mut props = HashMap::new();
        props.insert(
            "albedo".to_string(),
            MaterialValue::Texture(albedo_texture_path.to_string()),
        );
        props.insert("roughness".to_string(), MaterialValue::Float(roughness));
        props.insert("metalness".to_string(), MaterialValue::Float(metalness));
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }

    pub fn new_pbr_color(albedo: Vec4, roughness: f32, metalness: f32) -> Self {
        let mut props = HashMap::new();
        props.insert("albedo".to_string(), MaterialValue::Vec4(albedo));
        props.insert("roughness".to_string(), MaterialValue::Float(roughness));
        props.insert("metalness".to_string(), MaterialValue::Float(metalness));
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }
}

#[derive(Clone)]
pub enum MaterialValue {
    Texture(String),
    Float(f32),
    Vec4(Vec4),
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

pub trait AssetContainerAsync: Send + Sync + 'static {
    async fn contains(&self, path: &str) -> bool {
        self.load(path).await.is_some()
    }
    async fn load(&self, path: &str) -> Option<AssetFile>;
}

pub trait ErasedAssetContainerAsync: Send + Sync {
    fn contains<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = bool> + 'a>>;
    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<AssetFile>> + 'a>>;
}

impl<T> ErasedAssetContainerAsync for T
    where T: AssetContainerAsync {
    fn contains<'a> (&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = bool> + 'a>> {
        Box::pin(AssetContainerAsync::contains(self, path))
    }

    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<AssetFile>> + 'a>> {
        Box::pin(AssetContainerAsync::load(self, path))
    }
}

pub trait AssetContainer: Send + Sync + 'static {
    fn contains(&self, path: &str) -> bool {
        self.load(path).is_some()
    }
    fn load(&self, path: &str) -> Option<AssetFile>;
}

struct SyncAssetContainerWrapper<T: AssetContainer + 'static>(Arc<T>);

#[cfg(feature = "threading")]
impl<T: AssetContainer + 'static> AssetContainerAsync for SyncAssetContainerWrapper<T> {
     fn contains(&self, path: &str) -> impl Future<Output = bool> {
        let c_inner = self.0.clone();
        let c_path = path.to_string();
        let task_pool = IoTaskPool::get();
        task_pool.spawn(async move {
            c_inner.contains(&c_path)
        }).await
    }

    async fn load(&self, path: &str) -> Option<AssetFile> {
        let c_inner = self.0.clone();
        let c_path = path.to_string();
        let task_pool = IoTaskPool::get();
        task_pool.spawn(async move {
            c_inner.load(&c_path)
        }).await
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

pub struct AssetLoaderAsyncResult {
    pub file_requests: SmallVec<[String; 1]>,
    pub requests: SmallVec<[AssetLoadRequest; 4]>,
    pub primary_asset: DirectlyLoadedAsset,
    pub loaded_assets: SmallVec<[LoadedAsset; 4]>
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AssetLoadPriority {
    High,
    Normal,
    Low,
}

pub trait AssetLoaderAsync<P: Platform>: Send + Sync + 'static {
    fn matches(&self, file: &mut AssetFile) -> bool;
    async fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()>;
}

pub trait ErasedAssetLoaderAsync<P: Platform>: Send + Sync {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load<'a>(
        &'a self,
        file: AssetFile,
        manager: &'a Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &'a Arc<AssetLoaderProgress>,
    ) -> Pin<Box<dyn Future<Output = Result<DirectlyLoadedAsset, ()>> + 'a>>;
}

impl<T, P: Platform> ErasedAssetLoaderAsync<P> for T
    where T: AssetLoaderAsync<P> {
    fn matches(&self, file: &mut AssetFile) -> bool {
        AssetLoaderAsync::<P>::matches(self, file)
    }

    fn load<'a>(
        &'a self,
        file: AssetFile,
        manager: &'a Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &'a Arc<AssetLoaderProgress>,
    ) -> Pin<Box<dyn Future<Output = Result<DirectlyLoadedAsset, ()>> + 'a>> {
        Box::pin(AssetLoaderAsync::<P>::load(self, file, manager, priority, progress))
    }
}

pub trait AssetLoader<P: Platform>: Send + Sync + 'static {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()>;
}

struct SyncAssetLoaderWrapper<P: Platform, T: AssetLoader<P> + 'static>(Arc<T>, PhantomData<P>);

unsafe impl<P: Platform, T: AssetLoader<P> + 'static> Send for SyncAssetLoaderWrapper<P, T> {}
unsafe impl<P: Platform, T: AssetLoader<P> + 'static> Sync for SyncAssetLoaderWrapper<P, T> {}

#[cfg(feature = "threading")]
impl<P: Platform, T: AssetLoader<P> + 'static> AssetLoaderAsync<P> for SyncAssetLoaderWrapper<P, T> {
    fn matches(&self, file: &mut AssetFile) -> bool {
        self.0.matches(file)
    }

    async fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()> {
        let c_inner = self.0.clone();
        let c_manager = manager.clone();
        let c_progress = progress.clone();
        let task_pool = bevy_tasks::IoTaskPool::get();
        task_pool.spawn(async move {
            c_inner.load(file, &c_manager, priority, &c_progress)
        }).await;
        Ok(DirectlyLoadedAsset::None)
    }
}

pub enum Asset {
    Texture(Texture),
    Mesh(Mesh),
    Model(Model),
    Sound,
    Material(Material),
    Shader(PackedShader),
}

pub struct AssetManager<P: Platform> {
    device: Arc<crate::graphics::Device<P::GPUBackend>>,
    renderer_receiver: Receiver<LoadedAsset>,
    containers: RwLock<Vec<Box<dyn ErasedAssetContainerAsync>>>,
    loaders: RwLock<Vec<Box<dyn ErasedAssetLoaderAsync<P>>>>,
    renderer_sender: Sender<LoadedAsset>,
    assets: Mutex<AssetManagerAssets>,
}

struct AssetManagerAssets {
    requested_assets: HashMap<String, AssetType>,
    loaded_assets: HashMap<String, AssetType>,
}

impl<P: Platform> AssetManager<P> {
    pub fn new(
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
    ) -> Arc<Self> {
        let (renderer_sender, renderer_receiver) = unbounded();

        let manager = Arc::new(Self {
            device: device.clone(),
            loaders: RwLock::new(Vec::new()),
            containers: RwLock::new(Vec::new()),
            renderer_sender,
            assets: Mutex::new(AssetManagerAssets {
                requested_assets: HashMap::new(),
                loaded_assets: HashMap::new(),
            }),
            renderer_receiver,
        });

        manager
    }

    pub fn graphics_device(&self) -> &Arc<crate::graphics::Device<P::GPUBackend>> {
        &self.device
    }

    pub fn add_mesh(
        self: &Arc<Self>,
        path: &str,
        vertex_buffer_data: Box<[u8]>,
        vertex_count: u32,
        index_buffer_data: Box<[u8]>,
        parts: Box<[MeshRange]>,
        bounding_box: Option<BoundingBox>,
    ) {
        assert_ne!(vertex_count, 0);
        let mesh = Mesh {
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
        self.add_asset(path, Asset::Mesh(mesh), AssetLoadPriority::Normal);
    }

    pub fn add_material(self: &Arc<Self>, path: &str, albedo: &str, roughness: f32, metalness: f32) {
        let material = Material::new_pbr(albedo, roughness, metalness);
        self.add_asset(path, Asset::Material(material), AssetLoadPriority::Normal);
    }

    pub fn add_material_color(self: &Arc<Self>, path: &str, albedo: Vec4, roughness: f32, metalness: f32) {
        let material = Material::new_pbr_color(albedo, roughness, metalness);
        self.add_asset(path, Asset::Material(material), AssetLoadPriority::Normal);
    }

    pub fn add_model(self: &Arc<Self>, path: &str, mesh_path: &str, material_paths: &[&str]) {
        let model = Model {
            mesh_path: mesh_path.to_string(),
            material_paths: material_paths.iter().map(|mat| (*mat).to_owned()).collect(),
        };
        self.add_asset(path, Asset::Model(model), AssetLoadPriority::Normal);
    }

    pub fn add_texture(self: &Arc<Self>, path: &str, info: &TextureInfo, texture_data: Box<[u8]>) {
        self.add_asset(
            path,
            Asset::Texture(Texture {
                info: info.clone(),
                data: Box::new([texture_data.to_vec().into_boxed_slice()]),
            }),
            AssetLoadPriority::Normal,
        );
    }

    pub fn add_container(self: &Arc<Self>, container: impl AssetContainerAsync) {
        self.add_container_with_progress(container, None)
    }

    #[cfg(feature = "threading")]
    pub fn add_container_sync(self: &Arc<Self>, container: impl AssetContainer) {
        self.add_container_with_progress(SyncAssetContainerWrapper(Arc::new(container)), None)
    }

    #[cfg(feature = "threading")]
    pub fn add_container_with_progress_sync(
        self: &Arc<Self>,
        container: impl AssetContainer,
        progress: Option<&Arc<AssetLoaderProgress>>,
    ) {
        self.add_container_with_progress(SyncAssetContainerWrapper(Arc::new(container)), progress)
    }

    pub fn add_container_with_progress(
        self: &Arc<Self>,
        container: impl AssetContainerAsync,
        progress: Option<&Arc<AssetLoaderProgress>>,
    ) {
        let mut containers = self.containers.write().unwrap();
        containers.push(Box::new(container));
        if let Some(progress) = progress {
            progress.finished.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn add_loader(self: &Arc<Self>, loader: impl AssetLoaderAsync<P>) {
        let mut loaders = self.loaders.write().unwrap();
        loaders.push(Box::new(loader));
    }

    #[cfg(feature = "threading")]
    pub fn add_sync_loader(self: &Arc<Self>, loader: impl AssetLoader<P>) {
        let mut loaders = self.loaders.write().unwrap();
        loaders.push(Box::new(SyncAssetLoaderWrapper(Arc::new(loader), PhantomData)));
    }

    pub fn add_asset(self: &Arc<Self>, path: &str, asset: Asset, priority: AssetLoadPriority) {
        self.add_asset_with_progress(path, asset, None, priority)
    }

    pub fn add_asset_with_progress(
        self: &Arc<Self>,
        path: &str,
        asset: Asset,
        progress: Option<&Arc<AssetLoaderProgress>>,
        priority: AssetLoadPriority,
    ) {
        let asset_type = match &asset {
            Asset::Texture(_) => AssetType::Texture,
            Asset::Material(_) => AssetType::Material,
            Asset::Mesh(_) => AssetType::Mesh,
            Asset::Model(_) => AssetType::Model,
            Asset::Sound => AssetType::Sound,
            Asset::Shader(_) => AssetType::Shader,
        };

        {
            let mut assets = self.assets.lock().unwrap();
            assets.loaded_assets.insert(path.to_string(), asset_type);
            assets.requested_assets.remove(path);
        }

        if let Some(progress) = progress {
            progress.finished.fetch_add(1, Ordering::SeqCst);
        }
        match asset {
            Asset::Material(material) => {
                self.renderer_sender
                    .send(LoadedAsset {
                        asset: Asset::Material(material),
                        path: path.to_owned(),
                        priority,
                    })
                    .unwrap();
            }
            Asset::Mesh(mesh) => {
                assert_ne!(mesh.vertex_count, 0);
                self.renderer_sender
                    .send(LoadedAsset {
                        asset: Asset::Mesh(mesh),
                        path: path.to_owned(),
                        priority,
                    })
                    .unwrap();
            }
            Asset::Texture(texture) => {
                self.renderer_sender
                    .send(LoadedAsset {
                        asset: Asset::Texture(texture),
                        path: path.to_owned(),
                        priority,
                    })
                    .unwrap();
            }
            Asset::Model(model) => {
                self.renderer_sender
                    .send(LoadedAsset {
                        asset: Asset::Model(model),
                        path: path.to_owned(),
                        priority,
                    })
                    .unwrap();
            }
            Asset::Shader(shader) => {
                self.renderer_sender
                    .send(LoadedAsset {
                        asset: Asset::Shader(shader),
                        path: path.to_owned(),
                        priority,
                    })
                    .unwrap();
            }
            _ => unimplemented!(),
        }
    }

    pub fn request_asset_update(self: &Arc<Self>, path: &str) {
        log::info!("Reloading: {}", path);
        let asset_type = {
            let assets = self.assets.lock().unwrap();
            assets.loaded_assets.get(path).copied()
        };
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

        {
            let mut assets = self.assets.lock().unwrap();
            if (assets.loaded_assets.contains_key(path) && !refresh)
                || assets.requested_assets.contains_key(path)
            {
                progress.finished.fetch_add(1, Ordering::SeqCst);
                return progress;
            }
            assets.requested_assets.insert(path.to_owned(), asset_type);
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
                asset_mgr.load_asset(file, priority, &load_request.progress);
            });
        });
        progress
    }

    pub fn load_level(self: &Arc<Self>, path: &str) -> Option<LoadedLevel> {
        None

        /*
        let containers = self.containers.read().unwrap();
        let file_opt = AssetManager::<P>::load_file(&containers, path);
        if file_opt.is_none() {
            error!("Could not load file: {:?}", path);
            return None;
        }
        let mut file = file_opt.unwrap();

        let loaders = self.loaders.read().unwrap();
        let loader_opt = AssetManager::find_loader(&mut file, &loaders);
        if loader_opt.is_none() {
            error!("Could not find loader for file: {:?}", path);
            return None;
        }

        let progress = Arc::new(AssetLoaderProgress {
            expected: AtomicU32::new(1),
            finished: AtomicU32::new(0),
        });
        let loader = loader_opt.unwrap();
        let result = loader.load(file, AssetLoadPriority::Normal, &progress);
        if result.is_err() {
            error!("Could not load file: {:?}", path);
            return None;
        }
        let result = result.unwrap();
        let level = match result {
            AssetLoaderAsyncResult::Level(level) => Some(level),
            _ => None,
        };
        progress.finished.fetch_add(1, Ordering::SeqCst);
        level
        */
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
        if file_opt.is_none() {
            error!("Could not find file: {:?}, working dir: {:?}", path, std::env::current_dir());
            {
                let mut assets = self.assets.lock().unwrap();
                assets.requested_assets.remove(path);
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
        loaders: &'a [Box<dyn ErasedAssetLoaderAsync<P>>],
    ) -> Option<&'a dyn ErasedAssetLoaderAsync<P>> {
        let start = AsyncSeekExt::seek(file, SeekFrom::Current(0)).await
            .unwrap_or_else(|_| panic!("Failed to read file: {:?}", file.path));

        let mut loader_opt = Option::<&Box<dyn ErasedAssetLoaderAsync<P>>>::None;
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
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) {
        let path = file.path.clone();

        let loaders = self.loaders.read().unwrap();
        let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref()).await;
        if loader_opt.is_none() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            {
                let mut assets = self.assets.lock().unwrap();
                assets.requested_assets.remove(&path);
            }
            error!("Could not find loader for file: {:?}", path.as_str());
            return;
        }
        let loader = loader_opt.unwrap();

        let assets_opt = loader.load(file, self, priority, progress).await;
        if assets_opt.is_err() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            {
                let mut assets = self.assets.lock().unwrap();
                assets.requested_assets.remove(&path);
            }
            error!("Could not load file: {:?}", path.as_str());
        }
    }

    pub fn has_open_renderer_assets(&self) -> bool {
        !self.renderer_receiver.is_empty()
    }

    pub fn receive_render_asset(&self) -> Option<LoadedAsset> {
        self.renderer_receiver.try_recv().ok()
    }

    pub fn notify_loaded(self: &Arc<Self>, path: &str) {
        let mut assets = self.assets.lock().unwrap();
        if let Some(asset_type) = assets.requested_assets.remove(path) {
            assets.loaded_assets.insert(path.to_string(), asset_type);
        }
    }

    pub fn notify_unloaded(self: &Arc<Self>, path: &str) {
        let mut assets = self.assets.lock().unwrap();
        assets.loaded_assets.remove(path);
    }

    pub fn stop(&self) {
        trace!("Stopping asset manager");
    }
}
