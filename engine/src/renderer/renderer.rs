use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Arc,
    Condvar,
    Mutex
};

use bevy_ecs::entity::Entity;
use bevy_ecs::system::Resource;
use bevy_math::Affine3A;
use crossbeam_channel::{
    unbounded,
    Sender,
};
use instant::Duration;
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
};
use super::StaticRenderableComponent;
use crate::asset::AssetManager;
use crate::input::Input;
use crate::renderer::command::RendererCommand;
use crate::renderer::RendererInternal;
use crate::transform::InterpolatedTransform;
use crate::ui::UIDrawData;
use crate::graphics::*;

struct RendererState {
    queued_frames_counter: Mutex<u32>, // we need the mutex for the condvar anyway
    is_running: AtomicBool,
    cond_var: Condvar,
}

pub struct RendererSender<B: GPUBackend> {
    sender: Sender<RendererCommand<B>>,
    state: Arc<RendererState>
}

pub struct Renderer<P: Platform> {
    window_event_sender: Sender<Event<P>>,
    device: Arc<Device<P::GPUBackend>>,
    internal: RendererInternal<P>,
    state: Arc<RendererState>
}

impl<P: Platform> Renderer<P> {
    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
        swapchain: Swapchain<P::GPUBackend>,
        asset_manager: &Arc<AssetManager<P>>,
        console: &Arc<Console>,
    ) -> (Renderer<P>, RendererSender<P::GPUBackend>) {
        let (sender, receiver) = unbounded::<RendererCommand<P::GPUBackend>>();
        let (window_event_sender, window_event_receiver) = unbounded();

        let internal = RendererInternal::new(
            device,
            swapchain,
            asset_manager,
            window_event_receiver,
            receiver,
            console,
        );

        let renderer = Self {
            device: device.clone(),
            window_event_sender,
            internal,
            state: Arc::new(RendererState {
                queued_frames_counter: Mutex::new(0),
                is_running: AtomicBool::new(true),
                cond_var: Condvar::new(),
            }),
        };
        let renderer_sender = RendererSender {
            sender,
            state: renderer.state.clone()
        };

        (renderer, renderer_sender)
    }

    pub(crate) fn instance(&self) -> &Arc<Instance<P::GPUBackend>> {
        self.device.instance()
    }

    pub fn dispatch_window_event(&self, event: Event<P>) {
        self.window_event_sender.send(event).unwrap();
    }

    pub fn device(&self) -> &Arc<Device<P::GPUBackend>> {
        &self.device
    }

    pub fn render(&mut self) {
        P::thread_memory_management_pool(|| {
            self.internal.render();
        });

        // Dec queued frame counter
        let mut counter_guard = self.state.queued_frames_counter.lock().unwrap();
        *counter_guard -= 1;
        self.state.cond_var.notify_all();
    }

    pub fn is_running(&self) -> bool {
        self.state.is_running.load(Ordering::Acquire)
    }

    pub fn notify_stopped_running(&self) {
        self.state.is_running.store(false, Ordering::Release);
    }
}

impl<B: GPUBackend> RendererSender<B> {
    pub fn register_static_renderable(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        renderable: &StaticRenderableComponent,
    ) {
        let result = self.sender.send(RendererCommand::<B>::RegisterStatic {
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

    pub fn unregister_static_renderable(&self, entity: Entity) {
        let result = self.sender.send(RendererCommand::<B>::UnregisterStatic(entity));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn register_point_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        component: &PointLightComponent,
    ) {
        let result = self.sender.send(RendererCommand::<B>::RegisterPointLight {
            entity,
            transform: transform.0,
            intensity: component.intensity,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn unregister_point_light(&self, entity: Entity) {
        let result = self
            .sender
            .send(RendererCommand::UnregisterPointLight(entity));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn register_directional_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        component: &DirectionalLightComponent,
    ) {
        let result = self.sender.send(RendererCommand::<B>::RegisterDirectionalLight {
            entity,
            transform: transform.0,
            intensity: component.intensity,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn unregister_directional_light(&self, entity: Entity) {
        let result = self
            .sender
            .send(RendererCommand::UnregisterDirectionalLight(entity));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn update_camera_transform(&self, camera_transform: Affine3A, fov: f32) {
        let result = self.sender.send(RendererCommand::<B>::UpdateCameraTransform {
            camera_transform,
            fov,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn update_transform(&self, entity: Entity, transform: Affine3A) {
        let result = self.sender.send(RendererCommand::<B>::UpdateTransform {
            entity,
            transform: transform,
        });
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn end_frame(&self) {
        let mut queued_guard = self.state.queued_frames_counter.lock().unwrap();
        *queued_guard += 1;
        let result = self.sender.send(RendererCommand::<B>::EndFrame);
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn update_lightmap(&self, path: &str) {
        let result = self
            .sender
            .send(RendererCommand::<B>::SetLightmap(path.to_string()));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn wait_until_available(&self, timeout: Duration) {
        let queued_guard = self.state.queued_frames_counter.lock().unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self.state.cond_var
            .wait_timeout_while(queued_guard, timeout, |queued| {
                *queued > 1 || !self.state.is_running.load(Ordering::Acquire)
            })
            .unwrap();
        #[cfg(target_arch = "wasm32")]
        let _ = self
            .cond_var
            .wait_while(queued_guard, |queued| *queued > 1)
            .unwrap();
    }

    pub fn is_saturated(&self) -> bool {
        let queued_guard: std::sync::MutexGuard<u32> = self.state.queued_frames_counter.lock().unwrap();
        *queued_guard > 1
    }

    pub fn is_running(&self) -> bool {
        self.state.is_running.load(Ordering::Acquire)
    }

    pub fn update_ui(&self, ui_data: UIDrawData<B>) {
        let result = self.sender.send(RendererCommand::RenderUI(ui_data));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }

    pub fn unblock_game_thread(&self) {
        self.state.cond_var.notify_all();
    }

    pub fn stop(&self) {
        trace!("Stopping renderer");
        if cfg!(feature = "threading") {
            let was_running = self.state.is_running.swap(false, Ordering::Release);
            if !was_running {
                return;
            }

            let end_frame_res = self.sender.send(RendererCommand::<B>::Quit);
            if end_frame_res.is_err() {
                log::error!("Render thread crashed.");
            }

            self.unblock_game_thread();
        }
    }
}
