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
  rotation: AtomicCell<Quaternion>
}

impl<B: Backend> LateLatchCamera<B> {
  pub fn new(device: &B::Device) -> Self {
    Self {
      buffer: device.create_buffer(std::mem::size_of::<PrimaryCameraBuffer>(), MemoryUsage::CpuOnly, BufferUsage::STORAGE, None),
      read_counter: AtomicU32::new(0),
      write_counter: AtomicU32::new(0),
      position: AtomicCell::new(Vec3::new(0f32, 0f32, 0f32)),
      rotation: AtomicCell::new(Quaternion::identity())
    }
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
    let fov = 1.57f32;
    let aspect_ratio = 16f32 / 9f32;
    let vertical_fov = 2f32 * ((fov / 2f32).tan() * (1f32 / aspect_ratio)).atan();

    let transform = Matrix4::new_translation(&position)
        * Matrix4::new_rotation(rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount));

    let position = transform.transform_point(&Point3::new(0.0f32, 0.0f32, 0.0f32));
    let target = transform.transform_point(&Point3::new(0.0f32, 0.0f32, 1.0f32));

    self.update_buffer(Matrix4::new_perspective(aspect_ratio, vertical_fov, 0.001f32, 20000.0f32)
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
