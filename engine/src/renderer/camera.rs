use std::sync::Arc;

use sourcerenderer_core::graphics::{Backend, Device, MemoryUsage, BufferUsage, Buffer};
use sourcerenderer_core::{Matrix4, Vec3, Quaternion};
use std::sync::atomic::{AtomicU32, Ordering};
use crossbeam_utils::atomic::AtomicCell;
use nalgebra::Point3;

#[derive(Clone)]
struct PrimaryCameraBuffer {
  mats: [Matrix4; 16],
  counter: u32
}

pub struct PrimaryCamera<B: Backend> {
  buffer: Arc<B::Buffer>,
  counter: AtomicU32,
  position: AtomicCell<Vec3>, // AtomicCell uses a mutex for big structs, replace it by something like a lock less ring buffer
  rotation: AtomicCell<Quaternion>
}

impl<B: Backend> PrimaryCamera<B> {
  pub fn new(device: &B::Device) -> Self {
    Self {
      buffer: device.create_buffer(std::mem::size_of::<PrimaryCameraBuffer>(), MemoryUsage::CpuToGpu, BufferUsage::CONSTANT),
      counter: AtomicU32::new(0),
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

    self.update_buffer(Matrix4::new_perspective(aspect_ratio, vertical_fov, 0.001f32, 2000.0f32)
      * Matrix4::look_at_rh(&position, &target, &Vec3::new(0.0f32, 1.0f32, 0.0f32)));
  }

  fn update_buffer(&self, camera: Matrix4) {
    let mut map = self.buffer.map_mut::<PrimaryCameraBuffer>().expect("Failed to map camera buffer");
    let counter = self.counter.load(Ordering::SeqCst);
    map.mats[((counter + 1) % 16) as usize] = camera;
    let new_counter = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
    map.counter = new_counter;
  }

  pub fn get_camera(&self) -> Matrix4 {
    let counter = self.counter.load(Ordering::SeqCst);
    unsafe {
      let ptr = self.buffer.map_unsafe(false).expect("Failed to map camera buffer");
      let buf = (ptr as *mut PrimaryCameraBuffer).as_ref().unwrap();
      buf.mats[(counter % 16) as usize]
    }
  }

  pub fn buffer(&self) -> &Arc<B::Buffer> {
    &self.buffer
  }
}
