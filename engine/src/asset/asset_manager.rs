use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, Mutex};
use std::collections::{HashMap, VecDeque};
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::graphics;
use sourcerenderer_core::graphics::{Device, MemoryUsage, BufferUsage, TextureInfo, Format, SampleCount, TextureShaderResourceViewInfo, Filter, AddressMode};
use nalgebra::Vector4;
use std::hash::Hash;
use crate::Vertex;

use std::sync::Weak;
use std::thread;
use std::time::Duration;
pub type AssetKey = usize;

struct AssetLoadRequest {
  key: AssetKey,
  path: String,
  asset_type: AssetType
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
  pub mesh: AssetKey,
  pub materials: Vec<AssetKey>
}

#[derive(Clone)]
pub struct Material {
  pub albedo: AssetKey
}

pub enum Asset<P: Platform> {
  Texture(Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView>),
  Mesh(Mesh<P>),
  Model(Model),
  Sound,
  Material(Material),
  Container(Box<dyn AssetLoader<P> + Send + Sync>)
}

pub struct AssetManager<P: Platform> {
  graphics: RwLock<AssetManagerGraphics<P>>,
  load_queue: Mutex<VecDeque<AssetLoadRequest>>,
  loaders: RwLock<Vec<Box<dyn AssetLoader<P> + Send + Sync>>>,
  asset_key_counter: AtomicUsize
}

impl<P: Platform> AssetManager<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as graphics::Backend>::Device>) -> Arc<Self> {
    let manager = Arc::new(Self {
      graphics: RwLock::new(AssetManagerGraphics {
        device: device.clone(),
        meshes: HashMap::new(),
        models: HashMap::new(),
        materials: HashMap::new(),
        textures: HashMap::new()
      }),
      load_queue: Mutex::new(VecDeque::new()),
      loaders: RwLock::new(Vec::new()),
      asset_key_counter: AtomicUsize::new(0)
    });

    let zero_buffer = device.upload_data(&Vector4::<u8>::new(0u8, 0u8, 0u8, 0u8), MemoryUsage::CpuOnly, BufferUsage::COPY_SRC);
    let zero_texture = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
      width: 1,
      height: 1,
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

      let texture_key = manager.make_asset_key();
      let material = Material {
        albedo: texture_key.clone()
      };
      graphics.textures.insert(texture_key.clone(), zero_view);
      let material_key = manager.make_asset_key();
      graphics.materials.insert(material_key.clone(), material);
    }

    let thread_count = 1;
    for _ in 0..thread_count {
      let c_manager = Arc::downgrade(&manager);
      thread::spawn(move || asset_manager_thread_fn(c_manager));
    }

    manager
  }

  pub fn lookup_graphics(&self) -> RwLockReadGuard<'_, AssetManagerGraphics<P>> {
    self.graphics.read().unwrap()
  }

  fn make_asset_key(&self) -> AssetKey {
    self.asset_key_counter.fetch_add(1, Ordering::SeqCst)
  }

  pub fn add_mesh(self: &Arc<AssetManager<P>>, _name: &str, vertex_buffer_data: &[Vertex], index_buffer_data: &[u32]) -> AssetKey {
    let key = self.make_asset_key();
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
    graphics.meshes.insert(key, mesh);
    key
  }

  pub fn add_material(self: &Arc<AssetManager<P>>, _name: &str, albedo: AssetKey) -> AssetKey {
    let key = self.make_asset_key();
    let material = Material {
      albedo
    };
    let mut graphics = self.graphics.write().unwrap();
    graphics.materials.insert(key.clone(), material);
    key
  }

  pub fn add_model(self: &Arc<AssetManager<P>>, _name: &str, mesh: AssetKey, materials: &[AssetKey]) -> AssetKey {
    let key = self.make_asset_key();
    let mut graphics = self.graphics.write().unwrap();
    let model = Model {
      mesh: mesh.clone(),
      materials: materials.iter().map(|mat| *mat).collect()
    };
    graphics.models.insert(key.clone(), model);
    key
  }

  pub fn add_texture(self: &Arc<AssetManager<P>>, name: &str, info: &TextureInfo, texture_data: &[u8]) -> AssetKey {
    let key = self.make_asset_key();
    let mut graphics = self.graphics.write().unwrap();
    let src_buffer = graphics.device.upload_data_raw(texture_data, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let texture = graphics.device.create_texture(info, Some(name));
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
    graphics.textures.insert(key.clone(), srv);
    key
  }

  pub fn load(self: &Arc<AssetManager<P>>, path: &str, asset_type: AssetType) -> AssetKey {
    let key = self.make_asset_key();
    let mut queue_guard = self.load_queue.lock().unwrap();
    queue_guard.push_back(AssetLoadRequest {
      key,
      asset_type,
      path: path.to_owned()
    });

    key
  }

  pub fn flush(&self) {
    let guard = self.graphics.read().unwrap();
    guard.device.flush_transfers();
  }
}

