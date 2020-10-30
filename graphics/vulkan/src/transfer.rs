use ash::vk;
use ::{VkQueue, VkTexture};
use raw::{RawVkDevice, RawVkCommandPool};
use std::sync::{Arc, Mutex};
use ash::version::DeviceV1_0;
use buffer::VkBufferSlice;
use ::{VkCommandBufferSubmission, VkFence};
use crossbeam_channel::{Sender, Receiver, unbounded};
use command::VkCommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::Texture;
use std::cmp::{max, min};
use sourcerenderer_core::pool::Recyclable;
use std::collections::VecDeque;
use ::{VkShared, VkLifetimeTrackers};

pub(crate) struct VkTransfer {
  inner: Mutex<VkTransferInner>,
  transfer_queue: Option<Arc<VkQueue>>,
  graphics_queue: Arc<VkQueue>,
  graphics_pool: Arc<RawVkCommandPool>, // TODO synchronize
  transfer_pool: Option<Arc<RawVkCommandPool>>, // TODO synchronize
  device: Arc<RawVkDevice>,
  sender: Sender<Box<VkTransferCommandBuffer>>,
  receiver: Receiver<Box<VkTransferCommandBuffer>>,
  shared: Arc<VkShared>
}

struct VkTransferInner {
  current_transfer_buffer: Option<Box<VkTransferCommandBuffer>>,
  current_graphics_buffer: Box<VkTransferCommandBuffer>,
  used_graphics_buffers: VecDeque<Box<VkTransferCommandBuffer>>,
  current_fence: Arc<VkFence>
}

impl VkTransfer {
  pub fn new(device: &Arc<RawVkDevice>, graphics_queue: &Arc<VkQueue>, transfer_queue: &Option<Arc<VkQueue>>, shared: &Arc<VkShared>) -> Self {
    let graphics_pool_info = vk::CommandPoolCreateInfo {
      flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER | vk::CommandPoolCreateFlags::TRANSIENT,
      queue_family_index: graphics_queue.get_queue_family_index(),
      ..Default::default()
    };
    let graphics_pool = Arc::new(RawVkCommandPool::new(device, &graphics_pool_info).unwrap());
    let mut graphics_buffer = Box::new({
      let buffer_info = vk::CommandBufferAllocateInfo {
        command_pool: **graphics_pool,
        level: vk::CommandBufferLevel::PRIMARY,
        command_buffer_count: 1,
        ..Default::default()
      };
      let cmd_buffer = unsafe { device.allocate_command_buffers(&buffer_info) }.unwrap().pop().unwrap();
      let fence = shared.get_fence();
      VkTransferCommandBuffer {
        cmd_buffer,
        device: device.clone(),
        trackers: VkLifetimeTrackers::new(),
        fence,
        is_empty: true
      }
    });
    let begin_info = vk::CommandBufferBeginInfo {
      ..Default::default()
    };
    unsafe {
      device.begin_command_buffer(graphics_buffer.cmd_buffer, &begin_info);
    }

    let (transfer_pool, transfer_buffer) = if let Some(queue) = transfer_queue {
      let transfer_pool_info = vk::CommandPoolCreateInfo {
        flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER | vk::CommandPoolCreateFlags::TRANSIENT,
        queue_family_index: graphics_queue.get_queue_family_index(),
        ..Default::default()
      };
      let transfer_pool = Arc::new(RawVkCommandPool::new(device, &transfer_pool_info).unwrap());
      let mut transfer_buffer = Box::new(
        {
          let buffer_info = vk::CommandBufferAllocateInfo {
            command_pool: **transfer_pool,
            level: vk::CommandBufferLevel::PRIMARY,
            command_buffer_count: 1,
            ..Default::default()
          };
          let cmd_buffer = unsafe { device.allocate_command_buffers(&buffer_info) }.unwrap().pop().unwrap();
          let fence = shared.get_fence();
          VkTransferCommandBuffer {
            cmd_buffer,
            device: device.clone(),
            trackers: VkLifetimeTrackers::new(),
            fence,
            is_empty: true
          }
        });
      unsafe {
        device.begin_command_buffer(transfer_buffer.cmd_buffer, &begin_info);
      }
      (Some(transfer_pool), Some(transfer_buffer))
    } else {
      (None, None)
    };

    let (sender, receiver) = unbounded();

    let fence = shared.get_fence();

    Self {
      inner: Mutex::new(VkTransferInner {
        current_graphics_buffer: graphics_buffer,
        current_transfer_buffer: transfer_buffer,
        used_graphics_buffers: VecDeque::new(),
        current_fence: fence
      }),
      graphics_pool,
      transfer_pool,
      transfer_queue: transfer_queue.clone(),
      graphics_queue: graphics_queue.clone(),
      device: device.clone(),
      sender,
      receiver,
      shared: shared.clone()
    }
  }

