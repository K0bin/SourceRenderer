use std::sync::atomic::{AtomicUsize, AtomicU32};
use std::cmp::Ordering;
use std::sync::{Arc, MutexGuard, RwLock, RwLockReadGuard};
use std::collections::HashMap;
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::{graphics, Vec3};
use sourcerenderer_core::graphics::{Backend, Texture, Buffer, Device, MemoryUsage, BufferUsage, TextureInfo, Format, SampleCount, TextureShaderResourceViewInfo, Filter, AddressMode};
use nalgebra::{Vector3, Vector4};
use std::hash::{Hash, Hasher};
use crate::Vertex;

pub struct AssetKey<P: Platform> {
  name: String,
  asset_manager: Arc<AssetManager<P>>,
  priority: AtomicU32,
  asset_type: AssetType
}

impl<P: Platform> AssetKey<P> {
  fn new(asset_manager: &Arc<AssetManager<P>>, name: &str, asset_type: AssetType) -> Arc<AssetKey<P>> {
    Arc::new(Self {
      name: name.to_owned(),
      asset_manager: asset_manager.clone(),
      priority: AtomicU32::new(0),
      asset_type
    })
  }
}

impl<P: Platform> PartialEq for AssetKey<P> {
  fn eq(&self, other: &Self) -> bool {
    self.name == other.name
  }
}

impl<P: Platform> Eq for AssetKey<P> {}

impl<P: Platform> Hash for AssetKey<P> {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.name.hash(state)
  }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetType {
  Texture,
  Model,
  Mesh,
  Material,
  Sound
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
pub struct Model<P: Platform> {
  pub mesh: Arc<AssetKey<P>>,
  pub materials: Vec<Arc<AssetKey<P>>>
}

#[derive(Clone)]
pub struct Material<P: Platform> {
  pub albedo: Arc<AssetKey<P>>
}

enum AssetContent<P: Platform> {
  Texture(Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView>),
  Mesh(Mesh<P>),
  Model(Model<P>),
  Sound,
  Material(Material<P>)
}

pub struct AssetManager<P: Platform> {
  graphics: RwLock<AssetManagerGraphics<P>>
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
      })
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
    });
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

      let texture_key = manager.make_asset_key(PLACEHOLDER_TEXTURE_NAME, AssetType::Texture);
      let material = Material {
        albedo: texture_key.clone()
      };
      graphics.textures.insert(texture_key.clone(), zero_view);
      let material_key = manager.make_asset_key(PLACEHOLDER_MATERIAL_NAME, AssetType::Material);
      graphics.materials.insert(material_key.clone(), material);
    }

    manager
  }

  pub fn lookup_graphics(&self) -> RwLockReadGuard<'_, AssetManagerGraphics<P>> {
    self.graphics.read().unwrap()
  }

  fn make_asset_key(self: &Arc<AssetManager<P>>, name: &str, asset_type: AssetType) -> Arc<AssetKey<P>> {
    AssetKey::new(self, name, asset_type)
  }

  pub fn add_mesh(self: &Arc<AssetManager<P>>, name: &str, vertex_buffer_data: &[Vertex], index_buffer_data: &[u32]) -> Arc<AssetKey<P>> {
    let key = Self::make_asset_key(self, name, AssetType::Mesh);
    let mut graphics = self.graphics.write().unwrap();

    vertex_buffer_data.clone();
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
    graphics.meshes.insert(key.clone(), mesh);
    key
  }

  pub fn add_material(self: &Arc<AssetManager<P>>, name: &str, albedo: &Arc<AssetKey<P>>) -> Arc<AssetKey<P>> {
    let key = Self::make_asset_key(self, name, AssetType::Material);
    let material = Material {
      albedo: albedo.clone()
    };
    let mut graphics = self.graphics.write().unwrap();
    graphics.materials.insert(key.clone(), material);
    key
  }

  pub fn add_model(self: &Arc<AssetManager<P>>, name: &str, mesh: &Arc<AssetKey<P>>, materials: &[&Arc<AssetKey<P>>]) -> Arc<AssetKey<P>> {
    let key = Self::make_asset_key(self, name, AssetType::Model);
    let mut graphics = self.graphics.write().unwrap();
    let model = Model {
      mesh: mesh.clone(),
      materials: materials.iter().map(|mat| (*mat).clone()).collect()
    };
    graphics.models.insert(key.clone(), model);
    key
  }

  pub fn add_texture(self: &Arc<AssetManager<P>>, name: &str, info: &TextureInfo, texture_data: &[u8]) -> Arc<AssetKey<P>> {
    let key = Self::make_asset_key(self, name, AssetType::Texture);
    let mut graphics = self.graphics.write().unwrap();
    let src_buffer = graphics.device.upload_data_raw(texture_data, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let texture = graphics.device.create_texture(info);
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

  pub fn load_texture(self: &Arc<AssetManager<P>>, name: &str) -> Arc<AssetKey<P>> {
    //task::spawn()
    unimplemented!();
  }

  pub fn flush(&self) {
    let guard = self.graphics.read().unwrap();
    guard.device.flush_transfers();
  }

  pub fn cleanup(&self) {
    let mut graphics = self.graphics.write().unwrap();

    graphics.meshes.retain(|key, mesh| Arc::strong_count(key) > 1);
    graphics.materials.retain(|key, materials| Arc::strong_count(key) > 1);
    graphics.textures.retain(|key, texture| Arc::strong_count(key) > 1);
  }
}

const PLACEHOLDER_TEXTURE_NAME: &'static str = "PLACEHOLDER_TEXTURE";
const PLACEHOLDER_MATERIAL_NAME: &'static str = "PLACEHOLDER_MATERIAL";

pub struct AssetManagerGraphics<P: Platform> {
  device: Arc<<P::GraphicsBackend as graphics::Backend>::Device>,
  meshes: HashMap<Arc<AssetKey<P>>, Mesh<P>>,
  models: HashMap<Arc<AssetKey<P>>, Model<P>>,
  materials: HashMap<Arc<AssetKey<P>>, Material<P>>,
  textures: HashMap<Arc<AssetKey<P>>, Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView>>
}

impl<P: Platform> AssetManagerGraphics<P> {
  pub fn get_model(&self, key: &Arc<AssetKey<P>>) -> &Model<P> {
    // TODO make optional variant of function
    self.models.get(key).unwrap()
  }
  pub fn get_mesh(&self, key: &Arc<AssetKey<P>>) -> &Mesh<P> {
    // TODO make optional variant of function
    self.meshes.get(key).unwrap()
  }
  pub fn get_material(&self, key: &Arc<AssetKey<P>>) -> &Material<P> {
    // TODO return placeholder if not ready
    self.materials.get(key).unwrap()
  }
  pub fn get_texture(&self, key: &Arc<AssetKey<P>>) -> &Arc<<P::GraphicsBackend as graphics::Backend>::TextureShaderResourceView> {
    // TODO return placeholder if not ready
    self.textures.get(key).unwrap()
  }
}
