use std::cell::Cell;
use std::collections::HashMap;

use bevy_ecs::entity::Entity;
use bevy_math::Affine3A;
use dear_imgui_rs::FrameSnapshot;
use log::warn;
use sourcerenderer_core::Vec3;

use super::drawable::{RendererVolumeDrawable, View, VolumeDrawableTransparencyMode};
use super::light::{DirectionalLight, RendererDirectionalLight, RendererPointLight};
use super::{PointLight, RendererStaticDrawable};
use crate::asset::TextureHandle;

struct RendererEntityType<T> {
    entries: Vec<T>,
    map: HashMap<Entity, usize>,
}

impl<T> RendererEntityType<T> {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            map: HashMap::new(),
        }
    }

    fn add(&mut self, entity: Entity, data: T) {
        debug_assert!(self.map.get(&entity).is_none());
        if cfg!(debug_assertions) {
            for (_entity, index) in &self.map {
                debug_assert_ne!(*index, self.entries.len());
                debug_assert!(*index < self.entries.len());
            }
        }
        debug_assert_eq!(self.map.len(), self.entries.len());

        self.map.insert(entity, self.entries.len());
        self.entries.push(data);
    }

    fn remove(&mut self, entity: Entity) {
        // TODO: Revamp how we store all of it so this isn't necessary.

        let removed_index_opt = self.map.remove(&entity);
        debug_assert!(removed_index_opt.is_some());
        if removed_index_opt.is_none() {
            return;
        }
        let removed_index = removed_index_opt.unwrap();
        self.entries.remove(removed_index);

        for (_, entry_index) in &mut self.map {
            debug_assert_ne!(*entry_index, removed_index);
            if *entry_index > removed_index {
                *entry_index -= 1;
            }
            debug_assert!(*entry_index < self.entries.len());
        }
        debug_assert_eq!(self.map.len(), self.entries.len());
    }

    #[allow(dead_code)]
    fn get(&self, entity: Entity) -> Option<&T> {
        let index = self.map.get(&entity)?;
        Some(&self.entries[*index])
    }

    fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let index = self.map.get(&entity)?;
        Some(&mut self.entries[*index])
    }

    fn entries(&self) -> &[T] {
        &self.entries
    }
}

pub struct RendererScene {
    views: Vec<View>,
    static_meshes: RendererEntityType<RendererStaticDrawable>,
    point_lights: RendererEntityType<RendererPointLight>,
    directional_lights: RendererEntityType<RendererDirectionalLight>,
    volume_meshes: RendererEntityType<RendererVolumeDrawable>,
    latest_imgui: Cell<Option<FrameSnapshot>>,
    lightmap: Option<TextureHandle>,
}

impl RendererScene {
    pub fn new() -> Self {
        Self {
            views: vec![View::default()],
            static_meshes: RendererEntityType::new(),
            point_lights: RendererEntityType::new(),
            volume_meshes: RendererEntityType::new(),
            directional_lights: RendererEntityType::new(),
            lightmap: None,
            latest_imgui: Default::default(),
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
        self.static_meshes.entries()
    }

    #[inline(always)]
    pub fn point_lights(&self) -> &[RendererPointLight] {
        self.point_lights.entries()
    }

    #[inline(always)]
    pub fn directional_lights(&self) -> &[RendererDirectionalLight] {
        self.directional_lights.entries()
    }

    #[inline(always)]
    pub fn volume_mesh_instances(&self) -> &[RendererVolumeDrawable] {
        self.volume_meshes.entries()
    }

    #[inline(always)]
    pub fn view_update_info(
        &mut self,
    ) -> (
        &mut [View],
        &[RendererStaticDrawable],
        &[RendererPointLight],
        &[RendererDirectionalLight],
    ) {
        (
            &mut self.views,
            self.static_meshes.entries(),
            self.point_lights.entries(),
            self.directional_lights.entries(),
        )
    }

    pub fn add_static_drawable(&mut self, entity: Entity, static_drawable: RendererStaticDrawable) {
        self.static_meshes.add(entity, static_drawable);
    }

    pub fn remove_static_drawable(&mut self, entity: Entity) {
        self.static_meshes.remove(entity);
    }

    pub fn update_transform(&mut self, entity: Entity, transform: Affine3A) {
        let entry_opt = self.static_meshes.get_mut(entity);
        if let Some(entry) = entry_opt {
            entry.transform = transform;
            return;
        }

        let entry_opt = self.point_lights.get_mut(entity);
        if let Some(entry) = entry_opt {
            entry.position = transform.transform_point3(Vec3::new(0f32, 0f32, 0f32));
            return;
        }

        let entry_opt = self.directional_lights.get_mut(entity);
        if let Some(entry) = entry_opt {
            entry.direction = transform.transform_vector3(Vec3::new(0f32, 0f32, 1f32));
            return;
        }

        let entry_opt = self.volume_meshes.get_mut(entity);
        if let Some(entry) = entry_opt {
            entry.transform = transform;
            return;
        }

        let static_drawable = self.static_meshes.get_mut(entity);
        if let Some(static_drawable) = static_drawable {
            static_drawable.transform = transform;
            return;
        }

        warn!(
            "Found no entity on the renderer for ecs entity: {:?}",
            entity
        );

        debug_assert!(false); // debug unreachable
    }

    pub fn update_volume_mesh_data(
        &mut self,
        entity: Entity,
        min_threshold: f32,
        texture_lod: u32,
        transparent: VolumeDrawableTransparencyMode,
    ) {
        let volume_mesh_opt = self.volume_meshes.get_mut(entity);
        if let Some(volume_mesh) = volume_mesh_opt {
            volume_mesh.min_threshold = min_threshold;
            volume_mesh.texture_lod = texture_lod;
            volume_mesh.transparent = transparent;
            return;
        }

        warn!(
            "Found no entity on the renderer for ecs entity: {:?}",
            entity
        );

        debug_assert!(false); // debug unreachable
    }

    pub fn add_point_light(&mut self, entity: Entity, light: PointLight) {
        self.point_lights.add(
            entity,
            RendererPointLight::new(light.position, light.intensity),
        );
    }

    pub fn remove_point_light(&mut self, entity: Entity) {
        self.point_lights.remove(entity);
    }

    pub fn add_directional_light(&mut self, entity: Entity, light: DirectionalLight) {
        self.directional_lights.add(
            entity,
            RendererDirectionalLight::new(light.direction, light.intensity),
        );
    }

    pub fn remove_directional_light(&mut self, entity: Entity) {
        self.directional_lights.remove(entity);
    }

    pub fn add_volume_drawable(&mut self, entity: Entity, volume_drawable: RendererVolumeDrawable) {
        self.volume_meshes.add(entity, volume_drawable);
    }

    pub fn remove_volume_drawable(&mut self, entity: Entity) {
        self.volume_meshes.remove(entity);
    }

    pub fn set_ui_data(&self, data: FrameSnapshot) {
        self.latest_imgui.replace(Some(data));
    }

    pub fn take_ui_data(&self) -> Option<FrameSnapshot> {
        self.latest_imgui.take()
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
