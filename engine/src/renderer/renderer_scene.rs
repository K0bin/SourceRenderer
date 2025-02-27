use std::collections::HashMap;
use std::usize;

use bevy_ecs::entity::Entity;
use log::warn;
use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::Vec3;
use bevy_math::Affine3A;

use crate::asset::TextureHandle;

use super::drawable::View;
use super::light::{
    DirectionalLight,
    RendererDirectionalLight,
    RendererPointLight,
};
use super::{
    PointLight,
    RendererStaticDrawable,
};

pub struct RendererScene<B: GPUBackend> {
    views: Vec<View>,
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
            views: vec![View::default()],
            static_meshes: Vec::new(),
            point_lights: Vec::new(),
            directional_lights: Vec::new(),
            drawable_entity_map: HashMap::new(),
            point_light_entity_map: HashMap::new(),
            directional_light_entity_map: HashMap::new(),
            lightmap: None,
        }
    }

    #[inline(always)]
    pub fn main_view(&self) -> &View {
        &self.views[0]
    }

    #[inline(always)]
    pub fn main_view_mut(&mut self) -> &mut View {
        &mut self.views[0]
    }

    #[inline(always)]
    pub fn views(&self) -> &[View] {
        &self.views
    }

    #[inline(always)]
    pub fn views_mut(&mut self) -> &mut [View] {
        &mut self.views
    }

    #[inline(always)]
    pub fn static_drawables(&self) -> &[RendererStaticDrawable] {
        &self.static_meshes[..]
    }

    #[inline(always)]
    pub fn point_lights(&self) -> &[RendererPointLight<B>] {
        &self.point_lights
    }

    #[inline(always)]
    pub fn directional_lights(&self) -> &[RendererDirectionalLight<B>] {
        &self.directional_lights
    }

    #[inline(always)]
    pub fn view_update_info(&mut self) -> (&mut [View], &[RendererStaticDrawable], &[RendererPointLight<B>], &[RendererDirectionalLight<B>]) {
        (&mut self.views, &self.static_meshes, &self.point_lights, &self.directional_lights)
    }

    pub fn add_static_drawable(&mut self, entity: Entity, static_drawable: RendererStaticDrawable) {
        debug_assert!(self.drawable_entity_map.get(&entity).is_none());
        if cfg!(debug_assertions) {
            for (_entity, index) in &self.drawable_entity_map {
                debug_assert_ne!(*index, self.static_meshes.len());
            }
        }
        debug_assert_eq!(self.drawable_entity_map.len(), self.static_meshes.len());

        self.drawable_entity_map
            .insert(entity, self.static_meshes.len());
        self.static_meshes.push(static_drawable);
    }

    pub fn remove_static_drawable(&mut self, entity: &Entity) {
        let index = self.drawable_entity_map.remove(entity);
        debug_assert!(index.is_some());
        if index.is_none() {
            return;
        }
        let index = index.unwrap();
        self.static_meshes.remove(index);
        debug_assert_eq!(self.drawable_entity_map.len(), self.static_meshes.len());
    }

    pub fn update_transform(&mut self, entity: &Entity, transform: Affine3A) {
        let index = self.drawable_entity_map.get(entity);
        if let Some(index) = index {
            let static_drawable = &mut self.static_meshes[*index];
            static_drawable.transform = transform;
            return;
        }

        let index = self.point_light_entity_map.get(entity);
        if let Some(index) = index {
            let point_light = &mut self.point_lights[*index];
            point_light.position = transform.transform_point3(Vec3::new(0f32, 0f32, 0f32));
            return;
        }

        let index = self.directional_light_entity_map.get(entity);
        if let Some(index) = index {
            let point_light = &mut self.directional_lights[*index];
            point_light.direction = transform.transform_vector3(Vec3::new(0f32, 0f32, 1f32));
            return;
        }

        warn!("Found no entity on the renderer for ecs entity: {:?}", entity);

        debug_assert!(false); // debug unreachable
    }

    pub fn add_point_light(&mut self, entity: Entity, light: PointLight) {
        debug_assert!(self.point_light_entity_map.get(&entity).is_none());
        if cfg!(debug_assertions) {
            for (_entity, index) in &self.point_light_entity_map {
                debug_assert_ne!(*index, self.point_lights.len());
            }
        }
        debug_assert_eq!(self.point_light_entity_map.len(), self.point_lights.len());

        self.point_light_entity_map
            .insert(entity, self.point_lights.len());
        let renderer_point_light = RendererPointLight::new(light.position, light.intensity);
        self.point_lights.push(renderer_point_light);
    }

    pub fn remove_point_light(&mut self, entity: &Entity) {
        let index = self.point_light_entity_map.remove(entity);
        debug_assert!(index.is_some());
        if index.is_none() {
            return;
        }
        let index = index.unwrap();
        self.point_lights.remove(index);
        debug_assert_eq!(self.point_light_entity_map.len(), self.point_lights.len());
    }

    pub fn add_directional_light(&mut self, entity: Entity, light: DirectionalLight) {
        debug_assert!(self.directional_light_entity_map.get(&entity).is_none());
        if cfg!(debug_assertions) {
            for (_entity, index) in &self.directional_light_entity_map {
                debug_assert_ne!(*index, self.directional_lights.len());
            }
        }
        debug_assert_eq!(self.directional_light_entity_map.len(), self.directional_lights.len());

        self.directional_light_entity_map
            .insert(entity, self.point_lights.len());
        let renderer_directional_light =
            RendererDirectionalLight::new(light.direction, light.intensity);
        self.directional_lights.push(renderer_directional_light);
    }

    pub fn remove_directional_light(&mut self, entity: &Entity) {
        let index = self.directional_light_entity_map.remove(entity);
        debug_assert!(index.is_some());
        if index.is_none() {
            return;
        }
        let index = index.unwrap();
        self.point_lights.remove(index);
        debug_assert_eq!(self.directional_light_entity_map.len(), self.directional_lights.len());
    }

    #[inline(always)]
    pub fn set_lightmap(&mut self, lightmap: Option<TextureHandle>) {
        self.lightmap = lightmap;
    }

    #[inline(always)]
    pub fn lightmap(&self) -> Option<TextureHandle> {
        self.lightmap
    }
}
