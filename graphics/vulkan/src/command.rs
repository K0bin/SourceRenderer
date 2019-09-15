#![feature(optin_builtin_traits)]

use std::rc::Rc;
use std::sync::Arc;

use ash::vk;
use ash::Device;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::CommandPool;
use sourcerenderer_core::graphics::CommandBuffer;

use crate::VkDevice;
use crate::VkQueue;

pub struct VkCommandPool {
  command_pool: vk::CommandPool,
  queue: Arc<VkQueue>
}

pub struct VkCommandBuffer {
  command_buffer: vk::CommandBuffer,
  pool: Rc<VkCommandPool>
}

impl VkCommandPool {
  pub fn new(queue: Arc<VkQueue>) -> Self {
    let create_info = vk::CommandPoolCreateInfo {
      queue_family_index: queue.get_queue_family_index(),
      ..Default::default()
    };
    let device = queue.get_device();
    let vk_device = device.get_device();
    let command_pool = unsafe { vk_device.create_command_pool(&create_info, None) }.unwrap();

    return VkCommandPool {
      command_pool: command_pool,
      queue: queue
    };
  }

  pub fn get_pool(&self) -> &vk::CommandPool {
    return &self.command_pool;
  }

  pub fn get_queue(&self) -> &VkQueue {
    return self.queue.as_ref();
  }
}

impl Drop for VkCommandPool {
  fn drop(&mut self) {
    unsafe {
      let vk_device = self.queue.get_device().get_device();
      vk_device.destroy_command_pool(self.command_pool, None);
    }
  }
}

impl CommandPool for VkCommandPool {
  fn create_command_buffer(self: Rc<Self>) -> Rc<dyn CommandBuffer> {
    return Rc::new(VkCommandBuffer::new(self.clone()));
  }

  fn reset(&mut self) {
    let vk_device = self.queue.get_device().get_device();
    let flags: vk::CommandPoolResetFlags = Default::default();
    unsafe { vk_device.reset_command_pool(self.command_pool, flags); }
  }
}

impl VkCommandBuffer {
  pub fn new(pool: Rc<VkCommandPool>) -> Self {
    let vk_device = pool.get_queue().get_device().get_device();
    let command_pool = pool.get_pool();
    let buffers_create_info = vk::CommandBufferAllocateInfo {
      command_pool: *command_pool,
      level: vk::CommandBufferLevel::PRIMARY, // TODO: support secondary command buffers / bundles
      command_buffer_count: 1, // TODO: figure out how many buffers per pool (maybe create a new pool once we've run out?)
      ..Default::default()
    };
    let mut buffers = unsafe { vk_device.allocate_command_buffers(&buffers_create_info) }.unwrap();
    let buffer = buffers.remove(0);
    return VkCommandBuffer {
      command_buffer: buffer,
      pool: pool
    };
  }
}

impl CommandBuffer for VkCommandBuffer {

}
