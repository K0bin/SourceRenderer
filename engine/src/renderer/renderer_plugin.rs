use std::marker::PhantomData;
use std::sync::Arc;

#[cfg(all(feature = "render_thread", not(target_arch = "wasm32")))]
use std::thread::JoinHandle;

#[cfg(all(feature = "render_thread", target_arch = "wasm32"))]
use crate::wasm::thread::JoinHandle;

use std::mem::ManuallyDrop;

use sourcerenderer_core::console::Console;
use sourcerenderer_core::gpu::Surface as _;
#[cfg(feature = "render_thread")]
use web_time::Duration;

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
    NonSend, NonSendMut, Query, Res, ResMut, Resource
};
use bevy_ecs::world::Ref;
use bevy_transform::components::GlobalTransform;
use bevy_utils::synccell::SyncCell;
use sourcerenderer_core::Vec2UI;
use sourcerenderer_core::platform::GraphicsPlatform;

use super::renderer::{RendererSender, RendererReceiver};
use super::{
    DirectionalLightComponent,
    PointLightComponent,
    Renderer,
    StaticRenderableComponent,
};
use crate::EngineLoopFuncResult;
use crate::asset::{AssetManager, AssetManagerECSResource};
use crate::engine::{
    ConsoleResource, WindowState, TICK_RATE
};
use crate::graphics::{ActiveBackend, Adapter, AdapterType, APIInstance, GPUInstanceResource, GPUSurfaceResource, Instance, Surface, Swapchain};
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

pub struct RendererPlugin<P: GraphicsPlatform<ActiveBackend>>(PhantomData<P>);
unsafe impl<P: GraphicsPlatform<ActiveBackend>> Send for RendererPlugin<P> {}
unsafe impl<P: GraphicsPlatform<ActiveBackend>> Sync for RendererPlugin<P> {}

impl<P: GraphicsPlatform<ActiveBackend>> Plugin for RendererPlugin<P> {
    fn build(&self, app: &mut App) {
        insert_renderer_resource::<P>(app);
        install_renderer_systems(app);
    }
}

impl<P: GraphicsPlatform<ActiveBackend>> RendererPlugin<P> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
    pub fn window_changed(app: &App, window_state: WindowState) {
        #[cfg(any(feature = "render_thread", not(target_arch = "wasm32")))]
        let resource = app.world().get_resource::<RendererResourceWrapper>();
        #[cfg(all(not(feature = "render_thread"), target_arch = "wasm32"))]
        let resource = app.world().get_non_send_resource::<RendererResourceWrapper>();
        if let Some(resource) = resource {
            // It might not be finished initializing yet.
            resource.sender.window_changed(window_state);
        }
    }
}

#[cfg(any(feature = "render_thread", not(target_arch = "wasm32")))]
#[derive(Resource)]
struct RendererResourceWrapper {
    sender: ManuallyDrop<RendererSender>,
    is_saturated: bool,

    #[cfg(not(feature = "render_thread"))]
    renderer: SyncCell<Renderer>,

    #[cfg(feature = "render_thread")]
    thread_handle: ManuallyDrop<JoinHandle<()>>,
}

#[cfg(all(not(feature = "render_thread"), target_arch = "wasm32"))]
struct RendererResourceWrapper {
    sender: ManuallyDrop<RendererSender>,
    is_saturated: bool,

    renderer: SyncCell<Renderer>,
}

impl Drop for RendererResourceWrapper {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.sender); }

        #[cfg(feature = "render_thread")]
        {
            let handle = unsafe { ManuallyDrop::take(&mut self.thread_handle) };
            handle.join().unwrap();
        }
    }
}

#[allow(unused)]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct SyncSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct ExtractSet;

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

#[cfg(any(feature = "render_thread", not(target_arch = "wasm32")))]
type RendererResourceAccessor<'a> = Res<'a, RendererResourceWrapper>;
#[cfg(all(not(feature = "render_thread"), target_arch = "wasm32"))]
type RendererResourceAccessor<'a> = NonSend<'a, RendererResourceWrapper>;

#[cfg(any(feature = "render_thread", not(target_arch = "wasm32")))]
type RendererResourceAccessorMut<'a> = ResMut<'a, RendererResourceWrapper>;
#[cfg(all(not(feature = "render_thread"), target_arch = "wasm32"))]
type RendererResourceAccessorMut<'a> = NonSendMut<'a, RendererResourceWrapper>;

