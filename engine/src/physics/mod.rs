use std::collections::{HashMap, HashSet};

use instant::Duration;
use rapier3d::prelude::*;
use rapier3d::prelude::IntegrationParameters;
use legion::{Entity, IntoQuery, Resources, World, component, maybe_changed, systems::Builder, world::SubWorld};
use sourcerenderer_core::Vec3;

use crate::Transform;

#[derive(Clone, Default, Debug)]
pub struct ActiveRigidBodies(HashSet<Entity>);

pub enum ColliderComponent {
  Capsule {
    radius: f32,
    height: f32
  },
  Box {
    width: f32,
    height: f32,
    depth: f32
  }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RigidBodyType {
  Static,
  Kinematic,
  Dynamic
}

pub struct RigidBodyComponent {
  pub body_type: RigidBodyType
}

pub struct PhysicsWorld {
  rigid_body_set: RigidBodySet,
  collider_set: ColliderSet,
  physics_pipeline: PhysicsPipeline,
  island_manager: IslandManager,
  broad_phase: BroadPhase,
  narrow_phase: NarrowPhase,
  impulse_joint_set: ImpulseJointSet,
  multibody_joint_set: MultibodyJointSet,
  ccd_solver: CCDSolver,
  integration_parameters: IntegrationParameters,
  gravity: Vector<f32>,
  entity_collider_map: HashMap<Entity, ColliderHandle>
}

impl PhysicsWorld {
  pub fn install(_world: &mut World, resources: &mut Resources, systems: &mut Builder, delta: Duration) {
    let rigid_body_set = RigidBodySet::new();
    let collider_set = ColliderSet::new();
    let physics_pipeline = PhysicsPipeline::new();
    let island_manager = IslandManager::new();
    let broad_phase = BroadPhase::new();
    let narrow_phase = NarrowPhase::new();
    let impulse_joint_set = ImpulseJointSet::new();
    let multibody_joint_set = MultibodyJointSet::new();
    let ccd_solver = CCDSolver::new();
    let gravity = vector![0f32, -9.81f32, 0f32];
    let integration_parameters = IntegrationParameters {
      dt: delta.as_secs_f32(),
      ..Default::default()
    };

    let physics_world = Self {
      rigid_body_set,
      collider_set,
      physics_pipeline,
      island_manager,
      broad_phase,
      narrow_phase,
      impulse_joint_set,
      multibody_joint_set,
      ccd_solver,
      gravity,
      integration_parameters,
      entity_collider_map: HashMap::new()
    };
    resources.insert(physics_world);

    systems.add_system(physics_tick_system(ActiveRigidBodies(HashSet::new())));
  }
}

#[system]
#[read_component(ColliderComponent)]
#[read_component(RigidBodyComponent)]
#[read_component(Transform)]
#[write_component(Transform)]
fn physics_tick(world: &mut SubWorld, #[resource] physics_world: &mut PhysicsWorld, #[state] active_rigid_bodies: &mut ActiveRigidBodies) {

  let mut query = <(Entity, &Transform)>::query()
    .filter(maybe_changed::<Transform>() & component::<RigidBodyComponent>() & component::<ColliderComponent>());
  for (entity, transform) in query.iter(world) {
    let collider_handle = physics_world.entity_collider_map.get(entity);
    if let Some(collider_handle) = collider_handle {
      let collider = physics_world.collider_set.get(*collider_handle).unwrap();
      let parent = collider.parent().unwrap();
      let rigid_body = physics_world.rigid_body_set.get_mut(parent).unwrap();
      rigid_body.set_translation(transform.position, true);
      let euler_angles = transform.rotation.euler_angles();
      rigid_body.set_rotation(Vec3::new(euler_angles.0, euler_angles.1, euler_angles.2), true);
    }
  }

  let mut query = <(Entity, &Transform, &RigidBodyComponent, &ColliderComponent)>::query();
  active_rigid_bodies.0.clear();
  for (entity, transform, rigidbody, collider) in query.iter(world) {
    if active_rigid_bodies.0.contains(entity) {
      continue;
    }

    // this is pretty bad
    let entity_raw: u64 = unsafe { std::mem::transmute_copy(entity) };

    if !physics_world.entity_collider_map.contains_key(entity) {
      let euler_angles = transform.rotation.euler_angles();

      // Add to ColliderSet and RigidBodySet
      let rigid_body = match rigidbody.body_type {
        RigidBodyType::Static => RigidBodyBuilder::new_static(),
        RigidBodyType::Kinematic => RigidBodyBuilder::new_kinematic_position_based(),
        RigidBodyType::Dynamic => RigidBodyBuilder::dynamic(),
      }
      .translation(transform.position)
      .rotation(Vec3::new(euler_angles.0, euler_angles.1, euler_angles.2))
      .user_data(entity_raw as u128)
      .build();

      let rigid_body_handle = physics_world.rigid_body_set.insert(rigid_body);

      let collider = match collider {
        ColliderComponent::Box { width, height, depth } => ColliderBuilder::cuboid(*width, *height, *depth),
        ColliderComponent::Capsule { radius, height } => ColliderBuilder::capsule_y(*height / 2f32, *radius),
      }
      .build();

      let collider_handle = physics_world.collider_set.insert_with_parent(collider, rigid_body_handle, &mut physics_world.rigid_body_set);
      physics_world.entity_collider_map.insert(*entity, collider_handle);
    }

    active_rigid_bodies.0.insert(*entity);
  }

  for (entity, collider_handle) in &physics_world.entity_collider_map {
    if active_rigid_bodies.0.contains(entity) {
      continue;
    }
    // Remove from ColliderSet and RigidBodySet
    let rigid_body_handle =  {
      let collider = physics_world.collider_set.get(*collider_handle).unwrap();
      collider.parent().unwrap()
    };
    physics_world.rigid_body_set.remove(rigid_body_handle, &mut physics_world.island_manager, &mut physics_world.collider_set, &mut physics_world.impulse_joint_set, &mut physics_world.multibody_joint_set, true);
    physics_world.collider_set.remove(*collider_handle, &mut physics_world.island_manager, &mut physics_world.rigid_body_set, true);
  }

  physics_world.entity_collider_map.retain(|entity, _collider_handle| {
    active_rigid_bodies.0.contains(entity)
  });

  physics_world.physics_pipeline.step(
    &physics_world.gravity,
    &physics_world.integration_parameters,
    &mut physics_world.island_manager,
    &mut physics_world.broad_phase,
    &mut physics_world.narrow_phase,
    &mut physics_world.rigid_body_set,
    &mut physics_world.collider_set,
    &mut physics_world.impulse_joint_set,
    &mut physics_world.multibody_joint_set,
    &mut physics_world.ccd_solver,
    &(),
    &()
  );

  // Sync back the transforms
  let mut query = <(Entity, &mut Transform)>::query()
    .filter(component::<RigidBodyComponent>() & component::<ColliderComponent>());
  for (entity, transform) in query.iter_mut(world) {
    let collider_handle = physics_world.entity_collider_map.get(entity).unwrap();
    let collider = physics_world.collider_set.get(*collider_handle).unwrap();
    let parent = collider.parent().unwrap();
    let rigid_body = physics_world.rigid_body_set.get(parent).unwrap();
    transform.position = *rigid_body.translation();
    transform.rotation = *rigid_body.rotation();
  }
}

