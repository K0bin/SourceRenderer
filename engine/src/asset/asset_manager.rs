use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock, Mutex, Condvar};
use std::collections::{VecDeque, HashSet};
use sourcerenderer_core::platform::{Platform, io::IO};
use sourcerenderer_core::graphics;
use sourcerenderer_core::graphics::TextureInfo;
use std::hash::Hash;

use std::sync::Weak;
use std::time::Duration;
use legion::World;
use std::io::{Cursor, Seek, SeekFrom, Read, Result as IOResult};

use crossbeam_channel::{unbounded, Sender, Receiver};

use crate::math::BoundingBox;

pub type AssetKey = usize;

pub struct AssetLoadRequest {
  path: String,
  asset_type: AssetType,
  progress: Arc<AssetLoaderProgress>,
  priority: AssetLoadPriority
}

pub struct LoadedAsset {
  pub path: String,
  pub asset: Asset,
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

pub struct Texture {
  pub info: TextureInfo,
  pub data: Box<[Box<[u8]>]>
}

pub struct Mesh {
  pub indices: Option<Box<[u8]>>,
  pub vertices: Box<[u8]>,
  pub parts: Box<[MeshRange]>,
  pub bounding_box: Option<BoundingBox>
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

pub struct AssetFile<P: Platform> {
  pub path: String,
  pub data: AssetFileData<P>
}

impl<P: Platform> Read for AssetFile<P> {
  fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
    self.data.read(buf)
  }
}

impl<P: Platform> Seek for AssetFile<P> {
  fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
    self.data.seek(pos)
  }
}

pub enum AssetFileData<P: Platform> {
  File(<P::IO as IO>::File),
  Memory(Cursor<Box<[u8]>>)
}

impl<P: Platform> Read for AssetFileData<P> {
  fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
    match self {
      AssetFileData::File(file) => {
        file.read(buf)
      }
      AssetFileData::Memory(cursor) => {
        cursor.read(buf)
      }
    }
  }
}

impl<P: Platform> Seek for AssetFileData<P> {
  fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
    match self {
      AssetFileData::File(file) => {
        file.seek(pos)
      }
      AssetFileData::Memory(cursor) => {
        cursor.seek(pos)
      }
    }
  }
}

pub trait AssetContainer<P: Platform>
  : Send + Sync {
  fn contains(&self, path: &str) -> bool {
    self.load(path).is_some()
  }
  fn load(&self, path: &str) -> Option<AssetFile<P>>;
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
  fn matches(&self, file: &mut AssetFile<P>) -> bool;
  fn load(&self, file: AssetFile<P>, manager: &Arc<AssetManager<P>>, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()>;
}

pub enum Asset {
  Texture(Texture),
  Mesh(Mesh),
  Model(Model),
  Sound,
  Material(Material)
}

pub struct AssetManager<P: Platform> {
  device: Arc<<P::GraphicsBackend as graphics::Backend>::Device>,
  inner: Mutex<AssetManagerInner>,
  containers: RwLock<Vec<Box<dyn AssetContainer<P>>>>,
  loaders: RwLock<Vec<Box<dyn AssetLoader<P>>>>,
  renderer_sender: Sender<LoadedAsset>,
  renderer_receiver: Receiver<LoadedAsset>,
  cond_var: Arc<Condvar>
}

struct AssetManagerInner {
  load_queue: VecDeque<AssetLoadRequest>,
  requested_assets: HashSet<String>,
  loaded_assets: HashSet<String>
}

impl<P: Platform> AssetManager<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as graphics::Backend>::Device>) -> Arc<Self> {
    let (renderer_sender, renderer_receiver) = unbounded();

    let cond_var = Arc::new(Condvar::new());

