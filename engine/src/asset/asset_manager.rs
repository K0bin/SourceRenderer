use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::hash::Hash;
use std::io::{
    Result as IOResult, SeekFrom
};
use std::pin::{pin, Pin};
use std::sync::atomic::{
    AtomicU32, AtomicU64, Ordering
};
use std::sync::Arc;
use crate::{AsyncCounter, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};
use futures_io::{AsyncRead, AsyncSeek};
use futures_lite::{io::Cursor, io::AsyncSeekExt};
use io_util::ReadEntireSeekableFileAsync as _;
use sourcerenderer_core::Vec4;
use strum::VariantArray as _;

use sourcerenderer_core::platform::{IOFutureMaybeSend, PlatformFile};

use crate::math::BoundingBox;
use crate::graphics::TextureInfo;

use super::{AssetData, AssetHandle, AssetType, AssetTypeGroup, MaterialData, MeshData, MeshRange, ModelData, TextureData};

pub struct AssetLoadRequest {
    pub path: String,
    pub progress: Arc<AssetLoaderProgress>,
    pub _priority: AssetLoadPriority,
}

enum AssetFileContents {
    File(Box<dyn PlatformFile>),
    Memory(Cursor<Box<[u8]>>),
}

pub struct AssetFile {
    path: String,
    contents: AssetFileContents,
}

impl AssetFile {
    pub fn new_file<F: PlatformFile + 'static>(path: &str, file: F) -> Self {
        AssetFile {
            path: path.to_string(),
            contents: AssetFileContents::File(Box::new(file)),
        }
    }

    pub fn new_boxed_file(path: &str, file: Box<dyn PlatformFile>) -> Self {
        AssetFile {
            path: path.to_string(),
            contents: AssetFileContents::File(file),
        }
    }

    pub fn new_memory(path: &str, memory: Box<[u8]>) -> Self {
        AssetFile {
            path: path.to_string(),
            contents: AssetFileContents::Memory(Cursor::new(memory)),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub async fn into_async_memory_cursor(self) -> IOResult<Cursor<Box<[u8]>>> {
        match self.contents {
            AssetFileContents::File(mut file) => {
                let data = file.read_seekable_to_end().await?;
                Ok(Cursor::new(data))
            },
            AssetFileContents::Memory(cursor) => Ok(cursor),
        }
    }

    pub async fn into_memory_cursor(self) -> IOResult<std::io::Cursor<Box<[u8]>>> {
        let async_cursor = self.into_async_memory_cursor().await?;
        Ok(std::io::Cursor::new(async_cursor.into_inner()))
    }

    pub async fn data(self) -> IOResult<Box<[u8]>> {
        match self.contents {
            AssetFileContents::File(mut file) => {
                let data = file.read_seekable_to_end().await?;
                Ok(data)
            },
            AssetFileContents::Memory(cursor) => Ok(cursor.into_inner()),
        }
    }
}

impl AsyncRead for AssetFile {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<IOResult<usize>> {
        match &mut self.contents {
            AssetFileContents::File(file) => pin!(file).poll_read(cx, buf),
            AssetFileContents::Memory(memory) => pin!(memory).poll_read(cx, buf),
        }
    }
}

impl AsyncSeek for AssetFile {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: SeekFrom,
    ) -> std::task::Poll<IOResult<u64>> {
        match &mut self.contents {
            AssetFileContents::File(file) => pin!(file).poll_seek(cx, pos),
            AssetFileContents::Memory(memory) => pin!(memory).poll_seek(cx, pos),
        }
    }
}

pub trait ContainerContainsFuture : Future<Output = bool> + IOFutureMaybeSend {}
impl<T: Future<Output = bool> + IOFutureMaybeSend> ContainerContainsFuture for T {}
pub trait ContainerFileOptionFuture : Future<Output = Option<AssetFile>> + IOFutureMaybeSend {}
impl<T: Future<Output = Option<AssetFile>> + IOFutureMaybeSend> ContainerFileOptionFuture for T {}
pub trait AssetContainer: Send + Sync + 'static {
    fn contains(&self, path: &str) -> impl ContainerContainsFuture;
    fn load(&self, path: &str) -> impl ContainerFileOptionFuture;
}

pub trait ErasedAssetContainer: Send + Sync {
    fn contains<'a>(&'a self, path: &'a str) -> Pin<Box<dyn ContainerContainsFuture + 'a>>;
    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn ContainerFileOptionFuture + 'a>>;
}

