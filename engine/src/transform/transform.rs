use sourcerenderer_core::{Vec3, Quaternion, Matrix4};
use legion::{Entity, World, IntoQuery, component, maybe_changed, EntityStore, Write, Resources};
use std::process::Child;
use legion::systems::{CommandBuffer, Builder};
use std::collections::HashMap;
use legion::world::SubWorld;
use nalgebra::{Vector3, Unit};

pub struct Transform {
  pub position: Vec3,
  pub rotation: Quaternion,
  pub scale: Vec3
}

struct TransformDirty(bool);

pub struct GlobalTransform(pub Matrix4);

pub struct Parent(Entity);
struct PreviousParent(Entity);

#[derive(Default)]
struct Children(Vec<Entity>);

impl From<Transform> for Matrix4 {
  fn from(transform: Transform) -> Self {
    Matrix4::new_translation(&transform.position)
      * Matrix4::new_rotation(transform.rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount))
      * Matrix4::new_nonuniform_scaling(&transform.scale)
  }
}

impl From<&Transform> for Matrix4 {
  fn from(transform: &Transform) -> Self {
    Matrix4::new_translation(&transform.position)
      * Matrix4::new_rotation(transform.rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount))
      * Matrix4::new_nonuniform_scaling(&transform.scale)
  }
}

impl Transform {
  pub fn new(position: Vec3) -> Self {
    Self {
      position,
      rotation: Quaternion::identity(),
      scale: Vec3::new(1.0f32, 1.0f32, 1.0f32),
    }
  }

  pub fn transform(&self, vector: &Vec3) -> Vec3 {
    Matrix4::from(self).transform_vector(&vector)
  }
}


pub fn install(systems: &mut Builder) {
  systems.add_system(maintain_children_system(HashMap::new()));
  systems.flush();
  systems.add_system(add_dirty_to_new_transforms_system());
  systems.add_system(mark_changed_transforms_dirty_system());
  systems.add_system(mark_transforms_dirty_because_parent_system());
  systems.flush();
  systems.add_system(update_global_transforms_system());
  systems.flush();
  systems.add_system(mark_transforms_clean_system());
}

#[system(for_each)]
#[filter(component::<Transform>() & !component::<TransformDirty>())]
fn add_dirty_to_new_transforms(entity: &Entity, command_buffer: &mut CommandBuffer) {
  command_buffer.add_component(*entity, TransformDirty(true));
}

#[system(for_each)]
#[filter(maybe_changed::<Transform>())]
fn mark_changed_transforms_dirty(dirty: &mut TransformDirty) {
  dirty.0 = true;
}

#[system(for_each)]
#[filter(maybe_changed::<Parent>())]
fn mark_transforms_dirty_because_parent(dirty: &mut TransformDirty) {
  dirty.0 = true;
}

#[system(par_for_each)]
fn mark_transforms_clean(dirty: &mut TransformDirty) {
  dirty.0 = false;
}

#[system]
#[read_component(Transform)]
#[read_component(TransformDirty)]
#[read_component(GlobalTransform)]
fn update_global_transforms(world: &SubWorld,
                            command_buffer: &mut CommandBuffer) {
  let mut root_transforms_query = <(Entity,)>::query()
    .filter(component::<Transform>() & !component::<Parent>());

  for (entity,) in root_transforms_query.iter(world) {
    propagade_transforms(entity, &Matrix4::identity(), false, world, command_buffer);
  }
}

