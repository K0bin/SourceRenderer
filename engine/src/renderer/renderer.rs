use std::sync::Arc;
use crate::{Mutex, Condvar};

use web_time::{Duration, Instant};
use bevy_ecs::entity::Entity;
use bevy_math::Affine3A;
use crossbeam_channel::{
    unbounded, Receiver, SendError, Sender, TryRecvError
};
use sourcerenderer_core::platform::Platform;
use sourcerenderer_core::{
    console::Console, Vec3
};

use super::drawable::{make_camera_proj, make_camera_view, RendererStaticDrawable};
use super::ecs::{
    DirectionalLightComponent,
    PointLightComponent,
};
use super::light::DirectionalLight;
use super::passes::web::WebRenderer;
use super::render_path::{FrameInfo, RenderPath, SceneInfo};
use super::renderer_culling::update_visibility;
use super::renderer_resources::RendererResources;
use super::renderer_scene::RendererScene;
use super::{PointLight, StaticRenderableComponent};
use crate::asset::{AssetManager, AssetType};
use crate::engine::{EngineLoopFuncResult, WindowState};
use crate::renderer::command::RendererCommand;
use crate::transform::InterpolatedTransform;
use crate::ui::UIDrawData;
use crate::graphics::*;

//#[cfg(not(target_arch = "wasm32"))]
//use super::passes::modern::ModernRenderer;

struct RendererState {
    queued_frames_counter: Mutex<u32>, // we need the mutex for the condvar anyway
    cond_var: Condvar,
}

pub struct RendererSender {
    sender: Option<Sender<RendererCommand>>,
    state: Arc<RendererState>,
}

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
enum ReceiveMessagesResult {
    FrameCompleted,
    Quit,
    WaitForMessages
}

pub struct Renderer<P: Platform> {
    device: Arc<Device>,
    state: Arc<RendererState>,
    receiver: Receiver<RendererCommand>,
    asset_manager: Arc<AssetManager>,
    resources: RendererResources,
    scene: RendererScene,
    context: GraphicsContext,
    swapchain: Arc<Mutex<Swapchain>>,
    render_path: Box<dyn RenderPath<P>>,

    last_frame: Instant,
    frame: u64
}

impl<P: Platform> Drop for Renderer<P> {
    fn drop(&mut self) {
        let mut counter_guard = self.state.queued_frames_counter.lock().unwrap();
        *counter_guard = 0;
        self.state.cond_var.notify_all();
    }
}

impl<P: Platform> Renderer<P> {
    pub fn new(
        device: &Arc<Device>,
        swapchain: Swapchain,
        asset_manager: &Arc<AssetManager>,
        _console: &Arc<Console>,
    ) -> (Self, RendererSender) {
        log::info!("Initializing renderer with {} backend", crate::graphics::ActiveBackend::name());

        let (sender, receiver) = unbounded::<RendererCommand>();
        let mut context: GraphicsContext = device.create_context();

        let mut resources = RendererResources::new(device);

        log::trace!("Initializing render path");
        let render_path = Box::new(WebRenderer::new(device, &swapchain, &mut context, &mut resources, asset_manager));
        //let render_path: Box<dyn RenderPath> = Box::new(NoOpRenderPath);

        let renderer = Self {
            device: device.clone(),
            state: Arc::new(RendererState {
                queued_frames_counter: Mutex::new(0),
                cond_var: Condvar::new(),
            }),
            receiver,
            asset_manager: asset_manager.clone(),
            resources,
            scene: RendererScene::new(),
            swapchain: Arc::new(Mutex::new(swapchain)),
            context,
            render_path,
            last_frame: Instant::now(),
            frame: 0u64
        };
        let renderer_sender = RendererSender {
            sender: Some(sender),
            state: renderer.state.clone()
        };

        (renderer, renderer_sender)
    }

    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn instance(&self) -> &Arc<Instance> {
        self.device.instance()
    }