impl<T> ErasedAssetContainer for T
    where T: AssetContainer {
    fn contains<'a> (&'a self, path: &'a str) -> Pin<Box<dyn ContainerContainsFuture + 'a>> {
        Box::pin(AssetContainer::contains(self, path))
    }

    fn load<'a>(&'a self, path: &'a str) -> Pin<Box<dyn ContainerFileOptionFuture + 'a>> {
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

pub trait LoaderFuture : Future<Output = Result<(), ()>> + IOFutureMaybeSend {}
impl<T: Future<Output = Result<(), ()>> + IOFutureMaybeSend> LoaderFuture for T {}
pub trait AssetLoader: Send + Sync + 'static {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> impl LoaderFuture;
}

pub trait ErasedAssetLoader: Send + Sync {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load<'a>(
        &'a self,
        file: AssetFile,
        manager: &'a Arc<AssetManager>,
        priority: AssetLoadPriority,
        progress: &'a Arc<AssetLoaderProgress>,
    ) -> Pin<Box<dyn LoaderFuture + 'a>>;
}

impl<T> ErasedAssetLoader for T
    where T: AssetLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        AssetLoader::matches(self, file)
    }

    fn load<'a>(
        &'a self,
        file: AssetFile,
        manager: &'a Arc<AssetManager>,
        priority: AssetLoadPriority,
        progress: &'a Arc<AssetLoaderProgress>,
    ) -> Pin<Box<dyn LoaderFuture + 'a>> {
        Box::pin(AssetLoader::load(self, file, manager, priority, progress))
    }
}

const LOAD_PRIORITY_THRESHOLD: u32 = 4;

pub struct LoadedAssetData {
    pub handle: AssetHandle,
    pub data: AssetData,
    pub priority: AssetLoadPriority,
}

#[derive(Default)]
struct AssetSets {
    requested: HashSet<AssetHandle>,
    loaded: HashSet<AssetHandle>,
    ready: HashSet<AssetHandle>,
}

pub struct AssetManager {
    containers: async_rwlock::RwLock<Vec<Box<dyn ErasedAssetContainer>>>,
    pending_containers: AsyncCounter,
    pending_loaders: AsyncCounter,
    pending_high_priority_loads: AsyncCounter,
    pending_normal_priority_loads: AsyncCounter,
    loaders: async_rwlock::RwLock<Vec<Box<dyn ErasedAssetLoader>>>,
    path_map: Mutex<HashMap<String, AssetHandle>>,
    next_asset_handle: AtomicU64,
    asset_sets: Mutex<AssetSets>,
    channels: HashMap<AssetTypeGroup, (Sender<LoadedAssetData>, Receiver<LoadedAssetData>)>,
}

