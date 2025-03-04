use std::collections::{HashMap, HashSet};
use std::future::{poll_fn, Future};
use std::hash::Hash;
use std::io::{
    Read,
    Result as IOResult,
    Seek,
    SeekFrom,
};
use std::pin::Pin;
use std::sync::atomic::{
    AtomicU32, AtomicU64, Ordering
};
use std::sync::Arc;
use crate::Mutex;
use std::task::{Poll, Waker};

use bevy_tasks::futures_lite::io::{Cursor, AsyncAsSync};
use bevy_tasks::futures_lite::AsyncSeekExt;
use bevy_tasks::{AsyncComputeTaskPool, IoTaskPool};
use futures_io::{AsyncRead, AsyncSeek};
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::Vec4;

use crate::math::BoundingBox;
use crate::graphics::{GraphicsContext, TextureInfo};
use crate::renderer::asset::{ComputePipelineHandle, GraphicsPipelineHandle, GraphicsPipelineInfo, RayTracingPipelineHandle, RayTracingPipelineInfo, RendererAssets, RendererAssetsReadOnly};

use super::{Asset, AssetData, AssetHandle, AssetType, AssetWithHandle, MaterialData, MeshData, MeshRange, ModelData, TextureData};

pub struct AssetLoadRequest {
    pub path: String,
    pub progress: Arc<AssetLoaderProgress>,
    pub _priority: AssetLoadPriority,
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
    fn contains(&self, path: &str) -> impl Future<Output = bool> + Send;
    fn load(&self, path: &str) -> impl Future<Output = Option<AssetFile>> + Send;
}

pub trait ErasedAssetContainer: Send + Sync {
    fn contains<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<AssetFile>> + Send + 'a>>;
}

impl<T> ErasedAssetContainer for T
    where T: AssetContainer {
    fn contains<'a> (&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(AssetContainer::contains(self, path))
    }

    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<AssetFile>> + Send + 'a>> {
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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AssetLoadPriority {
    High,
    Normal,
    Low,
}

pub trait AssetLoader<P: Platform>: Send + Sync + 'static {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> impl Future<Output = Result<(), ()>> + Send;
}

pub trait ErasedAssetLoader<P: Platform>: Send + Sync {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load<'a>(
        &'a self,
        file: AssetFile,
        manager: &'a Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &'a Arc<AssetLoaderProgress>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>>;
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
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        Box::pin(AssetLoader::<P>::load(self, file, manager, priority, progress))
    }
}

struct AsyncCounter {
    counter: AtomicU32,
    wakers: Mutex<Vec<Waker>>,
    min_value_for_waking: u32
}
impl AsyncCounter {
    fn new(min_value_for_waking: u32) -> Self {
        Self {
            counter: AtomicU32::new(0u32),
            wakers: Mutex::new(Vec::new()),
            min_value_for_waking
        }
    }

    fn increment(&self) -> u32 {
        self.counter.fetch_add(1, Ordering::Acquire) + 1
    }

    fn decrement(&self) -> u32 {
        let mut count = self.counter.fetch_sub(1, Ordering::Release) - 1;
        while count <= self.min_value_for_waking {
            let waker = {
                let mut guard = self.wakers.lock().unwrap();
                guard.pop()
            };
            if let Some(waker) = waker {
                waker.wake();
            } else {
                break;
            }
            count = self.counter.load(Ordering::Relaxed);
        }
        count
    }

    #[allow(unused)]
    fn load(&self) -> u32 {
        self.counter.load(Ordering::Relaxed)
    }

    fn wait_for_zero<'a>(&'a self) -> impl Future<Output = ()> + 'a {
        self.wait_for_value(0)
    }

    fn wait_for_value<'a>(&'a self, value: u32) -> impl Future<Output = ()> + 'a {
        assert!(value <= self.min_value_for_waking);
        poll_fn(move |ctx| {
            let mut pending_count = self.counter.load(Ordering::Acquire);
            if pending_count <= value {
                Poll::Ready(())
            } else {
                let mut guard = self.wakers.lock().unwrap();
                pending_count = self.counter.load(Ordering::Relaxed);
                if pending_count <= value {
                    return Poll::Ready(());
                }
                guard.push(ctx.waker().clone());

                Poll::Pending
            }
        })
    }
}

