use std::sync::{Arc, Mutex};

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{GraphicsPipelineInfo, Backend, Texture, BindingFrequency, MemoryUsage, Buffer, BufferUsage, PipelineBinding};
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::RenderpassRecordingMode;
use sourcerenderer_core::graphics::Viewport;
use sourcerenderer_core::graphics::Scissor;
use sourcerenderer_core::graphics::Resettable;

use sourcerenderer_core::pool::Recyclable;

use crate::VkQueue;
use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkRenderPass;
use crate::VkFrameBuffer;
use crate::VkBuffer;
use crate::VkPipeline;
use crate::VkBackend;

use crate::raw::*;
use pipeline::{VkGraphicsPipelineInfo, VkPipelineLayout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use ::{VkThreadManager, VkShared};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use buffer::{VkBufferSlice, BufferAllocator};
use VkTexture;
use std::cmp::{max, min};
use descriptor::{VkBindingManager, VkBoundResource};
use texture::VkTextureView;
use transfer::VkTransferCommandBuffer;
use std::ffi::{CStr, CString};
use lifetime_tracker::VkLifetimeTrackers;
use crossbeam_channel::{Receiver, Sender, unbounded};

pub struct VkCommandPool {
  raw: Arc<RawVkCommandPool>,
  primary_buffers: Vec<Box<VkCommandBuffer>>,
  secondary_buffers: Vec<Box<VkCommandBuffer>>,
  receiver: Receiver<Box<VkCommandBuffer>>,
  sender: Sender<Box<VkCommandBuffer>>,
  shared: Arc<VkShared>,
  queue_family_index: u32,
  buffer_allocator: Arc<BufferAllocator>
}

impl VkCommandPool {
  pub fn new(device: &Arc<RawVkDevice>, queue_family_index: u32, shared: &Arc<VkShared>, buffer_allocator: &Arc<BufferAllocator>) -> Self {
    let create_info = vk::CommandPoolCreateInfo {
      queue_family_index,
      flags: vk::CommandPoolCreateFlags::empty(),
      ..Default::default()
    };

    let (sender, receiver) = unbounded();

    return Self {
      raw: Arc::new(RawVkCommandPool::new(device, &create_info).unwrap()),
      primary_buffers: Vec::new(),
      secondary_buffers: Vec::new(),
      receiver,
      sender,
      shared: shared.clone(),
      queue_family_index,
      buffer_allocator: buffer_allocator.clone()
    };
  }

  pub fn get_command_buffer(&mut self, frame: u64, command_buffer_type: CommandBufferType) -> VkCommandBufferRecorder {
    let buffers = if command_buffer_type == CommandBufferType::PRIMARY {
      &mut self.primary_buffers
    } else {
      &mut self.secondary_buffers
    };

    let mut buffer = buffers.pop().unwrap_or_else(|| Box::new(VkCommandBuffer::new(&self.raw.device, &self.raw, command_buffer_type, self.queue_family_index, &self.shared, &self.buffer_allocator)));
    buffer.begin(frame);
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

pub struct VkCommandBuffer {
  buffer: vk::CommandBuffer,
  pool: Arc<RawVkCommandPool>,
  device: Arc<RawVkDevice>,
  state: VkCommandBufferState,
  command_buffer_type: CommandBufferType,
  shared: Arc<VkShared>,
  render_pass: Option<Arc<VkRenderPass>>,
  pipeline: Option<Arc<VkPipeline>>,
  sub_pass: u32,
  trackers: VkLifetimeTrackers,
  queue_family_index: u32,
  descriptor_manager: VkBindingManager,
  buffer_allocator: Arc<BufferAllocator>,
  frame: u64
}

impl VkCommandBuffer {
  pub(crate) fn new(device: &Arc<RawVkDevice>, pool: &Arc<RawVkCommandPool>, command_buffer_type: CommandBufferType, queue_family_index: u32, shared: &Arc<VkShared>, buffer_allocator: &Arc<BufferAllocator>) -> Self {
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
      pipeline: None,
      sub_pass: 0u32,
      shared: shared.clone(),
      state: VkCommandBufferState::Ready,
      trackers: VkLifetimeTrackers::new(),
      queue_family_index,
      descriptor_manager: VkBindingManager::new(device),
      buffer_allocator: buffer_allocator.clone(),
      frame: 0
    };
  }

  pub fn get_handle(&self) -> &vk::CommandBuffer {
    return &self.buffer;
  }

  pub fn get_type(&self) -> CommandBufferType {
    self.command_buffer_type
  }

  pub(crate) fn reset(&mut self) {
    self.state = VkCommandBufferState::Ready;
    self.trackers.reset();
    self.descriptor_manager.reset();
  }

  pub(crate) fn begin(&mut self, frame: u64) {
    assert_eq!(self.state, VkCommandBufferState::Ready);
    debug_assert!(frame >= self.frame );

    self.state = VkCommandBufferState::Recording;
    self.frame = frame;
    unsafe {
      let begin_info = vk::CommandBufferBeginInfo {
        ..Default::default()
      };
      self.device.begin_command_buffer(self.buffer, &begin_info);
    }
  }

  pub(crate) fn end(&mut self) {
    assert_eq!(self.state, VkCommandBufferState::Recording);
    self.state = VkCommandBufferState::Finished;
    unsafe {
      self.device.end_command_buffer(self.buffer);
    }
  }

  pub(crate) fn set_pipeline(&mut self, pipeline: PipelineBinding<VkBackend>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);

    match &pipeline {
      PipelineBinding::Graphics(graphics_pipeline) => {
        let vk_pipeline = graphics_pipeline.get_handle();
        unsafe {
          self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::GRAPHICS, *vk_pipeline);
        }
        self.pipeline = Some((*graphics_pipeline).clone())
      }
      PipelineBinding::Compute(compute_pipeline) => {
        let vk_pipeline = compute_pipeline.get_handle();
        unsafe {
          self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::COMPUTE, *vk_pipeline);
        }
        self.pipeline = Some((*compute_pipeline).clone())
      },
    };
  }

  pub(crate) fn begin_render_pass(&mut self, render_pass: &Arc<VkRenderPass>, frame_buffer: &Arc<VkFrameBuffer>, clear_values: &[vk::ClearValue], recording_mode: RenderpassRecordingMode) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    // TODO: begin info fields
    unsafe {
      let begin_info = vk::RenderPassBeginInfo {
        framebuffer: *frame_buffer.get_handle(),
        render_pass: *render_pass.get_handle(),
        render_area: vk::Rect2D {
          offset: vk::Offset2D { x: 0i32, y: 0i32 },
          extent: vk::Extent2D { width: frame_buffer.width(), height: frame_buffer.height() }
        },
        clear_value_count: clear_values.len() as u32,
        p_clear_values: clear_values.as_ptr(),
        ..Default::default()
      };
      self.device.cmd_begin_render_pass(self.buffer, &begin_info, if recording_mode == RenderpassRecordingMode::Commands { vk::SubpassContents::INLINE } else { vk::SubpassContents::SECONDARY_COMMAND_BUFFERS });
    }
    self.render_pass = Some(render_pass.clone());
    self.sub_pass = 0;
    self.trackers.track_frame_buffer(frame_buffer);
    self.trackers.track_render_pass(render_pass);
  }

  pub(crate) fn end_render_pass(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.cmd_end_render_pass(self.buffer);
    }
    self.render_pass = None;
  }

  pub(crate) fn advance_subpass(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.cmd_next_subpass(self.buffer, vk::SubpassContents::INLINE);
    }
  }

  pub(crate) fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.trackers.track_buffer(vertex_buffer);
    unsafe {
      self.device.cmd_bind_vertex_buffers(self.buffer, 0, &[*vertex_buffer.get_buffer().get_handle()], &[vertex_buffer.get_offset() as u64]);
    }
  }

  pub(crate) fn set_index_buffer(&mut self, index_buffer: &Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.trackers.track_buffer(index_buffer);
    unsafe {
      self.device.cmd_bind_index_buffer(self.buffer, *index_buffer.get_buffer().get_handle(), index_buffer.get_offset() as u64, vk::IndexType::UINT32);
    }
  }

  pub(crate) fn set_viewports(&mut self, viewports: &[ Viewport ]) {
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

  pub(crate) fn set_scissors(&mut self, scissors: &[ Scissor ])  {
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

  pub(crate) fn draw(&mut self, vertices: u32, offset: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().is_graphics());
    unsafe {
      self.device.cmd_draw(self.buffer, vertices, 1, offset, 0);
    }
  }

  pub(crate) fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().is_graphics());
    unsafe {
      self.device.cmd_draw_indexed(self.buffer, indices, instances, first_index, vertex_offset, first_instance);
    }
  }

  pub(crate) fn init_texture_mip_level(&mut self, src_buffer: &Arc<VkBufferSlice>, texture: &Arc<VkTexture>, mip_level: u32, array_layer: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.cmd_pipeline_barrier(self.buffer, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[], &[], &[
      vk::ImageMemoryBarrier {
        src_access_mask: vk::AccessFlags::empty(),
        dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
        old_layout: vk::ImageLayout::UNDEFINED,
        new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        src_queue_family_index: self.queue_family_index,
        dst_queue_family_index: self.queue_family_index,
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
      self.device.cmd_copy_buffer_to_image(self.buffer, *src_buffer.get_buffer().get_handle(), *texture.get_handle(), vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[
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
      self.device.cmd_pipeline_barrier(self.buffer, vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::FRAGMENT_SHADER, vk::DependencyFlags::empty(), &[], &[], &[
        vk::ImageMemoryBarrier {
          src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
          dst_access_mask: vk::AccessFlags::SHADER_READ,
          old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
          new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
          src_queue_family_index: self.queue_family_index,
          dst_queue_family_index: self.queue_family_index,
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
    self.trackers.track_buffer(src_buffer);
    self.trackers.track_texture(texture);
  }

  pub(crate) fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResource::SampledTexture(texture.clone()));
    self.trackers.track_texture_view(texture);
  }

  pub(crate) fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResource::UniformBuffer(buffer.clone()));
    self.trackers.track_buffer(buffer);
  }

  pub(crate) fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResource::UniformBuffer(buffer.clone()));
    self.trackers.track_buffer(buffer);
  }

  pub(crate) fn finish_binding(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    let mut offsets: [u32; 16] = Default::default();
    let mut offsets_count = 0;
    let mut descriptor_sets: [vk::DescriptorSet; 4] = Default::default();
    let mut descriptor_sets_count = 0;
    let mut base_index = 0;

    let pipeline = self.pipeline.as_ref().expect("No pipeline bound");
    let pipeline_layout = pipeline.get_layout();

    let finished_sets = self.descriptor_manager.finish(self.frame, pipeline_layout);
    for (index, set_option) in finished_sets.iter().enumerate() {
      match set_option {
        None => {
          if descriptor_sets_count != 0 {
            unsafe {
              self.device.cmd_bind_descriptor_sets(self.buffer, if pipeline.is_graphics() { vk::PipelineBindPoint::GRAPHICS } else { vk::PipelineBindPoint::COMPUTE }, *pipeline_layout.get_handle(), base_index, &descriptor_sets[0..descriptor_sets_count], &offsets[0..offsets_count]);
              base_index = index as u32 + 1;
              offsets_count = 0;
              descriptor_sets_count = 0;
            }
          }
        },
        Some(set_binding) => {
          descriptor_sets[descriptor_sets_count] = *set_binding.set.get_handle();
          descriptor_sets_count += 1;
          for i in 0..set_binding.dynamic_offset_count as usize {
            offsets[offsets_count] = set_binding.dynamic_offsets[i] as u32;
            offsets_count += 1;
          }
        }
      }
    }

    if descriptor_sets_count != 0 {
      unsafe {
        self.device.cmd_bind_descriptor_sets(self.buffer, vk::PipelineBindPoint::GRAPHICS, *pipeline_layout.get_handle(), base_index, &descriptor_sets[0..descriptor_sets_count], &offsets[0..offsets_count]);
        offsets_count = 0;
        descriptor_sets_count = 0;
      }
    }
  }

  pub(crate) fn upload_dynamic_data<T>(&self, data: T, usage: BufferUsage) -> Arc<VkBufferSlice>
    where T: 'static + Send + Sync + Sized + Clone {
    let slice = self.buffer_allocator.get_slice(MemoryUsage::CpuToGpu, usage, std::mem::size_of::<T>());
    unsafe {
      let mut ptr = slice.map_unsafe(false).expect("Failed to map buffer");
      std::mem::replace::<T>((ptr as *mut T).as_mut().unwrap(), data);
      slice.unmap_unsafe(true);
    }
    Arc::new(slice)
  }

  pub(crate) fn begin_label(&self, label: &str) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    let label_cstring = CString::new(label).unwrap();
    unsafe {
      self.device.instance.debug_utils_loader.cmd_begin_debug_utils_label(self.buffer, &vk::DebugUtilsLabelEXT {
        p_label_name: label_cstring.as_ptr(),
        color: [0.0f32; 4],
        ..Default::default()
      });
    }
  }

  pub(crate) fn end_label(&self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.instance.debug_utils_loader.cmd_end_debug_utils_label(self.buffer);
    }
  }

  pub(crate) fn execute_inner_command_buffer(&mut self, mut submission: VkCommandBufferSubmission) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    assert_eq!(submission.command_buffer_type(), CommandBufferType::SECONDARY);
    unsafe {
      self.device.cmd_execute_commands(self.buffer, &[*submission.get_handle()]);
    }
    submission.mark_submitted();
  }

  pub(crate) fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(!self.pipeline.as_ref().unwrap().is_graphics());
    unsafe {
      self.device.cmd_dispatch(self.buffer, group_count_x, group_count_y, group_count_z);
    }
  }

  pub(crate) fn barrier(
    &mut self,
    src_stage_mask: vk::PipelineStageFlags,
    dst_stage_mask: vk::PipelineStageFlags,
    dependency_flags: vk::DependencyFlags,
    memory_barriers: &[vk::MemoryBarrier],
    buffer_memory_barriers: &[vk::BufferMemoryBarrier],
    image_memory_barriers: &[vk::ImageMemoryBarrier]) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      self.device.cmd_pipeline_barrier(self.buffer, src_stage_mask, dst_stage_mask, dependency_flags, memory_barriers, buffer_memory_barriers, image_memory_barriers);
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