  pub fn init_texture(&self, texture: &Arc<VkTexture>, src_buffer: &Arc<VkBufferSlice>, mip_level: u32, array_layer: u32) -> Arc<VkFence> {
    let mut guard = self.inner.lock().unwrap();
    unsafe {
      self.device.cmd_pipeline_barrier(*guard.current_graphics_buffer.get_handle(), vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[], &[], &[
        vk::ImageMemoryBarrier {
          src_access_mask: vk::AccessFlags::empty(),
          dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
          old_layout: vk::ImageLayout::UNDEFINED,
          new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
          src_queue_family_index: self.graphics_queue.get_queue_family_index(),
          dst_queue_family_index: self.graphics_queue.get_queue_family_index(),
          subresource_range: vk::ImageSubresourceRange {
            base_mip_level: mip_level,
            level_count: 1,
            base_array_layer: array_layer,
            aspect_mask: vk::ImageAspectFlags::COLOR,
            layer_count: 1
          },
          image: *texture.get_handle(),
          ..Default::default()
        }]);
      self.device.cmd_copy_buffer_to_image(*guard.current_graphics_buffer.get_handle(), *src_buffer.get_buffer().get_handle(), *texture.get_handle(), vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[
        vk::BufferImageCopy {
          buffer_offset: src_buffer.get_offset_and_length().0 as u64,
          image_offset: vk::Offset3D {
            x: 0,
            y: 0,
            z: 0
          },
          buffer_row_length: 0,
          buffer_image_height: 0,
          image_extent: vk::Extent3D {
            width: max(texture.get_info().width >> mip_level, 1),
            height: max(texture.get_info().height >> mip_level, 1),
            depth: max(texture.get_info().depth >> mip_level, 1),
          },
          image_subresource: vk::ImageSubresourceLayers {
            mip_level,
            base_array_layer: array_layer,
            aspect_mask: vk::ImageAspectFlags::COLOR,
            layer_count: 1
          }
      }]);
      self.device.cmd_pipeline_barrier(*guard.current_graphics_buffer.get_handle(), vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::FRAGMENT_SHADER, vk::DependencyFlags::empty(), &[], &[], &[
        vk::ImageMemoryBarrier {
          src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
          dst_access_mask: vk::AccessFlags::SHADER_READ,
          old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
          new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
          src_queue_family_index: self.graphics_queue.get_queue_family_index(),
          dst_queue_family_index: self.graphics_queue.get_queue_family_index(),
          subresource_range: vk::ImageSubresourceRange {
            base_mip_level: mip_level,
            level_count: 1,
            base_array_layer: array_layer,
            aspect_mask: vk::ImageAspectFlags::COLOR,
            layer_count: 1
          },
          image: *texture.get_handle(),
          ..Default::default()
      }]);

      guard.current_graphics_buffer.trackers.track_buffer(src_buffer);
      guard.current_graphics_buffer.trackers.track_texture(texture);
      guard.current_graphics_buffer.is_empty = false;

      guard.current_fence.clone()
    }
  }