const LOAD_PRIORITY_THRESHOLD: u32 = 4;

pub struct AssetManager<P: Platform> {
    device: Arc<crate::graphics::Device<P::GPUBackend>>,
    containers: async_rwlock::RwLock<Vec<Box<dyn ErasedAssetContainer>>>,
    pending_containers: AsyncCounter,
    pending_loaders: AsyncCounter,
    pending_high_priority_loads: AsyncCounter,
    pending_normal_priority_loads: AsyncCounter,
    loaders: async_rwlock::RwLock<Vec<Box<dyn ErasedAssetLoader<P>>>>,
    path_map: Mutex<HashMap<String, AssetHandle>>,
    next_asset_handle: AtomicU64,
    requested_assets: Mutex<HashSet<AssetHandle>>,
    unintegrated_assets: Mutex<HashMap<AssetHandle, AssetData>>,
    renderer: RendererAssets<P>,
}

impl<P: Platform> AssetManager<P> {
    pub fn new(
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
    ) -> Arc<Self> {
        let manager = Arc::new(Self {
            device: device.clone(),
            loaders: async_rwlock::RwLock::new(Vec::new()),
            containers: async_rwlock::RwLock::new(Vec::new()),
            unintegrated_assets: Mutex::new(HashMap::new()),
            renderer: RendererAssets::<P>::new(device),
            path_map: Mutex::new(HashMap::new()),
            next_asset_handle: AtomicU64::new(1),
            requested_assets: Mutex::new(HashSet::new()),
            pending_containers: AsyncCounter::new(0),
            pending_loaders: AsyncCounter::new(0),
            pending_high_priority_loads: AsyncCounter::new(LOAD_PRIORITY_THRESHOLD),
            pending_normal_priority_loads: AsyncCounter::new(LOAD_PRIORITY_THRESHOLD),
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

    pub fn request_graphics_pipeline(self: &Arc<Self>, info: &GraphicsPipelineInfo) -> GraphicsPipelineHandle {
        self.renderer.request_graphics_pipeline(self, info)
    }

    pub fn request_compute_pipeline(self: &Arc<Self>, shader_path: &str) -> ComputePipelineHandle {
        self.renderer.request_compute_pipeline(self, shader_path)
    }

    pub fn request_ray_tracing_pipeline(self: &Arc<Self>, info: &RayTracingPipelineInfo) -> RayTracingPipelineHandle {
        self.renderer.request_ray_tracing_pipeline(self, info)
    }

    pub fn add_container_async(
        self: &Arc<Self>,
        future: impl Future<Output = impl AssetContainer> + Send + 'static
    ) {
        self.add_container_with_progress_async(future, None);
    }

    pub fn add_container(
        self: &Arc<Self>,
        container: impl AssetContainer
    ) {
        self.add_container_with_progress_async(async move {
            container
        }, None);
    }

    pub fn add_container_with_progress(
        self: &Arc<Self>,
        container: impl AssetContainer,
        progress: Option<&Arc<AssetLoaderProgress>>
    ) {
        self.add_container_with_progress_async(async move {
            container
        }, progress);
    }

    pub fn add_container_with_progress_async(
        self: &Arc<Self>,
        future: impl Future<Output = impl AssetContainer> + Send + 'static,
        progress: Option<&Arc<AssetLoaderProgress>>
    ) {
        self.pending_containers.increment();

        let c_progress = progress.cloned();
        let c_self = self.clone();
        IoTaskPool::get().spawn(async move {
            {
                let container_box = Box::new(future.await);
                let mut containers = c_self.containers.write().await;
                containers.push(container_box);
            }
            if let Some(progress) = c_progress {
                progress.finished.fetch_add(1, Ordering::SeqCst);
            }

            let _count = c_self.pending_containers.decrement();
        }).detach();
    }

    pub fn add_loader(self: &Arc<Self>, loader: impl AssetLoader<P>) {
        self.pending_loaders.increment();

        let c_self = self.clone();
        IoTaskPool::get().spawn(async move {
            let mut loaders = c_self.loaders.write().await;
            loaders.push(Box::new(loader));

            let _count = c_self.pending_loaders.decrement();
        }).detach();
    }

    pub fn add_asset_data(self: &Arc<Self>, path: &str, asset: AssetData, priority: AssetLoadPriority) {
        self.add_asset_data_with_progress(path, asset, None, priority)
    }

    pub fn add_asset_data_with_progress(
        self: &Arc<Self>,
        path: &str,
        asset_data: AssetData,
        progress: Option<&Arc<AssetLoaderProgress>>,
        priority: AssetLoadPriority,
    ) {
        let handle = self.get_or_reserve_handle(path, asset_data.asset_type());
        log::trace!("Adding asset data for path: {:?} {} to handle: {:?}", asset_data.asset_type(), path, handle);
        let integrated = if asset_data.is_renderer_asset() {
            self.renderer.integrate(self, handle, &asset_data, priority);
            true
        } else if let AssetData::Level(_level) = &asset_data {
            // Remove unintegrated level before loading a new one
            let _ = self.take_any_unintegrated_asset_data_of_type(AssetType::Level);
            false
        } else {
            unimplemented!()
        };

        if !integrated {
            let mut unintegrated_list = self.unintegrated_assets.lock().unwrap();
            unintegrated_list.insert(handle, asset_data);
        }
        if let Some(progress) = progress {
            progress.finished.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn reserve_handle(
        &self,
        path: &str,
        asset_type: AssetType
    ) -> AssetHandle {
        let handle = self.reserve_handle_without_path(asset_type);

        let mut path_map = self.path_map.lock().unwrap();
        let existing = path_map.insert(path.to_string(), handle);
        if let Some(existing) = existing {
            log::error!("Already had a handle for the given path: {:?}: {:?}", path, existing);
        }

        log::trace!("Reserving handle {:?} for path {}", handle, path);
        handle
    }

    #[inline(always)]
    pub fn reserve_handle_without_path(
        &self,
        asset_type: AssetType
    ) -> AssetHandle {
        AssetHandle::new(self.next_asset_handle.fetch_add(1, Ordering::AcqRel), asset_type)
    }

    pub fn add_asset(
        &self,
        path: &str,
        asset: Asset<P>
    ) {
        log::trace!("Adding asset of type {:?} with path {}", asset.asset_type(), path);
        let handle = self.get_or_reserve_handle(path, asset.asset_type());
        self.add_asset_with_handle(AssetWithHandle::combine(handle, asset));
    }

    pub fn add_asset_with_handle(
        &self,
        asset: AssetWithHandle<P>
    ) {
        log::trace!("Adding asset of type {:?} with handle {:?}", asset.asset_type(), asset.handle());
        if asset.is_renderer_asset() {
            self.renderer.add_asset(asset);
        } else {
            unimplemented!();
        }
    }

    pub fn request_asset_update(self: &Arc<Self>, path: &str) {
        let handle = {
            let path_map = self.path_map.lock().unwrap();
            let handle_opt = path_map.get(path).copied();
            if handle_opt.is_none() {
                return;
            }
            handle_opt.unwrap()
        };

        if self.is_loaded(handle) {
            log::info!("Reloading: {}", path);
            self.request_asset_internal(path, handle.asset_type(), AssetLoadPriority::Low, None, true);
        }
    }

    pub fn request_asset(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
    ) -> (AssetHandle, Arc<AssetLoaderProgress>) {
        self.request_asset_internal(path, asset_type, priority, None, false)
    }

    pub fn request_asset_refresh_by_handle<T: Into<AssetHandle>>(
        self: &Arc<Self>,
        handle: T,
        priority: AssetLoadPriority,
    ) -> (AssetHandle, Arc<AssetLoaderProgress>) {
        let handle: AssetHandle = handle.into();
        let path: String;
        {
            let path_map = self.path_map.lock().unwrap();
            let path_opt = path_map.iter().find_map(|(entry_path, entry_handle)|
                if handle == *entry_handle {
                    Some(entry_path.to_string())
                } else {
                    None
                }
            );
            if let Some(entry_path) = path_opt {
                path = entry_path;
            } else {
                log::error!("Requesting asset by handle: Could not find a path entry for the handle: {:?}", handle);
                return (handle, Arc::new(AssetLoaderProgress { expected: AtomicU32::new(1), finished: AtomicU32::new(1) }));
            }
        }
        self.request_asset_internal(&path, handle.asset_type(), priority, None, false)
    }

    pub fn request_asset_with_progress(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> (AssetHandle, Arc<AssetLoaderProgress>) {
        self.request_asset_internal(path, asset_type, priority, Some(progress), false)
    }

    fn request_asset_internal(
        self: &Arc<Self>,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
        progress: Option<&Arc<AssetLoaderProgress>>,
        refresh: bool,
    ) -> (AssetHandle, Arc<AssetLoaderProgress>) {
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

        if asset_type == AssetType::Level {
            // Remove unintegrated level before loading a new one
            let _ = self.take_any_unintegrated_asset_data_of_type(AssetType::Level);
        }

        let handle = self.get_or_reserve_handle(path, asset_type);

        {
            // Already requested?
            let requests = self.requested_assets.lock().unwrap();
            if let Some(requested_asset_handle) = requests.get(&handle) {
                if requested_asset_handle.asset_type() != asset_type {
                    log::error!("Requested an asset with the same path as a previously requested asset but with a different asset type. Path: {}, requested asset type: {:?}, previously requested asset type: {:?}.", path, asset_type, requested_asset_handle.asset_type());
                    progress.finished.fetch_add(1, Ordering::SeqCst);
                    return (*requested_asset_handle, progress);
                }
            }
        };

        let already_loaded = self.is_loaded(handle);
        if already_loaded {
            if handle.asset_type() != asset_type {
                log::error!("Requested an asset with the same path as a previously loaded asset but with a different asset type. Path: {}, requested asset type: {:?}, previously loaded asset type: {:?}.", path, asset_type, handle.asset_type());
                progress.finished.fetch_add(1, Ordering::SeqCst);
                return (handle, progress);
            }
            if !refresh {
                log::trace!("Skipping asset request because it is already loaded and request did not specify that it should be refreshed. Path: {}, asset type: {:?}.", path, asset_type);
                progress.finished.fetch_add(1, Ordering::SeqCst);
                return (handle, progress);
            }
        } else if refresh {
            log::trace!("Skipping asset request because it is a refresh request on an asset that isn't loaded. Path: {}, asset type: {:?}.", path, asset_type);
            progress.finished.fetch_add(1, Ordering::SeqCst);
            return (handle, progress);
        }

        let load_request = AssetLoadRequest {
            path: path.to_owned(),
            progress: progress.clone(),
            _priority: priority,
        };

        if priority == AssetLoadPriority::High {
            self.pending_high_priority_loads.increment();
        } else if priority == AssetLoadPriority::Normal {
            self.pending_normal_priority_loads.increment();
        }

        let asset_mgr = self.clone();
        let io_task = IoTaskPool::get().spawn(async move {
            // Avoid keeping the entire IoTaskPool busy with low priority loads while higher priority loads are waiting.
            if priority == AssetLoadPriority::Normal {
                asset_mgr.pending_high_priority_loads.wait_for_value(LOAD_PRIORITY_THRESHOLD).await;
            } else if priority == AssetLoadPriority::Low {
                asset_mgr.pending_high_priority_loads.wait_for_zero().await;
                asset_mgr.pending_normal_priority_loads.wait_for_value(LOAD_PRIORITY_THRESHOLD).await;
            }

            log::trace!("Loading file for {}", &load_request.path);
            let file_opt = asset_mgr.load_file(&load_request.path).await;
            if file_opt.is_none() {
                log::error!("Could not find file at path: {}", &load_request.path);
                load_request.progress.finished.fetch_add(1, Ordering::SeqCst);
                if priority == AssetLoadPriority::High {
                    asset_mgr.pending_high_priority_loads.decrement();
                } else if priority == AssetLoadPriority::Normal {
                    asset_mgr.pending_normal_priority_loads.decrement();
                }
                return;
            }
            let file = file_opt.unwrap();
            let load_task = AsyncComputeTaskPool::get().spawn(async move {
                log::trace!("Loading asset at path: {}", &file.path);
                let _ = asset_mgr.load_asset(file, handle, priority, &load_request.progress).await;
                if priority == AssetLoadPriority::High {
                    asset_mgr.pending_high_priority_loads.decrement();
                } else if priority == AssetLoadPriority::Normal {
                    asset_mgr.pending_normal_priority_loads.decrement();
                }
            });
            load_task.detach();
        });
        io_task.detach();
        (handle, progress)
    }

    pub(crate) fn take_any_unintegrated_asset_data_of_type(self: &Arc<Self>, asset_type: AssetType) -> Option<AssetData> {
        let mut unintegrated = self.unintegrated_assets.lock().unwrap();
        let path = unintegrated.iter().find_map(|(path, asset)| (asset.asset_type() == asset_type).then(|| path.clone()));
        path.and_then(|path| unintegrated.remove(&path))
    }

    pub async fn load_file(self: &Arc<Self>, path: &str) -> Option<AssetFile> {
        // Make sure there is no add_container task queued that hasn't been finished yet.
        self.pending_containers.wait_for_zero().await;

        let containers = self.containers.read().await;
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
        let containers = self.containers.read().await;
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
        pending_loaders: &'a AsyncCounter
    ) -> Option<&'a dyn ErasedAssetLoader<P>> {
        let start = AsyncSeekExt::seek(file, SeekFrom::Current(0)).await
            .unwrap_or_else(|_| panic!("Failed to read file: {:?}", file.path));

        // Make sure there is no add_loader task queued that hasn't been finished yet.
        pending_loaders.wait_for_zero().await;

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

    async fn load_asset<T : Into<AssetHandle>>(
        self: &Arc<Self>,
        mut file: AssetFile,
        handle: T,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        let handle: AssetHandle = handle.into();
        {
            let mut requests = self.requested_assets.lock().unwrap();
            requests.remove(&handle);
        }

        let loaders = self.loaders.read().await;
        let loader_opt: Option<&dyn ErasedAssetLoader<P>> = AssetManager::find_loader(
            &mut file,
            loaders.as_ref(),
            &self.pending_loaders
        ).await;
        if loader_opt.is_none() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            log::error!("Could not find loader for file: {:?}", &file.path);
            return Err(());
        }
        let loader = loader_opt.unwrap();

        let path = file.path.clone();
        let result = loader.load(file, self, priority, progress).await;
        if result.is_err() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            log::error!("Could not load file: {:?}", &path);
            return Err(());
        }
        if let Some(existing_asset_type) = self.contains(&path) {
            if existing_asset_type != handle.asset_type() {
                log::error!("Loader did load the wrong asset type from file: {:?}", &path);
                return Err(());
            }
        }
        Ok(())
    }

    pub fn contains(&self, path: &str) -> Option<AssetType> {
        let handle = {
            let path_map = self.path_map.lock().unwrap();
            path_map.get(path).copied()?
        };

        if handle.asset_type().is_renderer_asset() && self.renderer.contains(handle) {
            return Some(handle.asset_type());
        }

        {
            let unintegrated = self.unintegrated_assets.lock().unwrap();
            if let Some(asset) = unintegrated.get(&handle) {
                return Some(asset.asset_type());
            }
        }

        None
    }

    pub fn is_loaded<T: Into<AssetHandle>>(&self, handle: T) -> bool {
        let handle: AssetHandle = handle.into();
        if handle.asset_type().is_renderer_asset() {
            return self.renderer.contains(handle);
        }

        {
            let unintegrated = self.unintegrated_assets.lock().unwrap();
            if let Some(asset) = unintegrated.get(&handle) {
                assert_eq!(asset.asset_type(), handle.asset_type());
                return true;
            }
        }

        false
    }

    pub fn get_or_reserve_handle(&self, path: &str, asset_type: AssetType) -> AssetHandle {
        {
            let path_map = self.path_map.lock().unwrap();
            if let Some(handle) = path_map.get(path) {
                if handle.asset_type() != asset_type {
                    log::error!("An asset of a different type ({:?}) was previously loaded from the path \"{:?}\". Requested asset type now: {:?}", handle.asset_type(), path, asset_type);
                }
                return *handle;
            }
        }

        self.reserve_handle(path, asset_type)
    }

    pub(crate) fn read_renderer_assets(&self) -> RendererAssetsReadOnly<P> {
        self.renderer.read()
    }

    pub(crate) fn flush_renderer_assets(self: &Arc<Self>) {
        self.renderer.flush(self);
    }

    #[inline(always)]
    pub(crate) fn bump_frame(&self, context: &GraphicsContext<P::GPUBackend>) {
        self.renderer.bump_frame(context);
    }
}
