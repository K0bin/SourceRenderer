use std::sync::Arc;
use std::sync::Mutex;
use std::iter::*;
use std::sync::atomic::Ordering;

use ash::vk;

use sourcerenderer_core::graphics::Queue;
use sourcerenderer_core::graphics::{CommandBufferType, Swapchain};


use crate::VkBackend;
use crate::VkCommandBufferRecorder;
use crate::command::VkInnerCommandBufferInfo;
use crate::raw::RawVkDevice;
use crate::command::VkCommandPool;
use crate::swapchain::{VkSwapchain, VkSwapchainState};
use crate::sync::VkSemaphore;
use crate::sync::VkFence;

use crate::thread_manager::VkThreadManager;
use crate::{VkShared};
use crate::VkCommandBufferSubmission;
use crate::transfer::VkTransferCommandBuffer;
use crate::buffer::BufferAllocator;
use smallvec::SmallVec;

#[derive(Clone, Debug, Copy)]
pub struct VkQueueInfo {
  pub queue_family_index: usize,
  pub queue_index: usize,
  pub supports_presentation: bool
}

pub struct VkQueue {
  info: VkQueueInfo,
  queue: Mutex<VkQueueInner>,
  device: Arc<RawVkDevice>,
  shared: Arc<VkShared>,
  threads: Arc<VkThreadManager>
}

struct VkQueueInner {
  virtual_queue: Vec<VkVirtualSubmission>,
  queue: vk::Queue,
  last_fence: Option<Arc<VkFence>>
}

enum VkVirtualSubmission {
  CommandBuffer {
    command_buffer: vk::CommandBuffer,
    wait_semaphores: SmallVec<[vk::Semaphore; 4]>,
    wait_stages: SmallVec<[vk::PipelineStageFlags; 4]>,
    signal_semaphores: SmallVec<[vk::Semaphore; 4]>,
    fence: Option<Arc<VkFence>>
  },
  Present {
    wait_semaphores: SmallVec<[vk::Semaphore; 4]>,
    image_index: u32,
    swapchain: Arc<VkSwapchain>
  }
}

impl VkQueue {
  pub fn new(info: VkQueueInfo, queue: vk::Queue, device: &Arc<RawVkDevice>, shared: &Arc<VkShared>, threads: &Arc<VkThreadManager>) -> Self {
    Self {
      info,
      queue: Mutex::new(VkQueueInner {
        virtual_queue: Vec::new(),
        queue,
        last_fence: None
      }),
      device: device.clone(),
      shared: shared.clone(),
      threads: threads.clone()
    }
  }

  pub fn get_queue_family_index(&self) -> u32 {
    self.info.queue_family_index as u32
  }

  pub fn create_command_pool(&self, buffer_allocator: &Arc<BufferAllocator>) -> VkCommandPool {
    VkCommandPool::new(&self.device, self.info.queue_family_index as u32, &self.shared, buffer_allocator)
  }

  pub fn supports_presentation(&self) -> bool {
    self.info.supports_presentation
  }

