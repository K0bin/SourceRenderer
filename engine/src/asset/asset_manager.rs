use std::sync::atomic::{AtomicUsize, AtomicU32, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, Mutex};
use std::collections::{HashMap, VecDeque};
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::graphics::Backend as GraphicsBackend;
use sourcerenderer_core::graphics;
use sourcerenderer_core::graphics::{Device, MemoryUsage, BufferUsage, TextureInfo, Format, SampleCount, TextureShaderResourceViewInfo, Filter, AddressMode};
use nalgebra::Vector4;
use std::hash::Hash;
use crate::Vertex;

use std::sync::Weak;
use std::thread;
use std::time::Duration;
use legion::World;
use std::fs::File;
use std::io::{Cursor, Seek, SeekFrom};

pub type AssetKey = usize;

pub struct AssetLoadRequest {
  path: String,
  asset_type: AssetType,
  progress: Arc<AssetLoaderProgress>
}

pub struct AssociatedAssetLoadRequest {
  pub path: String,
  pub asset_type: AssetType
}

pub struct LoadedAsset<P: Platform> {
  pub path: String,
  pub asset: Asset<P>
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

pub struct AssetLoaderResult<P: Platform> {
  pub assets: Vec<LoadedAsset<P>>,
  pub containers: Vec<Box<dyn AssetContainer>>,
  pub level: Option<World>,
  pub requests: Vec<AssociatedAssetLoadRequest>
}

impl<P: Platform> AssetLoaderResult<P> {

}

pub trait AssetLoader<P: Platform>
  : Send + Sync {
  fn matches(&self, file: &mut AssetFile) -> bool;
  fn load(&self, file: AssetFile, manager: &AssetManager<P>) -> Result<AssetLoaderResult<P>, ()>;
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
  assets: Mutex<HashMap<String, Asset<P>>>,
}

impl<P: Platform> AssetManager<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as graphics::Backend>::Device>) -> Arc<Self> {

    let manager = Arc::new(Self {
      device: device.clone(),
      load_queue: Mutex::new(VecDeque::new()),
      loaders: RwLock::new(Vec::new()),
      containers: RwLock::new(Vec::new()),
      assets: Mutex::new(HashMap::new())
    });