fn insert_renderer_resource<P: GraphicsPlatform<ActiveBackend>>(app: &mut App) {
    let surface_resource: GPUSurfaceResource = app.world_mut().remove_non_send_resource().unwrap();
    let instace_resource: GPUInstanceResource = app.world_mut().remove_non_send_resource().unwrap();

    let GPUSurfaceResource { surface, width: swapchain_width, height: swapchain_height } = surface_resource;
    let instance = instace_resource.0;

    let console_resource = app.world().resource::<ConsoleResource>();
    let asset_manager_resource = app.world().resource::<AssetManagerECSResource>();

    let (sender, receiver) = Renderer::new_channel();

    #[cfg(feature = "render_thread")]
    log::info!("RENDER THREAD");
    #[cfg(not(feature = "render_thread"))]
    log::info!("NO RENDER THREAD");

    #[cfg(feature = "render_thread")]
    let handle = start_render_thread::<P>(
        receiver,
        instance,
        surface,
        swapchain_width,
        swapchain_height,
        &asset_manager_resource.0,
        &console_resource.0
    );

    #[cfg(not(feature = "render_thread"))]
    let renderer = create_renderer(
        receiver,
        instance,
        surface,
        swapchain_width,
        swapchain_height,
        &asset_manager_resource.0,
        &console_resource.0
    );

    let wrapper = RendererResourceWrapper {
        sender: ManuallyDrop::new(sender),
        is_saturated: false,

        #[cfg(not(feature = "render_thread"))]
        renderer: SyncCell::new(renderer),

        #[cfg(feature = "render_thread")]
        thread_handle: ManuallyDrop::new(handle),
    };
    #[cfg(any(feature = "render_thread", not(target_arch = "wasm32")))]
    app.insert_resource(wrapper);
    #[cfg(all(not(feature = "render_thread"), target_arch = "wasm32"))]
    app.insert_non_send_resource(wrapper);
}