  pub fn process_submissions(&self) {
    let mut guard = self.queue.lock().unwrap();
    if guard.virtual_queue.is_empty() {
      return;
    }

    if !self.device.is_alive.load(Ordering::SeqCst) {
      guard.virtual_queue.clear();
      return;
    }

    let mut command_buffers = SmallVec::<[vk::CommandBuffer; 32]>::new();
    let mut batch = SmallVec::<[vk::SubmitInfo; 8]>::new();
    let vk_queue = guard.queue;
    for submission in guard.virtual_queue.drain(..) {
      let mut append = false;
      match submission {
        VkVirtualSubmission::CommandBuffer {
          command_buffer, wait_semaphores, wait_stages, signal_semaphores, fence
        } => {
          if fence.is_none() && wait_semaphores.is_empty() && signal_semaphores.is_empty() {
            if let Some(last_info) = batch.last_mut() {
              if last_info.wait_semaphore_count == 0 && last_info.signal_semaphore_count == 0 && command_buffers.len() < command_buffers.capacity() {
                command_buffers.push(command_buffer);
                last_info.command_buffer_count += 1;
                append = true;
              }
            }
          }

          if !append {
            if let Some(fence) = fence {
              if !batch.is_empty() {
                unsafe {
                  let result = self.device.device.queue_submit(vk_queue, &batch, vk::Fence::null());
                  if result.is_err() {
                    self.device.is_alive.store(true, Ordering::SeqCst);
                    self.device.queue_wait_idle(vk_queue).unwrap();
                    panic!("Submit failed: {:?}", result);
                  }
                }
                batch.clear();
                command_buffers.clear();
              }

              let submit = vk::SubmitInfo {
                wait_semaphore_count: wait_semaphores.len() as u32,
                p_wait_semaphores: wait_semaphores.as_ptr(),
                p_wait_dst_stage_mask: wait_stages.as_ptr(),
                command_buffer_count: 1,
                p_command_buffers: &command_buffer as *const vk::CommandBuffer,
                signal_semaphore_count: signal_semaphores.len() as u32,
                p_signal_semaphores: signal_semaphores.as_ptr(),
                ..Default::default()
              };

              fence.mark_submitted();
              let fence_handle = fence.get_handle();
              unsafe {
                let result = self.device.device.queue_submit(vk_queue, &[submit], *fence_handle);
                if result.is_err() {
                  self.device.is_alive.store(true, Ordering::SeqCst);
                  self.device.queue_wait_idle(vk_queue).unwrap();
                  panic!("Submit failed: {:?}", result);
                }
              }
            } else {
              if batch.len() == batch.capacity() {
                unsafe {
                  let result = self.device.device.queue_submit(vk_queue, &batch, vk::Fence::null());
                  if result.is_err() {
                    self.device.is_alive.store(true, Ordering::SeqCst);
                    self.device.queue_wait_idle(vk_queue).unwrap();
                    panic!("Submit failed: {:?}", result);
                  }
                }
                batch.clear();
                command_buffers.clear();
              }

              command_buffers.push(command_buffer);
              let submit = vk::SubmitInfo {
                wait_semaphore_count: wait_semaphores.len() as u32,
                p_wait_semaphores: wait_semaphores.as_ptr(),
                p_wait_dst_stage_mask: wait_stages.as_ptr(),
                command_buffer_count: 1,
                p_command_buffers: unsafe { command_buffers.as_ptr().offset(command_buffers.len() as isize - 1) },
                signal_semaphore_count: signal_semaphores.len() as u32,
                p_signal_semaphores: signal_semaphores.as_ptr(),
                ..Default::default()
              };
              batch.push(submit);
            }
          }
        }

        VkVirtualSubmission::Present {
          wait_semaphores, image_index, swapchain
        } => {
          if !batch.is_empty() {
            unsafe {
              let result = self.device.device.queue_submit(vk_queue, &batch, vk::Fence::null());
              if result.is_err() {
                self.device.is_alive.store(true, Ordering::SeqCst);
                self.device.queue_wait_idle(vk_queue).unwrap();
                panic!("Submit failed: {:?}", result);
              }
            }
            batch.clear();
            command_buffers.clear();
          }

          let swapchain_handle = swapchain.get_handle();
          let present_info = vk::PresentInfoKHR {
            p_swapchains: &*swapchain_handle,
            swapchain_count: 1,
            p_image_indices: &image_index as *const u32,
            p_wait_semaphores: wait_semaphores.as_ptr(),
            wait_semaphore_count: wait_semaphores.len() as u32,
            ..Default::default()
          };
          unsafe {
            let result = swapchain.get_loader().queue_present(vk_queue, &present_info);
            swapchain.set_presented_image(image_index);
            match result {
              Ok(suboptimal) => {
                if suboptimal {
                  swapchain.set_state(VkSwapchainState::Suboptimal);
                }
              },
              Err(err) => {
                match err {
                  vk::Result::ERROR_OUT_OF_DATE_KHR => { swapchain.set_state(VkSwapchainState::OutOfDate); }
                  vk::Result::ERROR_SURFACE_LOST_KHR => { swapchain.surface().mark_lost(); }
                  _ => {
                    self.device.is_alive.store(true, Ordering::SeqCst);
                    self.device.queue_wait_idle(vk_queue).unwrap();
                    panic!("Present failed: {:?}", err);
                  }
                }
              }
            }
          }
        }
      }
    }

    if !batch.is_empty() {
      unsafe {
        let result = self.device.device.queue_submit(vk_queue, &batch, vk::Fence::null());
        if result.is_err() {
          self.device.is_alive.store(true, Ordering::SeqCst);
          panic!("Submit failed: {:?}", result);
        }
      }
    }
  }

  pub fn submit_transfer(&self, command_buffer: &VkTransferCommandBuffer) {
    debug_assert!(!command_buffer.get_fence().is_signalled());
    debug_assert_eq!(command_buffer.queue_family_index(), self.info.queue_family_index as u32);

    let vk_cmd_buffer = *command_buffer.get_handle();
    let submission = VkVirtualSubmission::CommandBuffer {
      command_buffer: vk_cmd_buffer,
      wait_semaphores: SmallVec::new(),
      wait_stages: SmallVec::new(),
      signal_semaphores: SmallVec::new(),
      fence: Some(command_buffer.get_fence().clone())
    };
    let mut guard = self.queue.lock().unwrap();
    guard.virtual_queue.push(submission);
  }

