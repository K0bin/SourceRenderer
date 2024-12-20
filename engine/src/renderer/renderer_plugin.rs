use std::marker::PhantomData;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use bevy_app::{
    App,
    Last,
    Plugin,
};
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::entity::Entity;
use bevy_ecs::event::Event;
use bevy_ecs::query::{
    Added,
    With,
};
use bevy_ecs::removal_detection::RemovedComponents;
use bevy_ecs::schedule::{
    IntoSystemConfigs,
    SystemSet,
};
use bevy_ecs::system::{
    Query,
    Res,
    ResMut,
    Resource,
};
use bevy_ecs::world::{Ref, World};
use bevy_log::trace;
use bevy_transform::components::GlobalTransform;
use sourcerenderer_core::{
    Platform,
    Vec2UI,
};

use super::renderer::RendererSender;
use super::{
    DirectionalLightComponent,
    PointLightComponent,
    Renderer,
    StaticRenderableComponent,
};
use crate::engine::{
    AssetManagerResource,
    ConsoleResource,
    GPUDeviceResource,
    GPUSwapchainResource,
    WindowState, TICK_RATE,
};
use crate::transform::InterpolatedTransform;
use crate::{
    ActiveCamera,
    Camera,
};

#[derive(Event)]
struct WindowSizeChangedEvent {
    size: Vec2UI,
}

#[derive(Event)]
struct WindowMinimized {}

pub struct RendererPlugin<P: Platform> {
    _a: PhantomData<P>,
}

unsafe impl<P: Platform> Send for RendererPlugin<P> {}
unsafe impl<P: Platform> Sync for RendererPlugin<P> {}

impl<P: Platform> Plugin for RendererPlugin<P> {
    fn build(&self, app: &mut App) {
        let swapchain = app
            .world_mut()
            .remove_resource::<GPUSwapchainResource<P::GPUBackend>>()
            .unwrap()
            .0;
        let gpu_resources = app.world().resource::<GPUDeviceResource<P::GPUBackend>>();
        let console_resource = app.world().resource::<ConsoleResource>();
        let asset_manager_resource = app.world().resource::<AssetManagerResource<P>>();

        let (renderer, sender) = Renderer::new(
            &gpu_resources.0,
            swapchain,
            &asset_manager_resource.0,
            &console_resource.0,
        );

        install_renderer(app, renderer, sender);
    }
}

impl<P: Platform> RendererPlugin<P> {
    pub fn new() -> Self {
        Self { _a: PhantomData }
    }

    pub fn stop(app: &App) {
        let resource = app.world().resource::<RendererResourceWrapper<P>>();
        resource.sender.stop();
    }

    pub fn window_changed(app: &App, window_state: WindowState) {
        let resource = app.world().resource::<RendererResourceWrapper<P>>();
        resource.sender.window_changed(window_state);
    }
}

#[derive(Resource)]
struct RendererResourceWrapper<P: Platform> {
    sender: RendererSender<P::GPUBackend>,

    #[cfg(not(feature = "threading"))]
    renderer: SyncCell<Renderer<P>>,
}

#[cfg(not(feature = "threading"))]
fn install_renderer<P: Platform>(
    app: &mut App,
    renderer: Renderer<P>,
    _sender: RendererSender<P::GPUBackend>,
) {
    let wrapper = RendererResourceWrapper {
        renderer: SyncCell::new(renderer),
    };
    app.insert_resource(wrapper);
    app.add_systems(
        Last,
        (
            extract_camera::<P>,
            extract_static_renderables::<P>,
            extract_point_lights::<P>,
            extract_directional_lights::<P>,
        )
            .in_set(ExtractSet),
    );
    app.add_systems(Last, run_renderer::<P>.after(ExtractSet));
}

