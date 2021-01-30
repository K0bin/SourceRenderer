use std::sync::Arc;
use std::collections::HashMap;

use sourcerenderer_core::graphics::{Backend, Device, Fence};
use crate::asset::{Mesh, Model, Material, AssetManager, Asset, AssetLoadPriority};
use sourcerenderer_core::Platform;
use sourcerenderer_core::graphics::{ TextureInfo, MemoryUsage, SampleCount, Format, Filter, AddressMode, TextureShaderResourceViewInfo, BufferUsage };
use nalgebra::Vector4;

use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use std::option::Option::Some;

pub(super) struct RendererTexture<B: Backend> {
  pub(super) view: AtomicRefCell<Arc<B::TextureShaderResourceView>>
}

pub(super) struct RendererMaterial<B: Backend> {
  pub(super) albedo: AtomicRefCell<Arc<RendererTexture<B>>>
}

pub(super) struct RendererModel<B: Backend> {
  pub(super) mesh: Arc<Mesh<B>>,
  pub(super) materials: Box<[Arc<RendererMaterial<B>>]>
}

pub(super) struct RendererAssets<P: Platform> {
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  models: HashMap<String, Arc<RendererModel<P::GraphicsBackend>>>,
  meshes: HashMap<String, Arc<Mesh<P::GraphicsBackend>>>,
  materials: HashMap<String, Arc<RendererMaterial<P::GraphicsBackend>>>,
  textures: HashMap<String, Arc<RendererTexture<P::GraphicsBackend>>>,
  zero_view: Arc<<P::GraphicsBackend as Backend>::TextureShaderResourceView>,
  delayed_assets: Vec<(Arc<<P::GraphicsBackend as Backend>::Fence>, String, Asset<P>)>
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

  pub fn integrate_mesh(&mut self, mesh_path: &str, mesh: &Arc<Mesh<P::GraphicsBackend>>) -> Arc<Mesh<P::GraphicsBackend>> {
    self.meshes.insert(mesh_path.to_owned(), mesh.clone());
    mesh.clone()
  }

  pub fn integrate_material(&mut self, material_path: &str, material: &Material) -> Arc<RendererMaterial<P::GraphicsBackend>> {
    let albedo = self.textures.get(&material.albedo_texture_path)
      .map(|m| m.clone())
      .or_else(|| {
        let zero_view = self.zero_view.clone();
        Some(self.integrate_texture(&material.albedo_texture_path, &zero_view))
      }).unwrap();

    let existing_material = self.materials.get(material_path);
    if let Some(existing_material) = existing_material {
      *existing_material.albedo.borrow_mut() = albedo.clone();
      return existing_material.clone();
    }

    let renderer_material = Arc::new(RendererMaterial {
      albedo: AtomicRefCell::new(albedo)
    });
    self.materials.insert(material_path.to_owned(), renderer_material.clone());
    renderer_material
  }

  pub fn integrate_model(&mut self, model_path: &str, model: &Model) -> Option<Arc<RendererModel<P::GraphicsBackend>>> {
    let mesh = self.meshes.get(&model.mesh_path);
    if mesh.is_none() {
      return None;
    }
    let mesh = mesh.unwrap().clone();
    let mut renderer_materials = Vec::<Arc<RendererMaterial<P::GraphicsBackend>>>::new();
    for material in &model.material_paths {
      let renderer_material = self.materials.get(material)
        .map(|m| m.clone())
        .or_else(|| {
        Some(self.integrate_material(material, &Material {
          albedo_texture_path: "NULL".to_string()
        }))
      }).unwrap();
      renderer_materials.push(renderer_material);
    }

    let renderer_model = Arc::new(RendererModel {
      materials: renderer_materials.into_boxed_slice(),
      mesh: mesh.clone()
    });
    self.models.insert(model_path.to_owned(), renderer_model.clone());
    Some(renderer_model)
  }

  pub fn get_model(&self, model_path: &str) -> Arc<RendererModel<P::GraphicsBackend>> {
    self.models.get(model_path)
      .map(|m| m.clone())
      .expect("Model not yet loaded")
  }

  pub fn get_texture(&self, texture_path: &str) -> Arc<RendererTexture<P::GraphicsBackend>> {
    self.textures.get(texture_path)
      .map(|t| t.clone())
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
    let mut retained_delayed_assets = Vec::<(Arc<<P::GraphicsBackend as Backend>::Fence>, String, Asset<P>)>::new();
    let mut ready_delayed_assets = Vec::<(String, Asset<P>)>::new();
    for (fence, name, asset) in self.delayed_assets.drain(..) {
      if fence.is_signaled() {
        ready_delayed_assets.push((name, asset));
      } else {
        retained_delayed_assets.push((fence, name, asset));
      }
    }
    self.delayed_assets.extend(retained_delayed_assets);

    // has to be called after deciding which fences have gotten signalled
    // to avoid an unlikely timing problem where the fence isn't ready yet when we
    // flush the transfers but becomes signalled after that just in time for the renderer
    // to decide to use the associated assets
    self.device.flush_transfers();

    for (name, asset) in ready_delayed_assets {
      match &asset {
        Asset::Texture(texture) => { self.integrate_texture(&name, texture); }
        Asset::Material(material) => { self.integrate_material(&name, material); }
        Asset::Mesh(mesh) => { self.integrate_mesh(&name, mesh); }
        Asset::Model(model) => { self.integrate_model(&name, model); }
        _ => unimplemented!()
      }
    }

    let mut asset_opt = asset_manager.receive_render_asset();
    while asset_opt.is_some() {
      let asset = asset_opt.unwrap();
      let fence = asset.fence.as_ref();
      if fence.is_some() {
        // delay the adding of assets with an associated fence by at least one frame
        // to avoid the race mentioned above
        self.delayed_assets.push((fence.unwrap().clone(), asset.path.clone(), asset.asset));
        asset_opt = asset_manager.receive_render_asset();
        continue;
      }

      match &asset.asset {
        Asset::Texture(texture) => { self.integrate_texture(&asset.path, texture); }
        Asset::Material(material) => { self.integrate_material(&asset.path, material); }
        Asset::Mesh(mesh) => { self.integrate_mesh(&asset.path, mesh); }
        Asset::Model(model) => { self.integrate_model(&asset.path, model); }
        _ => unimplemented!()
      }
      asset_opt = asset_manager.receive_render_asset();
    }
  }
}
