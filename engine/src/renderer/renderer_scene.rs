use std::collections::HashMap;
use std::usize;

use legion::Entity;
use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::{
    Matrix4,
    Vec4,
};

use super::light::{
    DirectionalLight,
    RendererDirectionalLight,
    RendererPointLight,
};
use super::renderer_assets::TextureHandle;
use super::{
    PointLight,
    RendererStaticDrawable,
};

pub struct RendererScene<B: GPUBackend> {
    static_meshes: Vec<RendererStaticDrawable>,
    point_lights: Vec<RendererPointLight<B>>,
    directional_lights: Vec<RendererDirectionalLight<B>>,
    drawable_entity_map: HashMap<Entity, usize>,
    point_light_entity_map: HashMap<Entity, usize>,
    directional_light_entity_map: HashMap<Entity, usize>,
    lightmap: Option<TextureHandle>,
}

impl<B: GPUBackend> RendererScene<B> {
    pub fn new() -> Self {
        Self {
            static_meshes: Vec::new(),
            point_lights: Vec::new(),
            directional_lights: Vec::new(),
            drawable_entity_map: HashMap::new(),
            point_light_entity_map: HashMap::new(),
            directional_light_entity_map: HashMap::new(),
            lightmap: None,
        }
    }

    pub fn static_drawables(&self) -> &[RendererStaticDrawable] {
        &self.static_meshes[..]
    }

    pub fn point_lights(&self) -> &[RendererPointLight<B>] {
        &self.point_lights
    }

    pub fn directional_lights(&self) -> &[RendererDirectionalLight<B>] {
        &self.directional_lights
    }

    pub fn add_static_drawable(&mut self, entity: Entity, static_drawable: RendererStaticDrawable) {
        self.drawable_entity_map
            .insert(entity, self.static_meshes.len());
        self.static_meshes.push(static_drawable);
    }

    pub fn remove_static_drawable(&mut self, entity: &Entity) {
        let index = self.drawable_entity_map.get(entity);
        debug_assert!(index.is_some());
        if index.is_none() {
            return;
        }
        let index = *index.unwrap();
        self.static_meshes.remove(index);
    }

    pub fn update_transform(&mut self, entity: &Entity, transform: Matrix4) {
        let index = self.drawable_entity_map.get(entity);
        if let Some(index) = index {
            let static_drawable = &mut self.static_meshes[*index];
            static_drawable.transform = transform;
            return;
        }

        let index = self.point_light_entity_map.get(entity);
        if let Some(index) = index {
            let point_light = &mut self.point_lights[*index];
            point_light.position = (transform * Vec4::new(0f32, 0f32, 0f32, 1f32)).xyz();
            return;
        }

        debug_assert!(false); // debug unreachable
    }

    pub fn add_point_light(&mut self, entity: Entity, light: PointLight) {
        self.point_light_entity_map
            .insert(entity, self.point_lights.len());
        let renderer_point_light = RendererPointLight::new(light.position, light.intensity);
        self.point_lights.push(renderer_point_light);
    }

    pub fn remove_point_light(&mut self, entity: &Entity) {
        let index = self.point_light_entity_map.get(entity);
        debug_assert!(index.is_some());
        if index.is_none() {
            return;
        }
        let index = *index.unwrap();
        self.point_lights.remove(index);
    }

    pub fn add_directional_light(&mut self, entity: Entity, light: DirectionalLight) {
        self.point_light_entity_map
            .insert(entity, self.point_lights.len());
        let renderer_directional_light =
            RendererDirectionalLight::new(light.direction, light.intensity);
        self.directional_lights.push(renderer_directional_light);
    }

    pub fn remove_directional_light(&mut self, entity: &Entity) {
        let index = self.point_light_entity_map.get(entity);
        debug_assert!(index.is_some());
        if index.is_none() {
            return;
        }
        let index = *index.unwrap();
        self.point_lights.remove(index);
    }

    pub fn set_lightmap(&mut self, lightmap: Option<TextureHandle>) {
        self.lightmap = lightmap;
    }

    pub fn lightmap(&self) -> Option<TextureHandle> {
        self.lightmap
    }
}
