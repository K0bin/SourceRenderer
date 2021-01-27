use ash::vk;
use crate::{VkQueue, VkTexture};
use crate::raw::{RawVkDevice, RawVkCommandPool};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use ash::version::DeviceV1_0;
use crate::buffer::VkBufferSlice;
use crate::{VkFence};
use crossbeam_channel::{Sender, Receiver, unbounded};
use rayon;

use sourcerenderer_core::graphics::Texture;
use std::cmp::{max, min};
use crate::{VkShared, VkLifetimeTrackers};

pub(crate) struct VkTransfer {
  inner: Mutex<VkTransferInner>,
  transfer_queue: Option<Arc<VkQueue>>,
  graphics_queue: Arc<VkQueue>,
  device: Arc<RawVkDevice>,
  sender: Sender<Box<VkTransferCommandBuffer>>,
  receiver: Receiver<Box<VkTransferCommandBuffer>>,
  shared: Arc<VkShared>
}

enum VkTransferBarrier {
  Image(vk::ImageMemoryBarrier),
  Buffer(vk::BufferMemoryBarrier)
}

unsafe impl Send for VkTransferBarrier {}
unsafe impl Sync for VkTransferBarrier {}

enum VkTransferCopy {
  BufferToImage {
    src: Arc<VkBufferSlice>,
    dst: Arc<VkTexture>,
    region: vk::BufferImageCopy
  },
  BufferToBuffer {
    src: Arc<VkBufferSlice>,
    dst: Arc<VkBufferSlice>,
    region: vk::BufferCopy
  }
}

unsafe impl Send for VkTransferCopy {}
unsafe impl Sync for VkTransferCopy {}

struct VkTransferInner {
  graphics: VkTransferCommands,
  transfer_commands: Option<VkTransferCommands>
}

struct VkTransferCommands {
  pre_barriers: Vec<VkTransferBarrier>,
  copies: Vec<VkTransferCopy>,
  post_barriers: Vec<(Option<Arc<VkFence>>, VkTransferBarrier)>,
  used_cmd_buffers: VecDeque<Box<VkTransferCommandBuffer>>,
  pool: Arc<RawVkCommandPool>,
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

    let transfer_pool = if let Some(_queue) = transfer_queue {
      let transfer_pool_info = vk::CommandPoolCreateInfo {
        flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER | vk::CommandPoolCreateFlags::TRANSIENT,
        queue_family_index: graphics_queue.get_queue_family_index(),
        ..Default::default()
      };
      let transfer_pool = Arc::new(RawVkCommandPool::new(device, &transfer_pool_info).unwrap());
      Some(transfer_pool)
    } else {
      None
    };

    let (sender, receiver) = unbounded();

    let graphics_fence = shared.get_fence();
    let transfer_fence = if transfer_pool.is_some() {
      Some(shared.get_fence())
    } else {
      None
    };

