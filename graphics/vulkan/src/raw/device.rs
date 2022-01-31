use std::sync::{Arc, MutexGuard, Mutex};
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use parking_lot::{ReentrantMutex, ReentrantMutexGuard};

use ash::vk;

use crate::raw::RawVkInstance;
use crate::VkAdapterExtensionSupport;
use crate::queue::VkQueueInfo;

pub struct RawVkDevice {
  pub device: ash::Device,
  pub allocator: vk_mem::Allocator,
  pub physical_device: vk::PhysicalDevice,
  pub instance: Arc<RawVkInstance>,
  pub extensions: VkAdapterExtensionSupport,
  pub graphics_queue_info: VkQueueInfo,
  pub compute_queue_info: Option<VkQueueInfo>,
  pub transfer_queue_info: Option<VkQueueInfo>,
  pub is_alive: AtomicBool,
  pub graphics_queue: ReentrantMutex<vk::Queue>,
  pub compute_queue: Option<ReentrantMutex<vk::Queue>>,
  pub transfer_queue: Option<ReentrantMutex<vk::Queue>>,
}

impl RawVkDevice {
  pub fn new(
    device: ash::Device,
    allocator: vk_mem::Allocator,
    physical_device: vk::PhysicalDevice,
    instance: Arc<RawVkInstance>,
    extensions: VkAdapterExtensionSupport,
    graphics_queue_info: VkQueueInfo,
    compute_queue_info: Option<VkQueueInfo>,
    transfer_queue_info: Option<VkQueueInfo>,
    graphics_queue: vk::Queue,
    compute_queue: Option<vk::Queue>,
    transfer_queue: Option<vk::Queue>) -> Self {
      Self {
        device,
        allocator,
        physical_device,
        instance,
        extensions,
        graphics_queue_info,
        compute_queue_info,
        transfer_queue_info,
        graphics_queue: ReentrantMutex::new(graphics_queue),
        compute_queue: compute_queue.map(|queue| ReentrantMutex::new(queue)),
        transfer_queue: transfer_queue.map(|queue| ReentrantMutex::new(queue)),
        is_alive: AtomicBool::new(true)
      }
  }

  pub fn graphics_queue(&self) -> ReentrantMutexGuard<vk::Queue> {
    self.graphics_queue.lock()
  }

  pub fn compute_queue(&self) -> Option<ReentrantMutexGuard<vk::Queue>> {
    self.compute_queue.as_ref().map(|queue| queue.lock())
  }

  pub fn transfer_queue(&self) -> Option<ReentrantMutexGuard<vk::Queue>> {
    self.transfer_queue.as_ref().map(|queue| queue.lock())
  }

  pub fn wait_for_idle(&self) {
    let _graphics_queue_lock = self.graphics_queue();
    let _compute_queue_lock = self.compute_queue();
    let _transfer_queue_lock = self.transfer_queue();
    unsafe { self.device.device_wait_idle().unwrap(); }
  }
}

impl Deref for RawVkDevice {
  type Target = ash::Device;

  fn deref(&self) -> &Self::Target {
    &self.device
  }
}

impl Drop for RawVkDevice {
  fn drop(&mut self) {
    self.allocator.destroy();
    unsafe {
      self.device.destroy_device(None);
    }
  }
}