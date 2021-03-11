use crate::renderer::{Drawable, DrawableType};
use std::collections::HashSet;
use legion::{Entity, IntoQuery, maybe_changed, EntityStore};

use legion::systems::Builder;
use legion::component;
use legion::world::SubWorld;
use crate::{ActiveCamera, Camera};
use std::sync::Arc;
use sourcerenderer_core::{Matrix4, Platform};
use crate::transform::interpolation::InterpolatedTransform;

#[cfg(feature = "threading")]
use super::Renderer;

pub trait RendererScene {
  fn register_static_renderable(&self, renderable: Drawable);
  fn unregister_static_renderable(&self, entity: Entity);
  fn update_camera_transform(&self, camera_transform_mat: Matrix4, fov: f32);
  fn update_transform(&self, entity: Entity, transform: Matrix4);
  fn end_frame(&self);
  fn is_saturated(&self) -> bool;
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticRenderableComponent {
  pub model_path: String,
  pub receive_shadows: bool,
  pub cast_shadows: bool,
  pub can_move: bool
}

#[derive(Clone, Default, Debug)]
pub struct ActiveStaticRenderables(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredStaticRenderables(HashSet<Entity>);

#[cfg(feature = "threading")]
pub fn install<P: Platform>(systems: &mut Builder, renderer: &Arc<Renderer<P>>) {
  systems.add_system(renderer_system::<P, Renderer<P>>(renderer.clone(), ActiveStaticRenderables(HashSet::new()), RegisteredStaticRenderables(HashSet::new())));
}

#[system]
#[read_component(StaticRenderableComponent)]
#[read_component(InterpolatedTransform)]
#[read_component(Camera)]
fn renderer<P: Platform, R: RendererScene + 'static>(world: &mut SubWorld,
            #[state] renderer: &Arc<R>,
            #[state] active_static_renderables: &mut ActiveStaticRenderables,
            #[state] registered_static_renderables: &mut RegisteredStaticRenderables,
            #[resource] active_camera: &ActiveCamera) {
  if renderer.is_saturated() {
    return;
  }

  let camera_entry = world.entry_ref(active_camera.0).ok();
  let transform_component = camera_entry.as_ref().and_then(|entry| entry.get_component::<InterpolatedTransform>().ok());
  let camera_component = camera_entry.as_ref().and_then(|entry| entry.get_component::<Camera>().ok());
  if let (Some(camera_component), Some(transform_component)) = (camera_component, transform_component) {
    renderer.update_camera_transform(transform_component.0, camera_component.fov);
  }

  let mut static_components_query = <(Entity, &StaticRenderableComponent, &InterpolatedTransform)>::query();
  for (entity, component, transform) in static_components_query.iter(world) {
    if active_static_renderables.0.contains(entity) {
      continue;
    }

    if !registered_static_renderables.0.contains(entity) {
      renderer.register_static_renderable(Drawable::new(*entity, DrawableType::Static {
          model_path: component.model_path.clone(),
          receive_shadows: component.receive_shadows,
          cast_shadows: component.cast_shadows,
          can_move: component.can_move
        }, transform.0)
      );

      registered_static_renderables.0.insert(*entity);
    }

    active_static_renderables.0.insert(*entity);
  }

  let mut static_components_update_transforms_query = <(Entity, &InterpolatedTransform)>::query()
    .filter(component::<StaticRenderableComponent>() & maybe_changed::<InterpolatedTransform>());

  for (entity, transform) in static_components_update_transforms_query.iter(world) {
    renderer.update_transform(*entity, transform.0);
  }

  registered_static_renderables.0.retain(|entity| {
    if !active_static_renderables.0.contains(entity) {
      renderer.unregister_static_renderable(*entity);
      false
    } else {
      true
    }
  });

  renderer.end_frame();
}
