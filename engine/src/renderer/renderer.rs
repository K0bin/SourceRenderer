use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Arc,
    Condvar,
    Mutex
};
use std::time::Instant;

use bevy_ecs::entity::Entity;
use bevy_ecs::system::Resource;
use bevy_math::Affine3A;
use crossbeam_channel::{
    unbounded, Receiver, Sender
};
use instant::Duration;
use log::{trace, warn};
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::platform::{
    Event,
    Platform,
    ThreadHandle,
};
use sourcerenderer_core::{
    Console, Matrix4, Vec2UI, Vec3
};

use super::drawable::{make_camera_proj, make_camera_view, RendererStaticDrawable};
use super::ecs::{
    DirectionalLightComponent,
    PointLightComponent,
};
use super::light::DirectionalLight;
use super::passes::web::WebRenderer;
use super::render_path::{FrameInfo, RenderPath, SceneInfo, ZeroTextures};
use super::renderer_assets::RendererAssets;
use super::renderer_culling::update_visibility;
use super::renderer_resources::RendererResources;
use super::renderer_scene::RendererScene;
use super::shader_manager::ShaderManager;
use super::{PointLight, StaticRenderableComponent};
use crate::asset::AssetManager;
use crate::engine::WindowState;
use crate::input::Input;
use crate::renderer::command::RendererCommand;
use crate::transform::InterpolatedTransform;
use crate::ui::UIDrawData;
use crate::graphics::*;

#[cfg(not(target_arch = "wasm32"))]
use super::passes::modern::ModernRenderer;

struct RendererState {
    queued_frames_counter: Mutex<u32>, // we need the mutex for the condvar anyway
    is_running: AtomicBool,
    cond_var: Condvar,
}

pub struct RendererSender<B: GPUBackend> {
    sender: Sender<RendererCommand<B>>,
    state: Arc<RendererState>
}

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
enum ReceiveMessagesResult {
    FrameCompleted,
    Quit,
    WaitForMessages
}

pub struct Renderer<P: Platform> {
    device: Arc<Device<P::GPUBackend>>,
    state: Arc<RendererState>,
    receiver: Receiver<RendererCommand<P::GPUBackend>>,
    asset_manager: Arc<AssetManager<P>>,
    assets: RendererAssets<P>,
    shader_manager: ShaderManager<P>,
    resources: RendererResources<P::GPUBackend>,
    scene: RendererScene<P::GPUBackend>,
    context: GraphicsContext<P::GPUBackend>,
    swapchain: Arc<Swapchain<P::GPUBackend>>,
    render_path: Box<dyn RenderPath<P>>,

    last_frame: Instant,
    frame: u64
}

