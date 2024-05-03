use std::collections::{
    HashMap,
    VecDeque,
};
use std::hash::Hash;
use std::io::{
    Cursor,
    Read,
    Result as IOResult,
    Seek,
    SeekFrom,
};
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

use crossbeam_channel::{
    unbounded,
    Receiver,
    Sender,
};
use legion::World;
use log::{
    error,
    trace,
    warn,
};
use sourcerenderer_core::gpu::PackedShader;
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::Vec4;

use crate::math::BoundingBox;
use crate::graphics::TextureInfo;

struct AssetLoadRequest {
    path: String,
    asset_type: AssetType,
    progress: Arc<AssetLoaderProgress>,
    priority: AssetLoadPriority,
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

impl Read for AssetFile {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
        self.data.read(buf)
    }
}

impl Seek for AssetFile {
    fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
        self.data.seek(pos)
    }
}

pub trait AssetContainer: Send + Sync {
    fn contains(&self, path: &str) -> bool {
        self.load(path).is_some()
    }
    fn load(&self, path: &str) -> Option<AssetFile>;
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

pub enum AssetLoaderResult {
    None,
    Level(World),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AssetLoadPriority {
    High,
    Normal,
    Low,
}

pub trait AssetLoader<P: Platform>: Send + Sync {
    fn matches(&self, file: &mut AssetFile) -> bool;
    fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<AssetLoaderResult, ()>;
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
    inner: Mutex<AssetManagerInner>,
    containers: RwLock<Vec<Box<dyn AssetContainer>>>,
    loaders: RwLock<Vec<Box<dyn AssetLoader<P>>>>,
    renderer_sender: Sender<LoadedAsset>,
    renderer_receiver: Receiver<LoadedAsset>,
    cond_var: Arc<Condvar>,
    is_running: AtomicBool,
}

struct AssetManagerInner {
    load_queue: VecDeque<AssetLoadRequest>,
    low_priority_load_queue: VecDeque<AssetLoadRequest>,
    high_priority_load_queue: VecDeque<AssetLoadRequest>,
    requested_assets: HashMap<String, AssetType>,
    loaded_assets: HashMap<String, AssetType>,
}

impl<P: Platform> AssetManager<P> {
    pub fn new(
        platform: &P,
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
    ) -> Arc<Self> {
        let (renderer_sender, renderer_receiver) = unbounded();

        let cond_var = Arc::new(Condvar::new());

        let manager = Arc::new(Self {
            device: device.clone(),
            inner: Mutex::new(AssetManagerInner {
                load_queue: VecDeque::new(),
                low_priority_load_queue: VecDeque::new(),
                high_priority_load_queue: VecDeque::new(),
                loaded_assets: HashMap::new(),
                requested_assets: HashMap::new(),
            }),
            loaders: RwLock::new(Vec::new()),
            containers: RwLock::new(Vec::new()),
            renderer_sender,
            renderer_receiver,
            cond_var,
            is_running: AtomicBool::new(true),
        });

        let thread_count = 4;
        for _ in 0..thread_count {
            let c_manager = Arc::downgrade(&manager);
            platform.start_thread("AssetManagerThread", move || {
                asset_manager_thread_fn(c_manager)
            });
        }

        manager
    }

    pub fn graphics_device(&self) -> &Arc<crate::graphics::Device<P::GPUBackend>> {
        &self.device
    }

