use std::{collections::HashMap, usize};

use legion::Entity;
use sourcerenderer_core::{Matrix4, graphics::Backend};

use super::{PointLight, RendererStaticDrawable};

pub struct RendererScene<B: Backend> {
  static_meshes: Vec<RendererStaticDrawable<B>>,
  point_lights: Vec<PointLight>,
  drawable_entity_map: HashMap<Entity, usize>,
  light_entity_map: HashMap<Entity, usize>
}

impl<B: Backend> RendererScene<B> {
  pub fn new() -> Self {
    Self {
      static_meshes: Vec::new(),
      point_lights: Vec::new(),
      drawable_entity_map: HashMap::new(),
      light_entity_map: HashMap::new()
    }
  }

  pub(super) fn static_drawables(&self) -> &[RendererStaticDrawable<B>] {
    &self.static_meshes[..]
  }

  pub(super) fn add_static_drawable(&mut self, entity: Entity, static_drawable: RendererStaticDrawable<B>) {
    self.drawable_entity_map.insert(entity, self.static_meshes.len());
    self.static_meshes.push(static_drawable);
  }

  pub(super) fn remove_static_drawable(&mut self, entity: &Entity) {
    let index = self.drawable_entity_map.get(&entity);
    debug_assert!(index.is_some());
    if index.is_none() {
      return;
    }
    let index = *index.unwrap();
    self.static_meshes.remove(index);
  }

  pub(super) fn update_transform(&mut self, entity: &Entity, transform: Matrix4) {
    let index = self.drawable_entity_map.get(&entity);
    debug_assert!(index.is_some());
    if index.is_none() {
      return;
    }
    let index = *index.unwrap();
    let static_drawable = &mut self.static_meshes[index];
    static_drawable.transform = transform;
  }

  pub(super) fn add_point_light(&mut self, _entity: Entity, light: PointLight) {
    self.point_lights.push(light);
  }
}