#[cfg(not(feature = "threading"))]
fn run_renderer<P: Platform>(mut renderer: ResMut<RendererResourceWrapper<P>>) {
    renderer.renderer.get().render();
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct SyncSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct ExtractSet;

#[cfg(feature = "threading")]
fn install_renderer<P: Platform>(
    app: &mut App,
    renderer: Renderer<P>,
    sender: RendererSender<P::GPUBackend>,
) {
    start_render_thread(renderer);

    let wrapper = RendererResourceWrapper::<P> { sender: sender };
    app.insert_resource(wrapper);
    app.add_systems(
        Last,
        (begin_frame::<P>,).in_set(SyncSet)
    );
    app.add_systems(
        Last,
        (
            extract_camera::<P>,
            extract_static_renderables::<P>,
            extract_point_lights::<P>,
            extract_directional_lights::<P>,
        )
            .in_set(ExtractSet)
            .after(SyncSet),
    );
    app.add_systems(Last, end_frame::<P>.after(ExtractSet));
}

#[cfg(feature = "threading")]
fn start_render_thread<P: Platform>(mut renderer: Renderer<P>) {
    std::thread::Builder::new()
        .name("RenderThread".to_string())
        .spawn(move || {
            trace!("Started renderer thread");
            loop {
                if !renderer.is_running() {
                    break;
                }
                P::thread_memory_management_pool(|| {
                    renderer.render();
                });
            }
            renderer.notify_stopped_running();
            trace!("Stopped renderer thread");
        })
        .unwrap();
}

fn extract_camera<P: Platform>(
    renderer: Res<RendererResourceWrapper<P>>,
    active_camera: Res<ActiveCamera>,
    camera_entities: Query<(&InterpolatedTransform, &Camera, &GlobalTransform)>,
) {
    if renderer.sender.is_saturated() {
        return;
    }

    if let Ok((interpolated, camera, transform)) = camera_entities.get(active_camera.0) {
        if camera.interpolate_rotation {
            renderer
                .sender
                .update_camera_transform(interpolated.0, camera.fov);
        } else {
            let mut combined_transform = transform.affine();
            combined_transform.translation = interpolated.0.translation;
            renderer
                .sender
                .update_camera_transform(combined_transform, camera.fov);
        }
    }
}

fn extract_static_renderables<P: Platform>(
    renderer: Res<RendererResourceWrapper<P>>,
    static_renderables: Query<(Entity, Ref<StaticRenderableComponent>, Ref<InterpolatedTransform>)>,
    mut removed_static_renderables: RemovedComponents<StaticRenderableComponent>,
) {
    for (entity, renderable, transform) in static_renderables.iter() {
        if renderable.is_added() || transform.is_added() {
            renderer
                .sender
                .register_static_renderable(entity, transform.as_ref(), renderable.as_ref());
        } else if !renderer.sender.is_saturated() {
            renderer.sender.update_transform(entity, transform.0);
        }
    }

    for entity in removed_static_renderables.read() {
        renderer.sender.unregister_static_renderable(entity);
    }
}

fn extract_point_lights<P: Platform>(
    renderer: Res<RendererResourceWrapper<P>>,
    point_lights: Query<(Entity, Ref<PointLightComponent>, Ref<InterpolatedTransform>)>,
    mut removed_point_lights: RemovedComponents<PointLightComponent>,
) {
    for (entity, light, transform) in point_lights.iter() {
        if light.is_added() || transform.is_added() {
            renderer
                .sender
                .register_point_light(entity, transform.as_ref(), light.as_ref());
        } else if !renderer.sender.is_saturated() {
            renderer.sender.update_transform(entity, transform.0);
        }
    }

    for entity in removed_point_lights.read() {
        renderer.sender.unregister_point_light(entity);
    }
}

fn extract_directional_lights<P: Platform>(
    renderer: Res<RendererResourceWrapper<P>>,
    directional_lights: Query<(Entity, Ref<DirectionalLightComponent>, Ref<InterpolatedTransform>)>,
    mut removed_directional_lights: RemovedComponents<DirectionalLightComponent>,
) {
        for (entity, light, transform) in directional_lights.iter() {
        if light.is_added() || transform.is_added() {
            renderer
                .sender
                .register_directional_light(entity, transform.as_ref(), light.as_ref());
        } else if !renderer.sender.is_saturated() {
            //renderer.sender.update_transform(entity, transform.0);
        }
    }

    for entity in removed_directional_lights.read() {
        renderer.sender.unregister_directional_light(entity);
    }
}

#[cfg(feature = "threading")]
fn end_frame<P: Platform>(renderer: ResMut<RendererResourceWrapper<P>>) {
    if renderer.sender.is_saturated() {
        return;
    }

    renderer.sender.end_frame();
}

fn begin_frame<P: Platform>(renderer: ResMut<RendererResourceWrapper<P>>) {
    // Unblock regularly so the fixed time systems can run.
    // All rendering systems check if the renderer is saturated before sending new commands.
    renderer.sender.wait_until_available(Duration::from_micros(1000000u64 / 4u64 / (TICK_RATE as u64)));
}
