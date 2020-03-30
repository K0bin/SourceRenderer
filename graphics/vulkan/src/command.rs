use std::sync::{Arc, Mutex};

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{CommandPool, PipelineInfo, Backend};
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::RenderpassRecordingMode;
use sourcerenderer_core::graphics::Viewport;
use sourcerenderer_core::graphics::Scissor;
use sourcerenderer_core::graphics::Resettable;

use sourcerenderer_core::pool::Recyclable;
use std::sync::mpsc::{ Sender, Receiver, channel };

use crate::VkQueue;
use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkRenderPass;
use crate::VkFrameBuffer;
use crate::VkBuffer;
use crate::VkPipeline;
use crate::VkBackend;

use crate::raw::*;
use pipeline::VkPipelineInfo;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use context::{VkGraphicsContext, VkShared};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use buffer::VkBufferSlice;

pub struct VkCommandPool {
  raw: Arc<RawVkCommandPool>,
  primary_buffers: Vec<Box<VkCommandBuffer>>,
  secondary_buffers: Vec<Box<VkCommandBuffer>>,
  receiver: Receiver<Box<VkCommandBuffer>>,
  sender: Sender<Box<VkCommandBuffer>>,
  shared: Arc<VkShared>
}

impl VkCommandPool {
  pub fn new(device: &Arc<RawVkDevice>, queue_family_index: u32, shared: &Arc<VkShared>) -> Self {
    let create_info = vk::CommandPoolCreateInfo {
      queue_family_index,
      flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
      ..Default::default()
    };

    let (sender, receiver) = channel();

    return Self {
      raw: Arc::new(RawVkCommandPool::new(device, &create_info).unwrap()),
      primary_buffers: Vec::new(),
      secondary_buffers: Vec::new(),
      receiver,
      sender,
      shared: shared.clone()
    };
  }
}

impl CommandPool<VkBackend> for VkCommandPool {
  fn get_command_buffer(&mut self, command_buffer_type: CommandBufferType) -> VkCommandBufferRecorder {
    let buffers = if command_buffer_type == CommandBufferType::PRIMARY {
      &mut self.primary_buffers
    } else {
      &mut self.secondary_buffers
    };

    let mut buffer = buffers.pop().unwrap_or_else(|| Box::new(VkCommandBuffer::new(&self.raw.device, &self.raw, command_buffer_type, &self.shared)));
    buffer.begin();
    return VkCommandBufferRecorder::new(buffer, self.sender.clone());
  }
}

impl Resettable for VkCommandPool {
  fn reset(&mut self) {
    unsafe {
      self.raw.device.reset_command_pool(**self.raw, vk::CommandPoolResetFlags::empty()).unwrap();
    }

    for mut cmd_buf in self.receiver.try_iter() {
      cmd_buf.reset();
      let buffers = if cmd_buf.command_buffer_type == CommandBufferType::PRIMARY {
        &mut self.primary_buffers
      } else {
        &mut self.secondary_buffers
      };
      buffers.push(cmd_buf);
    }
  }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum VkCommandBufferState {
  Ready,
  Recording,
  Finished,
  Submitted
}

struct VkLifetimeTrackers {
  buffers: Vec<Arc<VkBufferSlice>>,
  render_passes: Vec<Arc<VkRenderPass>>,
  frame_buffers: Vec<Arc<VkFrameBuffer>>
}

struct VkCommandBuffer {
  buffer: vk::CommandBuffer,
  pool: Arc<RawVkCommandPool>,
  device: Arc<RawVkDevice>,
  state: VkCommandBufferState,
  command_buffer_type: CommandBufferType,
  shared: Arc<VkShared>,
  render_pass: Option<Arc<VkRenderPass>>,
  sub_pass: u32,
  trackers: VkLifetimeTrackers
}

impl VkCommandBuffer {
  fn new(device: &Arc<RawVkDevice>, pool: &Arc<RawVkCommandPool>, command_buffer_type: CommandBufferType, shared: &Arc<VkShared>) -> Self {
    let buffers_create_info = vk::CommandBufferAllocateInfo {
      command_pool: ***pool,
      level: if command_buffer_type == CommandBufferType::PRIMARY { vk::CommandBufferLevel::PRIMARY } else { vk::CommandBufferLevel::SECONDARY }, // TODO: support secondary command buffers / bundles
      command_buffer_count: 1, // TODO: figure out how many buffers per pool (maybe create a new pool once we've run out?)
      ..Default::default()
    };
    let mut buffers = unsafe { device.allocate_command_buffers(&buffers_create_info) }.unwrap();
    return VkCommandBuffer {
      buffer: buffers.pop().unwrap(),
      pool: pool.clone(),
      device: device.clone(),
      command_buffer_type,
      render_pass: None,
      sub_pass: 0u32,
      shared: shared.clone(),
      state: VkCommandBufferState::Ready,
      trackers: VkLifetimeTrackers {
        buffers: Vec::new(),
        render_passes: Vec::new(),
        frame_buffers: Vec::new()
      }
    };
  }

