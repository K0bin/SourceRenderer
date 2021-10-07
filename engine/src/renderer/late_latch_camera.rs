use std::sync::{Arc, Mutex};

use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::graphics::{Backend, Buffer, BufferInfo, BufferUsage, Device, MemoryUsage};
use sourcerenderer_core::{Matrix4, Vec3};

use crate::fps_camera::{FPSCamera, fps_camera_rotation};
use crate::transform::interpolation::deconstruct_transform;

use super::LateLatching;
use super::drawable::{View, make_camera_proj, make_camera_view};

#[derive(Clone)]
#[repr(C)]
struct LateLatchCamerabuffer {
  view_proj: Matrix4,
  inv_proj: Matrix4,
  view: Matrix4,
  proj: Matrix4,
  inv_view: Matrix4,
  position: Vec3
}

pub struct LateLatchCamera<B: Backend> {
  fps_camera: Mutex<FPSCamera>,
  buffer: AtomicRefCell<Arc<B::Buffer>>,
  history_buffer: AtomicRefCell<Arc<B::Buffer>>,
  aspect_ratio: f32,
  fov: f32,
  z_near: f32,
  z_far: f32
}

impl<B: Backend> LateLatching<B> for LateLatchCamera<B> {
  fn buffer(&self) -> Arc<B::Buffer> {
    let buffer_ref = self.buffer.borrow();
    buffer_ref.clone()
  }

  fn history_buffer(&self) -> Option<Arc<B::Buffer>> {
    let history_buffer_ref = self.history_buffer.borrow();
    Some(history_buffer_ref.clone())
  }

  fn before_recording(&self, _input: &crate::input::InputState, _view: &View) {}

  fn before_submit(&self, input: &crate::input::InputState, view: &View) {
    let mut fps_camera = self.fps_camera.lock().unwrap();
    let (position, _rotation, _) = deconstruct_transform(&view.camera_transform);
    let rotation = fps_camera_rotation(input, &mut fps_camera);

    let view = make_camera_view(position, rotation);
    let proj = make_camera_proj(self.fov, self.aspect_ratio, self.z_near, self.z_far);

    let buffer_mut = self.buffer.borrow_mut();
    let mut buffer_data = buffer_mut.map_mut::<LateLatchCamerabuffer>().expect("Failed to map camera buffer");
    buffer_data.view = view;
    buffer_data.proj = proj;
    buffer_data.inv_view = view.try_inverse().unwrap();
    buffer_data.inv_proj = proj.try_inverse().unwrap();
    buffer_data.view_proj = proj * view;
    buffer_data.position = position;
  }

  fn after_submit(&self, device: &B::Device) {
    let mut buffer_mut = self.buffer.borrow_mut();
    let mut history_buffer_mut = self.history_buffer.borrow_mut();
    *history_buffer_mut = std::mem::replace(&mut buffer_mut, Self::create_buffer(device));
  }
}

impl<B: Backend> LateLatchCamera<B> {
  pub fn new(device: &B::Device, aspect_ratio: f32, fov: f32) -> Self {
    let late_letch_cam = Self {
      fps_camera: Mutex::new(FPSCamera::new()),
      buffer: AtomicRefCell::new(Self::create_buffer(device)),
      history_buffer: AtomicRefCell::new(Self::create_buffer(device)),
      aspect_ratio,
      fov,
      z_near: 0.1f32,
      z_far: 100f32
    };
    late_letch_cam
  }

  fn create_buffer(device: &B::Device) -> Arc<B::Buffer> {
    device.create_buffer(&BufferInfo {
      size: std::mem::size_of::<LateLatchCamerabuffer>(),
      usage: BufferUsage::COMPUTE_SHADER_STORAGE_READ
        | BufferUsage::VERTEX_SHADER_STORAGE_READ
        | BufferUsage::FRAGMENT_SHADER_STORAGE_READ
        | BufferUsage::VERTEX_SHADER_CONSTANT
        | BufferUsage::FRAGMENT_SHADER_CONSTANT
        | BufferUsage::COMPUTE_SHADER_CONSTANT
    }, MemoryUsage::CpuToGpu, None)
  }

  pub fn z_near(&self) -> f32 {
    self.z_near
  }

  pub fn z_far(&self) -> f32 {
    self.z_far
  }

  pub fn fov(&self) -> f32 {
    self.fov
  }

  pub fn aspect_ratio(&self) -> f32 {
    self.aspect_ratio
  }
}
