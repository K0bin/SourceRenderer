use std::sync::Arc;
use std::collections::HashMap;

use sourcerenderer_core::{Vec4, graphics::{Backend, BufferInfo, Device, Fence, TextureUsage, SamplerInfo, Filter, AddressMode}};
use crate::{asset::{Asset, AssetManager, Material, Mesh, Model, Texture, AssetLoadPriority, MeshRange, MaterialValue}, math::BoundingBox};
use sourcerenderer_core::Platform;
use sourcerenderer_core::graphics::{ TextureInfo, MemoryUsage, SampleCount, Format, TextureShaderResourceViewInfo, BufferUsage };

use sourcerenderer_core::atomic_refcell::{AtomicRef, AtomicRefCell};

pub struct RendererTexture<B: Backend> {
  pub(super) view: Arc<B::TextureShaderResourceView>,
  pub(super) bindless_index: Option<u32>
}

impl<B: Backend> PartialEq for RendererTexture<B> {
  fn eq(&self, other: &Self) -> bool {
    self.view == other.view
  }
}
impl<B: Backend> Eq for RendererTexture<B> {}

pub struct RendererMaterial<B: Backend> {
  pub(super) properties: HashMap<String, RendererMaterialValue<B>>,
  pub(super) shader_name: String // TODO reference actual shader
}

impl<B: Backend> Clone for RendererMaterial<B> {
  fn clone(&self) -> Self {
    Self { properties: self.properties.clone(), shader_name: self.shader_name.clone() }
  }
}

pub enum RendererMaterialValue<B: Backend> {
  Float(f32),
  Vec4(Vec4),
  Texture(Arc<RendererTexture<B>>)
}

impl<B: Backend> PartialEq for RendererMaterialValue<B> {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Float(l0), Self::Float(r0)) => (l0 * 100f32) as u32 == (r0 * 100f32) as u32,
      (Self::Vec4(l0), Self::Vec4(r0)) => (l0.x * 100f32) as u32 == (r0.x * 100f32) as u32
                                                                                              && (l0.y * 100f32) as u32 == (r0.y * 100f32) as u32
                                                                                              && (l0.z * 100f32) as u32 == (r0.z * 100f32) as u32
                                                                                              && (l0.w * 100f32) as u32 == (r0.w * 100f32) as u32,
      (Self::Texture(l0), Self::Texture(r0)) => l0 == r0,
      _ => false
    }
  }
}

impl<B: Backend> Eq for RendererMaterialValue<B> {}

impl<B: Backend> Clone for RendererMaterialValue<B> {
  fn clone(&self) -> Self {
    match self {
      Self::Float(val) => Self::Float(*val),
      Self::Vec4(val) => Self::Vec4(*val),
      Self::Texture(tex) => Self::Texture(tex.clone())
    }
  }
}

impl<B: Backend> PartialOrd for RendererMaterialValue<B> {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl<B: Backend> Ord for RendererMaterialValue<B> {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    match (self, other) {
      (RendererMaterialValue::Float(val1), RendererMaterialValue::Float(val2)) => ((val1 * 100f32) as u32).cmp(&((val2 * 100f32) as u32)),
      (RendererMaterialValue::Float(_), RendererMaterialValue::Texture(_)) => std::cmp::Ordering::Less,
      (RendererMaterialValue::Float(_), RendererMaterialValue::Vec4(_)) => std::cmp::Ordering::Less,
      (RendererMaterialValue::Texture(_), RendererMaterialValue::Float(_)) => std::cmp::Ordering::Greater,
      (RendererMaterialValue::Texture(_), RendererMaterialValue::Vec4(_)) => std::cmp::Ordering::Greater,
      (RendererMaterialValue::Texture(tex1), RendererMaterialValue::Texture(tex2)) => (tex1.view.as_ref() as *const B::TextureShaderResourceView).cmp(&(tex2.view.as_ref() as *const B::TextureShaderResourceView)),
      (RendererMaterialValue::Vec4(val1), RendererMaterialValue::Vec4(val2)) => ((val1.x * 100f32) as u32).cmp(&((val2.x * 100f32) as u32))
                                                                                                                                      .then(((val1.y * 100f32) as u32).cmp(&((val2.y * 100f32) as u32)))
                                                                                                                                      .then(((val1.z * 100f32) as u32).cmp(&((val2.z * 100f32) as u32)))
                                                                                                                                      .then(((val1.w * 100f32) as u32).cmp(&((val2.w * 100f32) as u32))),
      (RendererMaterialValue::Vec4(_), RendererMaterialValue::Texture(_)) => std::cmp::Ordering::Less,
      (RendererMaterialValue::Vec4(_), RendererMaterialValue::Float(_)) => std::cmp::Ordering::Greater,
    }
  }
}