  pub fn get_handle(&self) -> &vk::CommandBuffer {
    return &self.buffer;
  }

  pub fn get_type(&self) -> CommandBufferType {
    self.command_buffer_type
  }

  fn reset(&mut self) {
    self.state = VkCommandBufferState::Ready;
    self.trackers.buffers.clear();
    self.trackers.render_passes.clear();
    self.trackers.frame_buffers.clear();
  }

  fn begin(&mut self) {
    assert_eq!(self.state, VkCommandBufferState::Ready);
    self.state = VkCommandBufferState::Recording;
    unsafe {
      let begin_info = vk::CommandBufferBeginInfo {
        ..Default::default()
      };
      self.device.begin_command_buffer(self.buffer, &begin_info);
    }
  }

  fn end(&mut self) {
    assert_eq!(self.state, VkCommandBufferState::Recording);
    self.state = VkCommandBufferState::Finished;
    unsafe {
      self.device.end_command_buffer(self.buffer);
    }
  }

  fn set_pipeline(&mut self, pipeline: &PipelineInfo<VkBackend>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    if self.render_pass.is_none() {
      panic!("Cant set pipeline outside of render pass");
    }

    let render_pass = self.render_pass.clone().unwrap();

    let info = VkPipelineInfo {
      info: pipeline,
      render_pass: &render_pass,
      sub_pass: self.sub_pass
    };

    let mut hasher = DefaultHasher::new();
    info.hash(&mut hasher);
    let hash = hasher.finish();

    {
      let lock = self.shared.get_pipelines().read().unwrap();
      let cached_pipeline = lock.get(&hash);
      if let Some(pipeline) = cached_pipeline {
        let vk_pipeline = *pipeline.get_handle();
        unsafe {
          self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::GRAPHICS, vk_pipeline);
        }
        return;
      }
    }
    let pipeline = VkPipeline::new(&self.device, &info);
    let mut lock = self.shared.get_pipelines().write().unwrap();
    lock.insert(hash, pipeline);
    let stored_pipeline = lock.get(&hash).unwrap();
    let vk_pipeline = *stored_pipeline.get_handle();
    unsafe {
      self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::GRAPHICS, vk_pipeline);
    }
  }

  fn begin_render_pass(&mut self, render_pass: &Arc<VkRenderPass>, frame_buffer: &Arc<VkFrameBuffer>, recording_mode: RenderpassRecordingMode) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    // TODO: begin info fields
    unsafe {
      let begin_info = vk::RenderPassBeginInfo {
        framebuffer: *frame_buffer.get_handle(),
        render_pass: *render_pass.get_handle(),
        render_area: vk::Rect2D {
          offset: vk::Offset2D { x: 0i32, y: 0i32 },
          extent: vk::Extent2D { width: 1280, height: 720 }
        },
        clear_value_count: 1,
        p_clear_values: &[
          vk::ClearValue {
            color: vk::ClearColorValue {
              float32: [0.0f32, 0.0f32, 0.0f32, 1.0f32]
            }
          },
          vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
              depth: 0.0f32,
              stencil: 0u32
            }
          }
        ] as *const vk::ClearValue,
        ..Default::default()
      };
      self.device.cmd_begin_render_pass(self.buffer, &begin_info, if recording_mode == RenderpassRecordingMode::Commands { vk::SubpassContents::INLINE } else { vk::SubpassContents::SECONDARY_COMMAND_BUFFERS });
    }
    self.render_pass = Some(render_pass.clone());
    self.sub_pass = 0;
    self.trackers.frame_buffers.push(frame_buffer.clone());
    self.trackers.render_passes.push(render_pass.clone());
  }

  fn end_render_pass(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.cmd_end_render_pass(self.buffer);
    }
    self.render_pass = None;
  }

  fn set_vertex_buffer(&mut self, vertex_buffer: Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.trackers.buffers.push(vertex_buffer.clone());
    unsafe {
      self.device.cmd_bind_vertex_buffers(self.buffer, 0, &[*vertex_buffer.get_buffer().get_handle()], &[vertex_buffer.get_offset_and_length().0 as u64]);
    }
  }

  fn set_index_buffer(&mut self, index_buffer: Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.trackers.buffers.push(index_buffer.clone());
    unsafe {
      self.device.cmd_bind_index_buffer(self.buffer, *index_buffer.get_buffer().get_handle(), index_buffer.get_offset_and_length().0 as u64, vk::IndexType::UINT32);
    }
  }

  fn set_viewports(&mut self, viewports: &[ Viewport ]) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      for i in 0..viewports.len() {
        self.device.cmd_set_viewport(self.buffer, i as u32, &[vk::Viewport {
          x: viewports[i].position.x,
          y: viewports[i].position.y,
          width: viewports[i].extent.x,
          height: viewports[i].extent.y,
          min_depth: viewports[i].min_depth,
          max_depth: viewports[i].max_depth
        }]);
      }
    }
  }

  fn set_scissors(&mut self, scissors: &[ Scissor ])  {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      let vk_scissors: Vec<vk::Rect2D> = scissors.iter().map(|scissor| vk::Rect2D {
        offset: vk::Offset2D {
          x: scissor.position.x,
          y: scissor.position.y
        },
        extent: vk::Extent2D {
          width: scissor.extent.x,
          height: scissor.extent.y
        }
      }).collect();
      self.device.cmd_set_scissor(self.buffer, 0, &vk_scissors);
    }
  }

  fn draw(&mut self, vertices: u32, offset: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.cmd_draw(self.buffer, vertices, 1, offset, 0);
    }
  }

  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.cmd_draw_indexed(self.buffer, indices, instances, first_index, vertex_offset, first_instance);
    }
  }
}