    pub fn add_mesh(
        &self,
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

    pub fn add_material(&self, path: &str, albedo: &str, roughness: f32, metalness: f32) {
        let material = Material::new_pbr(albedo, roughness, metalness);
        self.add_asset(path, Asset::Material(material), AssetLoadPriority::Normal);
    }

    pub fn add_material_color(&self, path: &str, albedo: Vec4, roughness: f32, metalness: f32) {
        let material = Material::new_pbr_color(albedo, roughness, metalness);
        self.add_asset(path, Asset::Material(material), AssetLoadPriority::Normal);
    }

    pub fn add_model(&self, path: &str, mesh_path: &str, material_paths: &[&str]) {
        let model = Model {
            mesh_path: mesh_path.to_string(),
            material_paths: material_paths.iter().map(|mat| (*mat).to_owned()).collect(),
        };
        self.add_asset(path, Asset::Model(model), AssetLoadPriority::Normal);
    }

    pub fn add_texture(&self, path: &str, info: &TextureInfo, texture_data: Box<[u8]>) {
        self.add_asset(
            path,
            Asset::Texture(Texture {
                info: info.clone(),
                data: Box::new([texture_data.to_vec().into_boxed_slice()]),
            }),
            AssetLoadPriority::Normal,
        );
    }

    pub fn add_container(&self, container: Box<dyn AssetContainer>) {
        self.add_container_with_progress(container, None)
    }

    pub fn add_container_with_progress(
        &self,
        container: Box<dyn AssetContainer>,
        progress: Option<&Arc<AssetLoaderProgress>>,
    ) {
        let mut containers = self.containers.write().unwrap();
        containers.push(container);
        if let Some(progress) = progress {
            progress.finished.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn add_loader(&self, loader: Box<dyn AssetLoader<P>>) {
        let mut loaders = self.loaders.write().unwrap();
        loaders.push(loader);
    }

    pub fn add_asset(&self, path: &str, asset: Asset, priority: AssetLoadPriority) {
        self.add_asset_with_progress(path, asset, None, priority)
    }

    pub fn add_asset_with_progress(
        &self,
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
            let mut inner = self.inner.lock().unwrap();
            inner.loaded_assets.insert(path.to_string(), asset_type);
            inner.requested_assets.remove(path);
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

    pub fn request_asset_update(&self, path: &str) {
        log::info!("Reloading: {}", path);
        let asset_type = {
            let inner = self.inner.lock().unwrap();
            inner.loaded_assets.get(path).copied()
        };
        if let Some(asset_type) = asset_type {
            self.request_asset_internal(path, asset_type, AssetLoadPriority::Low, None, true);
        } else {
            warn!("Cannot reload unloaded asset {}", path);
        }
    }

    pub fn request_asset(
        &self,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
    ) -> Arc<AssetLoaderProgress> {
        self.request_asset_internal(path, asset_type, priority, None, false)
    }

    pub fn request_asset_with_progress(
        &self,
        path: &str,
        asset_type: AssetType,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Arc<AssetLoaderProgress> {
        self.request_asset_internal(path, asset_type, priority, Some(progress), false)
    }

    fn request_asset_internal(
        &self,
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
            let mut inner = self.inner.lock().unwrap();
            if (inner.loaded_assets.contains_key(path) && !refresh)
                || inner.requested_assets.contains_key(path)
            {
                progress.finished.fetch_add(1, Ordering::SeqCst);
                return progress;
            }
            inner.requested_assets.insert(path.to_owned(), asset_type);

            let queue = match priority {
                AssetLoadPriority::High => &mut inner.high_priority_load_queue,
                AssetLoadPriority::Normal => &mut inner.load_queue,
                AssetLoadPriority::Low => &mut inner.low_priority_load_queue,
            };

            queue.push_back(AssetLoadRequest {
                asset_type,
                path: path.to_owned(),
                progress: progress.clone(),
                priority,
            });
        }
        self.cond_var.notify_one();

        progress
    }

    pub fn load_level(self: &Arc<Self>, path: &str) -> Option<World> {
        let file_opt = self.load_file(path);
        if file_opt.is_none() {
            error!("Could not load file: {:?}", path);
            return None;
        }
        let mut file = file_opt.unwrap();

        let loaders = self.loaders.read().unwrap();
        let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref());
        if loader_opt.is_none() {
            error!("Could not find loader for file: {:?}", path);
            return None;
        }

        let progress = Arc::new(AssetLoaderProgress {
            expected: AtomicU32::new(1),
            finished: AtomicU32::new(0),
        });
        let loader = loader_opt.unwrap();
        let assets_opt = loader.load(file, self, AssetLoadPriority::Normal, &progress);
        if assets_opt.is_err() {
            error!("Could not load file: {:?}", path);
            return None;
        }
        let result = assets_opt.unwrap();
        let level = match result {
            AssetLoaderResult::Level(level) => Some(level),
            _ => None,
        };
        progress.finished.fetch_add(1, Ordering::SeqCst);
        level
    }

    pub fn load_file(&self, path: &str) -> Option<AssetFile> {
        let containers = self.containers.read().unwrap();
        let mut file_opt: Option<AssetFile> = None;
        for container in containers.iter().rev() {
            let container_file_opt = container.load(path);
            if container_file_opt.is_some() {
                file_opt = container_file_opt;
                break;
            }
        }
        if file_opt.is_none() {
            error!("Could not find file: {:?}", path);
            {
                let mut inner = self.inner.lock().unwrap();
                inner.requested_assets.remove(path);
            }
        }
        file_opt
    }

    pub fn file_exists(&self, path: &str) -> bool {
        let containers = self.containers.read().unwrap();
        for container in containers.iter() {
            if container.contains(path) {
                return true;
            }
        }
        false
    }

    fn find_loader<'a>(
        file: &mut AssetFile,
        loaders: &'a [Box<dyn AssetLoader<P>>],
    ) -> Option<&'a dyn AssetLoader<P>> {
        let start = file
            .seek(SeekFrom::Current(0))
            .unwrap_or_else(|_| panic!("Failed to read file: {:?}", file.path));
        let loader_opt = loaders.iter().find(|loader| {
            let loader_matches = loader.matches(file);
            file.seek(SeekFrom::Start(start)).unwrap();
            loader_matches
        });
        loader_opt.map(|b| b.as_ref())
    }

    fn load_asset(
        self: &Arc<Self>,
        mut file: AssetFile,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) {
        let path = file.path.clone();

        let loaders = self.loaders.read().unwrap();
        let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref());
        if loader_opt.is_none() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            {
                let mut inner = self.inner.lock().unwrap();
                inner.requested_assets.remove(&path);
            }
            error!("Could not find loader for file: {:?}", path.as_str());
            return;
        }
        let loader = loader_opt.unwrap();

