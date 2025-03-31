use web_time::Duration;

use atomic_refcell::AtomicRefCell;
use bevy_app::{
    App,
    Last,
    Plugin,
};
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::entity::Entity;
use bevy_ecs::event::Event;
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
use bevy_ecs::world::Ref;
use bevy_transform::components::GlobalTransform;
use bevy_utils::synccell::SyncCell;
use log::{debug, info};
use sourcerenderer_core::{
    Platform, PlatformPhantomData, Vec2UI
};

use super::renderer::RendererSender;
use super::{
    DirectionalLightComponent,
    PointLightComponent,
    Renderer,
    StaticRenderableComponent,
};
use crate::asset::AssetManagerECSResource;
use crate::engine::{
    ConsoleResource,
    WindowState, TICK_RATE,
};
use crate::graphics::{GPUDeviceResource, GPUSwapchainResource};
use crate::transform::InterpolatedTransform;
use crate::{
    ActiveCamera,
    Camera,
};

#[allow(unused)]
#[derive(Event)]
struct WindowSizeChangedEvent {
    size: Vec2UI,
}

#[allow(unused)]
#[derive(Event)]
struct WindowMinimized {}

pub struct RendererPlugin<P: Platform>(PlatformPhantomData<P>);
unsafe impl<P: Platform> Send for RendererPlugin<P> {}
unsafe impl<P: Platform> Sync for RendererPlugin<P> {}

impl<P: Platform> Plugin for RendererPlugin<P> {
    fn build(&self, app: &mut App) {
        let swapchain: crate::graphics::Swapchain = app
            .world_mut()
            .remove_resource::<GPUSwapchainResource>()
            .unwrap()
            .0;
        let gpu_resources = app.world().resource::<GPUDeviceResource>();
        let console_resource = app.world().resource::<ConsoleResource>();
        let asset_manager_resource = app.world().resource::<AssetManagerECSResource<P>>();

        let (renderer, sender) = Renderer::<P>::new(
            &gpu_resources.0,
            swapchain,
            &asset_manager_resource.0,
            &console_resource.0,
        );

        let pre_init_wrapper = PreInitRendererResourceWrapper {
            renderer: AtomicRefCell::new(SyncCell::new(renderer)),
            sender
        };
        app.insert_resource(pre_init_wrapper);
    }

    fn ready(&self, app: &App) -> bool {
        let pre_init_res = app.world().resource::<PreInitRendererResourceWrapper<P>>();
        let mut renderer_borrow = pre_init_res.renderer.borrow_mut();
        let renderer = renderer_borrow.get();

        let ready = renderer.is_ready();
        if ready {
            info!("Renderer ready! Done compiling all mandatory shaders.")
        }
        ready
    }

    fn finish(&self, app: &mut App) {
        let pre_init_wrapper = app.world_mut().remove_resource::<PreInitRendererResourceWrapper<P>>().unwrap();

        let PreInitRendererResourceWrapper { renderer: renderer_cell, sender } = pre_init_wrapper;
        let renderer = SyncCell::to_inner(AtomicRefCell::into_inner(renderer_cell));
        insert_renderer_resource::<P>(app, renderer, sender);
        install_renderer_systems::<P>(app);
    }
}

impl<P: Platform> RendererPlugin<P> {
    pub fn new() -> Self {
        Self(PlatformPhantomData::default())
    }

    pub fn stop(app: &mut App) {
        let renderer_resource_opt = app.world_mut().remove_resource::<RendererResourceWrapper<P>>();
        if let Some(resource) = renderer_resource_opt {
            resource.sender.stop();
        }
    }

    pub fn window_changed(app: &App, window_state: WindowState) {
        let resource = app.world().get_resource::<RendererResourceWrapper<P>>();
        if let Some(resource) = resource {
            // It might not be finished initializing yet.
            resource.sender.window_changed(window_state);
        }
    }
}

#[derive(Resource)]
struct PreInitRendererResourceWrapper<P: Platform> {
    renderer: AtomicRefCell<SyncCell<Renderer<P>>>,
    sender: RendererSender,
}

#[derive(Resource)]
struct RendererResourceWrapper<P: Platform> {
    sender: RendererSender,
    _p: PlatformPhantomData<P>,

    #[cfg(not(feature = "threading"))]
    renderer: SyncCell<Renderer<P>>,
}

#[cfg(not(feature = "threading"))]
fn install_renderer_systems<P: Platform>(
    app: &mut App,
) {
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
    app.add_systems(Last, end_frame::<P>.after(ExtractSet));
}

#[cfg(not(feature = "threading"))]
fn insert_renderer_resource<P: Platform>(
    app: &mut App,
    renderer: Renderer<P>,
    sender: RendererSender
) {
    let wrapper = RendererResourceWrapper {
        renderer: SyncCell::new(renderer),
        _p: PlatformPhantomData::default(),
        sender
    };
    app.insert_resource(wrapper);
}

#[allow(unused)]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct SyncSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct ExtractSet;

#[cfg(feature = "threading")]
fn install_renderer_systems<P: Platform>(
    app: &mut App,
) {
    app.add_systems(
        Last,
        (begin_frame::<P>).in_set(SyncSet)
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
fn insert_renderer_resource<P: Platform>(
    app: &mut App,
    renderer: Renderer<P>,
    sender: RendererSender
) {
    let wrapper = RendererResourceWrapper::<P> { sender, _p: PlatformPhantomData::default() };
    app.insert_resource(wrapper);

    start_render_thread(renderer);
}

#[cfg(feature = "threading")]
fn start_render_thread<P: Platform>(mut renderer: Renderer<P>) {
    std::thread::Builder::new()
        .name("RenderThread".to_string())
        .spawn(move || {
            log::trace!("Started renderer thread");
            loop {
                if !renderer.is_running() {
                    break;
                }
                P::thread_memory_management_pool(|| {
                    renderer.render();
                });
            }
            renderer.notify_stopped_running();
            log::trace!("Stopped renderer thread");
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

    if !removed_static_renderables.is_empty() {
        debug!("Removing {} static renderables", removed_static_renderables.len());
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
            renderer.sender.update_transform(entity, transform.0);
        }
    }

    for entity in removed_directional_lights.read() {
        renderer.sender.unregister_directional_light(entity);
    }
}

#[allow(unused_mut)]
fn end_frame<P: Platform>(mut renderer: ResMut<RendererResourceWrapper<P>>) {
    if renderer.sender.is_saturated() {
        return;
    }

    renderer.sender.end_frame();

    #[cfg(not(feature = "threading"))]
    renderer.renderer.get().render();
}

#[allow(unused)]
fn begin_frame<P: Platform>(renderer: ResMut<RendererResourceWrapper<P>>) {
    // Unblock regularly so the fixed time systems can run.
    // All rendering systems check if the renderer is saturated before sending new commands.
    renderer.sender.wait_until_available(Duration::from_micros(1000000u64 / 4u64 / (TICK_RATE as u64)));
}
