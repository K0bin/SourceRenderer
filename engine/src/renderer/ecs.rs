use std::collections::HashSet;

use instant::Duration;
use legion::systems::Builder;
use legion::world::SubWorld;
use legion::{
    component,
    maybe_changed,
    Entity,
    EntityStore,
    IntoQuery,
};
use sourcerenderer_core::{
    Matrix4,
    Platform,
};

use crate::transform::interpolation::InterpolatedTransform;
use crate::transform::GlobalTransform;
use crate::{
    ActiveCamera,
    Camera,
};

pub trait RendererInterface {
    fn register_static_renderable(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        renderable: &StaticRenderableComponent,
    );
    fn unregister_static_renderable(&self, entity: Entity);
    fn register_point_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        point_light: &PointLightComponent,
    );
    fn unregister_point_light(&self, entity: Entity);
    fn register_directional_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        directional_light: &DirectionalLightComponent,
    );
    fn unregister_directional_light(&self, entity: Entity);
    fn update_camera_transform(&self, camera_transform_mat: Matrix4, fov: f32);
    fn update_transform(&self, entity: Entity, transform: Matrix4);
    fn update_lightmap(&self, path: &str);
    fn end_frame(&self);
    fn is_saturated(&self) -> bool;
    fn wait_until_available(&self, timeout: Duration);
    fn is_running(&self) -> bool;
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticRenderableComponent {
    pub model_path: String,
    pub receive_shadows: bool,
    pub cast_shadows: bool,
    pub can_move: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointLightComponent {
    pub intensity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DirectionalLightComponent {
    pub intensity: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Lightmap {
    pub path: String,
}

#[derive(Clone, Default, Debug)]
pub struct ActiveStaticRenderables(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredStaticRenderables(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct ActivePointLights(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredPointLights(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct ActiveDirectionalLights(HashSet<Entity>);
#[derive(Clone, Default, Debug)]
pub struct RegisteredDirectionalLights(HashSet<Entity>);

pub fn install<P: Platform, R: RendererInterface + Send + Sync + 'static>(
    systems: &mut Builder,
    renderer: R,
) {
    systems.add_system(renderer_system::<P, R>(
        renderer,
        ActiveStaticRenderables(HashSet::new()),
        RegisteredStaticRenderables(HashSet::new()),
        ActivePointLights(HashSet::new()),
        RegisteredPointLights(HashSet::new()),
        ActiveDirectionalLights(HashSet::new()),
        RegisteredDirectionalLights(HashSet::new()),
    ));
}

#[system]
#[read_component(StaticRenderableComponent)]
#[read_component(InterpolatedTransform)]
#[read_component(PointLightComponent)]
#[read_component(DirectionalLightComponent)]
#[read_component(GlobalTransform)]
#[read_component(Camera)]
#[read_component(Lightmap)]
fn renderer<P: Platform, R: RendererInterface + 'static>(
    world: &mut SubWorld,
    #[state] renderer: &R,
    #[state] active_static_renderables: &mut ActiveStaticRenderables,
    #[state] registered_static_renderables: &mut RegisteredStaticRenderables,
    #[state] active_point_lights: &mut ActivePointLights,
    #[state] registered_point_lights: &mut RegisteredPointLights,
    #[state] active_directional_lights: &mut ActiveDirectionalLights,
    #[state] registered_directional_lights: &mut RegisteredDirectionalLights,
    #[resource] active_camera: &ActiveCamera,
) {
    if renderer.is_saturated() {
        return;
    }

    let camera_entry = world.entry_ref(active_camera.0).ok();
    let interpolated_transform_component = camera_entry
        .as_ref()
        .and_then(|entry| entry.get_component::<InterpolatedTransform>().ok());
    let camera_component = camera_entry
        .as_ref()
        .and_then(|entry| entry.get_component::<Camera>().ok());
    let transform_component = camera_entry
        .as_ref()
        .and_then(|entry| entry.get_component::<GlobalTransform>().ok());

    if let (Some(camera), Some(interpolated), Some(transform)) = (
        camera_component,
        interpolated_transform_component,
        transform_component,
    ) {
        if camera.interpolate_rotation {
            renderer.update_camera_transform(interpolated.0, camera.fov);
        } else {
            let mut combined_transform = transform.0;
            *combined_transform.column_mut(3) = *interpolated.0.column(3);
            renderer.update_camera_transform(combined_transform, camera.fov);
        }
    }

    active_static_renderables.0.clear();
    let mut static_components_query =
        <(Entity, &StaticRenderableComponent, &InterpolatedTransform)>::query();
    for (entity, component, transform) in static_components_query.iter(world) {
        if active_static_renderables.0.contains(entity) {
            continue;
        }

        if !registered_static_renderables.0.contains(entity) {
            renderer.register_static_renderable(*entity, transform, component);

            registered_static_renderables.0.insert(*entity);
        }

        active_static_renderables.0.insert(*entity);
    }

    let mut static_components_update_transforms_query = <(Entity, &InterpolatedTransform)>::query()
        .filter(
            component::<StaticRenderableComponent>() & maybe_changed::<InterpolatedTransform>(),
        );

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

    let mut point_lights_query = <(Entity, &PointLightComponent, &InterpolatedTransform)>::query();
    for (entity, component, transform) in point_lights_query.iter(world) {
        if active_point_lights.0.contains(entity) {
            continue;
        }

        if !registered_point_lights.0.contains(entity) {
            renderer.register_point_light(*entity, transform, component);

            registered_point_lights.0.insert(*entity);
        }

        active_point_lights.0.insert(*entity);
    }

    let mut point_lights_update_transforms_query = <(Entity, &InterpolatedTransform)>::query()
        .filter(component::<PointLightComponent>() & maybe_changed::<InterpolatedTransform>());

    for (entity, transform) in point_lights_update_transforms_query.iter(world) {
        renderer.update_transform(*entity, transform.0);
    }

    registered_point_lights.0.retain(|entity| {
        if !active_point_lights.0.contains(entity) {
            renderer.unregister_point_light(*entity);
            false
        } else {
            true
        }
    });

    let mut directional_lights_query =
        <(Entity, &DirectionalLightComponent, &InterpolatedTransform)>::query();
    for (entity, component, transform) in directional_lights_query.iter(world) {
        if active_directional_lights.0.contains(entity) {
            continue;
        }

        if !active_directional_lights.0.contains(entity) {
            renderer.register_directional_light(*entity, transform, component);

            active_directional_lights.0.insert(*entity);
        }

        active_directional_lights.0.insert(*entity);
    }

    let mut directional_lights_update_transforms_query =
        <(Entity, &InterpolatedTransform)>::query()
            .filter(component::<PointLightComponent>() & maybe_changed::<InterpolatedTransform>());

    for (entity, transform) in directional_lights_update_transforms_query.iter(world) {
        renderer.update_transform(*entity, transform.0);
    }

    registered_directional_lights.0.retain(|entity| {
        if !active_directional_lights.0.contains(entity) {
            renderer.unregister_directional_light(*entity);
            false
        } else {
            true
        }
    });

    let mut lightmap_query = <(&Lightmap,)>::query().filter(maybe_changed::<Lightmap>());

    for (lightmap,) in lightmap_query.iter(world) {
        renderer.update_lightmap(&lightmap.path);
        break;
    }

    renderer.end_frame();
}
