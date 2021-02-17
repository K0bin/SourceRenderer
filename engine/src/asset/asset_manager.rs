use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock, Mutex};
use std::collections::{VecDeque, HashSet};
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::graphics::Backend as GraphicsBackend;
use sourcerenderer_core::graphics;
use sourcerenderer_core::graphics::{Device, MemoryUsage, BufferUsage, TextureInfo, TextureShaderResourceViewInfo, Filter, AddressMode};
use std::hash::Hash;

use std::sync::Weak;
use std::thread;
use std::time::Duration;
use legion::World;
use std::fs::File;
use std::io::{Cursor, Seek, SeekFrom};

use crossbeam_channel::{unbounded, Sender, Receiver};

pub type AssetKey = usize;

pub struct AssetLoadRequest {
  path: String,
  asset_type: AssetType,
  progress: Arc<AssetLoaderProgress>,
  priority: AssetLoadPriority
}

pub struct LoadedAsset<P: Platform> {
  pub path: String,
  pub asset: Asset<P>,
  pub fence: Option<Arc<<P::GraphicsBackend as GraphicsBackend>::Fence>>,
  pub priority: AssetLoadPriority
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
  Container
}

#[derive(Clone)]
pub struct MeshRange {
  pub start: u32,
  pub count: u32
}

pub struct Mesh<B: GraphicsBackend> {
  pub vertices: Arc<B::Buffer>,
  pub indices: Option<Arc<B::Buffer>>,
  pub parts: Vec<MeshRange>
}

impl<B: GraphicsBackend> Clone for Mesh<B> {
  fn clone(&self) -> Self {
    Self {
      vertices: self.vertices.clone(),
      indices: self.indices.clone(),
      parts: self.parts.clone()
    }
  }
}

#[derive(Clone)]
pub struct Model {
  pub mesh_path: String,
  pub material_paths: Vec<String>
}

#[derive(Clone)]
pub struct Material {
  pub albedo_texture_path: String
}

pub struct AssetFile {
  pub path: String,
  pub data: AssetFileData
}

pub enum AssetFileData {
  File(File),
  Memory(Cursor<Box<[u8]>>)
}

pub trait AssetContainer
  : Send + Sync  {
  fn load(&self, path: &str) -> Option<AssetFile>;
}

pub struct AssetLoaderProgress {
  expected: AtomicU32,
  finished: AtomicU32
}

impl AssetLoaderProgress {
  pub fn is_done(&self) -> bool {
    self.finished.load(Ordering::SeqCst) == self.expected.load(Ordering::SeqCst)
  }
}

pub struct AssetLoaderResult {
  pub level: Option<World>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AssetLoadPriority {
  Normal,
  Low
}

pub trait AssetLoader<P: Platform>
  : Send + Sync {
  fn matches(&self, file: &mut AssetFile) -> bool;
  fn load(&self, file: AssetFile, manager: &AssetManager<P>, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()>;
}

pub enum Asset<P: Platform> {
  Texture(Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView>),
  Mesh(Arc<Mesh<P::GraphicsBackend>>),
  Model(Arc<Model>),
  Sound,
  Material(Arc<Material>)
}

impl<P: Platform> Clone for Asset<P> {
  fn clone(&self) -> Self {
    match self {
      Asset::Texture(tex) => Asset::Texture(tex.clone()),
      Asset::Mesh(mesh) => Asset::Mesh(mesh.clone()),
      Asset::Model(model) => Asset::Model(model.clone()),
      Asset::Sound => Asset::Sound,
      Asset::Material(mat) => Asset::Material(mat.clone())
    }
  }
}

pub struct AssetManager<P: Platform> {
  device: Arc<<P::GraphicsBackend as graphics::Backend>::Device>,
  load_queue: Mutex<VecDeque<AssetLoadRequest>>,
  containers: RwLock<Vec<Box<dyn AssetContainer>>>,
  loaders: RwLock<Vec<Box<dyn AssetLoader<P>>>>,
  loaded_assets: Mutex<HashSet<String>>,
  renderer_sender: Sender<LoadedAsset<P>>,
  renderer_receiver: Receiver<LoadedAsset<P>>
}

impl<P: Platform> AssetManager<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as graphics::Backend>::Device>) -> Arc<Self> {
    let (renderer_sender, renderer_receiver) = unbounded();

    let manager = Arc::new(Self {
      device: device.clone(),
      load_queue: Mutex::new(VecDeque::new()),
      loaders: RwLock::new(Vec::new()),
      containers: RwLock::new(Vec::new()),
      loaded_assets: Mutex::new(HashSet::new()),
      renderer_sender,
      renderer_receiver
    });

    let thread_count = 1;
    for _ in 0..thread_count {
      let c_manager = Arc::downgrade(&manager);
      std::thread::Builder::new().name("AssetManagerThread".to_string()).spawn(move || asset_manager_thread_fn(c_manager)).unwrap();
    }

    manager
  }

