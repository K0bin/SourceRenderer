use std::sync::Arc;

use sourcerenderer_core::graphics::{Backend, Device, MemoryUsage, BufferUsage, Buffer};
use sourcerenderer_core::{Matrix4, Vec3, Quaternion};
use std::sync::atomic::{AtomicU32, Ordering};
use crossbeam_utils::atomic::AtomicCell;
use nalgebra::Point3;

#[derive(Clone)]
#[repr(C)]
struct PrimaryCameraBuffer {
  mats: [Matrix4; 16],
  index: u32
}

pub struct LateLatchCamera<B: Backend> {
  buffer: Arc<B::Buffer>,
  read_counter: AtomicU32,
  write_counter: AtomicU32,
  position: AtomicCell<Vec3>, // AtomicCell uses a mutex for big structs, replace it by something like a lock less ring buffer
  rotation: AtomicCell<Quaternion>,
  aspect_ratio: f32,
  fov: f32,
  z_near: f32,
  z_far: f32
}

impl<B: Backend> LateLatchCamera<B> {
  pub fn new(device: &B::Device, aspect_ratio: f32, fov: f32) -> Self {
    Self {
      buffer: device.create_buffer(std::mem::size_of::<PrimaryCameraBuffer>(), MemoryUsage::CpuOnly, BufferUsage::STORAGE, None),
      read_counter: AtomicU32::new(0),
      write_counter: AtomicU32::new(0),
      position: AtomicCell::new(Vec3::new(0f32, 0f32, 0f32)),
      rotation: AtomicCell::new(Quaternion::identity()),
      aspect_ratio,
      fov,
      z_near: 0.1f32,
      z_far: 1000f32
    }
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
    let vertical_fov = 2f32 * ((self.fov / 2f32).tan() * (1f32 / self.aspect_ratio)).atan();

    let transform = Matrix4::new_translation(&position)
        * Matrix4::new_rotation(rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount));

    let position = transform.transform_point(&Point3::new(0.0f32, 0.0f32, 0.0f32));
    let target = transform.transform_point(&Point3::new(0.0f32, 0.0f32, 1.0f32));

    self.update_buffer(Matrix4::new_perspective(self.aspect_ratio, vertical_fov, self.z_near, self.z_far)
      * Matrix4::look_at_rh(&position, &target, &Vec3::new(0.0f32, 1.0f32, 0.0f32)));
  }

  fn update_buffer(&self, camera: Matrix4) {
    let mut map = self.buffer.map_mut::<PrimaryCameraBuffer>().expect("Failed to map camera buffer");
    let mats_len = map.mats.len();
    let counter = self.write_counter.fetch_add(1, Ordering::SeqCst) + 1;
    map.mats[counter as usize % mats_len] = camera;
    self.read_counter.store(counter, Ordering::SeqCst);
    map.index = counter % mats_len as u32;
  }

  pub fn view(&self) -> Matrix4 {
    let position = self.position.load();
    let rotation = self.rotation.load();
    let transform = Matrix4::new_translation(&position)
        * Matrix4::new_rotation(rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount));
    let position = transform.transform_point(&Point3::new(0.0f32, 0.0f32, 0.0f32));
    let target = transform.transform_point(&Point3::new(0.0f32, 0.0f32, 1.0f32));
    Matrix4::look_at_rh(&position, &target, &Vec3::new(0.0f32, 1.0f32, 0.0f32))
  }

  pub fn get_camera(&self) -> Matrix4 {
    unsafe {
      let ptr = self.buffer.map_unsafe(false).expect("Failed to map camera buffer");
      let buf = (ptr as *mut PrimaryCameraBuffer).as_ref().unwrap();
      let mats_len = buf.mats.len();
      let counter = self.read_counter.load(Ordering::SeqCst);
      let mat = buf.mats[counter as usize % mats_len];
      self.buffer.unmap_unsafe(false);
      mat
    }
  }

  pub fn buffer(&self) -> &Arc<B::Buffer> {
    &self.buffer
  }
}