impl Drop for VkCommandBuffer {
  fn drop(&mut self) {
    if self.state == VkCommandBufferState::Submitted {
      unsafe { self.device.device_wait_idle(); }
    }
  }
}

pub struct VkCommandBufferRecorder {
  item: Option<Box<VkCommandBuffer>>,
  sender: Sender<Box<VkCommandBuffer>>,
  phantom: PhantomData<*const u8>
}

impl Drop for VkCommandBufferRecorder {
  fn drop(&mut self) {
    if self.item.is_none() {
      return;
    }
    let item = std::mem::replace(&mut self.item, Option::None).unwrap();
    self.sender.send(item);
  }
}

impl VkCommandBufferRecorder {
  fn new(item: Box<VkCommandBuffer>, sender: Sender<Box<VkCommandBuffer>>) -> Self {
    Self {
      item: Some(item),
      sender,
      phantom: PhantomData
    }
  }

  #[inline(always)]
  pub fn begin_render_pass(&mut self, render_pass: &Arc<VkRenderPass>, frame_buffer: &Arc<VkFrameBuffer>, recording_mode: RenderpassRecordingMode) {
    self.item.as_mut().unwrap().begin_render_pass(render_pass, frame_buffer, recording_mode);
  }

  #[inline(always)]
  pub fn end_render_pass(&mut self) {
    self.item.as_mut().unwrap().end_render_pass();
  }
}

