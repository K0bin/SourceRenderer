use crate::renderer::renderable::{StaticModelRenderable, Renderable, RenderableType};
use std::collections::HashSet;
use legion::{Entity, Resources, SystemBuilder, IntoQuery, World, maybe_changed};
use crossbeam_channel::Sender;
use crate::renderer::command::RendererCommand;
use crate::asset::AssetKey;
use nalgebra::Matrix4;
use legion::systems::{Builder, CommandBuffer};
use legion::component;
use legion::world::SubWorld;
use crate::transform::GlobalTransform;
use crate::camera::GlobalCamera;

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

pub fn install(systems: &mut Builder, sender: Sender<RendererCommand>) {
  systems.add_system(renderer_system(sender, ActiveStaticRenderables(HashSet::new()), RegisteredStaticRenderables(HashSet::new())));
}

#[system]
#[read_component(GlobalCamera)]
#[read_component(StaticRenderableComponent)]
#[read_component(GlobalTransform)]
fn renderer(world: &mut SubWorld,
            #[state] sender: &Sender<RendererCommand>,
            #[state] active_static_renderables: &mut ActiveStaticRenderables,
            #[state] registered_static_renderables: &mut RegisteredStaticRenderables) {

  let mut camera_query = <(&GlobalCamera,)>::query();
  for (camera,) in camera_query.iter(world) {
    sender.send(RendererCommand::UpdateCamera(camera.0));
  }

  let mut static_components_query = <(Entity, &StaticRenderableComponent, &GlobalTransform)>::query();
  for (entity, component, transform) in static_components_query.iter(world) {
    if active_static_renderables.0.contains(entity) {
      continue;
    }

    if !registered_static_renderables.0.contains(entity) {
      sender.send(RendererCommand::Register(Renderable {
        renderable_type: RenderableType::Static(StaticModelRenderable {
          model: component.model,
          receive_shadows: component.receive_shadows,
          cast_shadows: component.cast_shadows,
          can_move: component.can_move
        }),
        entity: *entity,
        transform: transform.0,
        old_transform: Matrix4::<f32>::identity()
      }
      ));

      registered_static_renderables.0.insert(*entity);
    }

    active_static_renderables.0.insert(*entity);
  }

  let mut static_components_update_transforms_query = <(Entity, &GlobalTransform)>::query()
    .filter(component::<StaticRenderableComponent>() & maybe_changed::<GlobalTransform>());

  for (entity, transform) in static_components_update_transforms_query.iter(world) {
    sender.send(RendererCommand::UpdateTransform(*entity, transform.0.clone()));
  }

  registered_static_renderables.0.retain(|entity| {
    if !active_static_renderables.0.contains(entity) {
      sender.send(RendererCommand::UnregisterStatic(*entity));
      false
    } else {
      true
    }
  });

  sender.send(RendererCommand::EndFrame);
}
