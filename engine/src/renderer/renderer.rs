use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Arc,
    Condvar,
    Mutex
};

use crossbeam_channel::{
    unbounded,
    Sender,
};
use instant::Duration;
use legion::systems::Builder;
use legion::{
    Entity,
    Resources,
    World,
};
use log::trace;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::platform::{
    Event,
    Platform,
    ThreadHandle,
};
use sourcerenderer_core::{
    Console,
    Matrix4,
};

use super::ecs::{
    DirectionalLightComponent,
    PointLightComponent,
    RendererInterface,
};
use super::{
    LateLatching,
    StaticRenderableComponent,
};
use crate::asset::AssetManager;
use crate::input::Input;
use crate::renderer::command::RendererCommand;
use crate::renderer::RendererInternal;
use crate::transform::interpolation::InterpolatedTransform;
use crate::ui::UIDrawData;
use crate::graphics::*;

enum RendererImpl<P: Platform> {
    MultiThreaded(P::ThreadHandle),
    SingleThreaded(Box<RendererInternal<P>>),
    Uninitialized,
}

unsafe impl<P: Platform> Send for RendererImpl<P> {}
unsafe impl<P: Platform> Sync for RendererImpl<P> {}

pub struct Renderer<P: Platform> {
    sender: Sender<RendererCommand<P::GPUBackend>>,
    window_event_sender: Sender<Event<P>>,
    instance: Arc<Instance<P::GPUBackend>>,
    device: Arc<Device<P::GPUBackend>>,
    queued_frames_counter: Mutex<u32>,
    is_running: AtomicBool,
    input: Arc<Input>,
    late_latching: Option<Arc<dyn LateLatching<P::GPUBackend>>>,
    cond_var: Condvar,
    renderer_impl: AtomicRefCell<RendererImpl<P>>,
}

impl<P: Platform> Renderer<P> {
    fn new(
        sender: Sender<RendererCommand<P::GPUBackend>>,
        window_event_sender: Sender<Event<P>>,
        instance: &Arc<Instance<P::GPUBackend>>,
        device: &Arc<Device<P::GPUBackend>>,
        input: &Arc<Input>,
        late_latching: Option<&Arc<dyn LateLatching<P::GPUBackend>>>,
    ) -> Self {
        Self {
            sender,
            instance: instance.clone(),
            device: device.clone(),
            queued_frames_counter: Mutex::new(0),
            is_running: AtomicBool::new(true),
            window_event_sender,
            late_latching: late_latching.cloned(),
            input: input.clone(),
            cond_var: Condvar::new(),
            renderer_impl: AtomicRefCell::new(RendererImpl::Uninitialized),
        }
    }

    pub fn run(
        platform: &P,
        instance: &Arc<Instance<P::GPUBackend>>,
        device: &Arc<Device<P::GPUBackend>>,
        swapchain: Swapchain<P::GPUBackend>,
        asset_manager: &Arc<AssetManager<P>>,
        input: &Arc<Input>,
        late_latching: Option<&Arc<dyn LateLatching<P::GPUBackend>>>,
        console: &Arc<Console>,
    ) -> Arc<Renderer<P>> {
        let (sender, receiver) = unbounded::<RendererCommand<P::GPUBackend>>();
        let (window_event_sender, window_event_receiver) = unbounded();
        let renderer = Arc::new(Renderer::new(
            sender.clone(),
            window_event_sender,
            instance,
            device,
            input,
            late_latching,
        ));

        let c_device = device.clone();
        let c_renderer = renderer.clone();
        let c_asset_manager = asset_manager.clone();
        let c_console = console.clone();

        if cfg!(feature = "threading") {
            let thread_handle = platform.start_thread("RenderThread", move || {
                trace!("Started renderer thread");
                let mut internal = RendererInternal::new(
                    &c_device,
                    swapchain,
                    &c_asset_manager,
                    sender,
                    window_event_receiver,
                    receiver,
                    &c_console,
                );
                loop {
                    if !c_renderer.is_running.load(Ordering::SeqCst) {
                        break;
                    }
                    internal.render(&c_renderer);
                }
                c_renderer.is_running.store(false, Ordering::SeqCst);
                trace!("Stopped renderer thread");
            });

            let mut thread_handle_guard = renderer.renderer_impl.borrow_mut();
            *thread_handle_guard = RendererImpl::MultiThreaded(thread_handle);
        } else {
            let internal = RendererInternal::new(
                &c_device,
                swapchain,
                &c_asset_manager,
                sender,
                window_event_receiver,
                receiver,
                &c_console,
            );
            let mut thread_handle_guard = renderer.renderer_impl.borrow_mut();
            *thread_handle_guard = RendererImpl::SingleThreaded(Box::new(internal));
        }

        renderer
    }

    pub fn install(
        self: &Arc<Renderer<P>>,
        _world: &mut World,
        _resources: &mut Resources,
        systems: &mut Builder,
    ) {
        crate::renderer::ecs::install::<P, Arc<Renderer<P>>>(systems, self.clone());
    }

