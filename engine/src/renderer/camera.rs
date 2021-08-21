use std::sync::Arc;

use sourcerenderer_core::graphics::{Backend, Buffer, BufferInfo, BufferUsage, Device, MemoryUsage};
use sourcerenderer_core::{Matrix4, Vec3, Quaternion};
use std::sync::atomic::{AtomicU32, Ordering};
use crossbeam_utils::atomic::AtomicCell;
use nalgebra::Point3;

#[derive(Clone)]
#[repr(C)]
struct PrimaryCameraBuffer {
  proj: [Matrix4; 16],
  view: [Matrix4; 16],
  proj_index: u32,
  view_index: u32
}

pub struct LateLatchCamera<B: Backend> {
  buffer: Arc<B::Buffer>,
  proj_read_counter: AtomicU32,
  proj_write_counter: AtomicU32,
  view_read_counter: AtomicU32,
  view_write_counter: AtomicU32,
  position: AtomicCell<Vec3>, // AtomicCell uses a mutex for big structs, replace it by something like a lock less ring buffer
  rotation: AtomicCell<Quaternion>,
  aspect_ratio: f32,
  fov: f32,
  z_near: f32,
  z_far: f32
}

impl<B: Backend> LateLatchCamera<B> {
  pub fn new(device: &B::Device, aspect_ratio: f32, fov: f32) -> Self {
    let late_letch_cam = Self {
      buffer: device.create_buffer(&BufferInfo {
        size: std::mem::size_of::<PrimaryCameraBuffer>(),
        usage: BufferUsage::COMPUTE_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_STORAGE_READ | BufferUsage::FRAGMENT_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::FRAGMENT_SHADER_CONSTANT | BufferUsage::COMPUTE_SHADER_CONSTANT
      }, MemoryUsage::CpuOnly, None),
      proj_read_counter: AtomicU32::new(0),
      proj_write_counter: AtomicU32::new(0),
      view_read_counter: AtomicU32::new(0),
      view_write_counter: AtomicU32::new(0),
      position: AtomicCell::new(Vec3::new(0f32, 0f32, 0f32)),
      rotation: AtomicCell::new(Quaternion::identity()),
      aspect_ratio,
      fov,
      z_near: 0.1f32,
      z_far: 100f32
    };
    late_letch_cam.update_projection(late_letch_cam.proj());
    late_letch_cam
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

  pub fn rotation(&self) -> Quaternion {
    self.rotation.load()
  }

  pub fn update_position(&self, position: Vec3) {
    self.position.store(position);
    self.update_camera(position, self.rotation.load());
  }

  pub fn update_rotation(&self, rotation: Quaternion) {
    self.rotation.store(rotation);
    self.update_camera(self.position.load(), rotation);
  }

  fn update_camera(&self, position: Vec3, rotation: Quaternion) {
    let position = Point3::<f32>::new(position.x, position.y, position.z);
    let forward = rotation.transform_vector(&Vec3::new(0.0f32, 0.0f32, -1.0f32));
    self.update_view(Matrix4::look_at_rh(&position, &(position + forward), &Vec3::new(0.0f32, 1.0f32, 0.0f32)));
  }

  fn update_view(&self, view: Matrix4) {
    let mut map = self.buffer.map_mut::<PrimaryCameraBuffer>().expect("Failed to map camera buffer");
    let mats_len = map.view.len();
    let counter = self.view_write_counter.fetch_add(1, Ordering::SeqCst) + 1;
    map.view[counter as usize % mats_len] = view;
    self.view_read_counter.store(counter, Ordering::SeqCst);
    map.view_index = counter % mats_len as u32;
  }

  fn update_projection(&self, proj: Matrix4) {
    let mut map = self.buffer.map_mut::<PrimaryCameraBuffer>().expect("Failed to map camera buffer");
    let mats_len = map.proj.len();
    let counter = self.proj_write_counter.fetch_add(1, Ordering::SeqCst) + 1;
    map.proj[counter as usize % mats_len] = proj;
    self.proj_read_counter.store(counter, Ordering::SeqCst);
    map.proj_index = counter % mats_len as u32;
  }

  pub fn view(&self) -> Matrix4 {
    let position = self.position.load();
    let rotation = self.rotation.load();
    let position = Point3::<f32>::new(position.x, position.y, position.z);
    let forward = rotation.transform_vector(&Vec3::new(0.0f32, 0.0f32, -1.0f32));
    Matrix4::look_at_rh(&position, &(position + forward), &Vec3::new(0.0f32, 1.0f32, 0.0f32))
  }

  pub fn proj(&self) -> Matrix4 {
    let vertical_fov = 2f32 * ((self.fov / 2f32).tan() * (1f32 / self.aspect_ratio)).atan();
    Matrix4::new_perspective(self.aspect_ratio, vertical_fov, self.z_near, self.z_far)
  }

  pub fn get_camera(&self) -> Matrix4 {
    unsafe {
      let ptr = self.buffer.map_unsafe(false).expect("Failed to map camera buffer");
      let buf = (ptr as *mut PrimaryCameraBuffer).as_ref().unwrap();
      let proj_len = buf.proj.len();
      let proj_counter = self.proj_read_counter.load(Ordering::SeqCst);
      let proj = buf.proj[proj_counter as usize % proj_len];
      let view_len = buf.view.len();
      let view_counter = self.proj_read_counter.load(Ordering::SeqCst);
      let view = buf.view[view_counter as usize % view_len];
      self.buffer.unmap_unsafe(false);
      proj * view
    }
  }

  pub fn buffer(&self) -> &Arc<B::Buffer> {
    &self.buffer
  }
}
