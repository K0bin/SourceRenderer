#![feature(optin_builtin_traits)]

use std::rc::Rc;
use std::rc::Weak;
use std::sync::Arc;
use std::cell::RefCell;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::CommandPool;
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;

use crate::VkQueue;

struct VkCommandPoolState {
  pub free_buffers: Vec<Rc<VkCommandBuffer>>,
  pub used_buffers: Vec<Rc<VkCommandBuffer>>
}

pub struct VkCommandPool {
  command_pool: vk::CommandPool,
  queue: Arc<VkQueue>,
  state: RefCell<VkCommandPoolState>
}

pub struct VkCommandBuffer {
  command_buffer: vk::CommandBuffer,
  device: ash::Device,
  pool: Weak<VkCommandPool>
}

impl VkCommandPool {
  pub fn new(queue: Arc<VkQueue>) -> Self {
    let create_info = vk::CommandPoolCreateInfo {
      queue_family_index: queue.get_queue_family_index(),
      flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
      ..Default::default()
    };
    let device = queue.get_device();
    let vk_device = device.get_ash_device();
    let command_pool = unsafe { vk_device.create_command_pool(&create_info, None) }.unwrap();

    return VkCommandPool {
      command_pool: command_pool,
      queue: queue,
      state: RefCell::new(VkCommandPoolState {
        free_buffers: Vec::new(),
        used_buffers: Vec::new()
      })
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
    let mut state = self.state.borrow_mut();
    while let Some(ref mut cmd_buffer_ref) = state.free_buffers.pop() {
      let cmd_buffer = Rc::get_mut(cmd_buffer_ref).expect("Dropping command pool that is still in use!");
      cmd_buffer.drop_vk(self);
    }
    while let Some(ref mut cmd_buffer_ref) = state.used_buffers.pop() {
      let cmd_buffer = Rc::get_mut(cmd_buffer_ref).expect("Dropping command pool that is still in use!");
      cmd_buffer.drop_vk(self);
    }
    unsafe {
      let vk_device = self.queue.get_device().get_ash_device();
      vk_device.destroy_command_pool(self.command_pool, None);
    }
  }
}

impl CommandPool for VkCommandPool {
  fn create_command_buffer(self: Rc<Self>, command_buffer_type: CommandBufferType) -> Rc<dyn CommandBuffer> {
    let mut state = self.state.borrow_mut();
    let free_cmd_buffer_option = state.free_buffers.pop();
    return match free_cmd_buffer_option {
      Some(free_cmd_buffer) => {
        free_cmd_buffer
      }
      None => {
        let rc = Rc::new(VkCommandBuffer::new(&self, command_buffer_type));
        state.used_buffers.push(rc.clone());
        rc
      }
    };
  }

  fn reset(&self) {
    let mut state = self.state.borrow_mut();
    while let Some(buffer) = state.used_buffers.pop()
    {
      state.free_buffers.push(buffer);
    }
    unsafe {
      self.queue.get_device().get_ash_device().reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty());
    }
  }
}

impl VkCommandBuffer {
  pub fn new(pool: &Rc<VkCommandPool>, command_buffer_type: CommandBufferType) -> Self {
    let vk_device = pool.get_queue().get_device().get_ash_device();
    let command_pool = pool.get_pool();
    let buffers_create_info = vk::CommandBufferAllocateInfo {
      command_pool: *command_pool,
      level: if command_buffer_type == CommandBufferType::PRIMARY { vk::CommandBufferLevel::PRIMARY } else { vk::CommandBufferLevel::SECONDARY }, // TODO: support secondary command buffers / bundles
      command_buffer_count: 1, // TODO: figure out how many buffers per pool (maybe create a new pool once we've run out?)
      ..Default::default()
    };
    let mut buffers = unsafe { vk_device.allocate_command_buffers(&buffers_create_info) }.unwrap();
    let buffer = buffers.remove(0);
    return VkCommandBuffer {
      command_buffer: buffer,
      device: pool.get_queue().get_device().get_ash_device().clone(),
      pool: Rc::downgrade(&pool)
    };
  }

  fn drop_vk(&mut self, pool: &VkCommandPool) {
    unsafe {
      let device = pool
        .get_queue()
        .get_device()
        .get_ash_device();
      device.free_command_buffers(*pool.get_pool(), &[ self.command_buffer ] );
    }
  }
}

impl CommandBuffer for VkCommandBuffer {
}