  pub fn graphics_device(&self) -> &Arc<<P::GraphicsBackend as graphics::Backend>::Device> {
    &self.device
  }

  pub fn add_mesh(&self, path: &str, vertex_buffer_data: &[u8], index_buffer_data: &[u8]) {
    let vertex_buffer = self.device.upload_data_slice(vertex_buffer_data, MemoryUsage::CpuToGpu, BufferUsage::VERTEX | BufferUsage::COPY_SRC);
    let index_buffer = if index_buffer_data.len() != 0 {
      Some(self.device.upload_data_slice(index_buffer_data, MemoryUsage::CpuToGpu, BufferUsage::INDEX | BufferUsage::COPY_SRC))
    } else {
      None
    };
    let mesh = Arc::new(Mesh {
      vertices: vertex_buffer,
      indices: index_buffer,
      parts: vec![MeshRange {
        start: 0,
        count: if index_buffer_data.len() == 0 { vertex_buffer_data.len() } else { index_buffer_data.len() } as u32
      }]
    });
    self.add_asset(path, Asset::Mesh(mesh), AssetLoadPriority::Normal, None);
  }

  pub fn add_material(&self, path: &str, albedo: &str) {
    let material = Arc::new(Material {
      albedo_texture_path: albedo.to_string()
    });
    self.add_asset(path, Asset::Material(material), AssetLoadPriority::Normal, None);
  }

  pub fn add_model(&self, path: &str, mesh_path: &str, material_paths: &[&str]) {
    let model = Arc::new(Model {
      mesh_path: mesh_path.to_string(),
      material_paths: material_paths.iter().map(|mat| (*mat).to_owned()).collect()
    });
    self.add_asset(path, Asset::Model(model), AssetLoadPriority::Normal, None);
  }

  pub fn add_texture(&self, path: &str, info: &TextureInfo, texture_data: &[u8]) {
    let src_buffer = self.device.upload_data_raw(texture_data, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let texture = self.device.create_texture(info, Some(path));
    self.device.init_texture(&texture, &src_buffer, 0, 0);
    let srv = self.device.create_shader_resource_view(&texture, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
      mag_filter: Filter::Linear,
      min_filter: Filter::Linear,
      mip_filter: Filter::Linear,
      address_mode_u: AddressMode::Repeat,
      address_mode_v: AddressMode::Repeat,
      address_mode_w: AddressMode::Repeat,
      mip_bias: 0.0,
      max_anisotropy: 0.0,
      compare_op: None,
      min_lod: 0.0,
      max_lod: 0.0
    });
    self.add_asset(path, Asset::Texture(srv), AssetLoadPriority::Normal, None);
  }

  pub fn add_container(&self, container: Box<dyn AssetContainer>) {
    self.add_container_with_progress(container, None)
  }

