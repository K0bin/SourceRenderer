use std::{cmp::min, sync::Arc};
use std::hash::Hash;
use std::marker::PhantomData;
use std::cmp::max;
use std::ffi::{CString};

use ash::vk;

use crossbeam_channel::{Receiver, Sender, unbounded};

use smallvec::SmallVec;
use sourcerenderer_core::graphics::{AttachmentInfo, Barrier, BindingFrequency, Buffer, BufferInfo, BufferUsage, LoadOp, MemoryUsage, PipelineBinding, RenderPassBeginInfo, RenderPassInfo, ShaderType, StoreOp, Texture, TextureUsage, get_default_state};
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::RenderpassRecordingMode;
use sourcerenderer_core::graphics::Viewport;
use sourcerenderer_core::graphics::Scissor;
use sourcerenderer_core::graphics::Resettable;

use crate::{raw::RawVkDevice, texture::VkSampler};
use crate::VkRenderPass;
use crate::VkFrameBuffer;
use crate::VkPipeline;
use crate::VkBackend;
use crate::raw::*;
use crate::VkShared;
use crate::buffer::{VkBufferSlice, BufferAllocator};
use crate::VkTexture;
use crate::descriptor::{VkBindingManager, VkBoundResource};
use crate::texture::VkTextureView;
use crate::lifetime_tracker::VkLifetimeTrackers;