#[cfg(all(feature = "render_thread", not(target_arch = "wasm32")))]
fn start_render_thread<P: GraphicsPlatform<ActiveBackend>>(
    receiver: RendererReceiver,
    instance: APIInstance,
    surface: Surface,
    swapchain_width: u32,
    swapchain_height: u32,
    asset_manager: &Arc<AssetManager>,
    console: &Arc<Console>,
) -> std::thread::JoinHandle<()> {

    let c_asset_manager = asset_manager.clone();
    let c_console = console.clone();
    std::thread::Builder::new()
        .name("RenderThread".to_string())
        .spawn(move || {
            log::trace!("Started renderer thread");

            let mut renderer = create_renderer(
                receiver,
                instance,
                surface,
                swapchain_width,
                swapchain_height,
                &c_asset_manager,
                &c_console
            );

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
    renderer: RendererResourceAccessor,
    active_camera: Res<ActiveCamera>,
    camera_entities: Query<(&InterpolatedTransform, &Camera, &GlobalTransform)>,
) {
    if renderer.is_saturated {
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
    renderer: RendererResourceAccessor,
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
        } else if !renderer.is_saturated {
            let result = renderer.sender.update_transform(entity, transform.0);

            if result.is_err() {
                events.send(AppExit::from_code(1));
            }
        }
    }

    if !removed_static_renderables.is_empty() {
        log::debug!("Removing {} static renderables", removed_static_renderables.len());
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
    renderer: RendererResourceAccessor,
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
        } else if !renderer.is_saturated {
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
    renderer: RendererResourceAccessor,
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
        } else if !renderer.is_saturated {
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
    mut renderer: RendererResourceAccessorMut
) {
    #[cfg(feature = "render_thread")]
    if renderer.is_saturated {
        return;
    }

    if !renderer.is_saturated {
        let result = renderer.sender.end_frame();
        if result.is_err() {
            events.send(AppExit::from_code(1));
        }
    }

    #[cfg(not(feature = "render_thread"))]
    {
        let frame_result = renderer.renderer.get().render();
        if frame_result == EngineLoopFuncResult::Exit {
            events.send(AppExit::from_code(1));
        }
    }
}

#[allow(unused)]
fn begin_frame(mut renderer: RendererResourceAccessorMut) {
    // Unblock regularly so the fixed time systems can run.
    // All rendering systems check if the renderer is saturated before sending new commands.
    #[cfg(feature = "render_thread")]
    renderer.sender.wait_until_available(Duration::from_micros(1000000u64 / 4u64 / (TICK_RATE as u64)));

    // Update saturated only at the beginning of the frame to avoid inconsistent
    // states caused by the renderer suddenly becoming available in the middle of an update.
    renderer.is_saturated = renderer.sender.is_saturated();
}

fn create_renderer(
    receiver: RendererReceiver,
    instance: APIInstance,
    surface: Surface,
    swapchain_width: u32,
    swapchain_height: u32,
    asset_manager: &Arc<AssetManager>,
    console: &Arc<Console>,
) -> Renderer {
    let instance = Instance::new(instance);
    let adapter = pick_adapter(instance.list_adapters());
    let device = adapter.create_device(&surface);

    let core_swapchain = unsafe { surface.create_swapchain(swapchain_width, swapchain_height, true, device.handle()).unwrap() };
    let swapchain = Swapchain::new(core_swapchain, &device);

    Renderer::new(&device, swapchain, receiver, asset_manager, console)
}

fn pick_adapter(adapters: &[Adapter]) -> &Adapter {
    for adapter in adapters {
        if adapter.adapter_type() == AdapterType::Discrete {
            return adapter;
        }
    }
    for adapter in adapters {
        if adapter.adapter_type() == AdapterType::Integrated {
            return adapter;
        }
    }
    adapters.first().expect("No adapter found")
}

#[cfg(all(target_arch = "wasm32", feature = "render_thread"))]
mod wasm {
    use std::{cell::{Cell, RefCell}, rc::Rc};

    use super::*;
    use web_sys::{OffscreenCanvas, DedicatedWorkerGlobalScope, Navigator};
    use wasm_bindgen::{JsCast, prelude::wasm_bindgen, closure::Closure};
    use sourcerenderer_webgpu::{WebGPUInstance, WebGPUSurface, NavigatorKind};

    use crate::AsyncCounter;

    pub fn start_render_thread<P: GraphicsPlatform<ActiveBackend>>(
        receiver: RendererReceiver,
        instance: APIInstance,
        surface: Surface,
        swapchain_width: u32,
        swapchain_height: u32,
        asset_manager: &Arc<AssetManager>,
        console: &Arc<Console>,
    ) -> crate::wasm::thread::JoinHandle<()> {
        log::info!("Using WASM render thread");
        let c_asset_manager = asset_manager.clone();
        let c_console = console.clone();

        crate::wasm::thread::spawn_with_js_val(async move |data| {
            let scope: DedicatedWorkerGlobalScope = js_sys::global().dyn_into().unwrap();
            let navigator = scope.navigator();
            let webgpu_init = WebGPUInstance::async_init(NavigatorKind::Worker(&navigator)).await.unwrap();
            let instance = WebGPUInstance::new(&webgpu_init, false);
            let canvas: OffscreenCanvas = data.dyn_into().unwrap();
            let surface = WebGPUSurface::new(&instance, canvas).unwrap();

            let counter = Arc::new(AsyncCounter::new(0));
            counter.increment();

            let mut renderer = create_renderer(
                receiver,
                instance,
                surface,
                swapchain_width,
                swapchain_height,
                &c_asset_manager,
                &c_console,
            );

            let c_counter = counter.clone();
            let c_scope = scope.clone();
            let animation_id = Rc::new(Cell::new(0i32));
            let render_function = Rc::new(RefCell::new(None));
            let c_render_function = render_function.clone();
            let c_animation_id = animation_id.clone();
            *render_function.borrow_mut() = Some(Closure::new(move || {
                let result = renderer.render();
                if result == EngineLoopFuncResult::Exit {
                    // Clean up circular reference
                    let _ = c_render_function.borrow_mut().take();
                    c_counter.set(0);
                } else {
                    let render_function_borrow = c_render_function.borrow();
                    let render_function_ref: &Closure<dyn FnMut()> = render_function_borrow.as_ref().unwrap();
                    let id = c_scope.request_animation_frame(render_function_ref.as_ref().unchecked_ref()).unwrap();
                    c_animation_id.replace(id);
                }
            }));

            let render_function_borrow = render_function.borrow();
            let render_function_ref: &Closure<dyn FnMut()> = render_function_borrow.as_ref().unwrap();
            let id = scope.request_animation_frame(render_function_ref.as_ref().unchecked_ref()).unwrap();
            animation_id.replace(id);

            counter.wait_for_zero().await;
            let final_id = animation_id.take();
            if final_id != 0 {
                let _ = scope.cancel_animation_frame(final_id);
            }
        }, surface.take_js_val(), Some("RenderThread"))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "render_thread"))]
use wasm::start_render_thread;
