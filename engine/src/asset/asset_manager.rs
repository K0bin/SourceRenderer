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

#[derive(Clone)]
pub struct Mesh<P: Platform> {
  pub vertices: Arc<<P::GraphicsBackend as graphics::Backend>::Buffer>,
  pub indices: Option<Arc<<P::GraphicsBackend as graphics::Backend>::Buffer>>,
  pub parts: Vec<MeshRange>
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
  pub requests: Vec<AssociatedAssetLoadRequest>
}

impl<P: Platform> AssetLoaderResult<P> {

}

pub struct AssetLoaderContext<P: Platform> {
  pub graphics_device: Arc<<P::GraphicsBackend as GraphicsBackend>::Device>
}

pub trait AssetLoader<P: Platform>
  : Send + Sync {
  fn matches(&self, file: &mut AssetFile) -> bool;
  fn load(&self, file: AssetFile, context: &AssetLoaderContext<P>) -> Result<AssetLoaderResult<P>, ()>;
}

pub enum Asset<P: Platform> {
  Texture(Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView>),
  Mesh(Mesh<P>),
  Model(Model),
  Sound,
  Material(Material),
  Container(Box<dyn AssetContainer>),
  Level(World)
}

pub struct AssetManager<P: Platform> {
  graphics: RwLock<AssetManagerGraphicsCache<P>>,
  load_queue: Mutex<VecDeque<AssetLoadRequest>>,
  containers: RwLock<Vec<Box<dyn AssetContainer>>>,
  loaders: RwLock<Vec<Box<dyn AssetLoader<P>>>>,
  levels: RwLock<HashMap<String, World>>
}

impl<P: Platform> AssetManager<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as graphics::Backend>::Device>) -> Arc<Self> {
    let manager = Arc::new(Self {
      graphics: RwLock::new(AssetManagerGraphicsCache {
        device: device.clone(),
        meshes: HashMap::new(),
        models: HashMap::new(),
        materials: HashMap::new(),
        textures: HashMap::new()
      }),
      load_queue: Mutex::new(VecDeque::new()),
      loaders: RwLock::new(Vec::new()),
      containers: RwLock::new(Vec::new()),
      levels: RwLock::new(HashMap::new())
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
      max_anisotropy: 1.0,
      compare_op: None,
      min_lod: 0.0,
      max_lod: 0.0
    });

    {
      let mut graphics = manager.graphics.write().unwrap();

      let texture_key = "BLANK_TEXTURE";
      let material = Material {
        albedo_texture_path: texture_key.to_owned()
      };
      graphics.textures.insert(texture_key.to_owned(), zero_view);
      let material_key = "BLANK_MATERIAL";
      graphics.materials.insert(material_key.to_owned(), material);
    }

    let thread_count = 1;
    for _ in 0..thread_count {
      let c_manager = Arc::downgrade(&manager);
      thread::spawn(move || asset_manager_thread_fn(c_manager));
    }