#[allow(clippy::vec_box)]
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

    Self {
      raw: Arc::new(RawVkCommandPool::new(device, &create_info).unwrap()),
      primary_buffers: Vec::new(),
      secondary_buffers: Vec::new(),
      receiver,
      sender,
      shared: shared.clone(),
      queue_family_index,
      buffer_allocator: buffer_allocator.clone()
    }
  }

  pub fn get_command_buffer(&mut self, frame: u64) -> VkCommandBufferRecorder {
    let mut buffer = self.primary_buffers.pop().unwrap_or_else(|| Box::new(VkCommandBuffer::new(&self.raw.device, &self.raw, CommandBufferType::PRIMARY, self.queue_family_index, &self.shared, &self.buffer_allocator)));
    buffer.begin(frame, None);
    VkCommandBufferRecorder::new(buffer, self.sender.clone())
  }

  pub fn get_inner_command_buffer(&mut self, frame: u64, inner_info: Option<&VkInnerCommandBufferInfo>) -> VkCommandBufferRecorder {
    let mut buffer = self.secondary_buffers.pop().unwrap_or_else(|| Box::new(VkCommandBuffer::new(&self.raw.device, &self.raw, CommandBufferType::SECONDARY, self.queue_family_index, &self.shared, &self.buffer_allocator)));
    buffer.begin(frame, inner_info);
    VkCommandBufferRecorder::new(buffer, self.sender.clone())
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

pub struct VkInnerCommandBufferInfo {
  pub render_pass: Arc<VkRenderPass>,
  pub sub_pass: u32,
  pub frame_buffer: Arc<VkFrameBuffer>
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
  pending_image_barriers: Vec<vk::ImageMemoryBarrier>,
  pending_buffer_barriers: Vec<vk::BufferMemoryBarrier>,
  pending_src_stage_flags: vk::PipelineStageFlags,
  pending_dst_stage_flags: vk::PipelineStageFlags,  
  frame: u64,
  inheritance: Option<VkInnerCommandBufferInfo>
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
    VkCommandBuffer {
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
      pending_buffer_barriers: Vec::with_capacity(4),
      pending_image_barriers: Vec::with_capacity(4),
      pending_src_stage_flags: vk::PipelineStageFlags::empty(),
      pending_dst_stage_flags: vk::PipelineStageFlags::empty(),
      frame: 0,
      inheritance: None
    }
  }

  pub fn get_handle(&self) -> &vk::CommandBuffer {
    &self.buffer
  }

  pub fn get_type(&self) -> CommandBufferType {
    self.command_buffer_type
  }

  pub(crate) fn reset(&mut self) {
    self.state = VkCommandBufferState::Ready;
    self.trackers.reset();
    self.descriptor_manager.reset();
  }

  pub(crate) fn begin(&mut self, frame: u64, inner_info: Option<&VkInnerCommandBufferInfo>) {
    assert_eq!(self.state, VkCommandBufferState::Ready);
    debug_assert!(frame >= self.frame );

    self.state = VkCommandBufferState::Recording;
    self.frame = frame;

    let (flags, inhertiance_info) = if let Some(inner_info) = inner_info {
      (
        vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE,
        vk::CommandBufferInheritanceInfo {
          render_pass: *inner_info.render_pass.get_handle(),
          subpass: inner_info.sub_pass,
          framebuffer: *inner_info.frame_buffer.get_handle(),
          ..Default::default()
        }
      )
    } else {
      (vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT, Default::default())
    };

    unsafe {
      self.device.begin_command_buffer(self.buffer, &vk::CommandBufferBeginInfo {
        flags,
        p_inheritance_info: &inhertiance_info as *const vk::CommandBufferInheritanceInfo,
        ..Default::default()
      }).unwrap();
    }
  }

  pub(crate) fn end(&mut self) {
    assert_eq!(self.state, VkCommandBufferState::Recording);
    self.flush_barriers();
    self.state = VkCommandBufferState::Finished;

    if self.render_pass.is_some() {
      self.end_render_pass();
    }

    unsafe {
      self.device.end_command_buffer(self.buffer).unwrap();
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

        self.trackers.track_pipeline(*graphics_pipeline);
        self.pipeline = Some((*graphics_pipeline).clone())
      }
      PipelineBinding::Compute(compute_pipeline) => {
        let vk_pipeline = compute_pipeline.get_handle();
        unsafe {
          self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::COMPUTE, *vk_pipeline);
        }
        self.trackers.track_pipeline(*compute_pipeline);
        self.pipeline = Some((*compute_pipeline).clone())
      },
    };
  }

  pub(crate) fn begin_render_pass(&mut self, render_pass: &Arc<VkRenderPass>, frame_buffer: &Arc<VkFrameBuffer>, clear_values: &[vk::ClearValue], recording_mode: RenderpassRecordingMode) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pending_image_barriers.is_empty() && self.pending_buffer_barriers.is_empty() && self.pending_dst_stage_flags.is_empty() && self.pending_src_stage_flags.is_empty());
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
    debug_assert!(self.render_pass.is_some());
    unsafe {
      self.device.cmd_end_render_pass(self.buffer);
    }
    self.render_pass = None;
  }

  pub(crate) fn advance_subpass(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_some());
    unsafe {
      self.device.cmd_next_subpass(self.buffer, vk::SubpassContents::INLINE);
    }
    self.sub_pass += 1;
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
    debug_assert!(self.pending_image_barriers.is_empty() && self.pending_buffer_barriers.is_empty() && self.pending_dst_stage_flags.is_empty() && self.pending_src_stage_flags.is_empty());
    unsafe {
      self.device.cmd_draw(self.buffer, vertices, 1, offset, 0);
    }
  }

  pub(crate) fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().is_graphics());
    debug_assert!(self.pending_image_barriers.is_empty() && self.pending_buffer_barriers.is_empty() && self.pending_dst_stage_flags.is_empty() && self.pending_src_stage_flags.is_empty());
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

  pub(crate) fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>, sampler: &Arc<VkSampler>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResource::SampledTexture(texture.clone(), sampler.clone()));
    self.trackers.track_texture_view(texture);
    self.trackers.track_sampler(sampler);
  }

  pub(crate) fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResource::UniformBuffer(buffer.clone()));
    self.trackers.track_buffer(buffer);
  }

  pub(crate) fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResource::StorageBuffer(buffer.clone()));
    self.trackers.track_buffer(buffer);
  }

  pub(crate) fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResource::StorageTexture(texture.clone()));
    self.trackers.track_texture_view(texture);
  }

  pub(crate) fn finish_binding(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());

    self.flush_barriers();

    let mut offsets = SmallVec::<[u32; 16]>::new();
    let mut descriptor_sets = SmallVec::<[vk::DescriptorSet; 4]>::new();
    let mut base_index = 0;

    let pipeline = self.pipeline.as_ref().expect("No pipeline bound");
    let pipeline_layout = pipeline.get_layout();

    let finished_sets = self.descriptor_manager.finish(self.frame, pipeline_layout);
    for (index, set_option) in finished_sets.iter().enumerate() {
      match set_option {
        None => {
          if !descriptor_sets.is_empty() {
            unsafe {
              self.device.cmd_bind_descriptor_sets(self.buffer, if pipeline.is_graphics() { vk::PipelineBindPoint::GRAPHICS } else { vk::PipelineBindPoint::COMPUTE }, *pipeline_layout.get_handle(), base_index, &descriptor_sets, &offsets);
              offsets.clear();
              descriptor_sets.clear();
            }
          }
          base_index = index as u32 + 1;
        },
        Some(set_binding) => {
          descriptor_sets.push(*set_binding.set.get_handle());
          for i in 0..set_binding.dynamic_offset_count as usize {
            offsets.push(set_binding.dynamic_offsets[i] as u32);
          }
        }
      }
    }

    if !descriptor_sets.is_empty() {
      unsafe {
        self.device.cmd_bind_descriptor_sets(self.buffer, vk::PipelineBindPoint::GRAPHICS, *pipeline_layout.get_handle(), base_index, &descriptor_sets, &offsets);
      }
    }
  }

  pub(crate) fn upload_dynamic_data<T>(&self, data: &[T], usage: BufferUsage) -> Arc<VkBufferSlice>
    where T: 'static + Send + Sync + Sized + Clone {
    debug_assert!(get_default_state(MemoryUsage::CpuToGpu).is_empty() || usage.intersects(get_default_state(MemoryUsage::CpuToGpu)));

    let slice = self.buffer_allocator.get_slice(&BufferInfo {
      size: std::mem::size_of_val(data),
      usage
    }, MemoryUsage::CpuToGpu,  None);
    unsafe {
      let ptr = slice.map_unsafe(false).expect("Failed to map buffer");
      std::ptr::copy(data.as_ptr(), ptr as *mut T, data.len());
      slice.unmap_unsafe(true);
    }
    slice
  }


  pub(crate) fn upload_dynamic_data_inline<T>(&self, data: &[T], visible_for_shader_type: ShaderType)
  where T: 'static + Send + Sync + Sized + Clone {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    let pipeline = self.pipeline.as_ref().expect("No pipeline bound");
    let pipeline_layout = pipeline.get_layout();
    let range = pipeline_layout.push_constant_range(visible_for_shader_type).expect("No push constants set up for shader");
    let size = std::mem::size_of_val(data);
    unsafe {
      self.device.cmd_push_constants(
        self.buffer,
        *pipeline_layout.get_handle(),
        range.stage_flags,
        range.offset,
        std::slice::from_raw_parts(data.as_ptr() as *const u8, min(size, range.size as usize))
      );
    }
  }

  pub(crate) fn begin_label(&self, label: &str) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    let label_cstring = CString::new(label).unwrap();
    if let Some(debug_utils) = self.device.instance.debug_utils.as_ref() {
      unsafe {
        debug_utils.debug_utils_loader.cmd_begin_debug_utils_label(self.buffer, &vk::DebugUtilsLabelEXT {
          p_label_name: label_cstring.as_ptr(),
          color: [0.0f32; 4],
          ..Default::default()
        });
      }
    }
  }

  pub(crate) fn end_label(&self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    if let Some(debug_utils) = self.device.instance.debug_utils.as_ref() {
      unsafe {
        debug_utils.debug_utils_loader.cmd_end_debug_utils_label(self.buffer);
      }
    }
  }

  pub(crate) fn execute_inner(&mut self, mut submissions: Vec<VkCommandBufferSubmission>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    if submissions.is_empty() {
      return;
    }

    for submission in &submissions {
      assert_eq!(submission.command_buffer_type(), CommandBufferType::SECONDARY);
    }
    let submission_handles: SmallVec<[vk::CommandBuffer; 16]> = submissions
      .iter()
      .map(|s| *s.get_handle())
      .collect();
    unsafe {
      self.device.cmd_execute_commands(self.buffer, &submission_handles);
    }
    for submission in &mut submissions {
      submission.mark_submitted();
    }
  }

  pub(crate) fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(!self.pipeline.as_ref().unwrap().is_graphics());
    debug_assert!(self.pending_image_barriers.is_empty() && self.pending_buffer_barriers.is_empty() && self.pending_dst_stage_flags.is_empty() && self.pending_src_stage_flags.is_empty());
    unsafe {
      self.device.cmd_dispatch(self.buffer, group_count_x, group_count_y, group_count_z);
    }
  }


  pub(crate) fn blit(&mut self, src_texture: &Arc<VkTexture>, src_array_layer: u32, src_mip_level: u32, dst_texture: &Arc<VkTexture>, dst_array_layer: u32, dst_mip_level: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_none());
    debug_assert!(self.pending_image_barriers.is_empty() && self.pending_buffer_barriers.is_empty() && self.pending_dst_stage_flags.is_empty() && self.pending_src_stage_flags.is_empty());
    let src_info = src_texture.get_info();
    let dst_info = dst_texture.get_info();
    let mut src_aspect = vk::ImageAspectFlags::empty();
    if src_info.format.is_stencil() {
      src_aspect |= vk::ImageAspectFlags::STENCIL;
    }
    if src_info.format.is_depth() {
      src_aspect |= vk::ImageAspectFlags::DEPTH;
    }
    if src_aspect.is_empty() {
      src_aspect = vk::ImageAspectFlags::COLOR;
    }
    let mut dst_aspect = vk::ImageAspectFlags::empty();
    if dst_info.format.is_stencil() {
      dst_aspect |= vk::ImageAspectFlags::STENCIL;
    }
    if dst_info.format.is_depth() {
      dst_aspect |= vk::ImageAspectFlags::DEPTH;
    }
    if dst_aspect.is_empty() {
      dst_aspect = vk::ImageAspectFlags::COLOR;
    }

    unsafe {
      self.device.cmd_blit_image(self.buffer, *src_texture.get_handle(), vk::ImageLayout::TRANSFER_SRC_OPTIMAL, *dst_texture.get_handle(), vk::ImageLayout::TRANSFER_DST_OPTIMAL,
      &[vk::ImageBlit {
        src_subresource: vk::ImageSubresourceLayers {
          aspect_mask: src_aspect,
          mip_level: src_mip_level,
          base_array_layer: src_array_layer,
          layer_count: 1
        },
        src_offsets: [vk::Offset3D {
          x: 0,
          y: 0,
          z: 0
        }, vk::Offset3D {
          x: src_info.width as i32,
          y: src_info.height as i32,
          z: src_info.depth as i32,
        }],
        dst_subresource: vk::ImageSubresourceLayers {
          aspect_mask: dst_aspect,
          mip_level: dst_mip_level,
          base_array_layer: dst_array_layer,
          layer_count: 1
        },
        dst_offsets: [vk::Offset3D {
          x: 0,
          y: 0,
          z: 0
        }, vk::Offset3D {
          x: dst_info.width as i32,
          y: dst_info.height as i32,
          z: dst_info.depth as i32,
        }]
      }], vk::Filter::LINEAR);
    }
  }

  pub(crate) fn barrier_1<'a>(
    &mut self,
    barriers: &[Barrier<VkBackend>]
  ) {
    for barrier in barriers {
      match barrier {
        Barrier::TextureBarrier { old_primary_usage, new_primary_usage, old_usages, new_usages, texture } => {
          debug_assert!(old_usages.contains(*old_primary_usage) || old_usages.is_empty());
          debug_assert!(new_usages.contains(*new_primary_usage) || (!new_primary_usage.is_empty() && new_usages.is_empty()));
          debug_assert!(!new_usages.is_empty() || (new_usages.is_empty() && old_usages.is_empty()));
          debug_assert!(texture.get_info().usage.contains(*new_primary_usage));
          debug_assert!(texture.get_info().usage.contains(*new_usages));
          debug_assert!(new_primary_usage.bits().count_ones() == 1);
          debug_assert!(old_primary_usage.bits().count_ones() == 1 || old_primary_usage.bits().count_ones() == 0);

          let old_layout = texture_usage_to_image_layout(*old_primary_usage);
          let new_layout = texture_usage_to_image_layout(*new_primary_usage);

          let mut src_access = texture_usage_to_access(*old_usages);
          let mut src_stages = texture_usage_to_stage(*old_usages);

          let mut dst_access = vk::AccessFlags::empty();
          let mut dst_stages = vk::PipelineStageFlags::empty();

          let mut new_usages_bits = new_usages.bits();
          while new_usages_bits != 0 {
            let usage_bit = new_usages_bits & (1 << new_usages_bits.trailing_zeros());
            let usage = TextureUsage::from_bits_truncate(usage_bit);
            let usage_layout = texture_usage_to_image_layout(usage);
            if usage_layout == new_layout {
              // using the texture with this texture usage will not require a layout transition barrier
              dst_access |= texture_usage_to_access(usage);
              dst_stages |= texture_usage_to_stage(usage);
            }
            new_usages_bits &= !usage_bit;
          }

          let info = texture.get_info();
          let mut aspect_mask = vk::ImageAspectFlags::empty();
          if info.format.is_depth() {
            aspect_mask |= vk::ImageAspectFlags::DEPTH;
          }
          if info.format.is_stencil() {
            aspect_mask |= vk::ImageAspectFlags::STENCIL;
          }
          if aspect_mask.is_empty() {
            aspect_mask |= vk::ImageAspectFlags::COLOR;
          }

          if dst_access.is_empty() && src_access.is_empty() && src_stages.is_empty() && dst_stages.is_empty() && old_layout == new_layout {
            // Same layout, no memory operation, no wait
            continue;
          }

          if dst_access.is_empty() && src_access.is_empty() {
            // Pure layout transition
            dst_access = texture_usage_to_access(*new_primary_usage);
            dst_stages = texture_usage_to_stage(*new_primary_usage);
            src_access = texture_usage_to_access(*old_primary_usage);
            src_stages = texture_usage_to_stage(*old_primary_usage);
          }

          self.pending_image_barriers.push(vk::ImageMemoryBarrier {
            src_access_mask: src_access & WRITE_ACCESS_MASK,
            dst_access_mask: dst_access,
            old_layout,
            new_layout,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            image: *texture.get_handle(),
            subresource_range: vk::ImageSubresourceRange {
              aspect_mask,
              base_array_layer: 0,
              base_mip_level: 0,
              level_count: info.mip_levels,
              layer_count: info.array_length
            },
            ..Default::default()
          });
          self.pending_dst_stage_flags |= dst_stages;
          self.pending_src_stage_flags |= src_stages;
        },
        Barrier::BufferBarrier { new_primary_usage, old_primary_usage, old_usages, new_usages, buffer } => {
          debug_assert!(old_usages.contains(*old_primary_usage) || (!old_primary_usage.is_empty() && old_usages.is_empty()));
          debug_assert!(new_usages.contains(*new_primary_usage) || new_usages.is_empty());
          debug_assert!(!new_usages.is_empty() || (new_usages.is_empty() && old_usages.is_empty()));
          debug_assert!(buffer.get_info().usage.contains(*new_primary_usage));
          debug_assert!(buffer.get_info().usage.contains(*new_usages));
          debug_assert!(new_primary_usage.bits().count_ones() == 1);
          debug_assert!(old_primary_usage.bits().count_ones() == 1 || old_primary_usage.bits().count_ones() == 0);

          let src_stages = buffer_usage_to_stage(*old_usages);
          let dst_stages = buffer_usage_to_stage(*new_usages);
          let src_access = buffer_usage_to_access(*old_usages);
          let dst_access = buffer_usage_to_access(*new_usages);

          if dst_access.is_empty() && src_access.is_empty() && src_stages.is_empty() && dst_stages.is_empty() {
            continue;
          }

          self.pending_buffer_barriers.push(vk::BufferMemoryBarrier {
            src_access_mask: src_access & WRITE_ACCESS_MASK,
            dst_access_mask: dst_access,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: *buffer.get_buffer().get_handle(),
            offset: buffer.get_offset() as u64,
            size: buffer.get_length() as u64,
            ..Default::default()
          });
          self.pending_dst_stage_flags |= dst_stages;
          self.pending_src_stage_flags |= src_stages;
        },
      }
    }
  }

  pub(crate) fn flush_barriers(&mut self) {
    const FULL_BARRIER: bool = false; // IN CASE OF EMERGENCY, SET TO TRUE
    if FULL_BARRIER {
      let full_memory_barrier = vk::MemoryBarrier {
        src_access_mask: vk::AccessFlags::MEMORY_WRITE,
        dst_access_mask: vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
        ..Default::default()
      };
      unsafe {
        self.device.cmd_pipeline_barrier(
          self.buffer,
          vk::PipelineStageFlags::ALL_COMMANDS,
          vk::PipelineStageFlags::ALL_COMMANDS,
          vk::DependencyFlags::empty(),
          &[full_memory_barrier],
          &self.pending_buffer_barriers[..],
          &self.pending_image_barriers[..]);
      }
      self.pending_src_stage_flags = vk::PipelineStageFlags::empty();
      self.pending_dst_stage_flags = vk::PipelineStageFlags::empty();
      self.pending_image_barriers.clear();
      self.pending_buffer_barriers.clear();
      return;
    }

    if self.pending_src_stage_flags.is_empty() && self.pending_dst_stage_flags.is_empty() && self.pending_image_barriers.is_empty() && self.pending_buffer_barriers.is_empty() {
      return;
    }
    if self.pending_dst_stage_flags.is_empty() {
      self.pending_dst_stage_flags = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    }
    if self.pending_src_stage_flags.is_empty() {
      self.pending_src_stage_flags = vk::PipelineStageFlags::TOP_OF_PIPE;
    }

    unsafe {
      self.device.cmd_pipeline_barrier(
        self.buffer,
        self.pending_src_stage_flags,
        self.pending_dst_stage_flags,
        vk::DependencyFlags::empty(),
        &[],
        &self.pending_buffer_barriers[..],
        &self.pending_image_barriers[..]);
    }
    self.pending_src_stage_flags = vk::PipelineStageFlags::empty();
    self.pending_dst_stage_flags = vk::PipelineStageFlags::empty();
    self.pending_image_barriers.clear();
    self.pending_buffer_barriers.clear();
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

  pub(crate) fn begin_render_pass_1(&mut self, renderpass_begin_info: &RenderPassBeginInfo<VkBackend>, recording_mode: RenderpassRecordingMode) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(!self.render_pass.is_some());

    self.flush_barriers();

    let mut attachment_infos = Vec::with_capacity(renderpass_begin_info.attachments.len());
    let mut width = 0u32;
    let mut height = 0u32;
    let mut attachment_views = SmallVec::<[&Arc<VkTextureView>; 8]>::with_capacity(renderpass_begin_info.attachments.len());
    let mut clear_values = SmallVec::<[vk::ClearValue; 8]>::with_capacity(renderpass_begin_info.attachments.len());

    for attachment in renderpass_begin_info.attachments {
      let view = match &attachment.view {
        sourcerenderer_core::graphics::RenderPassAttachmentView::RenderTarget(view) => *view,
        sourcerenderer_core::graphics::RenderPassAttachmentView::DepthStencil(view) => *view
      };

      let info = view.texture().get_info();
      attachment_infos.push(AttachmentInfo {
        format: info.format,
        samples: info.samples,
        load_op: attachment.load_op,
        store_op: attachment.store_op,
        stencil_load_op: LoadOp::DontCare,
        stencil_store_op: StoreOp::DontCare,
      });
      width = width.max(info.width);
      height = height.max(info.height);
      attachment_views.push(view);

      clear_values.push(if info.format.is_depth() || info.format.is_stencil() {
        vk::ClearValue {
          depth_stencil: vk::ClearDepthStencilValue {
            depth: 1f32,
            stencil: 0u32,
          }
        }
      } else {
        vk::ClearValue {
          color: vk::ClearColorValue {
            float32: [0f32; 4]
          }
        }
      });
    }

    let renderpass_info = RenderPassInfo {
      attachments: attachment_infos,
      subpasses: renderpass_begin_info.subpasses.iter().map(|s| s.clone()).collect(),
    };

    let renderpass = self.shared.get_render_pass(&renderpass_info);
    let framebuffer = self.shared.get_framebuffer(&renderpass, &attachment_views);

    // TODO: begin info fields
    unsafe {
      let begin_info = vk::RenderPassBeginInfo {
        framebuffer: *framebuffer.get_handle(),
        render_pass: *renderpass.get_handle(),
        render_area: vk::Rect2D {
          offset: vk::Offset2D { x: 0i32, y: 0i32 },
          extent: vk::Extent2D { width, height }
        },
        clear_value_count: clear_values.len() as u32,
        p_clear_values: clear_values.as_ptr(),
        ..Default::default()
      };
      self.device.cmd_begin_render_pass(self.buffer, &begin_info, if recording_mode == RenderpassRecordingMode::Commands { vk::SubpassContents::INLINE } else { vk::SubpassContents::SECONDARY_COMMAND_BUFFERS });
    }
    self.sub_pass = 0;
    self.trackers.track_frame_buffer(&framebuffer);
    self.trackers.track_render_pass(&renderpass);
    self.render_pass = Some(renderpass.clone());
    self.inheritance = Some(VkInnerCommandBufferInfo {
      render_pass: renderpass,
      sub_pass: 0,
      frame_buffer: framebuffer
    });
  }

  pub fn inheritance(&self) -> &VkInnerCommandBufferInfo {
    self.inheritance.as_ref().unwrap()
  }

  pub fn wait_events(
    &mut self,
    events: &[vk::Event],
    src_stage_mask: vk::PipelineStageFlags,
    dst_stage_mask: vk::PipelineStageFlags,
    memory_barriers: &[vk::MemoryBarrier],
    buffer_memory_barriers: &[vk::BufferMemoryBarrier],
    image_memory_barriers: &[vk::ImageMemoryBarrier]
  ) {
    unsafe {
      self.device.cmd_wait_events(self.buffer, events, src_stage_mask, dst_stage_mask, memory_barriers, buffer_memory_barriers, image_memory_barriers);
    }
  }

  pub fn signal_event(
    &mut self,
    event: vk::Event,
    stage_mask: vk::PipelineStageFlags
  ) {
    unsafe {
      self.device.cmd_set_event(self.buffer, event, stage_mask);
    }
  }
}