    pub(super) fn dec_queued_frames_counter(&self) {
        let mut counter_guard = self.queued_frames_counter.lock().unwrap();
        *counter_guard -= 1;
        self.cond_var.notify_all();
    }

    pub(crate) fn instance(&self) -> &Arc<Instance<P::GPUBackend>> {
        &self.instance
    }

    pub(crate) fn unblock_game_thread(&self) {
        self.cond_var.notify_all();
    }

    pub fn stop(&self) {
        trace!("Stopping renderer");
        if cfg!(feature = "threading") {
            let was_running = self.is_running.swap(false, Ordering::SeqCst);
            if !was_running {
                return;
            }

            let end_frame_res = self.sender.send(RendererCommand::<P::GPUBackend>::EndFrame);
            if end_frame_res.is_err() {
                log::error!("Render thread crashed.");
            }

            let mut renderer_impl = self.renderer_impl.borrow_mut();

            if let RendererImpl::Uninitialized = &*renderer_impl {
                return;
            }

            self.unblock_game_thread();
            let renderer_impl = std::mem::replace(&mut *renderer_impl, RendererImpl::Uninitialized);

            match renderer_impl {
                RendererImpl::MultiThreaded(thread_handle) => {
                    if let Err(e) = thread_handle.join() {
                        log::error!("Renderer thread did not exit cleanly: {:?}", e);
                    }
                }
                RendererImpl::Uninitialized => {
                    panic!("Renderer was already stopped.");
                }
                _ => {}
            }
        }
    }

    pub fn dispatch_window_event(&self, event: Event<P>) {
        self.window_event_sender.send(event).unwrap();
    }

    pub fn late_latching(&self) -> Option<&dyn LateLatching<P::GPUBackend>> {
        self.late_latching.as_ref().map(|l| l.as_ref())
    }

    pub fn input(&self) -> &Input {
        &self.input
    }

    pub fn device(&self) -> &Arc<Device<P::GPUBackend>> {
        &self.device
    }

    pub fn render(&self) {
        let mut renderer_impl = self.renderer_impl.borrow_mut();
        if let RendererImpl::SingleThreaded(renderer) = &mut *renderer_impl {
            renderer.render(self);
        }
    }
}

impl<P: Platform> RendererInterface<P> for Arc<Renderer<P>> {
    fn register_static_renderable(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        renderable: &StaticRenderableComponent,
    ) {
        let result = self.sender.send(RendererCommand::<P::GPUBackend>::RegisterStatic {
            entity,
            transform: transform.0,
            model_path: renderable.model_path.to_string(),
            receive_shadows: renderable.receive_shadows,
            cast_shadows: renderable.cast_shadows,
            can_move: renderable.can_move,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn unregister_static_renderable(&self, entity: Entity) {
        let result = self.sender.send(RendererCommand::<P::GPUBackend>::UnregisterStatic(entity));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn register_point_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        component: &PointLightComponent,
    ) {
        let result = self.sender.send(RendererCommand::<P::GPUBackend>::RegisterPointLight {
            entity,
            transform: transform.0,
            intensity: component.intensity,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn unregister_point_light(&self, entity: Entity) {
        let result = self
            .sender
            .send(RendererCommand::UnregisterPointLight(entity));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn register_directional_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        component: &DirectionalLightComponent,
    ) {
        let result = self.sender.send(RendererCommand::<P::GPUBackend>::RegisterDirectionalLight {
            entity,
            transform: transform.0,
            intensity: component.intensity,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn unregister_directional_light(&self, entity: Entity) {
        let result = self
            .sender
            .send(RendererCommand::UnregisterDirectionalLight(entity));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn update_camera_transform(&self, camera_transform_mat: Matrix4, fov: f32) {
        let result = self.sender.send(RendererCommand::<P::GPUBackend>::UpdateCameraTransform {
            camera_transform_mat,
            fov,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn update_transform(&self, entity: Entity, transform: Matrix4) {
        let result = self.sender.send(RendererCommand::<P::GPUBackend>::UpdateTransform {
            entity,
            transform_mat: transform,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn end_frame(&self) {
        let mut queued_guard = self.queued_frames_counter.lock().unwrap();
        *queued_guard += 1;
        let result = self.sender.send(RendererCommand::<P::GPUBackend>::EndFrame);
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn update_lightmap(&self, path: &str) {
        let result = self
            .sender
            .send(RendererCommand::<P::GPUBackend>::SetLightmap(path.to_string()));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    fn wait_until_available(&self, timeout: Duration) {
        let queued_guard = self.queued_frames_counter.lock().unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self
            .cond_var
            .wait_timeout_while(queued_guard, timeout, |queued| {
                *queued > 1 || !self.is_running()
            })
            .unwrap();
        #[cfg(target_arch = "wasm32")]
        let _ = self
            .cond_var
            .wait_while(queued_guard, |queued| *queued > 1)
            .unwrap();
    }

    fn is_saturated(&self) -> bool {
        let queued_guard = self.queued_frames_counter.lock().unwrap();
        *queued_guard > 1
    }

    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    fn update_ui(&self, ui_data: UIDrawData<P::GPUBackend>) {
        let result = self.sender.send(RendererCommand::RenderUI(ui_data));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }
}
