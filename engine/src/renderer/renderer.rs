use std::sync::{Arc, Mutex};
use crossbeam_channel::{Sender, unbounded};

use sourcerenderer_core::platform::{Platform, Window, WindowState};
use sourcerenderer_core::graphics::{Backend, Surface, Swapchain};
use sourcerenderer_core::Matrix4;

use crate::asset::AssetManager;
use crate::renderer::Drawable;

use std::sync::atomic::{Ordering, AtomicUsize};

use crate::renderer::command::RendererCommand;
use legion::{World, Resources, Entity};
use legion::systems::Builder;

use crate::renderer::RendererInternal;
use crate::renderer::camera::LateLatchCamera;

pub struct Renderer<P: Platform> {
  sender: Sender<RendererCommand>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  window_state: Mutex<WindowState>,
  queued_frames_counter: AtomicUsize,
  primary_camera: Arc<LateLatchCamera<P::GraphicsBackend>>,
  surface: Mutex<Option<Arc<<P::GraphicsBackend as Backend>::Surface>>>
}

impl<P: Platform> Renderer<P> {
  fn new(sender: Sender<RendererCommand>, device: &Arc<<P::GraphicsBackend as Backend>::Device>, window: &P::Window, swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>) -> Self {
    Self {
      sender,
      device: device.clone(),
      window_state: Mutex::new(window.state()),
      queued_frames_counter: AtomicUsize::new(0),
      primary_camera: Arc::new(LateLatchCamera::new(device.as_ref())),
      surface: Mutex::new(Some(swapchain.surface().clone()))
    }
  }

  pub fn run(window: &P::Window,
             device: &Arc<<P::GraphicsBackend as Backend>::Device>,
             swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
             asset_manager: &Arc<AssetManager<P>>) -> Arc<Renderer<P>> {
    let (sender, receiver) = unbounded::<RendererCommand>();
    let renderer = Arc::new(Renderer::new(sender.clone(), device, window, swapchain));

    let c_device = device.clone();
    let c_renderer = renderer.clone();
    let c_swapchain = swapchain.clone();
    let c_asset_manager = asset_manager.clone();

    std::thread::Builder::new()
      .name("RenderThread".to_string())
      .spawn(move || {
      let mut internal = RendererInternal::new(&c_renderer, &c_device, &c_swapchain, &c_asset_manager, sender, receiver, c_renderer.primary_camera());
      loop {
        internal.render();
      }
    });
    renderer
  }

  pub fn primary_camera(&self) -> &Arc<LateLatchCamera<P::GraphicsBackend>> {
    &self.primary_camera
  }

  pub fn set_window_state(&self, window_state: WindowState) {
    let mut guard = self.window_state.lock().unwrap();
    *guard = window_state
  }

  pub fn install(self: &Arc<Renderer<P>>, _world: &mut World, _resources: &mut Resources, systems: &mut Builder) {
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

  pub fn change_surface(&self, surface: Option<&Arc<<P::GraphicsBackend as Backend>::Surface>>) {
    let mut guard = self.surface.lock().unwrap();
    *guard = surface.map(|s| s.clone());
  }

  pub fn is_saturated(&self) -> bool {
    self.queued_frames_counter.load(Ordering::SeqCst) > 2
  }

  pub(super) fn window_state(&self) -> &Mutex<WindowState> {
    &self.window_state
  }

  pub(super) fn surface(&self) -> &Mutex<Option<Arc<<P::GraphicsBackend as Backend>::Surface>>> {
    &self.surface
  }

  pub(super) fn dec_queued_frames_counter(&self) -> usize {
    self.queued_frames_counter.fetch_sub(1, Ordering::SeqCst)
  }
}