        let assets_opt = loader.load(file, self, priority, progress);
        if assets_opt.is_err() {
            progress.finished.fetch_add(1, Ordering::SeqCst);
            {
                let mut inner = self.inner.lock().unwrap();
                inner.requested_assets.remove(&path);
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

    pub fn notify_loaded(&self, path: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(asset_type) = inner.requested_assets.remove(path) {
            inner.loaded_assets.insert(path.to_string(), asset_type);
        }
    }

    pub fn notify_unloaded(&self, path: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.loaded_assets.remove(path);
    }

    pub fn stop(&self) {
        trace!("Stopping asset manager");
        let was_running = self.is_running.swap(false, Ordering::SeqCst);
        if !was_running {
            return;
        }
        self.cond_var.notify_all();
    }
}

fn asset_manager_thread_fn<P: Platform>(asset_manager: Weak<AssetManager<P>>) {
    trace!("Started asset manager thread");
    let cond_var = {
        let mgr_opt = asset_manager.upgrade();
        if mgr_opt.is_none() {
            return;
        }
        let mgr = mgr_opt.unwrap();
        mgr.cond_var.clone()
    };

    'asset_loop: loop {
        let mut break_loop = false;
        P::thread_memory_management_pool(|| {
            let mgr_opt = asset_manager.upgrade();
            if mgr_opt.is_none() {
                break_loop = true;
            }
            let mgr = mgr_opt.unwrap();
            if !mgr.is_running.load(Ordering::SeqCst) {
                break_loop = true;
            }
            let request = {
                let mut inner = mgr.inner.lock().unwrap();
                let mut request_opt = inner.high_priority_load_queue.pop_front();
                request_opt = request_opt.or_else(|| inner.load_queue.pop_front());
                request_opt = request_opt.or_else(|| inner.low_priority_load_queue.pop_front());
                while request_opt.is_none() {
                    if !mgr.is_running.load(Ordering::SeqCst) {
                        break_loop = true;
                    }

                    inner = cond_var.wait(inner).unwrap();
                    request_opt = inner.load_queue.pop_front();
                    request_opt = request_opt.or_else(|| inner.load_queue.pop_front());
                    request_opt = request_opt.or_else(|| inner.low_priority_load_queue.pop_front());
                }
                match request_opt {
                    Some(request) => request,
                    None => return
                }
            };

            {
                let file_opt = mgr.load_file(&request.path);
                if file_opt.is_none() {
                    request.progress.finished.fetch_add(1, Ordering::SeqCst);
                    return;
                }
                let file = file_opt.unwrap();
                mgr.load_asset(file, request.priority, &request.progress);
            }
        });
        if break_loop {
            break;
        }
    }
    trace!("Stopped asset manager thread");
}
