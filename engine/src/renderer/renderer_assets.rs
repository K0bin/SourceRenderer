use std::sync::Arc;
use std::collections::HashMap;

use sourcerenderer_core::graphics::{Backend, Device, Fence};
use crate::{asset::{Asset, AssetManager, Material, Mesh, Model, Texture, AssetLoadPriority, MeshRange}, math::BoundingBox};
use sourcerenderer_core::Platform;
use sourcerenderer_core::graphics::{ TextureInfo, MemoryUsage, SampleCount, Format, Filter, AddressMode, TextureShaderResourceViewInfo, BufferUsage };

use sourcerenderer_core::atomic_refcell::AtomicRefCell;

pub(super) struct RendererTexture<B: Backend> {
  pub(super) view: AtomicRefCell<Arc<B::TextureShaderResourceView>>
}

pub(super) struct RendererMaterial<B: Backend> {
  pub(super) albedo: AtomicRefCell<Arc<RendererTexture<B>>>
}

pub(super) struct RendererModel<B: Backend> {
  pub(super) mesh: Arc<RendererMesh<B>>,
  pub(super) materials: Box<[Arc<RendererMaterial<B>>]>
}

pub(super) struct RendererMesh<B: Backend> {
  pub(super) vertices: Arc<B::Buffer>,
  pub(super) indices: Option<Arc<B::Buffer>>,
  pub(super) parts: Box<[MeshRange]>,
  pub(super) bounding_box: Option<BoundingBox>
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
  zero_view: Arc<<P::GraphicsBackend as Backend>::TextureShaderResourceView>,
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
    device.flush_transfers();

    Self {
      device: device.clone(),
      models: HashMap::new(),
      meshes: HashMap::new(),
      materials: HashMap::new(),
      textures: HashMap::new(),
      zero_view,
      delayed_assets: Vec::new()
    }
  }

  pub fn integrate_texture(&mut self, texture_path: &str, texture: &Arc<<P::GraphicsBackend as Backend>::TextureShaderResourceView>) -> Arc<RendererTexture<P::GraphicsBackend>> {
    let existing_texture = self.textures.get(texture_path);
    if let Some(existing_texture) = existing_texture {
      *existing_texture.view.borrow_mut() = texture.clone();
      return existing_texture.clone();
    }

    let renderer_texture = Arc::new(RendererTexture {
      view: AtomicRefCell::new(texture.clone())
    });
    self.textures.insert(texture_path.to_owned(), renderer_texture.clone());
    renderer_texture
  }

  pub fn integrate_mesh(&mut self, mesh_path: &str, mesh: Mesh) {
    let vb_name = mesh_path.to_string() + "_vertices";
    let ib_name = mesh_path.to_string() + "_indices";

    let vertex_buffer = self.device.create_buffer(
      std::mem::size_of_val(&mesh.vertices[..]), MemoryUsage::GpuOnly, BufferUsage::COPY_DST | BufferUsage::VERTEX, Some(&vb_name));
    let temp_vertex_buffer = self.device.upload_data(&mesh.vertices[..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    self.device.init_buffer(&temp_vertex_buffer, &vertex_buffer);
    let index_buffer = mesh.indices.map(|indices| {
      let buffer = self.device.create_buffer(
      std::mem::size_of_val(&indices[..]), MemoryUsage::GpuOnly, BufferUsage::COPY_DST | BufferUsage::INDEX, Some(&ib_name));
      let temp_buffer = self.device.upload_data(&indices[..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
      self.device.init_buffer(&temp_buffer, &buffer);
      buffer
    });

    let mesh = Arc::new(RendererMesh {
      vertices: vertex_buffer,
      indices: index_buffer,
      parts: mesh.parts.into_iter().cloned().collect(), // TODO: change base type to boxed slice
      bounding_box: mesh.bounding_box
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
          array_level_length: texture.info.array_length,
          mag_filter: Filter::Linear,
          min_filter: Filter::Linear,
          mip_filter: Filter::Linear,
          address_mode_u: AddressMode::Repeat,
          address_mode_v: AddressMode::Repeat,
          address_mode_w: AddressMode::Repeat,
          mip_bias: 0f32,
          max_anisotropy: 0f32,
          compare_op: None,
          min_lod: 0f32,
          max_lod: 0f32,
    });

    (view, fence)
  }

  pub fn integrate_material(&mut self, material_path: &str, material: &Material) -> Arc<RendererMaterial<P::GraphicsBackend>> {
    let albedo = self.textures.get(&material.albedo_texture_path)
      .cloned()
      .or_else(|| {
        let zero_view = self.zero_view.clone();
        Some(self.integrate_texture(&material.albedo_texture_path, &zero_view))
      }).unwrap();

    let existing_material = self.materials.get(material_path);
    if let Some(existing_material) = existing_material {
      *existing_material.albedo.borrow_mut() = albedo;
      return existing_material.clone();
    }

    let renderer_material = Arc::new(RendererMaterial {
      albedo: AtomicRefCell::new(albedo)
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
        Some(self.integrate_material(material, &Material {
          albedo_texture_path: "NULL".to_string()
        }))
      }).unwrap();
      renderer_materials.push(renderer_material);
    }

    let renderer_model = Arc::new(RendererModel {
      materials: renderer_materials.into_boxed_slice(),
      mesh
    });
    self.models.insert(model_path.to_owned(), renderer_model.clone());
    Some(renderer_model)
  }

  pub fn get_model(&self, model_path: &str) -> Arc<RendererModel<P::GraphicsBackend>> {
    self.models.get(model_path)
      .cloned()
      .expect("Model not yet loaded")
  }

  pub fn get_texture(&self, texture_path: &str) -> Arc<RendererTexture<P::GraphicsBackend>> {
    self.textures.get(texture_path)
      .cloned()
      .expect("Texture not yet loaded")
  }

  pub fn insert_placeholder_texture(&mut self, texture_path: &str) -> Arc<RendererTexture<P::GraphicsBackend>> {
    if self.textures.contains_key(texture_path) {
      return self.textures.get(texture_path).unwrap().clone();
    }

    let texture = Arc::new(RendererTexture {
      view: AtomicRefCell::new(self.zero_view.clone())
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
