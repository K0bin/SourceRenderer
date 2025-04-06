#[cfg(feature = "threading")]
use std::thread::JoinHandle;

use std::mem::ManuallyDrop;

use web_time::Duration;

use atomic_refcell::AtomicRefCell;
use bevy_app::{
    App, AppExit, Last, Plugin
};
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::entity::Entity;
use bevy_ecs::event::{Event, EventWriter};
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
use crate::EngineLoopFuncResult;
use crate::asset::AssetManagerECSResource;
use crate::engine::{
    ConsoleResource, WindowState, TICK_RATE
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

        let (renderer, sender) = Renderer::new(
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
        let pre_init_res = app.world().resource::<PreInitRendererResourceWrapper>();
        let mut renderer_borrow = pre_init_res.renderer.borrow_mut();
        let renderer = renderer_borrow.get();

        let ready = renderer.is_ready();
        if ready {
            info!("Renderer ready! Done compiling all mandatory shaders.")
        }
        ready
    }

    fn finish(&self, app: &mut App) {
        let pre_init_wrapper = app.world_mut().remove_resource::<PreInitRendererResourceWrapper>().unwrap();

        let PreInitRendererResourceWrapper { renderer: renderer_cell, sender } = pre_init_wrapper;
        let renderer = SyncCell::to_inner(AtomicRefCell::into_inner(renderer_cell));
        insert_renderer_resource(app, renderer, sender);
        install_renderer_systems(app);
    }
}

impl<P: Platform> RendererPlugin<P> {
    pub fn new() -> Self {
        Self(PlatformPhantomData::default())
    }
    pub fn window_changed(app: &App, window_state: WindowState) {
        let resource = app.world().get_resource::<RendererResourceWrapper>();
        if let Some(resource) = resource {
            // It might not be finished initializing yet.
            resource.sender.window_changed(window_state);
        }
    }
}

#[derive(Resource)]
struct PreInitRendererResourceWrapper {
    renderer: AtomicRefCell<SyncCell<Renderer>>,
    sender: RendererSender,
}

#[derive(Resource)]
struct RendererResourceWrapper {
    sender: ManuallyDrop<RendererSender>,

    #[cfg(not(feature = "threading"))]
    renderer: SyncCell<Renderer>,

    #[cfg(feature = "threading")]
    thread_handle: ManuallyDrop<JoinHandle<()>>,
}

impl Drop for RendererResourceWrapper {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.sender); }

        #[cfg(feature = "threading")]
        {
            let handle = unsafe { ManuallyDrop::take(&mut self.thread_handle) };
            handle.join().unwrap();
        }
    }
}

#[cfg(not(feature = "threading"))]
fn install_renderer_systems(
    app: &mut App,
) {
    app.add_systems(
        Last,
        (
            extract_camera,
            extract_static_renderables,
            extract_point_lights,
            extract_directional_lights,
        )
            .in_set(ExtractSet),
    );
    app.add_systems(Last, end_frame.after(ExtractSet));
}

#[cfg(not(feature = "threading"))]
fn insert_renderer_resource(
    app: &mut App,
    renderer: Renderer,
    sender: RendererSender,
) {
    let wrapper = RendererResourceWrapper {
        renderer: SyncCell::new(renderer),
        sender: ManuallyDrop::new(sender),
    };
    app.insert_resource(wrapper);
}

#[allow(unused)]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct SyncSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct ExtractSet;

#[cfg(feature = "threading")]
fn install_renderer_systems(
    app: &mut App,
) {
    app.add_systems(
        Last,
        (begin_frame).in_set(SyncSet)
    );
    app.add_systems(
        Last,
        (
            extract_camera,
            extract_static_renderables,
            extract_point_lights,
            extract_directional_lights,
        )
            .in_set(ExtractSet)
            .after(SyncSet),
    );
    app.add_systems(Last, end_frame.after(ExtractSet));
}

#[cfg(feature = "threading")]
fn insert_renderer_resource(
    app: &mut App,
    renderer: Renderer,
    sender: RendererSender
) {
    let handle = start_render_thread(renderer);

    let wrapper = RendererResourceWrapper {
        sender: ManuallyDrop::new(sender),
        thread_handle: ManuallyDrop::new(handle),
    };
    app.insert_resource(wrapper);
}

#[cfg(feature = "threading")]
fn start_render_thread(mut renderer: Renderer) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("RenderThread".to_string())
        .spawn(move || {
            log::trace!("Started renderer thread");
            'renderer_loop: loop {
                let mut result = EngineLoopFuncResult::Exit;
                crate::autoreleasepool(|| {
                    result = renderer.render();
                });
                if result == EngineLoopFuncResult::Exit {
                    break 'renderer_loop;
                }
            }
        })
        .unwrap()
}