impl<B: Backend> PartialEq for RendererMaterial<B> {
  fn eq(&self, other: &Self) -> bool {
    if self.shader_name != other.shader_name {
      return false;
    }
    for (key, value) in self.properties.iter() {
      if other.properties.get(key) != Some(value) {
        return false;
      }
    }
    true
  }
}

impl<B: Backend> RendererMaterial<B> {
  pub fn new_pbr(albedo_texture: &Arc<RendererTexture<B>>) -> Self {
    let mut props = HashMap::new();
    props.insert("albedo".to_string(), RendererMaterialValue::Texture(albedo_texture.clone()));
    Self {
      shader_name: "pbr".to_string(),
      properties: props
    }
  }

  pub fn get(&self, key: &str) -> Option<&RendererMaterialValue<B>> {
    self.properties.get(key)
  }
}

impl<B: Backend> Eq for RendererMaterial<B> {}

impl<B: Backend> PartialOrd for RendererMaterial<B> {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl<B: Backend> Ord for RendererMaterial<B> {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    let mut last_result = self.shader_name.cmp(&other.shader_name)
    .then(self.properties.len().cmp(&other.properties.len()));

    if last_result != std::cmp::Ordering::Equal {
      return last_result;
    }

    for (key, value) in &self.properties {
      let other_val = other.properties.get(key);
      if let Some(other_val) = other_val {
        last_result = value.cmp(other_val);
        if last_result != std::cmp::Ordering::Equal {
          return last_result;
        }
      }
    }
    std::cmp::Ordering::Equal
  }
}

pub struct RendererModel<B: Backend> {
  inner: AtomicRefCell<RendererModelInner<B>>
}

struct RendererModelInner<B: Backend> {
  mesh: Arc<RendererMesh<B>>,
  materials: Box<[Arc<RendererMaterial<B>>]>
}

impl<B: Backend> RendererModel<B> {
  pub fn new(mesh: &Arc<RendererMesh<B>>, materials: Box<[Arc<RendererMaterial<B>>]>) -> Self {
    Self {
      inner: AtomicRefCell::new(RendererModelInner::<B> {
        mesh: mesh.clone(),
        materials
      })
    }
  }

  pub fn mesh(&self) -> AtomicRef<Arc<RendererMesh<B>>> {
    AtomicRef::map(self.inner.borrow(), |inner| &inner.mesh)
  }

  pub fn materials(&self) -> AtomicRef<Box<[Arc<RendererMaterial<B>>]>> {
    AtomicRef::map(self.inner.borrow(), |inner| &inner.materials)
  }
}

pub struct RendererMesh<B: Backend> {
  pub vertices: Arc<B::Buffer>,
  pub indices: Option<Arc<B::Buffer>>,
  pub parts: Box<[MeshRange]>,
  pub bounding_box: Option<BoundingBox>,
  pub vertex_count: u32,
}


struct DelayedAsset<B: Backend> {
  fence: Arc<B::Fence>,
  path: String,
  asset: DelayedAssetType<B>
}
enum DelayedAssetType<B: Backend> {
  TextureView(Arc<B::TextureShaderResourceView>)
}

pub(super) struct RendererAssets<P: Platform> {
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  models: HashMap<String, Arc<RendererModel<P::GraphicsBackend>>>,
  meshes: HashMap<String, Arc<RendererMesh<P::GraphicsBackend>>>,
  materials: HashMap<String, Arc<RendererMaterial<P::GraphicsBackend>>>,
  textures: HashMap<String, Arc<RendererTexture<P::GraphicsBackend>>>,
  texture_usages: HashMap<String, Vec<(String, String)>>,
  material_usages: HashMap<String, Vec<(String, usize)>>,
  zero_texture: Arc<RendererTexture<P::GraphicsBackend>>,
  zero_texture_black: Arc<RendererTexture<P::GraphicsBackend>>,
  delayed_assets: Vec<DelayedAsset<P::GraphicsBackend>>
}