impl<P: Platform> Renderer<P> {
    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
        swapchain: Swapchain<P::GPUBackend>,
        asset_manager: &Arc<AssetManager<P>>,
        console: &Arc<Console>,
    ) -> (Renderer<P>, RendererSender<P::GPUBackend>) {
        let (sender, receiver) = unbounded::<RendererCommand<P::GPUBackend>>();

        let mut context = device.create_context();
        let mut shader_manager = ShaderManager::new(device, asset_manager);
        //let render_path = Box::new(ModernRenderer::new(device, &swapchain, &mut context, &mut shader_manager));
        let render_path = Box::new(WebRenderer::new(device, &swapchain, &mut context, &mut shader_manager));

        let renderer = Self {
            device: device.clone(),
            state: Arc::new(RendererState {
                queued_frames_counter: Mutex::new(0),
                is_running: AtomicBool::new(true),
                cond_var: Condvar::new(),
            }),
            receiver,
            asset_manager: asset_manager.clone(),
            resources: RendererResources::new(device),
            assets: RendererAssets::new(device),
            shader_manager,
            scene: RendererScene::new(),
            swapchain: Arc::new(swapchain),
            context,
            render_path,
            last_frame: Instant::now(),
            frame: 0u64
        };
        let renderer_sender = RendererSender {
            sender,
            state: renderer.state.clone()
        };

        (renderer, renderer_sender)
    }

    pub async fn try_init(&mut self) {
        
    }

    pub(crate) fn instance(&self) -> &Arc<Instance<P::GPUBackend>> {
        self.device.instance()
    }

    pub fn device(&self) -> &Arc<Device<P::GPUBackend>> {
        &self.device
    }

    pub fn render(&mut self) {
        let mut message_receiving_result = ReceiveMessagesResult::WaitForMessages;
        if cfg!(feature = "threading") {
            while message_receiving_result == ReceiveMessagesResult::WaitForMessages {
                message_receiving_result = self.receive_messages();
            }
        } else {
            message_receiving_result = self.receive_messages();
            if message_receiving_result == ReceiveMessagesResult::WaitForMessages {
                warn!("No finished frame yet.");
                return;
            }
        }

        if message_receiving_result == ReceiveMessagesResult::Quit {
            self.notify_stopped_running();
            return;
        }

        let delta = Instant::now().duration_since(self.last_frame);
        self.last_frame = Instant::now();

        let frame_info = FrameInfo {
            frame: self.frame,
            delta: delta,
        };

        let zero_textures = ZeroTextures {
            zero_texture_view: &self.assets.placeholder_texture().view,
            zero_texture_view_black: &self.assets.placeholder_black().view,
        };

        update_visibility(&mut self.scene, &self.assets);

        let scene_info = SceneInfo {
            scene: &self.scene,
            active_view_index: 0,
            vertex_buffer: BufferRef::Regular(self.assets.vertex_buffer()),
            index_buffer: BufferRef::Regular(self.assets.index_buffer()),
            lightmap: None,
        };

        self.context.begin_frame();
        let result_cmd_buffer = self.render_path.render(
            &mut self.context,
            &self.swapchain,
            &scene_info,
            &zero_textures,
            &frame_info,
            &self.shader_manager,
            &self.assets
        );
        let frame_end_signal = self.context.end_frame();

        match result_cmd_buffer {
            Ok(cmd_buffer) => {
                self.device.submit(QueueType::Graphics, QueueSubmission {
                    command_buffer: cmd_buffer,
                    wait_fences: &[],
                    signal_fences: &[frame_end_signal],
                    acquire_swapchain: Some(&self.swapchain),
                    release_swapchain: Some(&self.swapchain)
                });
                self.device.present(QueueType::Graphics, &self.swapchain);

                let c_device = self.device.clone();
                bevy_tasks::ComputeTaskPool::get().spawn(async move {
                    c_device.flush(QueueType::Graphics)
                });
            },
            Err(_swapchain_err) => {
                todo!("Handle swapchain recreation");
            }
        }


        self.resources.swap_history_resources();
        self.frame += 1;

        // Dec queued frame counter
        let mut counter_guard = self.state.queued_frames_counter.lock().unwrap();
        *counter_guard -= 1;
        self.state.cond_var.notify_all();
    }

    fn receive_messages(&mut self) -> ReceiveMessagesResult {
        while self.shader_manager.has_remaining_mandatory_compilations() {
            // We're waiting for shader compilation on different threads, process some assets in the meantime.
            self.assets
                .receive_assets(&self.asset_manager, &mut self.shader_manager);
        }

        let mut message_opt = if self.assets.is_empty()
            || self.asset_manager.has_open_renderer_assets()
            || self.scene.static_drawables().is_empty()
        {
            // No assets loaded or assets to process in the queue, check if there are renderer messages but don't block.
            let message_res = self.receiver.try_recv();
            if let Err(err) = &message_res {
                if err.is_disconnected() {
                    panic!("Rendering channel closed {:?}", err);
                }
            }
            message_res.ok()
        } else if cfg!(target_arch = "wasm32") {
            let message_res = self.receiver.recv();
            if let Err(err) = &message_res {
                panic!("Rendering channel closed {:?}", err);
            }
            message_res.ok()
        } else {
            // No assets to process, wait for new messages.
            let message_res = self.receiver.recv_timeout(Duration::from_millis(16));
            if let Err(err) = &message_res {
                if err.is_disconnected() {
                    panic!("Rendering channel closed {:?}", err);
                }
            }
            message_res.ok()
        };

        // recv blocks, so do the preparation after receiving the first event
        //self.receive_window_events();
        self.assets
            .receive_assets(&self.asset_manager, &mut self.shader_manager);

        if message_opt.is_none() {
            // Don't even enter the loop below in case there have been messages pushed since then.
            return ReceiveMessagesResult::WaitForMessages;
        }

        while message_opt.is_some() {
            let message = message_opt.take().unwrap();
            match message {
                RendererCommand::<P::GPUBackend>::EndFrame => {
                    return ReceiveMessagesResult::FrameCompleted;
                }

                RendererCommand::<P::GPUBackend>::Quit => {
                    return ReceiveMessagesResult::Quit;
                }

                RendererCommand::<P::GPUBackend>::UpdateCameraTransform {
                    camera_transform,
                    fov,
                } => {
                    let main_view = self.scene.main_view_mut();
                    main_view.camera_transform = camera_transform;
                    main_view.camera_fov = fov;
                    main_view.old_camera_matrix = main_view.proj_matrix * main_view.view_matrix;
                    let (_, rotation, position) = camera_transform.to_scale_rotation_translation();
                    main_view.camera_position = position;
                    main_view.camera_rotation = rotation;
                    main_view.view_matrix = make_camera_view(position, rotation);
                    main_view.proj_matrix = make_camera_proj(
                        main_view.camera_fov,
                        main_view.aspect_ratio,
                        main_view.near_plane,
                        main_view.far_plane,
                    );
                }

                RendererCommand::<P::GPUBackend>::UpdateTransform {
                    entity,
                    transform,
                } => {
                    self.scene.update_transform(&entity, transform);
                }

                RendererCommand::<P::GPUBackend>::RegisterStatic {
                    model_path,
                    entity,
                    transform,
                    receive_shadows,
                    cast_shadows,
                    can_move,
                } => {
                    let model = self.assets.get_or_create_model_handle(&model_path);
                    self.scene.add_static_drawable(
                        entity,
                        RendererStaticDrawable {
                            entity,
                            transform,
                            old_transform: transform,
                            model,
                            receive_shadows,
                            cast_shadows,
                            can_move,
                        },
                    );
                }
                RendererCommand::<P::GPUBackend>::UnregisterStatic(entity) => {
                    self.scene.remove_static_drawable(&entity);
                }

                RendererCommand::<P::GPUBackend>::RegisterPointLight {
                    entity,
                    transform,
                    intensity,
                } => {
                    self.scene.add_point_light(
                        entity,
                        PointLight {
                            position: transform.transform_vector3(Vec3::new(0f32, 0f32, 0f32)),
                            intensity,
                        },
                    );
                }
                RendererCommand::<P::GPUBackend>::UnregisterPointLight(entity) => {
                    self.scene.remove_point_light(&entity);
                }

                RendererCommand::<P::GPUBackend>::RegisterDirectionalLight {
                    entity,
                    transform,
                    intensity,
                } => {
                    let (_, rotation, _) = transform.to_scale_rotation_translation();
                    let base_dir = Vec3::new(0f32, 0f32, 1f32);
                    let dir = rotation.mul_vec3(base_dir);
                    self.scene.add_directional_light(
                        entity,
                        DirectionalLight {
                            direction: dir,
                            intensity,
                        },
                    );
                }
                RendererCommand::<P::GPUBackend>::UnregisterDirectionalLight(entity) => {
                    self.scene.remove_directional_light(&entity);
                }
                RendererCommand::<P::GPUBackend>::SetLightmap(path) => {
                    let handle = self.assets.get_or_create_texture_handle(&path);
                    self.scene.set_lightmap(Some(handle));
                }
                RendererCommand::RenderUI(data) => { self.render_path.set_ui_data(data); },

                RendererCommand::WindowChanged(window_state) => {
                    match window_state {
                        WindowState::Fullscreen(size) => {},
                        WindowState::Window(size) => {},
                        WindowState::Minimized => {}
                    }
                }
            }

            let message_res = self.receiver.try_recv();
            if let Err(err) = &message_res {
                if err.is_disconnected() {
                    panic!("Rendering channel closed {:?}", err);
                }
            }
            message_opt = message_res.ok();
        }
        ReceiveMessagesResult::WaitForMessages
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
            .send(RendererCommand::<B>::UnregisterPointLight(entity));
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
            .send(RendererCommand::<B>::UnregisterDirectionalLight(entity));
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
    }

    pub fn is_saturated(&self) -> bool {
        let queued_guard: std::sync::MutexGuard<u32> = self.state.queued_frames_counter.lock().unwrap();
        *queued_guard > 1
    }

    pub fn is_running(&self) -> bool {
        self.state.is_running.load(Ordering::Acquire)
    }

    pub fn update_ui(&self, ui_data: UIDrawData<B>) {
        let result = self.sender.send(RendererCommand::<B>::RenderUI(ui_data));
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

    pub fn window_changed(&self, window_state: WindowState) {
        let result = self
            .sender
            .send(RendererCommand::<B>::WindowChanged(window_state));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }
}