    #[inline(always)]
    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }

    pub fn render(&mut self) -> EngineLoopFuncResult {
        self.asset_manager
            .flush_renderer_assets();

        // Flush all submissions from the last frame in case this hasn't happened yet.
        self.device.flush_all();

        let mut message_receiving_result = ReceiveMessagesResult::WaitForMessages;
        if cfg!(feature = "threading") {
            while message_receiving_result == ReceiveMessagesResult::WaitForMessages {
                message_receiving_result = self.receive_messages();
            }
        } else {
            message_receiving_result = self.receive_messages();
            if message_receiving_result == ReceiveMessagesResult::WaitForMessages {
                log::warn!("No finished frame yet.");
                return EngineLoopFuncResult::Exit;
            }
        }

        if message_receiving_result == ReceiveMessagesResult::Quit {
            log::info!("Quitting renderer.");
            return EngineLoopFuncResult::Exit;
        }

        let delta = Instant::now().duration_since(self.last_frame);
        self.last_frame = Instant::now();

        let frame_info = FrameInfo {
            frame: self.frame,
            delta: delta,
        };

        update_visibility(&mut self.scene, &self.asset_manager);

        let assets = self.asset_manager.read_renderer_assets();
        let scene_info = SceneInfo {
            scene: &self.scene,
            active_view_index: 0,
            vertex_buffer: BufferRef::Regular(assets.vertex_buffer()),
            index_buffer: BufferRef::Regular(assets.index_buffer()),
            lightmap: None,
        };

        let mut swapchain_guard = self.swapchain.lock().unwrap();
        let _ = self.context.begin_frame();
        self.asset_manager.bump_frame(&self.context);

        let render_path_result = self.render_path.render(
            &mut self.context,
            &mut swapchain_guard,
            &scene_info,
            &frame_info,
            &mut self.resources,
            &assets
        );
        let frame_end_signal = self.context.end_frame();

        match render_path_result {
            Ok(result) => {
                self.device.submit(QueueType::Graphics, QueueSubmission {
                    command_buffer: result.cmd_buffer,
                    wait_fences: &[],
                    signal_fences: &[frame_end_signal],
                    acquire_swapchain: result.backbuffer.as_ref().map(|backbuffer| (&self.swapchain, backbuffer)),
                    release_swapchain: result.backbuffer.as_ref().map(|backbuffer| (&self.swapchain, backbuffer))
                });
                if let Some(backbuffer) = result.backbuffer {
                    self.device.present(QueueType::Graphics, &self.swapchain, backbuffer);
                }
            },
            Err(_swapchain_err) => {
                todo!("Handle swapchain recreation");
            }
        }
        std::mem::drop(swapchain_guard);

        let c_device = self.device.clone();
        std::mem::drop(assets); // TODO: The asset manager needs a bit of an overhaul to avoid this dead lock scenario. (Spawning on a task pool in single thread mode while holding the RW lock)

        bevy_tasks::ComputeTaskPool::get().spawn(async move {
            c_device.flush(QueueType::Graphics)
        }).detach();

        // The WASM task pool will only run it after the function returns.
        // By this time the current context texture might be invalidated.
        // So do it immediately.
        #[cfg(target_arch = "wasm32")]
        self.device.flush(QueueType::Graphics);

        self.resources.swap_history_resources();
        self.frame += 1;

        // Dec queued frame counter
        let mut counter_guard = self.state.queued_frames_counter.lock().unwrap();
        *counter_guard -= 1;
        self.state.cond_var.notify_all();

        EngineLoopFuncResult::KeepRunning
    }

    fn receive_messages(&mut self) -> ReceiveMessagesResult {
        let message_res = self.receiver.try_recv();
        let mut message_opt: Option<RendererCommand>;
        match message_res {
            Ok(message) => { message_opt = Some(message); },
            Err(err) => {
                return match err {
                    TryRecvError::Disconnected => ReceiveMessagesResult::Quit,
                    TryRecvError::Empty => ReceiveMessagesResult::WaitForMessages,
                };
            }
        }

        while message_opt.is_some() {
            let message = message_opt.take().unwrap();
            match message {
                RendererCommand::EndFrame => {
                    return ReceiveMessagesResult::FrameCompleted;
                }

                RendererCommand::UpdateCameraTransform {
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

                RendererCommand::UpdateTransform {
                    entity,
                    transform,
                } => {
                    self.scene.update_transform(&entity, transform);
                }

                RendererCommand::RegisterStatic {
                    model_path,
                    entity,
                    transform,
                    receive_shadows,
                    cast_shadows,
                    can_move,
                } => {
                    let model_handle = self.asset_manager.get_or_reserve_handle(&model_path, AssetType::Model);
                    self.scene.add_static_drawable(
                        entity,
                        RendererStaticDrawable {
                            entity,
                            transform,
                            old_transform: transform,
                            model: model_handle.into(),
                            receive_shadows,
                            cast_shadows,
                            can_move,
                        },
                    );
                }
                RendererCommand::UnregisterStatic(entity) => {
                    self.scene.remove_static_drawable(&entity);
                }

                RendererCommand::RegisterPointLight {
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
                RendererCommand::UnregisterPointLight(entity) => {
                    self.scene.remove_point_light(&entity);
                }

                RendererCommand::RegisterDirectionalLight {
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
                RendererCommand::UnregisterDirectionalLight(entity) => {
                    self.scene.remove_directional_light(&entity);
                }
                RendererCommand::SetLightmap(path) => {
                    let handle = self.asset_manager.get_or_reserve_handle(&path, AssetType::Texture);
                    self.scene.set_lightmap(Some(handle.into()));
                }
                RendererCommand::RenderUI(data) => { self.render_path.set_ui_data(data); },

                RendererCommand::WindowChanged(window_state) => {
                    match window_state {
                        WindowState::Fullscreen(_size) => {},
                        WindowState::Window(_size) => {},
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

    pub fn set_render_path(&mut self, render_path: Box<dyn RenderPath<P>>) {
        self.render_path = render_path;
    }

    pub fn is_ready(&self) -> bool {
        self.render_path.is_ready(&self.asset_manager)
    }
}

impl RendererSender {
    pub fn register_static_renderable(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        renderable: &StaticRenderableComponent,
    ) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::RegisterStatic {
            entity,
            transform: transform.0,
            model_path: renderable.model_path.to_string(),
            receive_shadows: renderable.receive_shadows,
            cast_shadows: renderable.cast_shadows,
            can_move: renderable.can_move,
        })
            .map_err(|_| SendError(()))
    }

    pub fn unregister_static_renderable(&self, entity: Entity) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::UnregisterStatic(entity))
            .map_err(|_| SendError(()))
    }

    pub fn register_point_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        component: &PointLightComponent,
    ) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::RegisterPointLight {
            entity,
            transform: transform.0,
            intensity: component.intensity,
        })
            .map_err(|_| SendError(()))
    }

    pub fn unregister_point_light(&self, entity: Entity) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::UnregisterPointLight(entity))
            .map_err(|_| SendError(()))
    }

    pub fn register_directional_light(
        &self,
        entity: Entity,
        transform: &InterpolatedTransform,
        component: &DirectionalLightComponent,
    ) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::RegisterDirectionalLight {
            entity,
            transform: transform.0,
            intensity: component.intensity,
        })
            .map_err(|_| SendError(()))
    }

    pub fn unregister_directional_light(&self, entity: Entity) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::UnregisterDirectionalLight(entity))
            .map_err(|_| SendError(()))
    }

    pub fn update_camera_transform(&self, camera_transform: Affine3A, fov: f32) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::UpdateCameraTransform {
            camera_transform,
            fov,
        })
            .map_err(|_| SendError(()))
    }

    pub fn update_transform(&self, entity: Entity, transform: Affine3A) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::UpdateTransform {
            entity,
            transform: transform,
        })
            .map_err(|_| SendError(()))
    }

    pub fn end_frame(&self) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        let mut queued_guard = self.state.queued_frames_counter.lock().unwrap();
        *queued_guard += 1;
        sender.send(RendererCommand::EndFrame)
            .map_err(|_| SendError(()))
    }

    pub fn update_lightmap(&self, path: &str) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::SetLightmap(path.to_string()))
            .map_err(|_| SendError(()))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait_until_available(&self, timeout: Duration) {
        let queued_guard = self.state.queued_frames_counter.lock().unwrap();
        let _ = self.state.cond_var
            .wait_timeout_while(queued_guard, timeout, |queued| {
                *queued > 1
            })
            .unwrap();
    }

    #[cfg(target_arch = "wasm32")]
    pub fn wait_until_available(&self, _timeout: Duration) {
    }

    pub fn is_saturated(&self) -> bool {
        let queued_guard: crate::MutexGuard<u32> = self.state.queued_frames_counter.lock().unwrap();
        *queued_guard > 1
    }

    pub fn update_ui(&self, ui_data: UIDrawData) -> Result<(), SendError<()>> {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return Err(SendError(()));
        };

        sender.send(RendererCommand::RenderUI(ui_data))
            .map_err(|_| SendError(()))
    }

    pub fn unblock_game_thread(&self) {
        self.state.cond_var.notify_all();
    }

    pub fn stop(&mut self) {
        log::trace!("Stopping renderer");
        self.sender = None;

        if cfg!(feature = "threading") {
            self.unblock_game_thread();
        }
    }

    pub fn window_changed(&self, window_state: WindowState) {
        let sender = if let Some(sender) = self.sender.as_ref() {
            sender
        } else {
            return;
        };

        let result = sender.send(RendererCommand::WindowChanged(window_state));
        if let Result::Err(err) = result {
            panic!("Sending message to render thread failed {:?}", err);
        }
    }
}