    manager
  }

  pub fn lookup_graphics(&self) -> RwLockReadGuard<'_, AssetManagerGraphicsCache<P>> {
    self.graphics.read().unwrap()
  }

  pub fn add_mesh(self: &Arc<AssetManager<P>>, path: &str, vertex_buffer_data: &[Vertex], index_buffer_data: &[u32]) {
    let mut graphics = self.graphics.write().unwrap();

    //vertex_buffer_data.clone();
    let vertex_buffer = graphics.device.upload_data_slice(vertex_buffer_data, MemoryUsage::CpuToGpu, BufferUsage::VERTEX | BufferUsage::COPY_SRC);
    let index_buffer = if index_buffer_data.len() != 0 {
      Some(graphics.device.upload_data_slice(index_buffer_data, MemoryUsage::CpuToGpu, BufferUsage::INDEX | BufferUsage::COPY_SRC))
    } else {
      None
    };
    let mesh = Mesh {
      vertices: vertex_buffer,
      indices: index_buffer,
      parts: vec![MeshRange {
        start: 0,
        count: if index_buffer_data.len() == 0 { vertex_buffer_data.len() } else { index_buffer_data.len() } as u32
      }]
    };
    graphics.meshes.insert(path.to_owned(), mesh);
  }

  pub fn add_material(self: &Arc<AssetManager<P>>, path: &str, albedo: &str) {
    let material = Material {
      albedo_texture_path: albedo.to_string()
    };
    let mut graphics = self.graphics.write().unwrap();
    graphics.materials.insert(path.to_owned(), material);
  }

  pub fn add_model(self: &Arc<AssetManager<P>>, path: &str, mesh_path: &str, material_paths: &[&str]) {
    let mut graphics = self.graphics.write().unwrap();
    let model = Model {
      mesh_path: mesh_path.to_string(),
      material_paths: material_paths.iter().map(|mat| (*mat).to_owned()).collect()
    };
    graphics.models.insert(path.to_owned(), model);
  }

  pub fn add_texture(self: &Arc<AssetManager<P>>, path: &str, info: &TextureInfo, texture_data: &[u8]) {
    let mut graphics = self.graphics.write().unwrap();
    let src_buffer = graphics.device.upload_data_raw(texture_data, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let texture = graphics.device.create_texture(info, Some(path));
    graphics.device.init_texture(&texture, &src_buffer, 0, 0);
    let srv = graphics.device.create_shader_resource_view(&texture, &TextureShaderResourceViewInfo {
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
      max_anisotropy: 1.0,
      compare_op: None,
      min_lod: 0.0,
      max_lod: 0.0
    });
    graphics.textures.insert(path.to_owned(), srv);
  }

  pub fn add_container(&self, container: Box<dyn AssetContainer>) {
    let mut containers = self.containers.write().unwrap();
    containers.push(container);
  }

  pub fn add_loader(&self, loader: Box<dyn AssetLoader<P>>) {
    let mut loaders = self.loaders.write().unwrap();
    loaders.push(loader);
  }

  pub fn get_level(&self, path: &str) -> Option<World> {
    let mut levels = self.levels.write().unwrap();
    levels.remove(path)
  }

  pub fn load(self: &Arc<AssetManager<P>>, path: &str, asset_type: AssetType) -> Arc<AssetLoaderProgress> {
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

  pub fn flush(&self) {
    let guard = self.graphics.read().unwrap();
    guard.device.flush_transfers();
  }
}

pub struct AssetManagerGraphicsCache<P: Platform> {
  device: Arc<<P::GraphicsBackend as graphics::Backend>::Device>,
  meshes: HashMap<String, Mesh<P>>,
  models: HashMap<String, Model>,
  materials: HashMap<String, Material>,
  textures: HashMap<String, Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView>>
}

impl<P: Platform> AssetManagerGraphicsCache<P> {
  pub fn get_model(&self, key: &str) -> &Model {
    // TODO make optional variant of function
    self.models.get(key).unwrap()
  }
  pub fn get_mesh(&self, key: &str) -> &Mesh<P> {
    // TODO make optional variant of function
    self.meshes.get(key).unwrap()
  }
  pub fn get_material(&self, key: &str) -> &Material {
    // TODO return placeholder if not ready
    self.materials.get(key).unwrap_or_else(|| self.get_material("BLANK_MATERIAL"))
  }
  pub fn get_texture(&self, key: &str) -> &Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView> {
    // TODO return placeholder if not ready
    self.textures.get(key).unwrap_or_else(|| self.get_texture("BLANK_TEXTURE"))
  }
}

fn asset_manager_thread_fn<P: Platform>(asset_manager: Weak<AssetManager<P>>) {
  let device = {
    let mgr_opt = asset_manager.upgrade();
    if mgr_opt.is_none() {
      return;
    }
    let mgr = mgr_opt.unwrap();
    let graphics = mgr.graphics.read().unwrap();
    graphics.device.clone()
  };
  let context = AssetLoaderContext {
    graphics_device: device
  };
  loop {
    {
      let request_opt = {
        let mgr_opt = asset_manager.upgrade();
        if mgr_opt.is_none() {
          break;
        }
        let mgr = mgr_opt.unwrap();
        let mut queue = mgr.load_queue.lock().unwrap();
        queue.pop_front()
      };

      if request_opt.is_none() {
        thread::sleep(Duration::new(3, 0));
        continue;
      }

      let request = request_opt.unwrap();
      let mgr_opt = asset_manager.upgrade();
      if mgr_opt.is_none() {
        break;
      }
      let mgr = mgr_opt.unwrap();
      {
        let containers = mgr.containers.read().unwrap();
        let mut file_opt: Option<AssetFile> = None;
        'containers: for container in containers.iter() {
          let container_file_opt = container.load(request.path.as_str());
          if container_file_opt.is_some() {
            file_opt = container_file_opt;
            break 'containers;
          }
        }
        std::mem::drop(containers);

        if file_opt.is_none() {
          println!("Could not find file: {:?}", request.path.as_str());
          continue;
          // dunno, error i guess
        }
        let mut file = file_opt.unwrap();
        let loaders = mgr.loaders.read().unwrap();

        let start = match &mut file.data {
          AssetFileData::File(file) => {file.seek(SeekFrom::Current(0))}
          AssetFileData::Memory(cursor) => {cursor.seek(SeekFrom::Current(0))}
        }.expect(format!("Failed to read file: {:?}", request.path.as_str()).as_str());
        let loader_opt = loaders.iter().find(|loader| {
          let loader_matches = loader.matches(&mut file);
          match &mut file.data {
            AssetFileData::File(file) => { file.seek(SeekFrom::Start(start)); }
            AssetFileData::Memory(cursor) => { cursor.seek(SeekFrom::Start(start)); }
          }
          loader_matches
        });
        if loader_opt.is_none() {
          println!("Could not find loader for file: {:?}", request.path.as_str());
          continue;
        }
        let loader = loader_opt.unwrap();

        let assets_opt = loader.load(file, &context);
        if assets_opt.is_err() {
          println!("Could not load file: {:?}", request.path.as_str());
          continue;
          // dunno, error i guess
        }
        let mut assets = assets_opt.unwrap();
        let loaded_assets = std::mem::replace(&mut assets.assets, Vec::new());
        for asset in loaded_assets {
          match asset.asset {
            Asset::Texture(view) => {
              let mut graphics = mgr.graphics.write().unwrap();
              graphics.textures.insert(asset.path.clone(), view);
            },
            Asset::Model(model) => {
              let mut graphics = mgr.graphics.write().unwrap();
              graphics.models.insert(asset.path.clone(), model);
            },
            Asset::Container(container) => {
              let mut containers = mgr.containers.write().unwrap();
              containers.push(container)
            },
            Asset::Mesh(mesh) => {
              let mut graphics = mgr.graphics.write().unwrap();
              graphics.meshes.insert(asset.path.clone(), mesh);
            },
            Asset::Level(world) => {
              let mut levels = mgr.levels.write().unwrap();
              levels.insert(asset.path.clone(), world);
            },
            _ => {
              panic!("Could not store loaded asset {}.", asset.path.as_str());
            }
          }
        }

        for new_request in assets.requests {
          request.progress.expected.fetch_add(1, Ordering::SeqCst);
          let mgr_opt = asset_manager.upgrade();
          if mgr_opt.is_none() {
            break;
          }
          let mgr = mgr_opt.unwrap();
          let mut queue = mgr.load_queue.lock().unwrap();
          queue.push_back(AssetLoadRequest {
            asset_type: new_request.asset_type,
            path: new_request.path,
            progress: request.progress.clone()
          });
        }
      }
      request.progress.finished.fetch_add(1, Ordering::SeqCst);
    }

    thread::sleep(Duration::new(0, 10_000_000)); // 10ms
  }
}