  pub fn add_container_with_progress(&self, container: Box<dyn AssetContainer>, progress: Option<&Arc<AssetLoaderProgress>>) {
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

  pub fn add_asset(&self, path: &str, asset: Asset<P>, priority: AssetLoadPriority, fence: Option<Arc<<P::GraphicsBackend as GraphicsBackend>::Fence>>) {
    self.add_asset_with_progress(path, asset, None, priority, fence)
  }

  pub fn add_asset_with_progress(&self, path: &str, asset: Asset<P>, progress: Option<&Arc<AssetLoaderProgress>>, priority: AssetLoadPriority, fence: Option<Arc<<P::GraphicsBackend as GraphicsBackend>::Fence>>) {
    {
      let mut loaded = self.loaded_assets.lock().unwrap();
      loaded.insert(path.to_string());
    }

    if let Some(progress) = progress {
      progress.finished.fetch_add(1, Ordering::SeqCst);
    }
    match asset {
      Asset::Material(material) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Material(material),
          path: path.to_owned(),
          fence,
          priority
        }).unwrap();
      }
      Asset::Texture(texture) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Texture(texture),
          path: path.to_owned(),
          fence,
          priority
        }).unwrap();
      }
      Asset::Mesh(mesh) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Mesh(mesh),
          path: path.to_owned(),
          fence,
          priority
        }).unwrap();
      }
      Asset::Model(model) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Model(model),
          path: path.to_owned(),
          fence,
          priority
        }).unwrap();
      }
      _ => unimplemented!()
    }
  }

  pub fn request_asset(&self, path: &str, asset_type: AssetType, priority: AssetLoadPriority) -> Arc<AssetLoaderProgress> {
    self.request_asset_with_progress(path, asset_type, priority, None)
  }

  pub fn request_asset_with_progress(&self, path: &str, asset_type: AssetType, priority: AssetLoadPriority, progress: Option<&Arc<AssetLoaderProgress>>) -> Arc<AssetLoaderProgress> {
    let progress = progress.map_or_else(|| Arc::new(AssetLoaderProgress {
      expected: AtomicU32::new(0),
      finished: AtomicU32::new(0)
    }), |p| p.clone());
    progress.expected.fetch_add(1, Ordering::SeqCst);

    {
      let loaded_assets = self.loaded_assets.lock().unwrap();
      if loaded_assets.contains(path) {
        progress.finished.fetch_add(1, Ordering::SeqCst);
        return progress;
      }
    }

    let mut queue_guard = self.load_queue.lock().unwrap();
    queue_guard.push_back(AssetLoadRequest {
      asset_type,
      path: path.to_owned(),
      progress: progress.clone(),
      priority
    });

    progress
  }

  pub fn load_level(&self, path: &str) -> Option<World> {
    let file_opt = self.load_file(path);
    if file_opt.is_none() {
      println!("Could not load file: {:?}", path);
      return None;
    }
    let mut file = file_opt.unwrap();

    let loaders = self.loaders.read().unwrap();
    let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref());
    if loader_opt.is_none() {
      println!("Could not find loader for file: {:?}", path);
      return None;
    }

    let progress = Arc::new(AssetLoaderProgress {
      expected: AtomicU32::new(1),
      finished: AtomicU32::new(0)
    });
    let loader = loader_opt.unwrap();
    let assets_opt = loader.load(file, self, AssetLoadPriority::Normal, &progress);
    if assets_opt.is_err() {
      println!("Could not load file: {:?}", path);
      return None;
    }
    let assets = assets_opt.unwrap();
    let level = assets.level;
    progress.finished.fetch_add(1, Ordering::SeqCst);
    while level.is_some() && !progress.is_done() {}
    level
  }

  pub fn load_file(&self, path: &str) -> Option<AssetFile> {
    let containers = self.containers.read().unwrap();
    let mut file_opt: Option<AssetFile> = None;
    for container in containers.iter() {
      let container_file_opt = container.load(path);
      if container_file_opt.is_some() {
        file_opt = container_file_opt;
        break;
      }
    }
    file_opt
  }

  fn find_loader<'a>(file: &mut AssetFile, loaders: &'a [Box<dyn AssetLoader<P>>]) -> Option<&'a Box<dyn AssetLoader<P>>> {
    let start = match &mut file.data {
      AssetFileData::File(file) => { file.seek(SeekFrom::Current(0)) }
      AssetFileData::Memory(cursor) => { cursor.seek(SeekFrom::Current(0)) }
    }.expect(format!("Failed to read file: {:?}", file.path.as_str()).as_str());
    let loader_opt = loaders.iter().find(|loader| {
      let loader_matches = loader.matches(file);
      match &mut file.data {
        AssetFileData::File(file) => { file.seek(SeekFrom::Start(start)).unwrap(); }
        AssetFileData::Memory(cursor) => { cursor.seek(SeekFrom::Start(start)).unwrap(); }
      }
      loader_matches
    });
    loader_opt
  }

  fn load_asset(&self, mut file: AssetFile, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) {
    let path = file.path.clone();

    let loaders = self.loaders.read().unwrap();
    let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref());
    if loader_opt.is_none() {
      progress.finished.fetch_add(1, Ordering::SeqCst);
      println!("Could not find loader for file: {:?}", path.as_str());
      return;
    }
    let loader = loader_opt.unwrap();

    let assets_opt = loader.load(file, self, priority, progress);
    if assets_opt.is_err() {
      progress.finished.fetch_add(1, Ordering::SeqCst);
      println!("Could not load file: {:?}", path.as_str());
      return;
      // dunno, error i guess
    }
  }

  pub fn receive_render_asset(&self) -> Option<LoadedAsset<P>> {
    self.renderer_receiver.try_recv().ok()
  }

  pub fn notify_loaded(&self, path: &str) {
    let mut loaded_assets = self.loaded_assets.lock().unwrap();
    loaded_assets.insert(path.to_string());
  }

  pub fn notify_unloaded(&self, path: &str) {
    let mut loaded_assets = self.loaded_assets.lock().unwrap();
    loaded_assets.remove(path);
  }

  pub fn flush(&self) {
    self.device.flush_transfers();
  }
}

fn asset_manager_thread_fn<P: Platform>(asset_manager: Weak<AssetManager<P>>) {
  loop {
    {
      let mgr_opt = asset_manager.upgrade();
      if mgr_opt.is_none() {
        break;
      }
      let mgr = mgr_opt.unwrap();
      let request_opt = {
        let mut queue = mgr.load_queue.lock().unwrap();
        queue.pop_front()
      };

      if request_opt.is_none() {
        thread::sleep(Duration::new(3, 0));
        continue;
      }

      let request = request_opt.unwrap();
      {
        let file_opt = mgr.load_file(&request.path);
        if file_opt.is_none() {
          request.progress.finished.fetch_add(1, Ordering::SeqCst);
          continue;
        }
        let file = file_opt.unwrap();
        mgr.load_asset(file, request.priority, &request.progress);
      }
    }

    thread::sleep(Duration::new(0, 10_000_000)); // 10ms
  }
}