pub struct AssetManagerGraphics<P: Platform> {
  device: Arc<<P::GraphicsBackend as graphics::Backend>::Device>,
  meshes: HashMap<AssetKey, Mesh<P>>,
  models: HashMap<AssetKey, Model>,
  materials: HashMap<AssetKey, Material>,
  textures: HashMap<AssetKey, Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView>>
}

impl<P: Platform> AssetManagerGraphics<P> {
  pub fn get_model(&self, key: &AssetKey) -> &Model {
    // TODO make optional variant of function
    self.models.get(key).unwrap()
  }
  pub fn get_mesh(&self, key: &AssetKey) -> &Mesh<P> {
    // TODO make optional variant of function
    self.meshes.get(key).unwrap()
  }
  pub fn get_material(&self, key: &AssetKey) -> &Material {
    // TODO return placeholder if not ready
    self.materials.get(key).unwrap()
  }
  pub fn get_texture(&self, key: &AssetKey) -> &Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView> {
    // TODO return placeholder if not ready
    self.textures.get(key).unwrap()
  }
}

fn asset_manager_thread_fn<P: Platform>(asset_manager: Weak<AssetManager<P>>) {
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
        let mut loaders = mgr.loaders.read().unwrap();
        let mut loader_opt = loaders.iter().find(|l| l.matches(&request.path, request.asset_type));

        if loader_opt.is_none() {
          // drop so we can lock again for writing
          std::mem::drop(loader_opt);
          std::mem::drop(loaders);

          {
            let mut loaders_mut = mgr.loaders.write().unwrap();
            let container_loader = loaders_mut.iter().find(|f| (*f).matches(&request.path, AssetType::Container))
              .expect(&format!("Could not find loader or container for path: {}", &request.path));
            let new_loader_asset = container_loader.load(&request.path, AssetType::Container)
              .expect(&format!("Failed to create loader for path: {}", &request.path));
            let new_loader = match new_loader_asset {
              Asset::Container(loader) => loader,
              _ => panic!("Failed to create loader for path: {}, created wrong asset type", &request.path)
            };
            loaders_mut.push(new_loader);
          }
          loaders = mgr.loaders.read().unwrap();
          loader_opt = loaders.last();
        }

        let loader = loader_opt.unwrap();
        let asset_opt = loader.load(&request.path, request.asset_type);
        let asset = asset_opt.expect(&format!("Failed to load asset with path: {}", &request.path));
        match asset {
          Asset::Texture(view) => {
            let mut graphics = mgr.graphics.write().unwrap();
            graphics.textures.insert(request.key, view);
          },
          Asset::Model(model) => {
            let mut graphics = mgr.graphics.write().unwrap();
            graphics.models.insert(request.key, model);
          },
          _ => {
            panic!("");
          }
        }
      }
    }

    thread::sleep(Duration::new(0, 10_000_000)); // 10ms
  }
}

pub trait AssetLoader<P: Platform> {
  fn matches(&self, path: &str, asset_type: AssetType) -> bool;
  fn load(&self, path: &str, asset_type: AssetType) -> Option<Asset<P>>;
}