fn propagade_transforms(entity: &Entity,
                        parent_transform: &Matrix4,
                        parent_dirty: bool,
                        world: &SubWorld,
                        command_buffer: &mut CommandBuffer) {
  let entry_opt = world.entry_ref(*entity);
  if entry_opt.is_err() {
    return;
  }

  let mut entry = entry_opt.unwrap();
  let transform = entry.get_component::<Transform>().unwrap();
  let transform_dirty = entry.get_component::<TransformDirty>().unwrap();
  let dirty = parent_dirty || transform_dirty.0;
  let new_global_transform = if dirty {
    Some(GlobalTransform(parent_transform.clone() * Matrix4::from(transform)))
  } else {
    None
  };
  let global_transform = new_global_transform.as_ref().unwrap_or_else(|| {
    entry.get_component::<GlobalTransform>().expect("Parent does not have a global transform component")
  });

  let children_opt = entry.get_component::<Children>();
  if let Ok(children) = children_opt {
    for child in &children.0 {
      propagade_transforms(child, &global_transform.0, dirty, world, command_buffer);
    }
  }
  if let Some(global_transform) = new_global_transform {
    command_buffer.add_component(*entity, global_transform);
  }
}

#[system]
#[write_component(Parent)]
#[write_component(PreviousParent)]
#[write_component(Children)]
#[read_component(Transform)]
fn maintain_children(command_buffer: &mut CommandBuffer, world: &mut SubWorld, #[state] new_children: &mut HashMap<Entity, Vec<Entity>>) {
  let (ref mut children_world, ref mut other_world) = world.split::<&mut Children>();

  // handle added entities
  let mut added_parent_components_query = <(Entity, &Parent)>::query()
    .filter(!component::<PreviousParent>());
  for (entity, parent) in added_parent_components_query.iter(other_world) {
    let parent_entry = children_world.entry_mut(parent.0);
    if let Ok(mut parent_entry) = parent_entry {
      let mut children = parent_entry.get_component_mut::<Children>();
      if let Ok(children) = children {
        children.0.push(*entity);
      } else {
        new_children
          .entry(parent.0)
          .or_default()
          .push(*entity);
      }

      command_buffer.add_component(*entity, PreviousParent(parent.0));
    } else {
      println!("remove");
      command_buffer.remove(*entity);
    }

    command_buffer.add_component(*entity, PreviousParent(parent.0));
  }

  // handle changed parents
  let mut changed_parent_components_query = <(Entity, &Parent, &PreviousParent)>::query()
    .filter(maybe_changed::<Parent>());
  for (entity, parent, previous) in changed_parent_components_query.iter(other_world) {
    let mut previous_parent_entry = children_world.entry_mut(previous.0).ok();
    let previous_parent_children = previous_parent_entry.as_mut()
      .and_then(|mut entry| entry.get_component_mut::<Children>().ok());
    if let Some(mut previous_parent_children) = previous_parent_children {
      let index = previous_parent_children.0.iter().position(|child| child == entity);
      if let Some(index) = index {
        previous_parent_children.0.remove(index);
      }
    }

    let parent_entry = children_world.entry_mut(parent.0);
    if let Ok(mut parent_entry) = parent_entry {
      let mut children = parent_entry.get_component_mut::<Children>();
      if let Ok(children) = children {
        children.0.push(*entity);
      } else {
        new_children
          .entry(parent.0)
          .or_default()
          .push(*entity);
      }

      command_buffer.add_component(*entity, PreviousParent(parent.0));
    } else {
      println!("remove");
      command_buffer.remove(*entity);
    }
  }

  // remove broken children references
  let mut children_query = <(&mut Children)>::query()
    .filter(component::<Transform>());
  for (children) in children_query.iter_mut(children_world) {
    children.0.retain(|child| other_world.entry_ref(*child).is_ok());
  }

  // remove children component when entity has no transform
  let mut children_no_transform_query = <(Entity, &Children)>::query()
   .filter(!component::<Transform>());
  for (entity, _) in children_no_transform_query.iter_mut(children_world) {
    command_buffer.remove_component::<Children>(*entity);
  }

  // remove parent component when entity has no transform
  let mut parent_no_transform_query = <(Entity, &Parent)>::query()
    .filter(!component::<Transform>());
  for (entity, _) in parent_no_transform_query.iter_mut(other_world) {
    command_buffer.remove_component::<Parent>(*entity);
  }

   // flush new children components
  for (entity, children) in new_children.drain() {
    command_buffer.add_component(entity, Children(children));
  }
}
