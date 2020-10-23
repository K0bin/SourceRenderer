use crate::renderer::{Drawable, DrawableType};
use std::collections::HashSet;
use legion::{Entity, Resources, SystemBuilder, IntoQuery, World, maybe_changed, EntityStore};
use crossbeam_channel::Sender;
use crate::renderer::command::RendererCommand;
use crate::asset::AssetKey;
use nalgebra::Matrix4;
use legion::systems::{Builder, CommandBuffer};
use legion::component;
use legion::world::SubWorld;
use crate::transform::GlobalTransform;
use crate::{ActiveCamera, Camera};
use crate::renderer::Renderer;
use std::sync::Arc;
use sourcerenderer_core::Platform;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaticRenderableComponent {
  pub model: AssetKey,
  pub receive_shadows: bool,
  pub cast_shadows: bool,
  pub can_move: bool
}

#[derive(Clone, Default, Debug)]
pub struct ActiveStaticRenderables(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredStaticRenderables(HashSet<Entity>);

pub fn install<P: Platform>(systems: &mut Builder, renderer: &Arc<Renderer<P>>) {
  systems.add_system(renderer_system::<P>(renderer.clone(), ActiveStaticRenderables(HashSet::new()), RegisteredStaticRenderables(HashSet::new())));
}

#[system]
#[read_component(StaticRenderableComponent)]
#[read_component(GlobalTransform)]
#[read_component(Camera)]
fn renderer<P: Platform>(world: &mut SubWorld,
            #[state] renderer: &Arc<Renderer<P>>,
            #[state] active_static_renderables: &mut ActiveStaticRenderables,
            #[state] registered_static_renderables: &mut RegisteredStaticRenderables,
            #[resource] active_camera: &ActiveCamera) {

  let camera_entry = world.entry_ref(active_camera.0).ok();
  let transform_component = camera_entry.as_ref().and_then(|entry| entry.get_component::<GlobalTransform>().ok());
  let camera_component = camera_entry.as_ref().and_then(|entry| entry.get_component::<Camera>().ok());
  if camera_component.is_some() && transform_component.is_some() {
    renderer.update_camera_transform(transform_component.unwrap().0, camera_component.unwrap().fov);
  }

  let mut static_components_query = <(Entity, &StaticRenderableComponent, &GlobalTransform)>::query();
  for (entity, component, transform) in static_components_query.iter(world) {
    if active_static_renderables.0.contains(entity) {
      continue;
    }

    if !registered_static_renderables.0.contains(entity) {
      renderer.register_static_renderable(Drawable::new(*entity, DrawableType::Static {
          model: component.model,
          receive_shadows: component.receive_shadows,
          cast_shadows: component.cast_shadows,
          can_move: component.can_move
        }, transform.0)
      );

      registered_static_renderables.0.insert(*entity);
    }

    active_static_renderables.0.insert(*entity);
  }

  let mut static_components_update_transforms_query = <(Entity, &GlobalTransform)>::query()
    .filter(component::<StaticRenderableComponent>() & maybe_changed::<GlobalTransform>());

  for (entity, transform) in static_components_update_transforms_query.iter(world) {
    renderer.update_transform(*entity, transform.0.clone());
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