    let zero_buffer = device.upload_data(&Vector4::<u8>::new(255u8, 255u8, 255u8, 255u8), MemoryUsage::CpuOnly, BufferUsage::COPY_SRC);
    let zero_texture = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
      width: 2,
      height: 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1
    }, Some("AssetManagerZeroTexture"));
    device.init_texture(&zero_texture, &zero_buffer, 0, 0);
    let zero_view = device.create_shader_resource_view(&zero_texture, &TextureShaderResourceViewInfo {
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

    {
      let mut assets = manager.assets.lock().unwrap();

      let texture_key = "BLANK_TEXTURE";
      let material = Arc::new(Material {
        albedo_texture_path: texture_key.to_owned()
      });
      assets.insert(texture_key.to_owned(), Asset::Texture(zero_view));
      let material_key = "BLANK_MATERIAL";
      assets.insert(material_key.to_owned(), Asset::Material(material));
    }

    let thread_count = 1;
    for _ in 0..thread_count {
      let c_manager = Arc::downgrade(&manager);
      thread::spawn(move || asset_manager_thread_fn(c_manager));
    }

    manager
  }

  pub fn graphics_device(&self) -> &Arc<<P::GraphicsBackend as graphics::Backend>::Device> {
    &self.device
  }

  pub fn add_mesh(self: &Arc<AssetManager<P>>, path: &str, vertex_buffer_data: &[Vertex], index_buffer_data: &[u32]) {
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
    let mut assets = self.assets.lock().unwrap();
    assets.insert(path.to_owned(), Asset::Mesh(mesh));
  }

  pub fn add_material(self: &Arc<AssetManager<P>>, path: &str, albedo: &str) {
    let material = Arc::new(Material {
      albedo_texture_path: albedo.to_string()
    });
    let mut assets = self.assets.lock().unwrap();
    assets.insert(path.to_owned(), Asset::Material(material));
  }

  pub fn add_model(self: &Arc<AssetManager<P>>, path: &str, mesh_path: &str, material_paths: &[&str]) {
    let model = Arc::new(Model {
      mesh_path: mesh_path.to_string(),
      material_paths: material_paths.iter().map(|mat| (*mat).to_owned()).collect()
    });
    let mut assets = self.assets.lock().unwrap();
    assets.insert(path.to_owned(), Asset::Model(model));
  }

  pub fn add_texture(self: &Arc<AssetManager<P>>, path: &str, info: &TextureInfo, texture_data: &[u8]) {
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

    let mut assets = self.assets.lock().unwrap();
    assets.insert(path.to_owned(), Asset::Texture(srv));
  }

  pub fn add_container(&self, container: Box<dyn AssetContainer>) {
    let mut containers = self.containers.write().unwrap();
    containers.push(container);
  }

  pub fn add_loader(&self, loader: Box<dyn AssetLoader<P>>) {
    let mut loaders = self.loaders.write().unwrap();
    loaders.push(loader);
  }

  pub fn get_model(&self, path: &str) -> Arc<Model> {
    let mut assets = self.assets.lock().unwrap();
    let asset = assets.get(path).unwrap();
    match asset {
      Asset::Model(model) => model.clone(),
      _ => panic!("Wrong asset type")
    }
  }
  pub fn get_mesh(&self, path: &str) -> Arc<Mesh<P::GraphicsBackend>> {
    let mut assets = self.assets.lock().unwrap();
    let asset = assets.get(path).unwrap();
    match asset {
      Asset::Mesh(mesh) => mesh.clone(),
      _ => panic!("Wrong asset type")
    }
  }
  pub fn get_material(&self, path: &str) -> Arc<Material> {
    let mut assets = self.assets.lock().unwrap();
    let asset = assets.get(path).unwrap_or_else(|| assets.get("BLANK_MATERIAL").unwrap());
    match asset {
      Asset::Material(material) => material.clone(),
      _ => panic!("Wrong asset type")
    }
  }
  pub fn get_texture(&self, path: &str) -> Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView> {
    let mut assets = self.assets.lock().unwrap();
    let asset = assets.get(path).unwrap_or_else(|| assets.get("BLANK_TEXTURE").unwrap());
    match asset {
      Asset::Texture(texture) => texture.clone(),
      _ => panic!("Wrong asset type")
    }
  }

  pub fn request_asset(self: &Arc<AssetManager<P>>, path: &str, asset_type: AssetType) -> Arc<AssetLoaderProgress> {
    let progress = Arc::new(AssetLoaderProgress {
      expected: AtomicU32::new(1),
      finished: AtomicU32::new(0)
    });
    let mut queue_guard = self.load_queue.lock().unwrap();
    queue_guard.push_back(AssetLoadRequest {
      asset_type,
      path: path.to_owned(),
      progress: progress.clone()
    });

    progress
  }

  pub fn load_level(&self, path: &str) -> Option<World> {
    let mut file_opt = self.load_file(path);
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
    let loader = loader_opt.unwrap();
    let assets_opt = loader.load(file, self);
    if assets_opt.is_err() {
      println!("Could not load file: {:?}", path);
      return None;
    }
    let mut assets = assets_opt.unwrap();
    let progress = Arc::new(AssetLoaderProgress {
      expected: AtomicU32::new(1),
      finished: AtomicU32::new(0)
    });
    let level = self.finish_import(assets, Some(&progress));
    while level.is_some() && !progress.is_done() {}
    level
  }

  fn load_file(&self, path: &str) -> Option<AssetFile> {
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
        AssetFileData::File(file) => { file.seek(SeekFrom::Start(start)); }
        AssetFileData::Memory(cursor) => { cursor.seek(SeekFrom::Start(start)); }
      }
      loader_matches
    });
    loader_opt
  }

  fn load_asset(&self, mut file: AssetFile, progress: Option<&Arc<AssetLoaderProgress>>) -> Option<Asset<P>> {
    let path = file.path.clone();

    let loaders = self.loaders.read().unwrap();
    let loader_opt = AssetManager::find_loader(&mut file, loaders.as_ref());
    if loader_opt.is_none() {
      if let Some(progress) = progress {
        progress.finished.fetch_add(1, Ordering::SeqCst);
      }
      println!("Could not find loader for file: {:?}", path.as_str());
      return None;
    }
    let loader = loader_opt.unwrap();

    let assets_opt = loader.load(file, self);
    if assets_opt.is_err() {
      if let Some(progress) = progress {
        progress.finished.fetch_add(1, Ordering::SeqCst);
      }
      println!("Could not load file: {:?}", path.as_str());
      return None;
      // dunno, error i guess
    }
    let mut assets = assets_opt.unwrap();
    self.finish_import(assets, progress);
    let cache = self.assets.lock().unwrap();
    cache.get(&path).map(|a| a.clone())
  }

  fn finish_import(&self, mut asset_load_result: AssetLoaderResult<P>, progress: Option<&Arc<AssetLoaderProgress>>) -> Option<World> {
    let loaded_assets = std::mem::replace(&mut asset_load_result.assets, Vec::new());
    let loaded_containers = std::mem::replace(&mut asset_load_result.containers, Vec::new());
    let loaded_level = std::mem::replace(&mut asset_load_result.level, None);

    {
      let mut containers = self.containers.write().unwrap();
      for container in loaded_containers {
        containers.push(container);
      }
    }

    {
      let mut cache = self.assets.lock().unwrap();
      for asset in loaded_assets {
        cache.insert(asset.path.clone(), asset.asset.clone());
      }
    }

    {
      let mut queue = self.load_queue.lock().unwrap();
      if let Some(progress) = progress {
        progress.expected.fetch_add(asset_load_result.requests.len() as u32, Ordering::SeqCst);
      }
      for new_request in asset_load_result.requests {
        queue.push_back(AssetLoadRequest {
          asset_type: new_request.asset_type,
          path: new_request.path,
          progress: progress.map_or_else(|| Arc::new(AssetLoaderProgress {
            expected: AtomicU32::new(1),
            finished: AtomicU32::new(0)
          }),|p| p.clone())
        });
      }
    }

    if let Some(progress) = progress {
      progress.finished.fetch_add(1, Ordering::SeqCst);
    }

    loaded_level
  }

  pub fn flush(&self) {
    self.device.flush_transfers();
  }
}

fn asset_manager_thread_fn<P: Platform>(asset_manager: Weak<AssetManager<P>>) {
  let device = {
    let mgr_opt = asset_manager.upgrade();
    if mgr_opt.is_none() {
      return;
    }
    let mgr = mgr_opt.unwrap();
    mgr.device.clone()
  };
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
        let asset = mgr.load_asset(file, Some(&request.progress));
      }
    }

    thread::sleep(Duration::new(0, 10_000_000)); // 10ms
  }
}
