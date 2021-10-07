use std::sync::Weak;
use std::{cmp::Ordering, sync::Arc};
use std::collections::HashMap;

use sourcerenderer_core::graphics::{Backend, BufferInfo, Device, Fence, TextureUsage};
use crate::{asset::{Asset, AssetManager, Material, Mesh, Model, Texture, AssetLoadPriority, MeshRange}, math::BoundingBox};
use sourcerenderer_core::Platform;
use sourcerenderer_core::graphics::{ TextureInfo, MemoryUsage, SampleCount, Format, TextureShaderResourceViewInfo, BufferUsage };

use sourcerenderer_core::atomic_refcell::AtomicRefCell;

pub(super) struct RendererTexture<B: Backend> {
  pub(super) view: Arc<B::TextureShaderResourceView>
}

impl<B: Backend> PartialEq for RendererTexture<B> {
  fn eq(&self, other: &Self) -> bool {
    self.view == other.view
  }
}

impl<B: Backend> Eq for RendererTexture<B> {}

pub(super) enum RendererMaterial<B: Backend> {
  PBR {
    albedo: Arc<RendererTexture<B>>,
    metalness_map: Arc<RendererTexture<B>>,
    roughness_map: Arc<RendererTexture<B>>,
    normal_map: Arc<RendererTexture<B>>,
    metalness: f32,
    roughness: f32
  },
  Blended {
    material1: Arc<RendererMaterial<B>>,
    material2: Arc<RendererMaterial<B>>
  }
}

impl<B: Backend> PartialEq for RendererMaterial<B> {
  fn eq(&self, other: &Self) -> bool {
    match self {
      RendererMaterial::PBR { albedo, metalness, roughness, metalness_map, roughness_map, normal_map } => {
        if let RendererMaterial::PBR {albedo: other_albedo, metalness: other_metalness, roughness: other_roughness, metalness_map: other_metalness_map, roughness_map: other_roughness_map, normal_map: other_normal_map} = other {
          albedo.view == other_albedo.view
            && metalness == other_metalness
            && roughness == other_roughness
            && metalness_map.view == other_metalness_map.view
            && roughness_map.view == other_roughness_map.view
        } else {
          false
        }
      },
      RendererMaterial::Blended { material1, material2 } => {
        if let RendererMaterial::Blended { material1: other_material1, material2: other_material2 } = other {
          material1 == other_material1 && material2 == other_material2
        } else {
          false
        }
      }
    }
  }
}

impl<B: Backend> Eq for RendererMaterial<B> {}

impl<B: Backend> Ord for RendererMaterial<B> {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    match self {
      RendererMaterial::PBR { albedo, metalness_map, roughness_map, normal_map, metalness, roughness } => {
        if let RendererMaterial::PBR {albedo: other_albedo, metalness: other_metalness, roughness: other_roughness, metalness_map: other_metalness_map, roughness_map: other_roughness_map, normal_map: other_normal_map} = other {
            (albedo.view.as_ref() as *const B::TextureShaderResourceView).cmp(&(other_albedo.view.as_ref() as *const B::TextureShaderResourceView))
            .then((metalness_map.view.as_ref() as *const B::TextureShaderResourceView).cmp(&(metalness_map.view.as_ref() as *const B::TextureShaderResourceView)))
            .then((roughness_map.view.as_ref() as *const B::TextureShaderResourceView).cmp(&(roughness_map.view.as_ref() as *const B::TextureShaderResourceView)))
            .then((normal_map.view.as_ref() as *const B::TextureShaderResourceView).cmp(&(normal_map.view.as_ref() as *const B::TextureShaderResourceView)))
            .then(((roughness * 100f32) as u32).cmp(&((other_roughness * 100f32) as u32)))
            .then(((metalness * 100f32) as u32).cmp(&((other_metalness * 100f32) as u32)))
        } else {
          Ordering::Less
        }
      }
      RendererMaterial::Blended { material1, material2 } => {
        if let RendererMaterial::Blended { material1: other_material1, material2: other_material2 } = other {
            material1.cmp(other_material1)
            .then(material2.cmp(other_material2))
        } else {
          Ordering::Greater
        }
      }
    }
  }
}