impl Drop for VkCommandBuffer {
  fn drop(&mut self) {
    if self.state == VkCommandBufferState::Submitted {
      unsafe { self.device.device_wait_idle() }.unwrap();
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
    self.sender.send(item).unwrap();
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
  #[inline(always)]
  pub(crate) fn barrier_vk(
    &mut self,
    src_stage_mask: vk::PipelineStageFlags,
    dst_stage_mask: vk::PipelineStageFlags,
    dependency_flags: vk::DependencyFlags,
    memory_barriers: &[vk::MemoryBarrier],
    buffer_memory_barriers: &[vk::BufferMemoryBarrier],
    image_memory_barriers: &[vk::ImageMemoryBarrier]) {
    self.item.as_mut().unwrap().barrier(src_stage_mask, dst_stage_mask, dependency_flags, memory_barriers, buffer_memory_barriers, image_memory_barriers);
  }

  pub fn wait_events(
    &mut self,
    events: &[vk::Event],
    src_stage_mask: vk::PipelineStageFlags,
    dst_stage_mask: vk::PipelineStageFlags,
    memory_barriers: &[vk::MemoryBarrier],
    buffer_memory_barriers: &[vk::BufferMemoryBarrier],
    image_memory_barriers: &[vk::ImageMemoryBarrier]
  ) {
    self.item.as_mut().unwrap().wait_events(events, src_stage_mask, dst_stage_mask, memory_barriers, buffer_memory_barriers, image_memory_barriers);
  }


  pub fn signal_event(
    &mut self,
    event: vk::Event,
    stage_mask: vk::PipelineStageFlags
  ) {
    self.item.as_mut().unwrap().signal_event(event, stage_mask);
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
  fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Arc<VkBufferSlice>
    where T: 'static + Send + Sync + Sized + Clone {
    self.item.as_mut().unwrap().upload_dynamic_data(data, usage)
  }

  #[inline(always)]
  fn upload_dynamic_data_inline<T>(&mut self, data: &[T], visible_for_shader_type: ShaderType)
    where T: 'static + Send + Sync + Sized + Clone {
    self.item.as_mut().unwrap().upload_dynamic_data_inline(data, visible_for_shader_type);
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
  fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>, sampler: &Arc<VkSampler>) {
    self.item.as_mut().unwrap().bind_texture_view(frequency, binding, texture, sampler);
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
  fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    self.item.as_mut().unwrap().bind_storage_texture(frequency, binding, texture);
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

  #[inline(always)]
  fn blit(&mut self, src_texture: &Arc<VkTexture>, src_array_layer: u32, src_mip_level: u32, dst_texture: &Arc<VkTexture>, dst_array_layer: u32, dst_mip_level: u32) {
    self.item.as_mut().unwrap().blit(src_texture, src_array_layer, src_mip_level, dst_texture, dst_array_layer, dst_mip_level);
  }

  fn finish(self) -> VkCommandBufferSubmission {
    assert_eq!(self.item.as_ref().unwrap().state, VkCommandBufferState::Recording);
    let mut mut_self = self;
    let mut item = std::mem::replace(&mut mut_self.item, None).unwrap();
    item.end();
    VkCommandBufferSubmission::new(item, mut_self.sender.clone())
  }

  #[inline(always)]
  fn barrier<'a>(&mut self, barriers: &[Barrier<VkBackend>]) {
    self.item.as_mut().unwrap().barrier_1(barriers);
  }


  #[inline(always)]
  fn flush_barriers(&mut self) {
    self.item.as_mut().unwrap().flush_barriers();
  }

  fn begin_render_pass_1(&mut self, renderpass_info: &RenderPassBeginInfo<VkBackend>, recording_mode: RenderpassRecordingMode) {
    self.item.as_mut().unwrap().begin_render_pass_1(renderpass_info, recording_mode);
  }

  fn advance_subpass(&mut self) {
    self.item.as_mut().unwrap().advance_subpass();
  }

  fn end_render_pass(&mut self) {
    self.item.as_mut().unwrap().end_render_pass();
  }

  type CommandBufferInheritance = VkInnerCommandBufferInfo;

  fn inheritance(&self) -> &VkInnerCommandBufferInfo {
    self.item.as_ref().unwrap().inheritance()
  }

  fn execute_inner(&mut self, submission: Vec<VkCommandBufferSubmission>) {
    self.item.as_mut().unwrap().execute_inner(submission);
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
    let item = self.item.as_mut().unwrap();
    assert_eq!(item.state, VkCommandBufferState::Finished);
    item.state = VkCommandBufferState::Submitted;
  }

  pub(crate) fn get_handle(&self) -> &vk::CommandBuffer {
    &self.item.as_ref().unwrap().get_handle()
  }

  pub(crate) fn command_buffer_type(&self) -> CommandBufferType {
    self.item.as_ref().unwrap().command_buffer_type
  }

  pub(crate) fn queue_family_index(&self) -> u32 {
    self.item.as_ref().unwrap().queue_family_index
  }
}

impl Drop for VkCommandBufferSubmission {
  fn drop(&mut self) {
    let item = std::mem::replace(&mut self.item, None).unwrap();
    self.sender.send(item).unwrap();
  }
}

fn texture_usage_to_access(texture_usage: TextureUsage) -> vk::AccessFlags {
  let mut flags = vk::AccessFlags::empty();
  if texture_usage.contains(TextureUsage::BLIT_DST)
    || texture_usage.contains(TextureUsage::COPY_DST)
    || texture_usage.contains(TextureUsage::RESOLVE_DST) {
    flags |= vk::AccessFlags::TRANSFER_WRITE;
  }
  if texture_usage.contains(TextureUsage::BLIT_SRC)
    || texture_usage.contains(TextureUsage::COPY_SRC)
    || texture_usage.contains(TextureUsage::RESOLVE_SRC) {
    flags |= vk::AccessFlags::TRANSFER_READ;
  }
  if texture_usage.contains(TextureUsage::RENDER_TARGET) {
    flags |= vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::COLOR_ATTACHMENT_READ;
  }
  if texture_usage.contains(TextureUsage::DEPTH_READ) {
    flags |= vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ;
  }
  if texture_usage.contains(TextureUsage::DEPTH_WRITE) {
    flags |= vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
  }
  if texture_usage.contains(TextureUsage::VERTEX_SHADER_STORAGE_WRITE)
    || texture_usage.contains(TextureUsage::FRAGMENT_SHADER_STORAGE_WRITE)
    || texture_usage.contains(TextureUsage::COMPUTE_SHADER_STORAGE_WRITE) {
    flags |= vk::AccessFlags::SHADER_WRITE;
  }
  if texture_usage.contains(TextureUsage::VERTEX_SHADER_STORAGE_READ)
    || texture_usage.contains(TextureUsage::FRAGMENT_SHADER_STORAGE_READ)
    || texture_usage.contains(TextureUsage::COMPUTE_SHADER_STORAGE_READ)
    || texture_usage.contains(TextureUsage::VERTEX_SHADER_SAMPLED)
    || texture_usage.contains(TextureUsage::FRAGMENT_SHADER_SAMPLED)
    || texture_usage.contains(TextureUsage::COMPUTE_SHADER_SAMPLED) {
    flags |= vk::AccessFlags::SHADER_READ;
  }
  if texture_usage.contains(TextureUsage::FRAGMENT_SHADER_LOCAL) {
    flags |= vk::AccessFlags::INPUT_ATTACHMENT_READ;
  }
  flags
}

fn texture_usage_to_stage(texture_usage: TextureUsage) -> vk::PipelineStageFlags {
  let mut flags = vk::PipelineStageFlags::empty();
  if texture_usage.contains(TextureUsage::BLIT_DST)
    || texture_usage.contains(TextureUsage::COPY_DST)
    || texture_usage.contains(TextureUsage::RESOLVE_DST)
    || texture_usage.contains(TextureUsage::BLIT_SRC)
    || texture_usage.contains(TextureUsage::COPY_SRC)
    || texture_usage.contains(TextureUsage::RESOLVE_SRC) {
    flags |= vk::PipelineStageFlags::TRANSFER;
  }
  if texture_usage.contains(TextureUsage::RENDER_TARGET) {
    flags |= vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
  }
  if texture_usage.contains(TextureUsage::DEPTH_READ) {
    flags |= vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS;
  }
  if texture_usage.contains(TextureUsage::DEPTH_WRITE) {
    flags |= vk::PipelineStageFlags::LATE_FRAGMENT_TESTS;
  }
  if texture_usage.contains(TextureUsage::VERTEX_SHADER_STORAGE_WRITE)
    || texture_usage.contains(TextureUsage::VERTEX_SHADER_STORAGE_READ)
    || texture_usage.contains(TextureUsage::VERTEX_SHADER_SAMPLED) {
    flags |= vk::PipelineStageFlags::VERTEX_SHADER;
  }
  if texture_usage.contains(TextureUsage::FRAGMENT_SHADER_STORAGE_WRITE)
    || texture_usage.contains(TextureUsage::FRAGMENT_SHADER_STORAGE_READ)
    || texture_usage.contains(TextureUsage::FRAGMENT_SHADER_SAMPLED)
    || texture_usage.contains(TextureUsage::FRAGMENT_SHADER_LOCAL) {
    flags |= vk::PipelineStageFlags::FRAGMENT_SHADER;
  }
  if texture_usage.contains(TextureUsage::COMPUTE_SHADER_STORAGE_WRITE)
    || texture_usage.contains(TextureUsage::COMPUTE_SHADER_STORAGE_READ)
    || texture_usage.contains(TextureUsage::COMPUTE_SHADER_SAMPLED) {
    flags |= vk::PipelineStageFlags::COMPUTE_SHADER;
  }
  if texture_usage.contains(TextureUsage::PRESENT) {
    flags |= vk::PipelineStageFlags::BOTTOM_OF_PIPE;
  }
  flags
}

fn texture_usage_to_image_layout(texture_usage: TextureUsage) -> vk::ImageLayout {
  let storage_usages = TextureUsage::VERTEX_SHADER_STORAGE_READ
  | TextureUsage::VERTEX_SHADER_STORAGE_WRITE
  | TextureUsage::COMPUTE_SHADER_STORAGE_READ
  | TextureUsage::COMPUTE_SHADER_STORAGE_WRITE
  | TextureUsage::FRAGMENT_SHADER_STORAGE_READ
  | TextureUsage::FRAGMENT_SHADER_STORAGE_WRITE;
  if texture_usage.intersects(storage_usages) && (texture_usage & !storage_usages).is_empty() {
    return vk::ImageLayout::GENERAL;
  }

  let sampling_usages = TextureUsage::VERTEX_SHADER_SAMPLED
  | TextureUsage::COMPUTE_SHADER_SAMPLED
  | TextureUsage::FRAGMENT_SHADER_SAMPLED
  | TextureUsage::FRAGMENT_SHADER_LOCAL;
  if texture_usage.intersects(sampling_usages) && (texture_usage & !sampling_usages).is_empty() {
    return vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
  }

  let transfer_src_usages = TextureUsage::BLIT_SRC
  | TextureUsage::COPY_SRC
  | TextureUsage::RESOLVE_SRC;
  if texture_usage.intersects(transfer_src_usages) && (texture_usage & !transfer_src_usages).is_empty() {
    return vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
  }

  let transfer_dst_usages = TextureUsage::BLIT_DST
  | TextureUsage::COPY_DST
  | TextureUsage::RESOLVE_DST;
  if texture_usage.intersects(transfer_dst_usages) && (texture_usage & !transfer_dst_usages).is_empty() {
    return vk::ImageLayout::TRANSFER_DST_OPTIMAL;
  }

  if texture_usage == TextureUsage::DEPTH_READ {
    return vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
  }

  let ds_usages = TextureUsage::DEPTH_WRITE | TextureUsage::DEPTH_READ;
  if texture_usage.intersects(ds_usages) && (texture_usage &!ds_usages).is_empty() {
    return vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
  }

  if texture_usage == TextureUsage::UNINITIALIZED {
    return vk::ImageLayout::UNDEFINED;
  }

  if texture_usage == TextureUsage::RENDER_TARGET {
    return vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
  }

  if texture_usage == TextureUsage::PRESENT {
    return vk::ImageLayout::PRESENT_SRC_KHR;
  }
  
  panic!("Unsupported texture usage combination");
}

fn buffer_usage_to_access(buffer_usage: BufferUsage) -> vk::AccessFlags {
  let mut flags = vk::AccessFlags::empty();
  if buffer_usage.contains(BufferUsage::COPY_DST) {
    flags |= vk::AccessFlags::TRANSFER_WRITE;
  }
  if buffer_usage.contains(BufferUsage::COPY_SRC) {
    flags |= vk::AccessFlags::TRANSFER_READ;
  }
  if buffer_usage.contains(BufferUsage::VERTEX) {
    flags |= vk::AccessFlags::VERTEX_ATTRIBUTE_READ;
  }
  if buffer_usage.contains(BufferUsage::INDEX) {
    flags |= vk::AccessFlags::INDEX_READ;
  }
  if buffer_usage.contains(BufferUsage::INDIRECT) {
    flags |= vk::AccessFlags::INDIRECT_COMMAND_READ;
  }
  if buffer_usage.contains(BufferUsage::VERTEX_SHADER_STORAGE_WRITE)
    || buffer_usage.contains(BufferUsage::FRAGMENT_SHADER_STORAGE_WRITE)
    || buffer_usage.contains(BufferUsage::COMPUTE_SHADER_STORAGE_WRITE) {
    flags |= vk::AccessFlags::SHADER_WRITE;
  }
  if buffer_usage.contains(BufferUsage::VERTEX_SHADER_STORAGE_READ)
    || buffer_usage.contains(BufferUsage::FRAGMENT_SHADER_STORAGE_READ)
    || buffer_usage.contains(BufferUsage::COMPUTE_SHADER_STORAGE_READ)
    || buffer_usage.contains(BufferUsage::VERTEX_SHADER_CONSTANT)
    || buffer_usage.contains(BufferUsage::FRAGMENT_SHADER_CONSTANT)
    || buffer_usage.contains(BufferUsage::COMPUTE_SHADER_CONSTANT) {
    flags |= vk::AccessFlags::SHADER_READ;
  }
  /*if buffer_usage.contains(BufferUsage::CPU_IN_FLIGHT_WRITE) {
    flags |= vk::AccessFlags::HOST_WRITE;
  }*/
  flags
}

fn buffer_usage_to_stage(buffer_usage: BufferUsage) -> vk::PipelineStageFlags {
  let mut flags = vk::PipelineStageFlags::empty();
  if buffer_usage.contains(BufferUsage::COPY_DST)
    || buffer_usage.contains(BufferUsage::COPY_SRC) {
    flags |= vk::PipelineStageFlags::TRANSFER;
  }
  if buffer_usage.contains(BufferUsage::VERTEX) {
    flags |= vk::PipelineStageFlags::VERTEX_INPUT;
  }
  if buffer_usage.contains(BufferUsage::INDEX) {
    flags |= vk::PipelineStageFlags::VERTEX_INPUT;
  }
  if buffer_usage.contains(BufferUsage::INDIRECT) {
    flags |= vk::PipelineStageFlags::DRAW_INDIRECT;
  }
  if buffer_usage.contains(BufferUsage::VERTEX_SHADER_STORAGE_WRITE)
    || buffer_usage.contains(BufferUsage::VERTEX_SHADER_STORAGE_READ)
    || buffer_usage.contains(BufferUsage::VERTEX_SHADER_CONSTANT) {
    flags |= vk::PipelineStageFlags::VERTEX_SHADER;
  }
  if buffer_usage.contains(BufferUsage::FRAGMENT_SHADER_STORAGE_WRITE)
    || buffer_usage.contains(BufferUsage::FRAGMENT_SHADER_STORAGE_READ)
    || buffer_usage.contains(BufferUsage::FRAGMENT_SHADER_CONSTANT) {
    flags |= vk::PipelineStageFlags::FRAGMENT_SHADER;
  }
  if buffer_usage.contains(BufferUsage::COMPUTE_SHADER_STORAGE_WRITE)
    || buffer_usage.contains(BufferUsage::COMPUTE_SHADER_STORAGE_READ)
    || buffer_usage.contains(BufferUsage::COMPUTE_SHADER_CONSTANT) {
    flags |= vk::PipelineStageFlags::COMPUTE_SHADER;
  }
  flags
}

const WRITE_ACCESS_MASK: vk::AccessFlags = vk::AccessFlags::from_raw(vk::AccessFlags::HOST_WRITE.as_raw() | vk::AccessFlags::MEMORY_WRITE.as_raw() | vk::AccessFlags::SHADER_WRITE.as_raw() | vk::AccessFlags::TRANSFER_WRITE.as_raw() | vk::AccessFlags::COLOR_ATTACHMENT_WRITE.as_raw() | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw());
