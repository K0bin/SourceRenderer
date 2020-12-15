use std::sync::Arc;
use std::collections::HashMap;

use sourcerenderer_core::graphics::Backend;
use crate::asset::{Mesh, Model, Material, AssetManager};
use sourcerenderer_core::Platform;

pub(super) struct RendererMaterial<B: Backend> {
  pub(super) albedo: Arc<B::TextureShaderResourceView>
}

pub(super) struct RendererModel<B: Backend> {
  pub(super) mesh: Arc<Mesh<B>>,
  pub(super) materials: Box<[Arc<RendererMaterial<B>>]>
}

pub(super) struct RendererAssets<P: Platform> {
  models: HashMap<String, Arc<RendererModel<P::GraphicsBackend>>>,
  meshes: HashMap<String, Arc<Mesh<P::GraphicsBackend>>>,
  materials: HashMap<String, Arc<RendererMaterial<P::GraphicsBackend>>>,
  textures: HashMap<String, Arc<<P::GraphicsBackend as Backend>::TextureShaderResourceView>>
}

impl<P: Platform> RendererAssets<P> {
  pub(super) fn new() -> Self {
    Self {
      models: HashMap::new(),
      meshes: HashMap::new(),
      materials: HashMap::new(),
      textures: HashMap::new()
    }
  }

  pub fn get_model(&mut self, asset_manager: &AssetManager<P>, model_path: &str) -> Arc<RendererModel<P::GraphicsBackend>> {
    if let Some(model) = self.models.get(model_path) {
      return model.clone();
    }

    let asset_model = asset_manager.get_model(model_path);

    let mesh = self.meshes.entry(asset_model.mesh_path.clone()).or_insert_with(|| {
      asset_manager.get_mesh(&asset_model.mesh_path).clone()
    }).clone();

    let materials_vec: Vec<Arc<RendererMaterial<P::GraphicsBackend>>> = asset_model.material_paths.iter().map(|material_path| {
      if let Some(material) = self.materials.get(material_path) {
        material.clone()
      } else {
        let asset_material = asset_manager.get_material(material_path);
        let albedo_texture = self.textures.entry(asset_material.albedo_texture_path.clone()).or_insert_with(||
          asset_manager.get_texture(&asset_material.albedo_texture_path).clone()
        );
        let material = Arc::new(RendererMaterial {
          albedo: albedo_texture.clone()
        });
        self.materials.insert(material_path.clone(), material.clone());
        material
      }
    }).map(|m| m.clone()).collect();
    let materials = materials_vec.into_boxed_slice();
    let model = Arc::new(RendererModel {
      mesh,
      materials
    });
    self.models.insert(model_path.to_string(), model.clone());
    model
  }
}
