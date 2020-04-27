use ash::vk;
use ::{VkQueue, VkTexture};
use raw::{RawVkDevice, RawVkCommandPool};
use std::sync::{Arc, Mutex};
use ash::version::DeviceV1_0;
use buffer::VkBufferSlice;
use VkCommandBufferSubmission;
use crossbeam_channel::{Sender, Receiver, unbounded};
use command::VkCommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use context::VkShared;
use sourcerenderer_core::graphics::Texture;
use std::cmp::max;

pub(crate) struct VkTransfer {
  inner: Mutex<VkTransferInner>,
  transfer_queue: Option<Arc<VkQueue>>,
  graphics_queue: Arc<VkQueue>,
  graphics_pool: Arc<RawVkCommandPool>,
  transfer_pool: Option<Arc<RawVkCommandPool>>,
  device: Arc<RawVkDevice>,
  sender: Sender<Box<VkTransferCommandBuffer>>,
  receiver: Receiver<Box<VkTransferCommandBuffer>>,
  shared: Arc<VkShared>
}

struct VkTransferInner {
  current_transfer_buffer: Option<Box<VkTransferCommandBuffer>>,
  current_graphics_buffer: Box<VkTransferCommandBuffer>
}

impl VkTransfer {
  pub fn new(device: &Arc<RawVkDevice>, graphics_queue: &Arc<VkQueue>, transfer_queue: &Option<Arc<VkQueue>>, shared: &Arc<VkShared>) -> Self {
    let graphics_pool_info = vk::CommandPoolCreateInfo {
      flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER | vk::CommandPoolCreateFlags::TRANSIENT,
      queue_family_index: graphics_queue.get_queue_family_index(),
      ..Default::default()
    };
    let graphics_pool = Arc::new(RawVkCommandPool::new(device, &graphics_pool_info).unwrap());
    let mut graphics_buffer = Box::new(VkTransferCommandBuffer::new(device, &graphics_pool));
    graphics_buffer.begin();

    let (transfer_pool, transfer_buffer) = if let Some(queue) = transfer_queue {
      let transfer_pool_info = vk::CommandPoolCreateInfo {
        flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER | vk::CommandPoolCreateFlags::TRANSIENT,
        queue_family_index: graphics_queue.get_queue_family_index(),
        ..Default::default()
      };
      let transfer_pool = Arc::new(RawVkCommandPool::new(device, &transfer_pool_info).unwrap());
      let mut transfer_buffer = Box::new(VkTransferCommandBuffer::new(device, &transfer_pool));
      transfer_buffer.begin();
      (Some(transfer_pool), Some(transfer_buffer))
    } else {
      (None, None)
    };

    let (sender, receiver) = unbounded();

    Self {
      inner: Mutex::new(VkTransferInner {
        current_graphics_buffer: graphics_buffer,
        current_transfer_buffer: transfer_buffer
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

  pub fn init_texture(&self, texture: &VkTexture, src_buffer: &VkBufferSlice, mip_level: u32, array_layer: u32) {
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
    }
  }

  pub fn flush(&self) {
    let mut guard = self.inner.lock().unwrap();
    let new_cmd_buffer= Box::new(VkTransferCommandBuffer::new(&self.device, &self.graphics_pool));
    let mut cmd_buffer = std::mem::replace(&mut guard.current_graphics_buffer, new_cmd_buffer);
    cmd_buffer.finish();
    let submission = VkCommandBufferSubmission::Transfer {
      item: cmd_buffer
    };
    self.graphics_queue.submit(submission, None, &[], &[]);
  }
}

pub struct VkTransferCommandBuffer {
  cmd_buffer: vk::CommandBuffer,
  device: Arc<RawVkDevice>
}

impl VkTransferCommandBuffer {
  pub fn new(device: &Arc<RawVkDevice>, pool: &vk::CommandPool) -> Self {
    let buffer_info = vk::CommandBufferAllocateInfo {
      command_pool: *pool,
      level: vk::CommandBufferLevel::PRIMARY,
      command_buffer_count: 1,
      ..Default::default()
    };
    let cmd_buffer = unsafe { device.allocate_command_buffers(&buffer_info) }.unwrap().pop().unwrap();
    Self {
      cmd_buffer,
      device: device.clone()
    }
  }

  pub fn begin(&mut self) {
    let begin_info = vk::CommandBufferBeginInfo {
      ..Default::default()
    };
    unsafe {
      self.device.begin_command_buffer(self.cmd_buffer, &begin_info);
    }
  }
  
  pub fn finish(&mut self) {
    unsafe {
      self.device.end_command_buffer(self.cmd_buffer);
    }
  }

  #[inline]
  pub(crate) fn get_handle(&self) -> &vk::CommandBuffer {
    &self.cmd_buffer
  }
}
