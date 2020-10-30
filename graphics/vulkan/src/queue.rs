use std::sync::Arc;
use std::sync::Mutex;
use std::rc::Rc;
use std::iter::*;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::{Adapter, CommandBufferType};
use sourcerenderer_core::graphics::Device;
use crate::device::VkDevice;
use crate::raw::RawVkDevice;
use crate::command::VkCommandPool;
use crate::swapchain::VkSwapchain;
use crate::sync::VkSemaphore;
use crate::sync::VkFence;
use crate::VkBackend;
use sourcerenderer_core::graphics::Backend;
use ::{VkThreadManager, VkShared};
use VkCommandBufferSubmission;
use transfer::VkTransferCommandBuffer;
use buffer::BufferAllocator;
use ash::prelude::VkResult;

#[derive(Clone, Debug, Copy)]
pub struct VkQueueInfo {
  pub queue_family_index: usize,
  pub queue_index: usize,
  pub supports_presentation: bool
}

pub struct VkQueue {
  info: VkQueueInfo,
  queue: Mutex<vk::Queue>,
  device: Arc<RawVkDevice>,
  shared: Arc<VkShared>
}

impl VkQueue {
  pub fn new(info: VkQueueInfo, queue: vk::Queue, device: &Arc<RawVkDevice>, shared: &Arc<VkShared>) -> Self {
    return VkQueue {
      info,
      queue: Mutex::new(queue),
      device: device.clone(),
      shared: shared.clone()
    };
  }

  pub fn get_queue_family_index(&self) -> u32 {
    return self.info.queue_family_index as u32;
  }

  pub fn create_command_pool(&self, buffer_allocator: &Arc<BufferAllocator>) -> VkCommandPool {
    return VkCommandPool::new(&self.device, self.info.queue_family_index as u32, &self.shared, buffer_allocator);
  }

  pub fn supports_presentation(&self) -> bool {
    return self.info.supports_presentation;
  }

  pub fn submit_transfer(&self, command_buffer: &VkTransferCommandBuffer) {
    debug_assert!(!command_buffer.get_fence().is_signaled());

    let vk_cmd_buffer = *command_buffer.get_handle();
    let info = vk::SubmitInfo {
      p_command_buffers: &vk_cmd_buffer as *const vk::CommandBuffer,
      command_buffer_count: 1,
      p_wait_semaphores: std::ptr::null(),
      wait_semaphore_count: 0,
      p_wait_dst_stage_mask: std::ptr::null(),
      p_signal_semaphores: std::ptr::null(),
      signal_semaphore_count: 0,
      ..Default::default()
    };
    let vk_queue = self.queue.lock().unwrap();
    unsafe {
      let result = self.device.device.queue_submit(*vk_queue, &[info], *command_buffer.get_fence().get_handle());
      if result.is_err() {
        println!("Transfer submit failed: {:?}", result);
      }
    }
  }

  pub fn submit(&self, command_buffer: VkCommandBufferSubmission, fence: Option<&VkFence>, wait_semaphores: &[ &VkSemaphore ], signal_semaphores: &[ &VkSemaphore ]) {
    assert!(command_buffer.command_buffer_type(), CommandBufferType::PRIMARY);
    debug_assert!(fence.is_none() || !fence.unwrap().is_signaled());

    let mut cmd_buffer_mut = command_buffer;
    cmd_buffer_mut.mark_submitted();
    let wait_semaphore_handles = wait_semaphores.into_iter().map(|s| *s.get_handle()).collect::<Vec<vk::Semaphore>>();
    let signal_semaphore_handles = signal_semaphores.into_iter().map(|s| *s.get_handle()).collect::<Vec<vk::Semaphore>>();
    let stage_masks = wait_semaphores.into_iter().map(|_| vk::PipelineStageFlags::TOP_OF_PIPE).collect::<Vec<vk::PipelineStageFlags>>();

    let vk_cmd_buffer = *cmd_buffer_mut.get_handle();

    let info = vk::SubmitInfo {
      p_command_buffers: &vk_cmd_buffer as *const vk::CommandBuffer,
      command_buffer_count: 1,
      p_wait_semaphores: if wait_semaphores.len() == 0 { std::ptr::null() } else { wait_semaphore_handles.as_ptr() },
      wait_semaphore_count: wait_semaphores.len() as u32,
      p_wait_dst_stage_mask: if wait_semaphores.len() == 0 { std::ptr::null() } else { stage_masks.as_ptr() },
      p_signal_semaphores: if signal_semaphores.len() == 0 { std::ptr::null() } else { signal_semaphore_handles.as_ptr() },
      signal_semaphore_count: signal_semaphores.len() as u32,
      ..Default::default()
    };
    let vk_queue = self.queue.lock().unwrap();
    unsafe {
      let result = self.device.device.queue_submit(*vk_queue, &[info], fence.map_or(vk::Fence::null(), |f| *f.get_handle()));
      if result.is_err() {
        println!("Submit failed: {:?}", result);
      }
    }
  }

  pub fn present(&self, swapchain: &VkSwapchain, image_index: u32, wait_semaphores: &[ &VkSemaphore ]) -> VkResult<bool> {
    let wait_semaphore_handles = wait_semaphores.into_iter().map(|s| *s.get_handle()).collect::<Vec<vk::Semaphore>>();
    let present_info = vk::PresentInfoKHR {
      p_swapchains: swapchain.get_handle() as *const vk::SwapchainKHR,
      swapchain_count: 1,
      p_image_indices: &image_index as *const u32,
      p_wait_semaphores: if wait_semaphores.len() == 0 { std::ptr::null() } else { wait_semaphore_handles.as_ptr() },
      wait_semaphore_count: wait_semaphores.len() as u32,
      ..Default::default()
    };
    let vk_queue = self.queue.lock().unwrap();
    unsafe {
      let result = swapchain.get_loader().queue_present(*vk_queue, &present_info);
      if result.is_err() {
        println!("Present failed: {:?}", result);
      }
      result
    }
  }
}

// Vulkan queues are implicitly freed with the logical device