impl<P: Platform> RendererAssets<P> {
  pub(super) fn new(device: &Arc<<P::GraphicsBackend as Backend>::Device>) -> Self {
    let zero_data = [255u8; 16];
    let zero_buffer = device.upload_data(&zero_data, MemoryUsage::CpuOnly, BufferUsage::COPY_SRC);
    let zero_texture = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
      width: 2,
      height: 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST
    }, Some("AssetManagerZeroTexture"));
    device.init_texture(&zero_texture, &zero_buffer, 0, 0);
    let zero_view = device.create_shader_resource_view(&zero_texture, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1
    });
    let zero_index = if device.supports_bindless() {
      Some(device.insert_texture_into_bindless_heap(&zero_view))
    } else {
      None
    };
    let zero_rtexture = Arc::new(RendererTexture {
      view: zero_view,
      bindless_index: zero_index
    });

    let zero_data_black = [0u8, 0u8, 0u8, 255u8, 0u8, 0u8, 0u8, 255u8, 0u8, 0u8, 0u8, 255u8, 0u8, 0u8, 0u8, 255u8];
    let zero_buffer_black = device.upload_data(&zero_data_black, MemoryUsage::CpuOnly, BufferUsage::COPY_SRC);
    let zero_texture_black = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
      width: 2,
      height: 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST
    }, Some("AssetManagerZeroTextureBlack"));
    device.init_texture(&zero_texture_black, &zero_buffer_black, 0, 0);
    let zero_view_black = device.create_shader_resource_view(&zero_texture_black, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1
    });
    let zero_black_index = if device.supports_bindless() {
      Some(device.insert_texture_into_bindless_heap(&zero_view_black))
    } else {
      None
    };
    let zero_rtexture_black = Arc::new(RendererTexture {
      view: zero_view_black,
      bindless_index: zero_black_index
    });
    device.flush_transfers();

    Self {
      device: device.clone(),
      models: HashMap::new(),
      meshes: HashMap::new(),
      materials: HashMap::new(),
      textures: HashMap::new(),
      material_usages: HashMap::new(),
      texture_usages: HashMap::new(),
      zero_texture: zero_rtexture,
      zero_texture_black: zero_rtexture_black,
      delayed_assets: Vec::new()
    }
  }

  pub fn integrate_texture(&mut self, texture_path: &str, texture: &Arc<<P::GraphicsBackend as Backend>::TextureShaderResourceView>) -> Arc<RendererTexture<P::GraphicsBackend>> {
    let bindless_index = if self.device.supports_bindless() {
      Some(self.device.insert_texture_into_bindless_heap(&texture))
    } else {
      None
    };
    let renderer_texture = Arc::new(RendererTexture {
      view: texture.clone(),
      bindless_index
    });
    self.textures.insert(texture_path.to_owned(), renderer_texture.clone());

    if let Some(usages) = self.texture_usages.get(texture_path) {
      for (material_name, prop_name) in usages.iter() {
        let mut new_material = self.materials.get(material_name).unwrap().as_ref().clone();
        new_material.properties.insert(prop_name.clone(), RendererMaterialValue::Texture(renderer_texture.clone()));
        let mat_arc = Arc::new(new_material);
        self.materials.insert(material_name.clone(), mat_arc.clone());

        if let Some(usages) = self.material_usages.get(material_name) {
          for (model_name, index) in usages.iter() {
            let model = self.models.get(model_name).unwrap();
            let mut inner = model.inner.borrow_mut();
            inner.materials[*index] = mat_arc.clone();
          }
        }
      }
    }

    renderer_texture
  }

  pub fn integrate_mesh(&mut self, mesh_path: &str, mesh: Mesh) {
    let vb_name = mesh_path.to_string() + "_vertices";
    let ib_name = mesh_path.to_string() + "_indices";

    assert_ne!(mesh.vertex_count, 0);

    let vertex_buffer = self.device.create_buffer(&BufferInfo {
      size: std::mem::size_of_val(&mesh.vertices[..]),
      usage: BufferUsage::COPY_DST | BufferUsage::VERTEX
     }, MemoryUsage::GpuOnly, Some(&vb_name));
    let temp_vertex_buffer = self.device.upload_data(&mesh.vertices[..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    self.device.init_buffer(&temp_vertex_buffer, &vertex_buffer);
    let index_buffer = mesh.indices.map(|indices| {
      let buffer = self.device.create_buffer(
      &BufferInfo {
        size: std::mem::size_of_val(&indices[..]),
        usage: BufferUsage::COPY_DST | BufferUsage::INDEX
      },MemoryUsage::GpuOnly, Some(&ib_name));
      let temp_buffer = self.device.upload_data(&indices[..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
      self.device.init_buffer(&temp_buffer, &buffer);
      buffer
    });

    let mesh = Arc::new(RendererMesh {
      vertices: vertex_buffer,
      indices: index_buffer,
      parts: mesh.parts.iter().cloned().collect(), // TODO: change base type to boxed slice
      bounding_box: mesh.bounding_box,
      vertex_count: mesh.vertex_count
    });
    self.meshes.insert(mesh_path.to_owned(), mesh);
  }

  pub fn upload_texture(&mut self, texture_path: &str, texture: Texture, do_async: bool) -> (Arc<<P::GraphicsBackend as Backend>::TextureShaderResourceView>, Option<Arc<<P::GraphicsBackend as Backend>::Fence>>) {
    let gpu_texture = self.device.create_texture(&texture.info, Some(texture_path));
    let subresources = texture.info.array_length * texture.info.mip_levels;
    let mut fence = Option::<Arc<<P::GraphicsBackend as Backend>::Fence>>::None;
    for subresource in 0..subresources {
      let mip_level = subresource % texture.info.mip_levels;
      let array_index = subresource / texture.info.array_length;
      let init_buffer = self.device.upload_data(
        &texture.data[subresource as usize][..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
      if do_async {
        fence = self.device.init_texture_async(&gpu_texture, &init_buffer, mip_level, array_index);
      } else {
        self.device.init_texture(&gpu_texture, &init_buffer, mip_level, array_index);
      }
    }
    let view = self.device.create_shader_resource_view(
      &gpu_texture, &TextureShaderResourceViewInfo {
          base_mip_level: 0,
          mip_level_length: texture.info.mip_levels,
          base_array_level: 0,
          array_level_length: texture.info.array_length
    });

    (view, fence)
  }

  pub fn integrate_material(&mut self, material_path: &str, material: &Material) -> Arc<RendererMaterial<P::GraphicsBackend>> {
    let mut properties = HashMap::<String, RendererMaterialValue<P::GraphicsBackend>>::with_capacity(material.properties.len());
    for (key, value) in &material.properties {
      match value {
        MaterialValue::Texture(path) => {
          let texture = self.textures.get(path)
            .cloned()
            .or_else(|| {
              let zero_view = self.zero_texture.view.clone();
              Some(self.integrate_texture(path, &zero_view))
            }).unwrap();

          self.texture_usages.entry(path.to_string())
            .or_default()
            .push((material_path.to_string(), key.to_string()));

          properties.insert(key.to_string(), RendererMaterialValue::Texture(texture));
        }

        MaterialValue::Float(val) => {
          properties.insert(key.to_string(), RendererMaterialValue::Float(*val));
        }

        MaterialValue::Vec4(val) => {
          properties.insert(key.to_string(), RendererMaterialValue::Vec4(*val));
        }
      }
    }

    let renderer_material = Arc::new(RendererMaterial {
      shader_name: material.shader_name.clone(),
      properties
    });
    self.materials.insert(material_path.to_owned(), renderer_material.clone());
    renderer_material
  }

  pub fn integrate_model(&mut self, model_path: &str, model: &Model) -> Option<Arc<RendererModel<P::GraphicsBackend>>> {
    let mesh = self.meshes.get(&model.mesh_path).cloned()?;
    let mut renderer_materials = Vec::<Arc<RendererMaterial<P::GraphicsBackend>>>::new();
    for material in &model.material_paths {
      let renderer_material = self.materials.get(material).cloned()
        .or_else(|| {
        Some(self.integrate_material(material, &Material::new_pbr("NULL", 0f32, 0f32)))
      }).unwrap();
      renderer_materials.push(renderer_material.clone());

      self.material_usages.entry(material.clone())
        .or_default()
        .push((model_path.to_string(), renderer_materials.len() - 1));
    }

    let renderer_model = Arc::new(RendererModel::new(&mesh, renderer_materials.into_boxed_slice()));
    self.models.insert(model_path.to_owned(), renderer_model.clone());
    Some(renderer_model)
  }

  pub fn get_model(&self, model_path: &str) -> Arc<RendererModel<P::GraphicsBackend>> {
    self.models.get(model_path)
      .cloned()
      .unwrap_or_else(|| panic!("Model not yet loaded: {}", model_path))
  }

  pub fn get_texture(&self, texture_path: &str) -> Arc<RendererTexture<P::GraphicsBackend>> {
    self.textures.get(texture_path)
      .cloned()
      .unwrap_or_else(|| panic!("Texture not yet loaded: {}", texture_path))
  }

  pub fn placeholder_texture(&self) -> &Arc<RendererTexture<P::GraphicsBackend>> {
    &self.zero_texture
  }

  pub fn placeholder_black(&self) -> &Arc<RendererTexture<P::GraphicsBackend>> {
    &self.zero_texture_black
  }

  pub fn insert_placeholder_texture(&mut self, texture_path: &str, black: bool) -> Arc<RendererTexture<P::GraphicsBackend>> {
    if self.textures.contains_key(texture_path) {
      return self.textures.get(texture_path).unwrap().clone();
    }

    let texture = Arc::new(RendererTexture {
      view: if black { self.zero_texture.view.clone() } else { self.zero_texture_black.view.clone() },
      bindless_index: if black { self.zero_texture.bindless_index } else { self.zero_texture_black.bindless_index },
    });
    self.textures.insert(texture_path.to_string(), texture.clone());
    texture
  }

  pub(super) fn receive_assets(&mut self, asset_manager: &AssetManager<P>) {
    let mut retained_delayed_assets = Vec::<DelayedAsset<P::GraphicsBackend>>::new();
    let mut ready_delayed_assets = Vec::<DelayedAsset<P::GraphicsBackend>>::new();
    for delayed_asset in self.delayed_assets.drain(..) {
      if delayed_asset.fence.is_signaled() {
        ready_delayed_assets.push(delayed_asset);
      } else {
        retained_delayed_assets.push(delayed_asset);
      }
    }
    self.delayed_assets.extend(retained_delayed_assets);

    for delayed_asset in ready_delayed_assets.drain(..) {
      match &delayed_asset.asset {
        DelayedAssetType::TextureView(view) => {
          self.integrate_texture(&delayed_asset.path, view);
        }
      }
    }

    let mut asset_opt = asset_manager.receive_render_asset();
    while asset_opt.is_some() {
      let asset = asset_opt.unwrap();
      match asset.asset {
        Asset::Material(material) => { self.integrate_material(&asset.path, &material); }
        Asset::Model(model) => { self.integrate_model(&asset.path, &model); }
        Asset::Mesh(mesh) => { self.integrate_mesh(&asset.path, mesh); }
        Asset::Texture(texture) => {
          let do_async = asset.priority == AssetLoadPriority::Low;
          let (view, fence) = self.upload_texture(&asset.path, texture, do_async);
          if let Some(fence) = fence {
            self.delayed_assets.push(DelayedAsset {
              fence,
              path: asset.path.to_string(),
              asset: DelayedAssetType::TextureView(view)
            });
          } else {
            self.integrate_texture(&asset.path, &view);
          }
        }
        _ => unimplemented!()
      }
      asset_opt = asset_manager.receive_render_asset();
    }

    // Make sure the work initializing the resources actually gets submitted
    self.device.flush_transfers();
  }
}