    Self {
      inner: Mutex::new(VkTransferInner {
        graphics: VkTransferCommands {
          pre_barriers: Vec::new(),
          copies: Vec::new(),
          post_barriers: Vec::new(),
          current_fence: graphics_fence,
          pool: graphics_pool,
          used_cmd_buffers: VecDeque::new()
        },
        transfer_commands: transfer_pool.map(|transfer_pool| {
          VkTransferCommands {
            pre_barriers: Vec::new(),
            copies: Vec::new(),
            post_barriers: Vec::new(),
            current_fence: transfer_fence.unwrap(),
            pool: transfer_pool,
            used_cmd_buffers: VecDeque::new()
          }
        })
      }),
      transfer_queue: transfer_queue.clone(),
      graphics_queue: graphics_queue.clone(),
      device: device.clone(),
      sender,
      receiver,
      shared: shared.clone()
    }
  }

  pub fn init_texture(&self, texture: &Arc<VkTexture>, src_buffer: &Arc<VkBufferSlice>, mip_level: u32, array_layer: u32) {
    let mut guard = self.inner.lock().unwrap();
    guard.graphics.pre_barriers.push(VkTransferBarrier::Image (
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
      }));

    guard.graphics.copies.push(VkTransferCopy::BufferToImage {
      src: src_buffer.clone(),
      dst: texture.clone(),
      region: vk::BufferImageCopy {
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
      }
    });

    guard.graphics.post_barriers.push((None, VkTransferBarrier::Image (
      vk::ImageMemoryBarrier {
        src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
        dst_access_mask: vk::AccessFlags::MEMORY_READ,
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
    })));
  }

  pub fn init_buffer(&self, src_buffer: &Arc<VkBufferSlice>, dst_buffer: &Arc<VkBufferSlice>) {
    let mut guard = self.inner.lock().unwrap();
    guard.graphics.copies.push(VkTransferCopy::BufferToBuffer {
      src: src_buffer.clone(),
      dst: dst_buffer.clone(),
      region: vk::BufferCopy {
        src_offset: src_buffer.get_offset() as u64,
        dst_offset: dst_buffer.get_offset() as u64,
        size: min(src_buffer.get_length(), dst_buffer.get_length()) as u64
      }
    });

    guard.graphics.post_barriers.push((None, VkTransferBarrier::Buffer (
      vk::BufferMemoryBarrier {
        src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
        dst_access_mask: vk::AccessFlags::SHADER_READ,
        src_queue_family_index: self.graphics_queue.get_queue_family_index(),
        dst_queue_family_index: self.graphics_queue.get_queue_family_index(),
        buffer: *dst_buffer.get_buffer().get_handle(),
        offset: dst_buffer.get_offset() as u64,
        size: dst_buffer.get_length() as u64,
        ..Default::default()
      }
    )));
  }

  pub fn try_free_used_buffers(&self) {
    let mut guard = self.inner.lock().unwrap();
    for cmd_buffer in &mut guard.graphics.used_cmd_buffers {
      if cmd_buffer.fence.is_signaled() {
        cmd_buffer.reset();
      }
    }
  }

  pub fn flush(&self) {
    let mut guard = self.inner.lock().unwrap();

    if guard.graphics.copies.is_empty() {
      return;
    }

    let reuse_first_graphics_buffer = guard.graphics.used_cmd_buffers.front().map(|cmd_buffer| cmd_buffer.fence.is_signaled()).unwrap_or(false);
    let mut cmd_buffer = if reuse_first_graphics_buffer {
      let mut cmd_buffer= guard.graphics.used_cmd_buffers.pop_front().unwrap();
      cmd_buffer.reset();
      cmd_buffer
    } else {
      Box::new({
        let buffer_info = vk::CommandBufferAllocateInfo {
          command_pool: **guard.graphics.pool,
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
          fence: new_fence
        }
      })
    };
    unsafe {
      self.device.begin_command_buffer(*cmd_buffer.get_handle(), &vk::CommandBufferBeginInfo {
        ..Default::default()
      });
    }

    // commit pre barriers
    let mut image_barriers = Vec::<vk::ImageMemoryBarrier>::new();
    let mut buffer_barriers = Vec::<vk::BufferMemoryBarrier>::new();
    for barrier in guard.graphics.pre_barriers.drain(..) {
      match barrier {
        VkTransferBarrier::Buffer(buffer_memory_barrier) => { buffer_barriers.push(buffer_memory_barrier); }
        VkTransferBarrier::Image(image_memory_barrier) => { image_barriers.push(image_memory_barrier); }
      }
    }
    unsafe {
      self.device.cmd_pipeline_barrier(*cmd_buffer.get_handle(), vk::PipelineStageFlags::HOST, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[],
                                       &buffer_barriers,
                                       &image_barriers
      );
    }

    // commit copies
    for copy in guard.graphics.copies.drain(..) {
      match copy {
        VkTransferCopy::BufferToBuffer {
          src, dst, region
        } => {
          cmd_buffer.trackers.track_buffer(&src);
          cmd_buffer.trackers.track_buffer(&dst);
          unsafe {
            self.device.cmd_copy_buffer(*cmd_buffer.get_handle(), *src.get_buffer().get_handle(), *dst.get_buffer().get_handle(), &[region]);
          }
        },
        VkTransferCopy::BufferToImage {
          src, dst, region
        } => {
          cmd_buffer.trackers.track_buffer(&src);
          cmd_buffer.trackers.track_texture(&dst);
          unsafe {
            self.device.cmd_copy_buffer_to_image(*cmd_buffer.get_handle(), *src.get_buffer().get_handle(), *dst.get_handle(), vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
          }
        }
      }
    }

    // commit post barriers
    image_barriers.clear();
    buffer_barriers.clear();
    let mut retained_barriers = Vec::<(Option<Arc<VkFence>>, VkTransferBarrier)>::new();
    for (fence, barrier) in guard.graphics.post_barriers.drain(..) {
      if let Some(fence) = fence {
        if !fence.is_signaled() {
          retained_barriers.push((Some(fence), barrier));
          continue;
        }
      }
      match barrier {
        VkTransferBarrier::Buffer(buffer_memory_barrier) => { buffer_barriers.push(buffer_memory_barrier); }
        VkTransferBarrier::Image(image_memory_barrier) => { image_barriers.push(image_memory_barrier); }
      }
    }
    guard.graphics.post_barriers.append(&mut retained_barriers);
    unsafe {
      self.device.cmd_pipeline_barrier(*cmd_buffer.get_handle(), vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::ALL_COMMANDS, vk::DependencyFlags::empty(), &[],
                                       &buffer_barriers,
                                       &image_barriers
      );
    }

    unsafe {
      self.device.end_command_buffer(*cmd_buffer.get_handle());
    }
    self.graphics_queue.submit_transfer(&cmd_buffer);
    let c_queue = self.graphics_queue.clone();
    rayon::spawn(move || c_queue.process_submissions());
    guard.graphics.used_cmd_buffers.push_back(cmd_buffer);
  }
}

pub struct VkTransferCommandBuffer {
  cmd_buffer: vk::CommandBuffer,
  device: Arc<RawVkDevice>,
  trackers: VkLifetimeTrackers,
  fence: Arc<VkFence>
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
