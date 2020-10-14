use crate::renderer::renderable::{StaticModelRenderable, Renderable, RenderableType};
use std::collections::HashSet;
use legion::{Entity, Resources, SystemBuilder, IntoQuery, World};
use crossbeam_channel::Sender;
use crate::renderer::command::RendererCommand;
use crate::asset::AssetKey;
use nalgebra::Matrix4;
use legion::systems::{Builder, CommandBuffer};
use legion::component;
use legion::world::SubWorld;

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

pub fn install(resources: &mut Resources, systems: &mut Builder, sender: Sender<RendererCommand>) {
  resources.insert(sender);
  systems.add_system(renderer_system(ActiveStaticRenderables(HashSet::new()), RegisteredStaticRenderables(HashSet::new())));
}

#[system]
#[read_component(StaticRenderableComponent)]
fn renderer(world: &mut SubWorld,
            #[resource] sender: &Sender<RendererCommand>,
            #[state] active_static_renderables: &mut ActiveStaticRenderables,
            #[state] registered_static_renderables: &mut RegisteredStaticRenderables) {

  let mut static_components_query = <(Entity, &StaticRenderableComponent)>::query();
  for (entity, component) in static_components_query.iter(world) {
    if !registered_static_renderables.0.contains(entity) {
      sender.send(RendererCommand::Register(Renderable {
        renderable_type: RenderableType::Static(StaticModelRenderable {
          model: component.model,
          receive_shadows: component.receive_shadows,
          cast_shadows: component.cast_shadows,
          can_move: component.can_move
        }),
        entity: *entity,
        transform: Matrix4::<f32>::identity(),
        old_transform: Matrix4::<f32>::identity()
      }
      ));

      registered_static_renderables.0.insert(*entity);
    }

    active_static_renderables.0.insert(*entity);
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