// Small wrapper around VkCommandBuffer to
// disable Send + Sync because sending VkCommandBuffers across threads
// is only safe after recording is done
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
  pub fn begin_render_pass(&mut self, render_pass: &Arc<VkRenderPass>, frame_buffer: &Arc<VkFrameBuffer>, clear_values: &[vk::ClearValue], recording_mode: RenderpassRecordingMode) {
    self.item.as_mut().unwrap().begin_render_pass(render_pass, frame_buffer, clear_values, recording_mode);
  }

  #[inline(always)]
  pub fn end_render_pass(&mut self) {
    self.item.as_mut().unwrap().end_render_pass();
  }

  pub fn advance_subpass(&mut self) {
    self.item.as_mut().unwrap().advance_subpass();
  }

  pub fn execute_inner_command_buffer(&mut self, mut submission: VkCommandBufferSubmission) {
    self.item.as_mut().unwrap().execute_inner_command_buffer(submission);
  }

  pub fn finish(self) -> VkCommandBufferSubmission {
    assert_eq!(self.item.as_ref().unwrap().state, VkCommandBufferState::Recording);
    let mut mut_self = self;
    let mut item = std::mem::replace(&mut mut_self.item, None).unwrap();
    item.end();
    VkCommandBufferSubmission::new(item, mut_self.sender.clone())
  }

  #[inline(always)]
  pub(crate) fn barrier(
    &mut self,
    src_stage_mask: vk::PipelineStageFlags,
    dst_stage_mask: vk::PipelineStageFlags,
    dependency_flags: vk::DependencyFlags,
    memory_barriers: &[vk::MemoryBarrier],
    buffer_memory_barriers: &[vk::BufferMemoryBarrier],
    image_memory_barriers: &[vk::ImageMemoryBarrier]) {
      self.item.as_mut().unwrap().barrier(src_stage_mask, dst_stage_mask, dependency_flags, memory_barriers, buffer_memory_barriers, image_memory_barriers);
  }
}