impl AssetManager {
    pub fn new() -> Arc<Self> {
        let mut channels = HashMap::<AssetTypeGroup, (Sender<LoadedAssetData>, Receiver<LoadedAssetData>)>::new();
        for group in AssetTypeGroup::VARIANTS {
            let channel = unbounded();
            channels.insert(*group, channel);
        }

        let manager = Arc::new(Self {
            loaders: async_rwlock::RwLock::new(Vec::new()),
            containers: async_rwlock::RwLock::new(Vec::new()),
            path_map: Mutex::new(HashMap::new()),
            next_asset_handle: AtomicU64::new(1),
            asset_sets: Mutex::new(Default::default()),
            pending_containers: AsyncCounter::new(0),
            pending_loaders: AsyncCounter::new(0),
            pending_high_priority_loads: AsyncCounter::new(LOAD_PRIORITY_THRESHOLD),
            pending_normal_priority_loads: AsyncCounter::new(LOAD_PRIORITY_THRESHOLD),
            channels,
        });

        manager
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
        future: impl Future<Output = impl AssetContainer> + IOFutureMaybeSend + 'static,
        progress: Option<&Arc<AssetLoaderProgress>>
    ) {
        self.pending_containers.increment();

        let c_progress = progress.cloned();
        let c_self = self.clone();
        crate::tasks::spawn_io(async move {
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

    pub fn add_loader(self: &Arc<Self>, loader: impl AssetLoader) {
        self.pending_loaders.increment();

        let c_self = self.clone();
        crate::tasks::spawn_io(async move {
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

        let asset_type = asset_data.asset_type();
        let group = asset_type.group();
        let (sender, _) = self.channels.get(&group).unwrap();

        sender.send(LoadedAssetData {
            handle,
            data: asset_data,
            priority,
        }).unwrap();

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

    pub fn request_asset_update(self: &Arc<Self>, path: &str) {
        let handle = {
            let path_map = self.path_map.lock().unwrap();
            let handle_opt = path_map.get(path).copied();
            if handle_opt.is_none() {
                return;
            }
            handle_opt.unwrap()
        };

        log::info!("Reloading: {}", path);
        self.request_asset_internal(path, handle.asset_type(), AssetLoadPriority::Low, None, true);
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

        let handle = self.get_or_reserve_handle(path, asset_type);

        {
            // Already requested?
            let mut asset_set = self.asset_sets.lock().unwrap();
            let mut already_loaded = false;
            if let Some(existing_handle) = asset_set.requested.get(&handle) {
                if existing_handle.asset_type() != asset_type {
                    log::error!("Requested an asset with the same path as a previously requested asset but with a different asset type. Path: {}, requested asset type: {:?}, previously requested asset type: {:?}.", path, asset_type, handle.asset_type());
                }
                already_loaded = true;
            }
            if let Some(existing_handle) = asset_set.loaded.get(&handle) {
                if existing_handle.asset_type() != asset_type {
                    log::error!("Requested an asset with the same path as a previously loaded asset but with a different asset type. Path: {}, requested asset type: {:?}, previously requested asset type: {:?}.", path, asset_type, handle.asset_type());
                }
                already_loaded = true;
            }
            if let Some(existing_handle) = asset_set.ready.get(&handle) {
                if existing_handle.asset_type() != asset_type {
                    log::error!("Requested an asset with the same path as ready asset but with a different asset type. Path: {}, requested asset type: {:?}, previously requested asset type: {:?}.", path, asset_type, handle.asset_type());
                }
                already_loaded = true;
            }
            if already_loaded && !refresh {
                progress.finished.fetch_add(1, Ordering::SeqCst);
                return (handle, progress);
            }
            // Already add it before it's really loaded so it's not racy with the loading process.
            // TODO: Add a way for the renderer to notify the AssetManager about unloaded assets or failed loading.
            asset_set.requested.insert(handle);
        };

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
        let io_task = crate::tasks::spawn_io(async move {
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
            let load_task = crate::tasks::spawn_async_compute(async move {
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

    pub fn receive_asset_data_blocking(&self, asset_group: AssetTypeGroup) -> LoadedAssetData {
        let (_, receiver) = self.channels.get(&asset_group).unwrap();
        receiver.recv().unwrap()
    }

    pub fn receive_asset_data(&self, asset_group: AssetTypeGroup) -> Option<LoadedAssetData> {
        let (_, receiver) = self.channels.get(&asset_group).unwrap();
        receiver.try_recv().ok()
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
        loaders: &'a [Box<dyn ErasedAssetLoader>],
        pending_loaders: &'a AsyncCounter
    ) -> Option<&'a dyn ErasedAssetLoader> {
        let start = AsyncSeekExt::seek(file, SeekFrom::Current(0)).await
            .unwrap_or_else(|_| panic!("Failed to read file: {:?}", file.path));

        // Make sure there is no add_loader task queued that hasn't been finished yet.
        pending_loaders.wait_for_zero().await;

        let mut loader_opt = Option::<&Box<dyn ErasedAssetLoader>>::None;
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
        _handle: T,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        let loaders = self.loaders.read().await;
        let loader_opt: Option<&dyn ErasedAssetLoader> = AssetManager::find_loader(
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
        Ok(())
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

    pub fn asset_requested(&self, path: &str) -> bool {
        let handle = {
            let path_map = self.path_map.lock().unwrap();
            if let Some(handle) = path_map.get(path) {
                *handle
            } else {
                return false;
            }
        };
        let asset_set = self.asset_sets.lock().unwrap();
        if asset_set.ready.contains(&handle) {
            return true;
        }
        if asset_set.loaded.contains(&handle) {
            return true;
        }
        if asset_set.requested.contains(&handle) {
            return true;
        }
        false
    }

    pub fn asset_loaded(&self, path: &str) -> bool {
        let handle = {
            let path_map = self.path_map.lock().unwrap();
            if let Some(handle) = path_map.get(path) {
                *handle
            } else {
                return false;
            }
        };
        let asset_set = self.asset_sets.lock().unwrap();
        if asset_set.ready.contains(&handle) {
            return true;
        }
        if asset_set.loaded.contains(&handle) {
            return true;
        }
        false
    }
}