    let manager = Arc::new(Self {
      device: device.clone(),
      inner: Mutex::new(AssetManagerInner {
        load_queue: VecDeque::new(),
        loaded_assets: HashSet::new(),
        requested_assets: HashSet::new()
      }),
      loaders: RwLock::new(Vec::new()),
      containers: RwLock::new(Vec::new()),
      renderer_sender,
      renderer_receiver,
      cond_var
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

  pub fn add_mesh(&self, path: &str, vertex_buffer_data: Box<[u8]>, index_buffer_data: Box<[u8]>, parts: Box<[MeshRange]>) {
    let mesh = Mesh {
      vertices: vertex_buffer_data,
      indices: if !index_buffer_data.is_empty() { Some(index_buffer_data) } else { None },
      parts,
      bounding_box: None
    };
    self.add_asset(path, Asset::Mesh(mesh), AssetLoadPriority::Normal);
  }

  pub fn add_material(&self, path: &str, albedo: &str) {
    let material = Material {
      albedo_texture_path: albedo.to_string()
    };
    self.add_asset(path, Asset::Material(material), AssetLoadPriority::Normal);
  }

  pub fn add_model(&self, path: &str, mesh_path: &str, material_paths: &[&str]) {
    let model = Model {
      mesh_path: mesh_path.to_string(),
      material_paths: material_paths.iter().map(|mat| (*mat).to_owned()).collect()
    };
    self.add_asset(path, Asset::Model(model), AssetLoadPriority::Normal);
  }

  pub fn add_texture(&self, path: &str, info: &TextureInfo, texture_data: Box<[u8]>) {
    self.add_asset(path, Asset::Texture(Texture {
      info: info.clone(),
      data: Box::new([texture_data.to_vec().into_boxed_slice()]),
    }), AssetLoadPriority::Normal);
  }

  pub fn add_container(&self, container: Box<dyn AssetContainer<P>>) {
    self.add_container_with_progress(container, None)
  }

  pub fn add_container_with_progress(&self, container: Box<dyn AssetContainer<P>>, progress: Option<&Arc<AssetLoaderProgress>>) {
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

  pub fn add_asset_with_progress(&self, path: &str, asset: Asset, progress: Option<&Arc<AssetLoaderProgress>>, priority: AssetLoadPriority) {
    {
      let mut inner = self.inner.lock().unwrap();
      inner.loaded_assets.insert(path.to_string());
      inner.requested_assets.remove(path);
    }

    if let Some(progress) = progress {
      progress.finished.fetch_add(1, Ordering::SeqCst);
    }
    match asset {
      Asset::Material(material) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Material(material),
          path: path.to_owned(),
          priority
        }).unwrap();
      }
      Asset::Mesh(mesh) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Mesh(mesh),
          path: path.to_owned(),
          priority
        }).unwrap();
      }
      Asset::Texture(texture) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Texture(texture),
          path: path.to_owned(),
          priority
        }).unwrap();
      }
      Asset::Model(model) => {
        self.renderer_sender.send(LoadedAsset {
          asset: Asset::Model(model),
          path: path.to_owned(),
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
      let mut inner = self.inner.lock().unwrap();
      if inner.loaded_assets.contains(path) || inner.requested_assets.contains(path) {
        progress.finished.fetch_add(1, Ordering::SeqCst);
        return progress;
      }
      inner.requested_assets.insert(path.to_owned());

      inner.load_queue.push_back(AssetLoadRequest {
        asset_type,
        path: path.to_owned(),
        progress: progress.clone(),
        priority
      });
    }
    self.cond_var.notify_all();

    progress
  }

  pub fn load_level(self: &Arc<Self>, path: &str) -> Option<World> {
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
    if level.is_none() {
      return None;
    }
    while !progress.is_done() {}
    level
  }

  pub fn load_file(&self, path: &str) -> Option<AssetFile<P>> {
    let containers = self.containers.read().unwrap();
    let mut file_opt: Option<AssetFile<P>> = None;
    for container in containers.iter() {
      let container_file_opt = container.load(path);
      if container_file_opt.is_some() {
        file_opt = container_file_opt;
        break;
      }
    }
    if file_opt.is_none() {
      println!("Could not find file: {:?}", path);
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
    return false;
  }

  fn find_loader<'a>(file: &mut AssetFile<P>, loaders: &'a [Box<dyn AssetLoader<P>>]) -> Option<&'a dyn AssetLoader<P>> {
    let start = match &mut file.data {
      AssetFileData::File(file) => { file.seek(SeekFrom::Current(0)) }
      AssetFileData::Memory(cursor) => { cursor.seek(SeekFrom::Current(0)) }
    }.unwrap_or_else(|_| panic!("Failed to read file: {:?}", file.path));
    let loader_opt = loaders.iter().find(|loader| {
      let loader_matches = loader.matches(file);
      match &mut file.data {
        AssetFileData::File(file) => { file.seek(SeekFrom::Start(start)).unwrap(); }
        AssetFileData::Memory(cursor) => { cursor.seek(SeekFrom::Start(start)).unwrap(); }
      }
      loader_matches
    });
    loader_opt.map(|b| b.as_ref())
  }

  fn load_asset(self: &Arc<Self>, mut file: AssetFile<P>, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) {
    let path = file.path.clone();

    let loaders = self.loaders.read().unwrap();
    let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref());
    if loader_opt.is_none() {
      progress.finished.fetch_add(1, Ordering::SeqCst);
      {
        let mut inner = self.inner.lock().unwrap();
        inner.requested_assets.remove(&path);
      }
      println!("Could not find loader for file: {:?}", path.as_str());
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
      println!("Could not load file: {:?}", path.as_str());
      return;
      // dunno, error i guess
    }
  }

  pub fn receive_render_asset(&self) -> Option<LoadedAsset> {
    self.renderer_receiver.try_recv().ok()
  }

  pub fn notify_loaded(&self, path: &str) {
    let mut inner = self.inner.lock().unwrap();
    inner.loaded_assets.insert(path.to_string());
  }

  pub fn notify_unloaded(&self, path: &str) {
    let mut inner = self.inner.lock().unwrap();
    inner.loaded_assets.remove(path);
  }
}

fn asset_manager_thread_fn<P: Platform>(asset_manager: Weak<AssetManager<P>>) {
  let cond_var = {
    let mgr_opt = asset_manager.upgrade();
    if mgr_opt.is_none() {
      return;
    }
    let mgr = mgr_opt.unwrap();
    mgr.cond_var.clone()
  };

  'asset_loop: loop {
    let mgr_opt = asset_manager.upgrade();
    if mgr_opt.is_none() {
      break;
    }
    let mgr = mgr_opt.unwrap();
    let request = {
      let mut inner = mgr.inner.lock().unwrap();
      let mut request_opt = inner.load_queue.pop_front();
      let _ = cond_var.wait_timeout_while(
        inner,
        Duration::from_millis(2000),
        |inner| {
        if request_opt.is_some() {
          return false;
        }
        request_opt = inner.load_queue.pop_front();
        request_opt.is_none()
      }).unwrap();
      match request_opt {
        Some(request) => request,
        None => continue 'asset_loop
      }
    };

    {
      let file_opt = mgr.load_file(&request.path);
      if file_opt.is_none() {
        request.progress.finished.fetch_add(1, Ordering::SeqCst);
        continue 'asset_loop;
      }
      let file = file_opt.unwrap();
      mgr.load_asset(file, request.priority, &request.progress);
    }
  }
}