impl CommandBuffer<VkBackend> for VkCommandBufferRecorder {
  fn finish(self) -> VkCommandBufferSubmission {
    assert_eq!(self.item.as_ref().unwrap().state, VkCommandBufferState::Recording);
    let mut mut_self = self;
    let mut item = std::mem::replace(&mut mut_self.item, None).unwrap();
    item.end();
    VkCommandBufferSubmission::new(item, mut_self.sender.clone())
  }

  #[inline(always)]
  fn set_pipeline(&mut self, pipeline: &PipelineInfo<VkBackend>) {
    self.item.as_mut().unwrap().set_pipeline(pipeline);
  }

  #[inline(always)]
  fn set_vertex_buffer(&mut self, vertex_buffer: Arc<VkBufferSlice>) {
    self.item.as_mut().unwrap().set_vertex_buffer(vertex_buffer)
  }

  #[inline(always)]
  fn set_index_buffer(&mut self, index_buffer: Arc<VkBufferSlice>) {
    self.item.as_mut().unwrap().set_index_buffer(index_buffer)
  }

  #[inline(always)]
  fn set_viewports(&mut self, viewports: &[Viewport]) {
    self.item.as_mut().unwrap().set_viewports(viewports);
  }

  #[inline(always)]
  fn set_scissors(&mut self, scissors: &[Scissor]) {
    self.item.as_mut().unwrap().set_scissors(scissors);
  }

  #[inline(always)]
  fn draw(&mut self, vertices: u32, offset: u32) {
    self.item.as_mut().unwrap().draw(vertices, offset);
  }
  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    self.item.as_mut().unwrap().draw_indexed(instances, first_instance, indices, first_index, vertex_offset);
  }
}

pub struct VkCommandBufferSubmission {
  item: Box<VkCommandBuffer>,
  sender: Sender<Box<VkCommandBuffer>>
}

unsafe impl Send for VkCommandBufferSubmission {}

impl VkCommandBufferSubmission {
  fn new(item: Box<VkCommandBuffer>, sender: Sender<Box<VkCommandBuffer>>) -> Self {
    Self {
      item,
      sender
    }
  }

  pub(crate) fn mark_submitted(&mut self) {
    assert_eq!(self.item.state, VkCommandBufferState::Finished);
    self.item.state = VkCommandBufferState::Submitted;
  }

  pub(crate) fn get_handle(&self) -> &vk::CommandBuffer {
    &self.item.buffer
  }
}

pub struct VkTransfer {
  pool: Arc<RawVkCommandPool>,
  receiver: Receiver<Box<VkCommandBuffer>>,
  sender: Sender<Box<VkCommandBuffer>>,
  current_buffer: VkCommandBufferTransferRecorder
}

pub struct VkCommandBufferTransferRecorder {
  item: Option<Box<VkCommandBuffer>>,
  sender: Sender<Box<VkCommandBuffer>>,
  phantom: PhantomData<*const u8>
}

impl VkTransfer {
  pub fn new(device: &Arc<RawVkDevice>, transfer_queue_family_index: u32, shared: &Arc<VkShared>) -> Self {
    let pool_info = vk::CommandPoolCreateInfo {
      queue_family_index: transfer_queue_family_index,
      flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER | vk::CommandPoolCreateFlags::TRANSIENT,
      ..Default::default()
    };
    let (sender, receiver) = channel();
    let pool = Arc::new(RawVkCommandPool::new(device, &pool_info).unwrap());
    let buffer = VkCommandBufferTransferRecorder {
      item: Some(Box::new(VkCommandBuffer::new(device, &pool, CommandBufferType::PRIMARY, shared))),
      sender: sender.clone(),
      phantom: PhantomData
    };
    Self {
      pool,
      receiver,
      sender,
      current_buffer: buffer
    }
  }

  pub fn get_command_buffer(&mut self) -> &mut VkCommandBufferTransferRecorder {
    &mut self.current_buffer
  }
}