  pub fn submit(&self, command_buffer: VkCommandBufferSubmission, fence: Option<&Arc<VkFence>>, wait_semaphores: &[ &VkSemaphore ], signal_semaphores: &[ &VkSemaphore ]) {
    assert_eq!(command_buffer.command_buffer_type(), CommandBufferType::PRIMARY);
    debug_assert_eq!(command_buffer.queue_family_index(), self.info.queue_family_index as u32);
    debug_assert!(fence.is_none() || !fence.unwrap().is_signalled());
    if wait_semaphores.len() > 4 || signal_semaphores.len() > 4 {
      panic!("Can only signal and wait for 4 semaphores each.");
    }

    let mut _new_fence = Option::<Arc<VkFence>>::None;
    let mut fence = fence;
    if fence.is_none() && !signal_semaphores.is_empty() {
      _new_fence = Some(self.threads.shared().get_fence());
      fence = Some(_new_fence.as_ref().unwrap());
    }

    let mut cmd_buffer_mut = command_buffer;
    cmd_buffer_mut.mark_submitted();
    let wait_semaphore_handles = wait_semaphores.iter().map(|s| *s.get_handle()).collect::<SmallVec<[vk::Semaphore; 4]>>();
    let signal_semaphore_handles = signal_semaphores.iter().map(|s| *s.get_handle()).collect::<SmallVec<[vk::Semaphore; 4]>>();
    let stage_masks = wait_semaphores.iter().map(|_| vk::PipelineStageFlags::TOP_OF_PIPE).collect::<SmallVec<[vk::PipelineStageFlags; 4]>>();

    let vk_cmd_buffer = *cmd_buffer_mut.get_handle();
    let submission = VkVirtualSubmission::CommandBuffer {
      command_buffer: vk_cmd_buffer,
      wait_semaphores: wait_semaphore_handles,
      wait_stages: stage_masks,
      signal_semaphores: signal_semaphore_handles,
      fence: fence.cloned()
    };

    let mut guard = self.queue.lock().unwrap();
    if fence.is_some() {
      guard.last_fence = fence.cloned();
    }
    guard.virtual_queue.push(submission);
  }

  pub fn present(&self, swapchain: &Arc<VkSwapchain>, image_index: u32, wait_semaphores: &[ &VkSemaphore ]) {
    if wait_semaphores.len() > 4 {
      panic!("Can only wait for 4 semaphores.");
    }
    let wait_semaphore_handles = wait_semaphores.iter().map(|s| *s.get_handle()).collect::<SmallVec<[vk::Semaphore; 4]>>();

    let submission = VkVirtualSubmission::Present {
      wait_semaphores: wait_semaphore_handles,
      image_index,
      swapchain: swapchain.clone()
    };
    let mut guard = self.queue.lock().unwrap();
    guard.virtual_queue.push(submission);

    if let Some(fence) = guard.last_fence.as_ref() {
      self.threads.end_frame(fence);
    }
    guard.last_fence = None;
  }

  pub(crate) fn wait_for_idle(&self) {
    self.process_submissions();
    let queue_guard = self.queue.lock().unwrap();
    unsafe {
      self.device.queue_wait_idle(queue_guard.queue).unwrap();
    }
  }
}

impl Queue<VkBackend> for VkQueue {
  fn create_command_buffer(&self) -> VkCommandBufferRecorder {
    self.threads.get_thread_local().get_frame_local().get_command_buffer()
  }

  fn submit(&self, submission: VkCommandBufferSubmission, fence: Option<&Arc<VkFence>>, wait_semaphores: &[&Arc<VkSemaphore>], signal_semaphores: &[&Arc<VkSemaphore>]) {
    let mut wait_semaphore_refs = SmallVec::<[&VkSemaphore; 8]>::with_capacity(wait_semaphores.len());
    let mut signal_semaphore_refs = SmallVec::<[&VkSemaphore; 8]>::with_capacity(signal_semaphores.len());
    for sem in wait_semaphores {
      wait_semaphore_refs.push(sem.as_ref());
    }
    for sem in signal_semaphores {
      signal_semaphore_refs.push(sem.as_ref());
    }
    // TODO: clean up

    self.submit(submission, fence, &wait_semaphore_refs, &signal_semaphore_refs);
    self.process_submissions(); // TODO bring back threaded submission
  }

  fn present(&self, swapchain: &Arc<VkSwapchain>, wait_semaphores: &[&Arc<VkSemaphore>]) {
    let mut wait_semaphore_refs = SmallVec::<[&VkSemaphore; 8]>::with_capacity(wait_semaphores.len());
    for sem in wait_semaphores {
      wait_semaphore_refs.push(sem.as_ref());
    }
    self.present(swapchain, swapchain.acquired_image(), &wait_semaphore_refs);
  }

  fn create_inner_command_buffer(&self, inheritance: &VkInnerCommandBufferInfo) -> VkCommandBufferRecorder {
    self.threads.get_thread_local().get_frame_local().get_inner_command_buffer(Some(inheritance))
  }
}

// Vulkan queues are implicitly freed with the logical device
