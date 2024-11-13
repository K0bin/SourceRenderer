use std::{any::{Any, TypeId}, marker::PhantomPinned, ops::Deref, pin::Pin};

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use bevy_hierarchy::{BuildChildren, Parent};
use bevy_transform::components::Transform;
use bumpalo::Bump;
use bumpalo::collections::Vec;
use bumpalo::boxed::Box;

use crate::renderer::{DirectionalLightComponent, PointLightComponent, StaticRenderableComponent};

pub struct LoadedEntityParent(pub usize);

pub struct LoadedEntity<'a> {
    components: Vec<'a, Box<'a, dyn Any>>
}

struct BumpUnpin {
    _unpinned: PhantomPinned,
    bump: Bump

}

impl Deref for BumpUnpin {
    type Target = Bump;

    fn deref(&self) -> &Self::Target {
        &self.bump
    }
}

pub struct LoadedLevel {
    total_component_count: usize,
    entities: Vec<'static, LoadedEntity<'static>>,
    bump: Pin<std::boxed::Box<BumpUnpin>>,
}

impl LoadedLevel {
    pub fn new(estimated_allocation_size: usize, estimated_entity_count: usize) -> Self {
        let bump = Bump::with_capacity(estimated_allocation_size);
        let static_bump: &'static Bump = unsafe {
            std::mem::transmute(&bump)
        };
        let mut new_loaded_level = Self {
            total_component_count: 0,
            bump: std::boxed::Box::pin(BumpUnpin {
                _unpinned: PhantomPinned,
                bump
            }),
            entities: Vec::new_in(static_bump), // Vec does not allocate until it gets entries
        };

        let static_bump: &'static Bump = unsafe {
            let bump_ref = new_loaded_level.bump.as_ref().get_ref();
            std::mem::transmute(bump_ref)
        };
        new_loaded_level.entities = Vec::with_capacity_in(estimated_entity_count, static_bump);
        new_loaded_level
    }

    pub fn push_entity(&mut self, estimated_component_count: usize) -> usize {
        let static_bump: &'static Bump = unsafe {
            let bump_ref = self.bump.as_ref().get_ref();
            std::mem::transmute(bump_ref)
        };
        let index = self.entities.len();
        let components = Vec::with_capacity_in(estimated_component_count, static_bump);
        let entity = LoadedEntity {
            components
        };
        self.entities.push(entity);
        index
    }

    pub fn push_component<T: Any>(&mut self, entity_index: usize, component: T) {
        let entity = &mut self.entities[entity_index];
        let static_bump: &'static Bump = unsafe {
            let bump_ref = self.bump.as_ref().get_ref();
            std::mem::transmute(bump_ref)
        };
        let alloced_component: &'static mut T = static_bump.alloc(component);
        let boxed_component: Box<'static, dyn Any> = unsafe { Box::from_raw(alloced_component) };
        entity.components.push(boxed_component);
        self.total_component_count += 1;
    }

    pub fn get_component_mut<T: Any>(&mut self, entity_index: usize) -> Option<&mut T> {
        let entity= &mut self.entities[entity_index];
        for component in &mut entity.components {
            let component_type_id = component.as_ref().type_id();
            let expected_type_id = TypeId::of::<T>();
            if component_type_id == expected_type_id {
                return component.downcast_mut();
            }
        }
        None
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn memory_usage(&self) -> usize {
        self.bump.allocated_bytes()
    }

    pub fn component_count(&self) -> usize {
        self.total_component_count
    }

    pub fn import_into_world(mut self, world: &mut World) {
        let mut ecs_entities = Vec::<(Entity, Option<LoadedEntityParent>)>::with_capacity_in(self.entities.len(), &self.bump);

        for mut loaded_entity in self.entities.drain(..) {
            let mut parent = Option::<LoadedEntityParent>::None;
            let mut entity = world.spawn(());
            for loaded_component in loaded_entity.components.drain(..) {
                let component_type_id = loaded_component.as_ref().type_id();
                if component_type_id == TypeId::of::<Transform>() {
                    entity.insert(Self::loaded_component_into::<Transform>(loaded_component));
                } else if component_type_id == TypeId::of::<LoadedEntityParent>() {
                    parent = Some(Self::loaded_component_into::<LoadedEntityParent>(loaded_component));
                } else if component_type_id == TypeId::of::<StaticRenderableComponent>() {
                    entity.insert(Self::loaded_component_into::<StaticRenderableComponent>(loaded_component));
                } else if component_type_id == TypeId::of::<DirectionalLightComponent>() {
                    entity.insert(Self::loaded_component_into::<DirectionalLightComponent>(loaded_component));
                } else if component_type_id == TypeId::of::<PointLightComponent>() {
                    entity.insert(Self::loaded_component_into::<PointLightComponent>(loaded_component));
                } else {
                    panic!("Unsupported type in LoadedLevel");
                }
            }
            ecs_entities.push((entity.flush(), parent));
        }

        let mut commands = world.commands();
        for (entity, entity_parent_index_opt) in &ecs_entities {
            if let Some(entity_parent_index) = entity_parent_index_opt {
                commands.entity(*entity).set_parent(ecs_entities[entity_parent_index.0].0);
            }
        }
    }

    fn loaded_component_into<T: Any + Sized>(component: Box<dyn Any>) -> T {
        assert!(component.as_ref().is::<T>());

        let any_ref = component.as_ref();
        let t_ref = any_ref.downcast_ref::<T>().unwrap();
        let t_ptr = t_ref as *const T;
        std::mem::forget(component); // The Box is bump allocated so we can just leak its memory.
        unsafe { core::ptr::read(t_ptr) }
    }
}