  pub fn init_buffer(&self, src_buffer: &Arc<VkBufferSlice>, dst_buffer: &Arc<VkBufferSlice>) -> Arc<VkFence> {
    let mut guard = self.inner.lock().unwrap();
    unsafe {
      self.device.cmd_copy_buffer(*guard.current_graphics_buffer.get_handle(), *src_buffer.get_buffer().get_handle(), *dst_buffer.get_buffer().get_handle(), &[
        vk::BufferCopy {
          src_offset: src_buffer.get_offset() as u64,
          dst_offset: dst_buffer.get_offset() as u64,
          size: min(src_buffer.get_length(), dst_buffer.get_length()) as u64
        }]);
      self.device.cmd_pipeline_barrier(*guard.current_graphics_buffer.get_handle(), vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::TOP_OF_PIPE, vk::DependencyFlags::empty(), &[], &[
        vk::BufferMemoryBarrier {
          src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
          dst_access_mask: vk::AccessFlags::SHADER_READ
              | vk::AccessFlags::INDEX_READ
              | vk::AccessFlags::VERTEX_ATTRIBUTE_READ
              | vk::AccessFlags::MEMORY_READ,
          src_queue_family_index: self.graphics_queue.get_queue_family_index(),
          dst_queue_family_index: self.graphics_queue.get_queue_family_index(),
          buffer: *dst_buffer.get_buffer().get_handle(),
          offset: dst_buffer.get_offset() as u64,
          size: dst_buffer.get_length() as u64,
          ..Default::default()
        }], &[]);

      guard.current_graphics_buffer.trackers.track_buffer(src_buffer);
      guard.current_graphics_buffer.trackers.track_buffer(dst_buffer);
      guard.current_graphics_buffer.is_empty = false;

      guard.current_fence.clone()
    }
  }

  pub fn try_free_used_buffers(&self) {
    let mut guard = self.inner.lock().unwrap();
    for cmd_buffer in &mut guard.used_graphics_buffers {
      if cmd_buffer.fence.is_signaled() {
        cmd_buffer.reset();
      }
    }
  }

  pub fn flush(&self) {
    let mut guard = self.inner.lock().unwrap();

    if guard.current_graphics_buffer.is_empty {
      return;
    }

    let reuse_first_graphics_buffer = guard.used_graphics_buffers.front().map(|cmd_buffer| cmd_buffer.fence.is_signaled()).unwrap_or(false);
    let new_cmd_buffer = if reuse_first_graphics_buffer {
      let mut cmd_buffer= guard.used_graphics_buffers.pop_front().unwrap();
      cmd_buffer.reset();
      cmd_buffer
    } else {
      Box::new({
        let buffer_info = vk::CommandBufferAllocateInfo {
          command_pool: **self.graphics_pool,
          level: vk::CommandBufferLevel::PRIMARY,
          command_buffer_count: 1,
          ..Default::default()
        };
        let cmd_buffer = unsafe { self.device.allocate_command_buffers(&buffer_info) }.unwrap().pop().unwrap();
        let new_fence = self.shared.get_fence();
        VkTransferCommandBuffer {
          cmd_buffer,
          device: self.device.clone(),
          trackers: VkLifetimeTrackers::new(),
          fence: new_fence,
          is_empty: true
        }
      })
    };
    let mut cmd_buffer = std::mem::replace(&mut guard.current_graphics_buffer, new_cmd_buffer);
    unsafe {
      self.device.end_command_buffer(cmd_buffer.cmd_buffer);
      self.device.begin_command_buffer(guard.current_graphics_buffer.cmd_buffer, &vk::CommandBufferBeginInfo {
        ..Default::default()
      });
    }
    self.graphics_queue.submit_transfer(&cmd_buffer);
    guard.used_graphics_buffers.push_back(cmd_buffer);
  }
}

pub struct VkTransferCommandBuffer {
  cmd_buffer: vk::CommandBuffer,
  device: Arc<RawVkDevice>,
  trackers: VkLifetimeTrackers,
  fence: Arc<VkFence>,
  is_empty: bool
}

impl VkTransferCommandBuffer {
  #[inline]
  pub(crate) fn get_handle(&self) -> &vk::CommandBuffer {
    &self.cmd_buffer
  }

  #[inline]
  pub(crate) fn get_fence(&self) -> &VkFence {
    &self.fence
  }

  fn reset(&mut self) {
    self.fence.reset();
    unsafe {
      self.device.reset_command_buffer(self.cmd_buffer, vk::CommandBufferResetFlags::RELEASE_RESOURCES);
    }
    self.trackers.reset();
  }
}