fn extract_camera(
    mut events: EventWriter<AppExit>,
    renderer: Res<RendererResourceWrapper>,
    active_camera: Res<ActiveCamera>,
    camera_entities: Query<(&InterpolatedTransform, &Camera, &GlobalTransform)>,
) {
    if renderer.sender.is_saturated() {
        return;
    }

    if let Ok((interpolated, camera, transform)) = camera_entities.get(active_camera.0) {
        if camera.interpolate_rotation {
            let result = renderer
                .sender
                .update_camera_transform(interpolated.0, camera.fov);

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        } else {
            let mut combined_transform = transform.affine();
            combined_transform.translation = interpolated.0.translation;
            let result = renderer
                .sender
                .update_camera_transform(combined_transform, camera.fov);

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        }
    }
}

fn extract_static_renderables(
    mut events: EventWriter<AppExit>,
    renderer: Res<RendererResourceWrapper>,
    static_renderables: Query<(Entity, Ref<StaticRenderableComponent>, Ref<InterpolatedTransform>)>,
    mut removed_static_renderables: RemovedComponents<StaticRenderableComponent>,
) {
    for (entity, renderable, transform) in static_renderables.iter() {
        if renderable.is_added() || transform.is_added() {
            let result = renderer
                .sender
                .register_static_renderable(entity, transform.as_ref(), renderable.as_ref());

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        } else if !renderer.sender.is_saturated() {
            let result = renderer.sender.update_transform(entity, transform.0);

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        }
    }

    if !removed_static_renderables.is_empty() {
        debug!("Removing {} static renderables", removed_static_renderables.len());
    }
    for entity in removed_static_renderables.read() {
        let result = renderer.sender.unregister_static_renderable(entity);

        if result.is_err() {
            events.send(AppExit::from_code(1));
        }
    }
}

fn extract_point_lights(
    mut events: EventWriter<AppExit>,
    renderer: Res<RendererResourceWrapper>,
    point_lights: Query<(Entity, Ref<PointLightComponent>, Ref<InterpolatedTransform>)>,
    mut removed_point_lights: RemovedComponents<PointLightComponent>,
) {
    for (entity, light, transform) in point_lights.iter() {
        if light.is_added() || transform.is_added() {
            let result = renderer
                .sender
                .register_point_light(entity, transform.as_ref(), light.as_ref());

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        } else if !renderer.sender.is_saturated() {
            let result = renderer.sender.update_transform(entity, transform.0);

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        }
    }

    for entity in removed_point_lights.read() {
        let result = renderer.sender.unregister_point_light(entity);

        if result.is_err() {
            events.send(AppExit::from_code(1));
        }
    }
}

fn extract_directional_lights(
    mut events: EventWriter<AppExit>,
    renderer: Res<RendererResourceWrapper>,
    directional_lights: Query<(Entity, Ref<DirectionalLightComponent>, Ref<InterpolatedTransform>)>,
    mut removed_directional_lights: RemovedComponents<DirectionalLightComponent>,
) {
        for (entity, light, transform) in directional_lights.iter() {
        if light.is_added() || transform.is_added() {
            let result = renderer
                .sender
                .register_directional_light(entity, transform.as_ref(), light.as_ref());

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        } else if !renderer.sender.is_saturated() {
            let result = renderer.sender.update_transform(entity, transform.0);

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        }
    }

    for entity in removed_directional_lights.read() {
        let result = renderer.sender.unregister_directional_light(entity);

        if result.is_err() {
            events.send(AppExit::from_code(1));
        }
    }
}

#[allow(unused_mut)]
fn end_frame(
    mut events: EventWriter<AppExit>,
    mut renderer: ResMut<RendererResourceWrapper>
) {
    if renderer.sender.is_saturated() {
        return;
    }

    let result = renderer.sender.end_frame();
    if result.is_err() {
        events.send(AppExit::from_code(1));
    }

    #[cfg(not(feature = "threading"))]
    {
        let frame_result = renderer.renderer.get().render();
        if frame_result == EngineLoopFuncResult::Exit {
            events.send(AppExit::from_code(1));
        }
    }
}

#[allow(unused)]
fn begin_frame(renderer: ResMut<RendererResourceWrapper>) {
    // Unblock regularly so the fixed time systems can run.
    // All rendering systems check if the renderer is saturated before sending new commands.
    renderer.sender.wait_until_available(Duration::from_micros(1000000u64 / 4u64 / (TICK_RATE as u64)));
}
