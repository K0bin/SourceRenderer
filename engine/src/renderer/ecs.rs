use crate::renderer::renderable::{StaticModelRenderable, Renderable, RenderableType};
use std::collections::HashSet;
use legion::{Entity, Resources, SystemBuilder, IntoQuery, World};
use crossbeam_channel::Sender;
use crate::renderer::command::RendererCommand;
use crate::asset::AssetKey;
use nalgebra::Matrix4;
use legion::systems::Builder;
use legion::component;

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
  resources.insert(ActiveStaticRenderables(HashSet::new()));
  resources.insert(RegisteredStaticRenderables(HashSet::new()));
  resources.insert(sender);

  systems.add_system(register_in_renderer_system());
  systems.flush();
  systems.add_system(update_in_renderer_system());
  systems.flush();
  systems.add_system(unregister_from_renderer_system());
  systems.flush();
  systems.add_system(finish_frame_system());
}

#[system(for_each)]
fn register_in_renderer(entity: &Entity,
                        component: &StaticRenderableComponent,
                        #[resource] registered_static_renderables: &mut RegisteredStaticRenderables,
                        #[resource] sender: &Sender<RendererCommand>) {
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
  }

  registered_static_renderables.0.insert(*entity);
}

#[system(for_each)]
fn update_in_renderer(entity: &Entity,
                      component: &StaticRenderableComponent,
                      #[resource] active_static_renderables: &mut ActiveStaticRenderables,
                      #[resource] sender: &Sender<RendererCommand>) {
  active_static_renderables.0.insert(*entity);
}

#[system]
fn unregister_from_renderer(#[resource] registered_static_renderables: &mut RegisteredStaticRenderables,
                            #[resource] active_static_renderables: &mut ActiveStaticRenderables,
                            #[resource] sender: &Sender<RendererCommand>) {
  registered_static_renderables.0.retain(|entity| {
    if !active_static_renderables.0.contains(entity) {
      sender.send(RendererCommand::UnregisterStatic(*entity));
      true
    } else {
      false
    }
  });
}


#[system]
fn finish_frame(#[resource] sender: &Sender<RendererCommand>) {
  sender.send(RendererCommand::EndFrame);
}