impl CommandBuffer<VkBackend> for VkCommandBufferRecorder {
  #[inline(always)]
  fn set_pipeline(&mut self, pipeline: PipelineBinding<VkBackend>) {
    self.item.as_mut().unwrap().set_pipeline(pipeline);
  }

  #[inline(always)]
  fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<VkBufferSlice>) {
    self.item.as_mut().unwrap().set_vertex_buffer(vertex_buffer)
  }

  #[inline(always)]
  fn set_index_buffer(&mut self, index_buffer: &Arc<VkBufferSlice>) {
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
  fn init_texture_mip_level(&mut self, src_buffer: &Arc<VkBufferSlice>, texture: &Arc<VkTexture>, mip_level: u32, array_layer: u32) {
    self.item.as_mut().unwrap().init_texture_mip_level(src_buffer, texture, mip_level, array_layer);
  }

  #[inline(always)]
  fn upload_dynamic_data<T>(&mut self, data: T, usage: BufferUsage) -> Arc<VkBufferSlice>
    where T: 'static + Send + Sync + Sized + Clone {
    self.item.as_mut().unwrap().upload_dynamic_data(data, usage)
  }

  #[inline(always)]
  fn draw(&mut self, vertices: u32, offset: u32) {
    self.item.as_mut().unwrap().draw(vertices, offset);
  }

  #[inline(always)]
  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    self.item.as_mut().unwrap().draw_indexed(instances, first_instance, indices, first_index, vertex_offset);
  }

  #[inline(always)]
  fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    self.item.as_mut().unwrap().bind_texture_view(frequency, binding, texture);
  }

  #[inline(always)]
  fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>) {
    self.item.as_mut().unwrap().bind_uniform_buffer(frequency, binding, buffer);
  }

  #[inline(always)]
  fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>) {
    self.item.as_mut().unwrap().bind_storage_buffer(frequency, binding, buffer);
  }

  #[inline(always)]
  fn finish_binding(&mut self) {
    self.item.as_mut().unwrap().finish_binding();
  }

  #[inline(always)]
  fn begin_label(&mut self, label: &str) {
    self.item.as_mut().unwrap().begin_label(label);
  }

  #[inline(always)]
  fn end_label(&mut self) {
    self.item.as_mut().unwrap().end_label();
  }

  #[inline(always)]
  fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
    self.item.as_mut().unwrap().dispatch(group_count_x, group_count_y, group_count_z);
  }
}

pub struct VkCommandBufferSubmission {
  item: Option<Box<VkCommandBuffer>>,
  sender: Sender<Box<VkCommandBuffer>>
}

unsafe impl Send for VkCommandBufferSubmission {}

impl VkCommandBufferSubmission {
  fn new(item: Box<VkCommandBuffer>, sender: Sender<Box<VkCommandBuffer>>) -> Self {
    Self {
      item: Some(item),
      sender
    }
  }

  pub(crate) fn mark_submitted(&mut self) {
    unsafe {
      let mut item = self.item.as_mut().unwrap();
      assert_eq!(item.state, VkCommandBufferState::Finished);
      item.state = VkCommandBufferState::Submitted;
    }
  }

  pub(crate) fn get_handle(&self) -> &vk::CommandBuffer {
    unsafe {
      &self.item.as_ref().unwrap().get_handle()
    }
  }

  pub(crate) fn command_buffer_type(&self) -> CommandBufferType {
    self.item.as_ref().unwrap().command_buffer_type
  }
}

impl Drop for VkCommandBufferSubmission {
  fn drop(&mut self) {
    let item = std::mem::replace(&mut self.item, None).unwrap();
    self.sender.send(item);
  }
}