impl<B: Backend> PartialOrd for RendererMaterial<B> {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
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
  material_usages: HashMap<String, Vec<Weak<RendererModel<P::GraphicsBackend>>>>,
  texture_usages: HashMap<String, Vec<Weak<RendererMaterial<P::GraphicsBackend>>>>,
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
      samples: SampleCount::Samples1,
      usage: TextureUsage::VERTEX_SHADER_SAMPLED | TextureUsage::FRAGMENT_SHADER_SAMPLED | TextureUsage::COMPUTE_SHADER_SAMPLED | TextureUsage::COPY_DST
    }, Some("AssetManagerZeroTexture"));
    device.init_texture(&zero_texture, &zero_buffer, 0, 0);
    let zero_view = device.create_shader_resource_view(&zero_texture, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1
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
      zero_view,
      delayed_assets: Vec::new()
    }
  }

  pub fn integrate_texture(&mut self, texture_path: &str, texture: &Arc<<P::GraphicsBackend as Backend>::TextureShaderResourceView>) -> Arc<RendererTexture<P::GraphicsBackend>> {
    let renderer_texture = Arc::new(RendererTexture {
      view: texture.clone()
    });
    if let Some(texture_materials) = self.texture_usages.get(texture_path) {
      for material_weak in texture_materials {
        let material_opt = material_weak.upgrade();
        if material_opt.is_none() {
          continue;
        }
        let old_texture = self.textures.get(texture_path).unwrap().clone();
        let material = material_opt.unwrap();
        self.rebuild_material(&material, &old_texture, &Arc::new(RendererTexture {
          view: texture.clone()
        }));
        break;
      }
    }
    self.textures.insert(texture_path.to_owned(), renderer_texture.clone());
    renderer_texture
  }

  fn rebuild_material(&mut self, material: &Arc<RendererMaterial<P::GraphicsBackend>>, old_texture: &Arc<RendererTexture<P::GraphicsBackend>>, new_texture: &Arc<RendererTexture<P::GraphicsBackend>>) -> Arc<RendererMaterial<P::GraphicsBackend>> {
    match material.as_ref() {
      RendererMaterial::PBR { albedo, metalness_map, roughness_map, normal_map, metalness, roughness } => {
        let mut albedo = albedo.clone();
        let mut metalness_map = metalness_map.clone();
        let mut roughness_map = roughness_map.clone();
        let mut normal_map = normal_map.clone();
        if &albedo == old_texture {
          albedo = new_texture.clone();
        } else if &metalness_map == old_texture {
          metalness_map = new_texture.clone();
        } else if &roughness_map == old_texture {
          roughness_map = new_texture.clone();
        } else if &normal_map == old_texture {
          normal_map = new_texture.clone();
        }
        Arc::new(RendererMaterial::PBR {
          albedo,
          metalness: *metalness,
          metalness_map,
          roughness: *roughness,
          roughness_map,
          normal_map
        })
      },
      RendererMaterial::Blended { material1, material2 } => unreachable!(),
    }
  }

  pub fn integrate_mesh(&mut self, mesh_path: &str, mesh: Mesh) {
    let vb_name = mesh_path.to_string() + "_vertices";
    let ib_name = mesh_path.to_string() + "_indices";

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
          array_level_length: texture.info.array_length
    });

    (view, fence)
  }

  pub fn integrate_material(&mut self, material_path: &str, material: &Material) -> Arc<RendererMaterial<P::GraphicsBackend>> {
    let existing_material = self.materials.get(material_path);
    if let Some(existing_material) = existing_material {
      return existing_material.clone();
    }

    match material {
      Material::PBR { albedo_texture_path, metalness, roughness, metalness_texture_path, roughness_texture_path, normal_map } => {
        let albedo = self.textures.get(albedo_texture_path)
        .cloned()
        .or_else(|| {
          let zero_view = self.zero_view.clone();
          Some(self.integrate_texture(albedo_texture_path, &zero_view))
        }).unwrap();
        let metalness_map = self.textures.get(metalness_texture_path)
        .cloned()
        .or_else(|| {
          let zero_view = self.zero_view.clone();
          Some(self.integrate_texture(metalness_texture_path, &zero_view))
        }).unwrap();
        let roughness_map = self.textures.get(roughness_texture_path)
        .cloned()
        .or_else(|| {
          let zero_view = self.zero_view.clone();
          Some(self.integrate_texture(roughness_texture_path, &zero_view))
        }).unwrap();
        let normal_map_texture = self.textures.get(normal_map)
        .cloned()
        .or_else(|| {
          let zero_view = self.zero_view.clone();
          Some(self.integrate_texture(normal_map, &zero_view))
        }).unwrap();

        let renderer_material = Arc::new(RendererMaterial::PBR {
          albedo,
          metalness_map,
          roughness_map,
          normal_map: normal_map_texture,
          metalness: *metalness,
          roughness: *roughness,
        });
        self.materials.insert(material_path.to_owned(), renderer_material.clone());
        return renderer_material;
      }
      Material::BlendedMaterial {
        ..
      } => {
        unimplemented!()
      }
    }
  }

  pub fn integrate_model(&mut self, model_path: &str, model: &Model) -> Option<Arc<RendererModel<P::GraphicsBackend>>> {
    let mesh = self.meshes.get(&model.mesh_path).cloned()?;
    let mut renderer_materials = Vec::<Arc<RendererMaterial<P::GraphicsBackend>>>::new();
    for material in &model.material_paths {
      let renderer_material = self.materials.get(material).cloned()
        .or_else(|| {
        Some(self.integrate_material(material, &Material::PBR {
          albedo_texture_path: "NULL".to_string(),
          normal_map: "NULL".to_string(),
          roughness_texture_path: "NULL".to_string(),
          metalness_texture_path: "NULL".to_string(),
          metalness: 0.0f32,
          roughness: 1.0f32
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
      view: self.zero_view.clone()
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
