use std::sync::{Arc, Mutex, MutexGuard};
use std::fs::File;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use std::io::Read as IORead;

use crossbeam_channel::{Sender, bounded, Receiver, unbounded, TryRecvError};

use nalgebra::{Transform, Matrix3, Matrix1x3, UnitQuaternion};

use sourcerenderer_core::platform::{Platform, Window, WindowState};
use sourcerenderer_core::graphics::{Instance, Adapter, Device, Backend, ShaderType, PipelineInfo, VertexLayoutInfo, InputAssemblerElement, InputRate, ShaderInputElement, Format, RasterizerInfo, FillMode, CullMode, FrontFace, SampleCount, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, BufferUsage, CommandBuffer, Viewport, Scissor, BindingFrequency, Swapchain, RenderGraphTemplateInfo, GraphicsSubpassInfo, PassType, PipelineBinding, PassOutput, RenderPassTextureExtent};
use sourcerenderer_core::graphics::{BACK_BUFFER_ATTACHMENT_NAME, RenderGraphInfo, RenderGraph, LoadAction, StoreAction, PassInfo, SubpassOutput};
use sourcerenderer_core::{Vec2, Vec2I, Vec2UI, Matrix4, Vec3, Quaternion};

use crate::asset::AssetKey;
use crate::asset::AssetManager;
use crate::renderer::{View, Drawable, DrawableType};

use async_std::task;
use sourcerenderer_core::job::{JobScheduler};
use std::sync::atomic::{Ordering, AtomicUsize};
use sourcerenderer_vulkan::VkSwapchain;
use crate::renderer::command::RendererCommand;
use legion::{World, Resources, Schedule, Entity};
use legion::systems::{Builder as SystemBuilder, Builder};
use std::time::SystemTime;
use crate::renderer::RendererInternal;
use crate::renderer::camera::PrimaryCamera;

pub struct Renderer<P: Platform> {
  sender: Sender<RendererCommand>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  window_state: Mutex<WindowState>,
  queued_frames_counter: AtomicUsize,
  primary_camera: Arc<PrimaryCamera<P::GraphicsBackend>>
}

impl<P: Platform> Renderer<P> {
  fn new(sender: Sender<RendererCommand>, device: &Arc<<P::GraphicsBackend as Backend>::Device>, window: &P::Window) -> Self {
    Self {
      sender,
      device: device.clone(),
      window_state: Mutex::new(window.state()),
      queued_frames_counter: AtomicUsize::new(0),
      primary_camera: Arc::new(PrimaryCamera::new(device.as_ref()))
    }
  }

  pub fn run(window: &P::Window,
             device: &Arc<<P::GraphicsBackend as Backend>::Device>,
             swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
             asset_manager: &Arc<AssetManager<P>>,
             simulation_tick_rate: u32) -> Arc<Renderer<P>> {
    let (sender, receiver) = unbounded::<RendererCommand>();
    let renderer = Arc::new(Renderer::new(sender.clone(), device, window));
    let mut internal = RendererInternal::new(&renderer, &device, &swapchain, asset_manager, sender, receiver, simulation_tick_rate, renderer.primary_camera());

    std::thread::spawn(move || {
      'render_loop: loop {
        internal.render();
      }
    });
    renderer
  }

  pub fn primary_camera(&self) -> &Arc<PrimaryCamera<P::GraphicsBackend>> {
    &self.primary_camera
  }

  pub fn set_window_state(&self, window_state: WindowState) {
    let mut guard = self.window_state.lock().unwrap();
    std::mem::replace(&mut *guard, window_state);
  }

  pub fn install(self: &Arc<Renderer<P>>, world: &mut World, resources: &mut Resources, systems: &mut Builder) {
    crate::renderer::ecs::install(systems, self);
  }

  pub fn register_static_renderable(&self, renderable: Drawable) {
    let result = self.sender.send(RendererCommand::Register(renderable));
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn unregister_static_renderable(&self, entity: Entity) {
    let result = self.sender.send(RendererCommand::UnregisterStatic(entity));
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn update_camera_transform(&self, camera_transform_mat: Matrix4, fov: f32) {
    let result = self.sender.send(RendererCommand::UpdateCameraTransform { camera_transform_mat, fov });
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn update_transform(&self, entity: Entity, transform: Matrix4) {
    let result = self.sender.send(RendererCommand::UpdateTransform { entity, transform_mat: transform });
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn end_frame(&self) {
    self.queued_frames_counter.fetch_add(1, Ordering::SeqCst);
    let result = self.sender.send(RendererCommand::EndFrame);
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn is_saturated(&self) -> bool {
    self.queued_frames_counter.load(Ordering::SeqCst) > 2
  }

  pub(super) fn window_state(&self) -> &Mutex<WindowState> {
    &self.window_state
  }

  pub(super) fn dec_queued_frames_counter(&self) -> usize {
    self.queued_frames_counter.fetch_sub(1, Ordering::SeqCst)
  }
}
