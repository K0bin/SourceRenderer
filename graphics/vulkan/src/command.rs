use std::{cmp::min, sync::Arc};
use std::hash::Hash;
use std::marker::PhantomData;
use std::ffi::{CString};

use ash::vk;

use crossbeam_channel::{Receiver, Sender, unbounded};

use smallvec::SmallVec;
use sourcerenderer_core::graphics::{Barrier, BindingFrequency, Buffer, BufferInfo, BufferUsage, LoadOp, MemoryUsage, PipelineBinding, RenderPassBeginInfo, ShaderType, StoreOp, Texture, BarrierSync, BarrierAccess, TextureLayout, IndexFormat, BottomLevelAccelerationStructureInfo, AccelerationStructureInstance, WHOLE_BUFFER, TextureStorageView};
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::RenderpassRecordingMode;
use sourcerenderer_core::graphics::Viewport;
use sourcerenderer_core::graphics::Scissor;
use sourcerenderer_core::graphics::Resettable;

use crate::bindless::BINDLESS_TEXTURE_SET_INDEX;
use crate::pipeline::{shader_type_to_vk, VkPipelineType};
use crate::query::{VkQueryAllocator, VkQueryRange};
use crate::renderpass::{VkRenderPassInfo, VkAttachmentInfo, VkSubpassInfo};
use crate::rt::VkAccelerationStructure;
use crate::{raw::RawVkDevice, texture::VkSampler};
use crate::VkRenderPass;
use crate::VkFrameBuffer;
use crate::VkPipeline;
use crate::VkBackend;
use crate::raw::*;
use crate::VkShared;
use crate::buffer::{VkBufferSlice, BufferAllocator};
use crate::VkTexture;
use crate::descriptor::{VkBindingManager, VkBoundResourceRef};
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
  buffer_allocator: Arc<BufferAllocator>,
  query_allocator: Arc<VkQueryAllocator>,
}

impl VkCommandPool {
  pub fn new(
    device: &Arc<RawVkDevice>,
    queue_family_index: u32,
    shared: &Arc<VkShared>,
    buffer_allocator: &Arc<BufferAllocator>,
    query_allocator: &Arc<VkQueryAllocator>
  ) -> Self {
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
      buffer_allocator: buffer_allocator.clone(),
      query_allocator: query_allocator.clone()
    }
  }

  pub fn get_command_buffer(&mut self, frame: u64) -> VkCommandBufferRecorder {
    let mut buffer = self.primary_buffers.pop().unwrap_or_else(|| Box::new(VkCommandBuffer::new(&self.raw.device, &self.raw, CommandBufferType::Primary, self.queue_family_index, &self.shared, &self.buffer_allocator, &self.query_allocator)));
    buffer.begin(frame, None);
    VkCommandBufferRecorder::new(buffer, self.sender.clone())
  }

  pub fn get_inner_command_buffer(&mut self, frame: u64, inner_info: Option<&VkInnerCommandBufferInfo>) -> VkCommandBufferRecorder {
    let mut buffer = self.secondary_buffers.pop().unwrap_or_else(|| Box::new(VkCommandBuffer::new(&self.raw.device, &self.raw, CommandBufferType::Secondary, self.queue_family_index, &self.shared, &self.buffer_allocator, &self.query_allocator)));
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
      let buffers = if cmd_buf.command_buffer_type == CommandBufferType::Primary {
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
  pending_memory_barrier: vk::MemoryBarrier2,
  pending_image_barriers: Vec<vk::ImageMemoryBarrier2>,
  pending_buffer_barriers: Vec<vk::BufferMemoryBarrier2>,
  frame: u64,
  inheritance: Option<VkInnerCommandBufferInfo>,
  query_allocator: Arc<VkQueryAllocator>
}

impl VkCommandBuffer {
  pub(crate) fn new(device: &Arc<RawVkDevice>, pool: &Arc<RawVkCommandPool>, command_buffer_type: CommandBufferType, queue_family_index: u32, shared: &Arc<VkShared>, buffer_allocator: &Arc<BufferAllocator>, query_allocator: &Arc<VkQueryAllocator>) -> Self {
    let buffers_create_info = vk::CommandBufferAllocateInfo {
      command_pool: ***pool,
      level: if command_buffer_type == CommandBufferType::Primary { vk::CommandBufferLevel::PRIMARY } else { vk::CommandBufferLevel::SECONDARY }, // TODO: support secondary command buffers / bundles
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
      pending_memory_barrier: vk::MemoryBarrier2::default(),
      frame: 0,
      inheritance: None,
      query_allocator: query_allocator.clone(),
    }
  }

  pub fn handle(&self) -> &vk::CommandBuffer {
    &self.buffer
  }

  pub fn cmd_buffer_type(&self) -> CommandBufferType {
    self.command_buffer_type
  }

  pub(crate) fn reset(&mut self) {
    self.state = VkCommandBufferState::Ready;
    self.trackers.reset();
    self.descriptor_manager.reset(self.frame);
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
          render_pass: *inner_info.render_pass.handle(),
          subpass: inner_info.sub_pass,
          framebuffer: *inner_info.frame_buffer.handle(),
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
    if self.render_pass.is_some() {
      self.end_render_pass();
    }

    self.flush_barriers();
    self.state = VkCommandBufferState::Finished;
    unsafe {
      self.device.end_command_buffer(self.buffer).unwrap();
    }
  }

  pub(crate) fn set_pipeline(&mut self, pipeline: PipelineBinding<VkBackend>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);

    match &pipeline {
      PipelineBinding::Graphics(graphics_pipeline) => {
        let vk_pipeline = graphics_pipeline.handle();
        unsafe {
          self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::GRAPHICS, *vk_pipeline);
        }

        self.trackers.track_pipeline(*graphics_pipeline);
        if graphics_pipeline.uses_bindless_texture_set() && !self.device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
          panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
        }
        self.pipeline = Some((*graphics_pipeline).clone())
      }
      PipelineBinding::Compute(compute_pipeline) => {
        let vk_pipeline = compute_pipeline.handle();
        unsafe {
          self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::COMPUTE, *vk_pipeline);
        }
        self.trackers.track_pipeline(*compute_pipeline);
        if compute_pipeline.uses_bindless_texture_set() && !self.device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
          panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
        }
        self.pipeline = Some((*compute_pipeline).clone())
      },
      PipelineBinding::RayTracing(rt_pipeline) => {
        let vk_pipeline = rt_pipeline.handle();
        unsafe {
          self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::RAY_TRACING_KHR, *vk_pipeline);
        }
        self.trackers.track_pipeline(*rt_pipeline);
        if rt_pipeline.uses_bindless_texture_set() && !self.device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
          panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
        }
        self.pipeline = Some((*rt_pipeline).clone())
      },
    };
    self.descriptor_manager.mark_all_dirty();
  }

  pub(crate) fn end_render_pass(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary);
    unsafe {
      self.device.cmd_end_render_pass(self.buffer);
    }
    self.render_pass = None;
  }

  pub(crate) fn advance_subpass(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary);
    unsafe {
      self.device.cmd_next_subpass(self.buffer, vk::SubpassContents::INLINE);
    }
    self.sub_pass += 1;
  }

  pub(crate) fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<VkBufferSlice>, offset: usize) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.trackers.track_buffer(vertex_buffer);
    unsafe {
      self.device.cmd_bind_vertex_buffers(self.buffer, 0, &[*vertex_buffer.buffer().handle()], &[(vertex_buffer.offset() + offset) as u64]);
    }
  }

  pub(crate) fn set_index_buffer(&mut self, index_buffer: &Arc<VkBufferSlice>, offset: usize, format: IndexFormat) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.trackers.track_buffer(index_buffer);
    unsafe {
      self.device.cmd_bind_index_buffer(self.buffer, *index_buffer.buffer().handle(), (index_buffer.offset() + offset) as u64, index_format_to_vk(format));
    }
  }

  pub(crate) fn set_viewports(&mut self, viewports: &[ Viewport ]) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    unsafe {
      for (i, viewport) in viewports.iter().enumerate() {
        self.device.cmd_set_viewport(self.buffer, i as u32, &[vk::Viewport {
          x: viewport.position.x,
          y: viewport.extent.y - viewport.position.y,
          width: viewport.extent.x,
          height: -viewport.extent.y,
          min_depth: viewport.min_depth,
          max_depth: viewport.max_depth
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

  fn has_pending_barrier(&self) -> bool {
    !self.pending_image_barriers.is_empty() || !self.pending_buffer_barriers.is_empty() || !self.pending_memory_barrier.src_stage_mask.is_empty() || !self.pending_memory_barrier.dst_stage_mask.is_empty()
  }

  pub(crate) fn draw(&mut self, vertices: u32, offset: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
    debug_assert!(!self.has_pending_barrier());
    debug_assert!(self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary);
    unsafe {
      self.device.cmd_draw(self.buffer, vertices, 1, offset, 0);
    }
  }

  pub(crate) fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
    debug_assert!(!self.has_pending_barrier());
    debug_assert!(self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary);
    unsafe {
      self.device.cmd_draw_indexed(self.buffer, indices, instances, first_index, vertex_offset, first_instance);
    }
  }

  pub(crate) fn bind_sampling_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResourceRef::SampledTexture(texture));
    self.trackers.track_texture_view(texture);
  }

  pub(crate) fn bind_sampling_view_and_sampler(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>, sampler: &Arc<VkSampler>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResourceRef::SampledTextureAndSampler(texture, sampler));
    self.trackers.track_texture_view(texture);
    self.trackers.track_sampler(sampler);
  }

  pub(crate) fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>, offset: usize, length: usize) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert_ne!(length, 0);
    self.descriptor_manager.bind(frequency, binding, VkBoundResourceRef::UniformBuffer { buffer, offset, length: if length == WHOLE_BUFFER { buffer.length() } else { length } });
    self.trackers.track_buffer(buffer);
  }

  pub(crate) fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>, offset: usize, length: usize) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert_ne!(length, 0);
    self.descriptor_manager.bind(frequency, binding, VkBoundResourceRef::StorageBuffer { buffer, offset, length: if length == WHOLE_BUFFER { buffer.length() } else { length } });
    self.trackers.track_buffer(buffer);
  }

  pub(crate) fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResourceRef::StorageTexture(texture));
    self.trackers.track_texture_view(texture);
  }

  pub(crate) fn bind_sampler(&mut self, frequency: BindingFrequency, binding: u32, sampler: &Arc<VkSampler>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResourceRef::Sampler(sampler));
    self.trackers.track_sampler(sampler);
  }

  pub(crate) fn finish_binding(&mut self) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());

    self.flush_barriers();

    let mut offsets = SmallVec::<[u32; 16]>::new();
    let mut descriptor_sets = SmallVec::<[vk::DescriptorSet; (BINDLESS_TEXTURE_SET_INDEX + 1) as usize]>::new();
    let mut base_index = 0;

    let pipeline = self.pipeline.as_ref().expect("No pipeline bound");
    let pipeline_layout = pipeline.layout();

    let finished_sets = self.descriptor_manager.finish(self.frame, pipeline_layout);
    for (index, set_option) in finished_sets.iter().enumerate() {
      match set_option {
        None => {
          if !descriptor_sets.is_empty() {
            unsafe {
              self.device.cmd_bind_descriptor_sets(
                self.buffer,
                match pipeline.pipeline_type() {
                  VkPipelineType::Graphics => vk::PipelineBindPoint::GRAPHICS,
                  VkPipelineType::Compute => vk::PipelineBindPoint::COMPUTE,
                  VkPipelineType::RayTracing => vk::PipelineBindPoint::RAY_TRACING_KHR,
                },
                *pipeline_layout.handle(),
                base_index,
                &descriptor_sets,
                &offsets
              );
              offsets.clear();
              descriptor_sets.clear();
            }
          }
          base_index = index as u32 + 1;
        },
        Some(set_binding) => {
          descriptor_sets.push(*set_binding.set.handle());
          for i in 0..set_binding.dynamic_offset_count as usize {
            offsets.push(set_binding.dynamic_offsets[i] as u32);
          }
        }
      }
    }
    if !descriptor_sets.is_empty() && base_index + descriptor_sets.len() as u32 != 4 {
      unsafe {
        self.device.cmd_bind_descriptor_sets(
          self.buffer,
          match pipeline.pipeline_type() {
              VkPipelineType::Graphics => vk::PipelineBindPoint::GRAPHICS,
              VkPipelineType::Compute => vk::PipelineBindPoint::COMPUTE,
              VkPipelineType::RayTracing => vk::PipelineBindPoint::RAY_TRACING_KHR,
          },
          *pipeline_layout.handle(),
          base_index,
          &descriptor_sets,
          &offsets
        );
      }
      offsets.clear();
      descriptor_sets.clear();
      base_index = BINDLESS_TEXTURE_SET_INDEX;
    }

    if pipeline.uses_bindless_texture_set() {
      let bindless_texture_descriptor_set = self.shared.bindless_texture_descriptor_set().unwrap();
      descriptor_sets.push(bindless_texture_descriptor_set.descriptor_set_handle());
    }

    if !descriptor_sets.is_empty() {
      unsafe {
        self.device.cmd_bind_descriptor_sets(
          self.buffer,
          match pipeline.pipeline_type() {
              VkPipelineType::Graphics => vk::PipelineBindPoint::GRAPHICS,
              VkPipelineType::Compute => vk::PipelineBindPoint::COMPUTE,
              VkPipelineType::RayTracing => vk::PipelineBindPoint::RAY_TRACING_KHR,
          },
          *pipeline_layout.handle(),
          base_index,
          &descriptor_sets,
          &offsets
        );
      }
    }
  }

  pub(crate) fn upload_dynamic_data<T>(&self, data: &[T], usage: BufferUsage) -> Arc<VkBufferSlice>
    where T: 'static + Send + Sync + Sized + Clone {
    let slice = self.buffer_allocator.get_slice(&BufferInfo {
      size: std::mem::size_of_val(data),
      usage
    }, MemoryUsage::UncachedRAM,  None);
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
    let pipeline_layout = pipeline.layout();
    let range = pipeline_layout.push_constant_range(visible_for_shader_type).expect("No push constants set up for shader");
    let size = std::mem::size_of_val(data);
    unsafe {
      self.device.cmd_push_constants(
        self.buffer,
        *pipeline_layout.handle(),
        shader_type_to_vk(visible_for_shader_type),
        range.offset,
        std::slice::from_raw_parts(data.as_ptr() as *const u8, min(size, range.size as usize))
      );
    }
  }

  pub(crate) fn allocate_scratch_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage) -> Arc<VkBufferSlice> {
    self.buffer_allocator.get_slice(&BufferInfo {
      size: info.size as usize,
      usage: info.usage
    }, memory_usage,  None)
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
      assert_eq!(submission.command_buffer_type(), CommandBufferType::Secondary);
    }
    let submission_handles: SmallVec<[vk::CommandBuffer; 16]> = submissions
      .iter()
      .map(|s| *s.handle())
      .collect();
    unsafe {
      self.device.cmd_execute_commands(self.buffer, &submission_handles);
    }
    for mut submission in submissions.drain(..) {
      submission.mark_submitted();
      self.trackers.track_inner_command_buffer(submission);
    }
  }

  pub(crate) fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_none());
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Compute);
    debug_assert!(!self.has_pending_barrier());
    unsafe {
      self.device.cmd_dispatch(self.buffer, group_count_x, group_count_y, group_count_z);
    }
  }


  pub(crate) fn blit(&mut self, src_texture: &Arc<VkTexture>, src_array_layer: u32, src_mip_level: u32, dst_texture: &Arc<VkTexture>, dst_array_layer: u32, dst_mip_level: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_none());
    debug_assert!(!self.has_pending_barrier());
    let src_info = src_texture.info();
    let dst_info = dst_texture.info();
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
      self.device.cmd_blit_image(self.buffer, *src_texture.handle(), vk::ImageLayout::TRANSFER_SRC_OPTIMAL, *dst_texture.handle(), vk::ImageLayout::TRANSFER_DST_OPTIMAL,
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

    self.trackers.track_texture(src_texture);
    self.trackers.track_texture(dst_texture);
  }

  pub(crate) fn barrier(
    &mut self,
    barriers: &[Barrier<VkBackend>]
  ) {
    for barrier in barriers {
      match barrier {
        Barrier::TextureBarrier { old_sync, new_sync, old_layout, new_layout, old_access, new_access, texture, range } => {
          let info = texture.info();
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

          let dst_stages = barrier_sync_to_stage(*new_sync);
          let src_stages = barrier_sync_to_stage(*old_sync);
          self.pending_image_barriers.push(vk::ImageMemoryBarrier2 {
            src_stage_mask: src_stages,
            dst_stage_mask: dst_stages,
            src_access_mask: barrier_access_to_access(*old_access),
            dst_access_mask: barrier_access_to_access(*new_access),
            old_layout: texture_layout_to_image_layout(*old_layout),
            new_layout: texture_layout_to_image_layout(*new_layout),
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            image: *texture.handle(),
            subresource_range: vk::ImageSubresourceRange {
              aspect_mask,
              base_array_layer: range.base_array_layer,
              base_mip_level: range.base_mip_level,
              level_count: range.mip_level_length,
              layer_count: range.array_layer_length
            },
            ..Default::default()
          });
          self.trackers.track_texture(texture);
        },
        Barrier::BufferBarrier { old_sync, new_sync, old_access, new_access, buffer } => {
          let dst_stages = barrier_sync_to_stage(*new_sync);
          let src_stages = barrier_sync_to_stage(*old_sync);
          self.pending_buffer_barriers.push(vk::BufferMemoryBarrier2 {
            src_stage_mask: src_stages,
            dst_stage_mask: dst_stages,
            src_access_mask: barrier_access_to_access(*old_access),
            dst_access_mask: barrier_access_to_access(*new_access),
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: *buffer.buffer().handle(),
            offset: buffer.offset() as u64,
            size: buffer.length() as u64,
            ..Default::default()
          });
          self.trackers.track_buffer(buffer);
        },
        Barrier::GlobalBarrier { old_sync, new_sync, old_access, new_access } => {
          let dst_stages = barrier_sync_to_stage(*new_sync);
          let src_stages = barrier_sync_to_stage(*old_sync);
          self.pending_memory_barrier.dst_stage_mask |= dst_stages;
          self.pending_memory_barrier.src_stage_mask |= src_stages;
          self.pending_memory_barrier.src_access_mask |= barrier_access_to_access(*old_access);
          self.pending_memory_barrier.dst_access_mask |= barrier_access_to_access(*new_access);
        },
      }
    }
  }

  pub(crate) fn flush_barriers(&mut self) {
    const FULL_BARRIER: bool = false; // IN CASE OF EMERGENCY, SET TO TRUE
    if FULL_BARRIER {
      let full_memory_barrier = vk::MemoryBarrier2 {
        src_stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
        dst_stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
        src_access_mask: vk::AccessFlags2::MEMORY_WRITE,
        dst_access_mask: vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
        ..Default::default()
      };
      let dependency_info = vk::DependencyInfo {
        image_memory_barrier_count: 0,
        p_image_memory_barriers: std::ptr::null(),
        buffer_memory_barrier_count: 0,
        p_buffer_memory_barriers: std::ptr::null(),
        memory_barrier_count: 1,
        p_memory_barriers: &full_memory_barrier as *const vk::MemoryBarrier2,
        ..Default::default()
      };
      unsafe {
        self.device.synchronization2.cmd_pipeline_barrier2(self.buffer, &dependency_info);
      }
      self.pending_memory_barrier.src_stage_mask = vk::PipelineStageFlags2::empty();
      self.pending_memory_barrier.dst_stage_mask = vk::PipelineStageFlags2::empty();
      self.pending_memory_barrier.src_access_mask = vk::AccessFlags2::empty();
      self.pending_memory_barrier.dst_access_mask = vk::AccessFlags2::empty();
      self.pending_image_barriers.clear();
      self.pending_buffer_barriers.clear();
      return;
    }

    if !self.has_pending_barrier() {
      return;
    }

    let dependency_info = vk::DependencyInfo {
      image_memory_barrier_count: self.pending_image_barriers.len() as u32,
      p_image_memory_barriers: self.pending_image_barriers.as_ptr(),
      buffer_memory_barrier_count: self.pending_buffer_barriers.len() as u32,
      p_buffer_memory_barriers: self.pending_buffer_barriers.as_ptr(),
      memory_barrier_count: if self.pending_memory_barrier.src_stage_mask.is_empty() && self.pending_memory_barrier.dst_stage_mask.is_empty() { 1 } else { 0 },
      p_memory_barriers: &self.pending_memory_barrier as *const vk::MemoryBarrier2,
      ..Default::default()
    };

    unsafe {
      self.device.synchronization2.cmd_pipeline_barrier2(self.buffer, &dependency_info);
    }
    self.pending_memory_barrier.src_stage_mask = vk::PipelineStageFlags2::empty();
    self.pending_memory_barrier.dst_stage_mask = vk::PipelineStageFlags2::empty();
    self.pending_memory_barrier.src_access_mask = vk::AccessFlags2::empty();
    self.pending_memory_barrier.dst_access_mask = vk::AccessFlags2::empty();
    self.pending_image_barriers.clear();
    self.pending_buffer_barriers.clear();
  }

  pub(crate) fn begin_render_pass(&mut self, renderpass_begin_info: &RenderPassBeginInfo<VkBackend>, recording_mode: RenderpassRecordingMode) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_none());

    self.flush_barriers();

    let mut attachment_infos = SmallVec::<[VkAttachmentInfo; 16]>::with_capacity(renderpass_begin_info.attachments.len());
    let mut width = 0u32;
    let mut height = 0u32;
    let mut attachment_views = SmallVec::<[&Arc<VkTextureView>; 8]>::with_capacity(renderpass_begin_info.attachments.len());
    let mut clear_values = SmallVec::<[vk::ClearValue; 8]>::with_capacity(renderpass_begin_info.attachments.len());

    for attachment in renderpass_begin_info.attachments {
      let view = match &attachment.view {
        sourcerenderer_core::graphics::RenderPassAttachmentView::RenderTarget(view) => *view,
        sourcerenderer_core::graphics::RenderPassAttachmentView::DepthStencil(view) => *view
      };

      let info = view.texture().info();
      attachment_infos.push(VkAttachmentInfo {
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

    let renderpass_info = VkRenderPassInfo {
      attachments: attachment_infos,
      subpasses: renderpass_begin_info.subpasses.iter().map(|sp| VkSubpassInfo {
        input_attachments: sp.input_attachments.iter().cloned().collect(),
        output_color_attachments: sp.output_color_attachments.iter().cloned().collect(),
        depth_stencil_attachment: sp.depth_stencil_attachment.clone(),
      }).collect(),
    };

    let renderpass = self.shared.get_render_pass(renderpass_info);
    let framebuffer = self.shared.get_framebuffer(&renderpass, &attachment_views);

    // TODO: begin info fields
    unsafe {
      let begin_info = vk::RenderPassBeginInfo {
        framebuffer: *framebuffer.handle(),
        render_pass: *renderpass.handle(),
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

  pub fn create_query_range(&mut self, count: u32) -> Arc<VkQueryRange> {
    let query_range = Arc::new(self.query_allocator.get(vk::QueryType::OCCLUSION, count));
    if !query_range.pool.is_reset() {
      unsafe {
        self.device.cmd_reset_query_pool(self.buffer, *query_range.pool.handle(), 0, query_range.pool.query_count());
      }
      query_range.pool.mark_reset();
    }
    query_range
  }

  pub fn begin_query(&mut self, query_range: &Arc<VkQueryRange>, index: u32) {
    unsafe {
      self.device.cmd_begin_query(self.buffer, *query_range.pool.handle(), query_range.index + index, vk::QueryControlFlags::empty());
    }
  }

  pub fn end_query(&mut self, query_range: &Arc<VkQueryRange>, index: u32) {
    unsafe {
      self.device.cmd_end_query(self.buffer, *query_range.pool.handle(), query_range.index + index);
    }
  }

  pub fn copy_query_results_to_buffer(&mut self, query_range: &Arc<VkQueryRange>, buffer: &Arc<VkBufferSlice>, start_index: u32, count: u32) {
    let vk_start = query_range.index + start_index;
    let vk_count = query_range.count.min(count);
    unsafe {
      self.device.cmd_copy_query_pool_results(self.buffer, *query_range.pool.handle(), vk_start, vk_count,
        *buffer.buffer().handle(), buffer.offset() as u64, std::mem::size_of::<u32>() as u64, vk::QueryResultFlags::WAIT)
    }
    self.trackers.track_buffer(buffer);
  }

  pub fn create_bottom_level_acceleration_structure(&mut self, info: &BottomLevelAccelerationStructureInfo<VkBackend>, size: usize, target_buffer: &Arc<VkBufferSlice>, scratch_buffer: &Arc<VkBufferSlice>) -> Arc<VkAccelerationStructure> {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_none());
    self.trackers.track_buffer(scratch_buffer);
    self.trackers.track_buffer(target_buffer);
    self.trackers.track_buffer(info.vertex_buffer);
    self.trackers.track_buffer(info.index_buffer);
    let acceleration_structure = Arc::new(VkAccelerationStructure::new_bottom_level(&self.device, info, size, target_buffer, scratch_buffer, self.handle()));
    self.trackers.track_acceleration_structure(&acceleration_structure);
    acceleration_structure
  }

  fn upload_top_level_instances(&mut self, instances: &[AccelerationStructureInstance<VkBackend>]) -> Arc<VkBufferSlice> {
    for instance in instances {
      self.trackers.track_acceleration_structure(instance.acceleration_structure);
    }
    VkAccelerationStructure::upload_top_level_instances(self, instances)
  }

  fn create_top_level_acceleration_structure(&mut self, info: &sourcerenderer_core::graphics::TopLevelAccelerationStructureInfo<VkBackend>, size: usize, target_buffer: &Arc<VkBufferSlice>, scratch_buffer: &Arc<VkBufferSlice>) -> Arc<VkAccelerationStructure> {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_none());
    self.trackers.track_buffer(scratch_buffer);
    self.trackers.track_buffer(target_buffer);
    self.trackers.track_buffer(info.instances_buffer);
    for instance in info.instances {
      self.trackers.track_acceleration_structure(instance.acceleration_structure);
    }
    let acceleration_structure = Arc::new(VkAccelerationStructure::new_top_level(&self.device, info, size, target_buffer, scratch_buffer, self.handle()));
    self.trackers.track_acceleration_structure(&acceleration_structure);
    acceleration_structure
  }

  fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.render_pass.is_none());
    debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::RayTracing);

    let rt = self.device.rt.as_ref().unwrap();
    let rt_pipeline = self.pipeline.as_ref().unwrap();
    unsafe {
      rt.rt_pipelines.cmd_trace_rays(
        self.buffer,
        rt_pipeline.raygen_sbt_region(),
        rt_pipeline.miss_sbt_region(),
        rt_pipeline.closest_hit_sbt_region(),
        &vk::StridedDeviceAddressRegionKHR::default(),
        width,
        height,
        depth
      );
    }
  }

  fn bind_acceleration_structure(&mut self, frequency: BindingFrequency, binding: u32, acceleration_structure: &Arc<VkAccelerationStructure>) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    self.descriptor_manager.bind(frequency, binding, VkBoundResourceRef::AccelerationStructure(acceleration_structure));
    self.trackers.track_acceleration_structure(acceleration_structure);
  }

  fn track_texture_view(&mut self, texture_view: &Arc<VkTextureView>) {
    self.trackers.track_texture_view(texture_view);
  }

  fn draw_indexed_indirect(&mut self, draw_buffer: &Arc<VkBufferSlice>, draw_buffer_offset: u32, count_buffer: &Arc<VkBufferSlice>, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
    debug_assert!(!self.has_pending_barrier());
    debug_assert!(self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary);
    self.trackers.track_buffer(draw_buffer);
    self.trackers.track_buffer(count_buffer);
    unsafe {
      self.device.indirect_count.as_ref().unwrap().cmd_draw_indexed_indirect_count(
        self.buffer,
        *draw_buffer.buffer().handle(),
        draw_buffer.offset() as u64 + draw_buffer_offset as u64,
        *count_buffer.buffer().handle(),
        count_buffer.offset() as u64 + count_buffer_offset as u64,
        max_draw_count,
        stride
      );
    }
  }

  fn draw_indirect(&mut self, draw_buffer: &Arc<VkBufferSlice>, draw_buffer_offset: u32, count_buffer: &Arc<VkBufferSlice>, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(self.pipeline.is_some());
    debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
    debug_assert!(!self.has_pending_barrier());
    debug_assert!(self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary);
    self.trackers.track_buffer(draw_buffer);
    self.trackers.track_buffer(count_buffer);
    unsafe {
      self.device.indirect_count.as_ref().unwrap().cmd_draw_indirect_count(
        self.buffer,
        *draw_buffer.buffer().handle(),
        draw_buffer.offset() as u64 + draw_buffer_offset as u64,
        *count_buffer.buffer().handle(),
        count_buffer.offset() as u64 + count_buffer_offset as u64,
        max_draw_count,
        stride
      );
    }
  }

  fn clear_storage_view(&mut self, view: &Arc<VkTextureView>, values: [u32; 4]) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(!self.has_pending_barrier());
    debug_assert!(self.render_pass.is_none());

    let format = view.texture().info().format;
    let mut aspect_mask = vk::ImageAspectFlags::empty();
    if format.is_depth() {
      aspect_mask |= vk::ImageAspectFlags::DEPTH;
    }
    if format.is_stencil() {
      aspect_mask |= vk::ImageAspectFlags::STENCIL;
    }
    if aspect_mask.is_empty() {
      aspect_mask = vk::ImageAspectFlags::COLOR;
    }

    let range = vk::ImageSubresourceRange {
      aspect_mask: aspect_mask,
      base_mip_level: view.info().base_mip_level,
      level_count: view.info().mip_level_length,
      base_array_layer: view.info().base_array_layer,
      layer_count: view.info().array_layer_length,
    };

    unsafe {
      if aspect_mask.intersects(vk::ImageAspectFlags::DEPTH) || aspect_mask.intersects(vk::ImageAspectFlags::STENCIL) {
        self.device.cmd_clear_depth_stencil_image(
          self.buffer,
          *view.texture().handle(),
          vk::ImageLayout::GENERAL,
          &vk::ClearDepthStencilValue {
            depth: values[0] as f32,
            stencil: values[1],
          },
          &[range]
        );
      } else {
        self.device.cmd_clear_color_image(
          self.buffer,
          *view.texture().handle(),
          vk::ImageLayout::GENERAL,
          &vk::ClearColorValue {
            uint32: values
          },
          &[range]
        );
      }
    }
  }

  fn clear_storage_buffer(&mut self, buffer: &Arc<VkBufferSlice>, offset: usize, length_in_u32s: usize, value: u32) {
    debug_assert_eq!(self.state, VkCommandBufferState::Recording);
    debug_assert!(!self.has_pending_barrier());
    debug_assert!(self.render_pass.is_none());

    let actual_length = if length_in_u32s == WHOLE_BUFFER { buffer.length() - offset } else { length_in_u32s };
    #[repr(packed)]
    struct MetaClearShaderData {
      length: u32,
      value: u32,
    }
    let push_data = MetaClearShaderData {
      length: actual_length as u32,
      value: value,
    };

    let meta_pipeline = self.shared.get_clear_buffer_meta_pipeline().clone();
    let mut bindings = <[VkBoundResourceRef; 16]>::default();
    bindings[0] = VkBoundResourceRef::StorageBuffer { buffer, offset, length: actual_length };
    let binding_offsets = [ (buffer.offset() + offset) as u32 ];
    let is_dynamic_binding = meta_pipeline.layout().descriptor_set_layout(0).unwrap().is_dynamic_binding(0);
    let descriptor_set = self.descriptor_manager.get_or_create_set(self.frame, meta_pipeline.layout().descriptor_set_layout(0).as_ref().unwrap(), &bindings).unwrap();
    unsafe {
      self.device.cmd_bind_pipeline(self.buffer, vk::PipelineBindPoint::COMPUTE, *meta_pipeline.handle());

      self.device.cmd_push_constants(
        self.buffer,
        *meta_pipeline.layout().handle(),
        vk::ShaderStageFlags::COMPUTE,
        0,
        std::slice::from_raw_parts(std::mem::transmute(&push_data as *const MetaClearShaderData), std::mem::size_of::<MetaClearShaderData>())
      );
      self.device.cmd_bind_descriptor_sets(
        self.buffer,
        vk::PipelineBindPoint::COMPUTE,
        *meta_pipeline.layout().handle(),
        0, &[*descriptor_set.handle()],
        if is_dynamic_binding { &binding_offsets } else { &[] }
      );
      self.device.cmd_dispatch(self.buffer, (actual_length as u32 + 3) / 4, 1, 1);
    }
    self.descriptor_manager.mark_all_dirty();
  }
}

impl Drop for VkCommandBuffer {
  fn drop(&mut self) {
    if self.state == VkCommandBufferState::Submitted {
      self.device.wait_for_idle();
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
  pub fn end_render_pass(&mut self) {
    self.item.as_mut().unwrap().end_render_pass();
  }

  pub fn advance_subpass(&mut self) {
    self.item.as_mut().unwrap().advance_subpass();
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

  pub(crate) fn allocate_scratch_buffer(&mut self, info: &BufferInfo, memory_usage: MemoryUsage) -> Arc<VkBufferSlice> {
    self.item.as_mut().unwrap().allocate_scratch_buffer(info, memory_usage)
  }
}

impl CommandBuffer<VkBackend> for VkCommandBufferRecorder {
  #[inline(always)]
  fn set_pipeline(&mut self, pipeline: PipelineBinding<VkBackend>) {
    self.item.as_mut().unwrap().set_pipeline(pipeline);
  }

  #[inline(always)]
  fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<VkBufferSlice>, offset: usize) {
    self.item.as_mut().unwrap().set_vertex_buffer(vertex_buffer, offset)
  }

  #[inline(always)]
  fn set_index_buffer(&mut self, index_buffer: &Arc<VkBufferSlice>, offset: usize, format: IndexFormat) {
    self.item.as_mut().unwrap().set_index_buffer(index_buffer, offset, format)
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
  fn bind_sampling_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    self.item.as_mut().unwrap().bind_sampling_view(frequency, binding, texture);
  }

  #[inline(always)]
  fn bind_sampling_view_and_sampler(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>, sampler: &Arc<VkSampler>) {
    self.item.as_mut().unwrap().bind_sampling_view_and_sampler(frequency, binding, texture, sampler);
  }

  #[inline(always)]
  fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>, offset: usize, length: usize) {
    self.item.as_mut().unwrap().bind_uniform_buffer(frequency, binding, buffer, offset, length);
  }

  #[inline(always)]
  fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<VkBufferSlice>, offset: usize, length: usize) {
    self.item.as_mut().unwrap().bind_storage_buffer(frequency, binding, buffer, offset, length);
  }

  #[inline(always)]
  fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<VkTextureView>) {
    self.item.as_mut().unwrap().bind_storage_texture(frequency, binding, texture);
  }

  #[inline(always)]
  fn bind_sampler(&mut self, frequency: BindingFrequency, binding: u32, sampler: &Arc<VkSampler>) {
    self.item.as_mut().unwrap().bind_sampler(frequency, binding, sampler);
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
    self.item.as_mut().unwrap().barrier(barriers);
  }

  #[inline(always)]
  fn flush_barriers(&mut self) {
    self.item.as_mut().unwrap().flush_barriers();
  }

  #[inline(always)]
  fn begin_render_pass(&mut self, renderpass_info: &RenderPassBeginInfo<VkBackend>, recording_mode: RenderpassRecordingMode) {
    self.item.as_mut().unwrap().begin_render_pass(renderpass_info, recording_mode);
  }

  #[inline(always)]
  fn advance_subpass(&mut self) {
    self.item.as_mut().unwrap().advance_subpass();
  }

  #[inline(always)]
  fn end_render_pass(&mut self) {
    self.item.as_mut().unwrap().end_render_pass();
  }

  type CommandBufferInheritance = VkInnerCommandBufferInfo;

  #[inline(always)]
  fn inheritance(&self) -> &VkInnerCommandBufferInfo {
    self.item.as_ref().unwrap().inheritance()
  }

  #[inline(always)]
  fn execute_inner(&mut self, submission: Vec<VkCommandBufferSubmission>) {
    self.item.as_mut().unwrap().execute_inner(submission);
  }

  #[inline(always)]
  fn begin_query(&mut self, query_range: &Arc<VkQueryRange>, query_index: u32) {
    self.item.as_mut().unwrap().begin_query(query_range, query_index);
  }

  #[inline(always)]
  fn end_query(&mut self, query_range: &Arc<VkQueryRange>, query_index: u32) {
    self.item.as_mut().unwrap().end_query(query_range, query_index);
  }

  #[inline(always)]
  fn create_query_range(&mut self, count: u32) -> Arc<VkQueryRange> {
    self.item.as_mut().unwrap().create_query_range(count)
  }

  #[inline(always)]
  fn copy_query_results_to_buffer(&mut self, query_range: &Arc<VkQueryRange>, buffer: &Arc<VkBufferSlice>, start_index: u32, count: u32) {
    self.item.as_mut().unwrap().copy_query_results_to_buffer(query_range, buffer, start_index, count);
  }

  #[inline(always)]
  fn create_bottom_level_acceleration_structure(&mut self, info: &BottomLevelAccelerationStructureInfo<VkBackend>, size: usize, target_buffer: &Arc<VkBufferSlice>, scratch_buffer: &Arc<VkBufferSlice>) -> Arc<VkAccelerationStructure> {
    self.item.as_mut().unwrap().create_bottom_level_acceleration_structure(info, size, target_buffer, scratch_buffer)
  }

  #[inline(always)]
  fn upload_top_level_instances(&mut self, instances: &[AccelerationStructureInstance<VkBackend>]) -> Arc<VkBufferSlice> {
    self.item.as_mut().unwrap().upload_top_level_instances(instances)
  }

  #[inline(always)]
  fn create_top_level_acceleration_structure(&mut self, info: &sourcerenderer_core::graphics::TopLevelAccelerationStructureInfo<VkBackend>, size: usize, target_buffer: &Arc<VkBufferSlice>, scratch_buffer: &Arc<VkBufferSlice>) -> Arc<VkAccelerationStructure> {
    self.item.as_mut().unwrap().create_top_level_acceleration_structure(info, size, target_buffer, scratch_buffer)
  }

  #[inline(always)]
  fn create_temporary_buffer(&mut self, info: &BufferInfo, memory_usage: MemoryUsage) -> Arc<VkBufferSlice> {
    self.item.as_mut().unwrap().allocate_scratch_buffer(info, memory_usage)
  }

  #[inline(always)]
  fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
    self.item.as_mut().unwrap().trace_ray(width, height, depth);
  }

  #[inline(always)]
  fn bind_acceleration_structure(&mut self, frequency: BindingFrequency, binding: u32, acceleration_structure: &Arc<VkAccelerationStructure>) {
    self.item.as_mut().unwrap().bind_acceleration_structure(frequency, binding, acceleration_structure);
  }

  #[inline(always)]
  fn track_texture_view(&mut self, texture_view: &Arc<VkTextureView>) {
    self.item.as_mut().unwrap().track_texture_view(texture_view);
  }

  #[inline(always)]
  fn draw_indexed_indirect(&mut self, draw_buffer: &Arc<VkBufferSlice>, draw_buffer_offset: u32, count_buffer: &Arc<VkBufferSlice>, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
    self.item.as_mut().unwrap().draw_indexed_indirect(draw_buffer, draw_buffer_offset, count_buffer, count_buffer_offset, max_draw_count, stride);
  }

  #[inline(always)]
  fn draw_indirect(&mut self, draw_buffer: &Arc<VkBufferSlice>, draw_buffer_offset: u32, count_buffer: &Arc<VkBufferSlice>, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
    self.item.as_mut().unwrap().draw_indexed_indirect(draw_buffer, draw_buffer_offset, count_buffer, count_buffer_offset, max_draw_count, stride);
  }

  fn clear_storage_view(&mut self, view: &Arc<VkTextureView>, values: [u32; 4]) {
    self.item.as_mut().unwrap().clear_storage_view(view, values);
  }

  fn clear_storage_buffer(&mut self, buffer: &Arc<VkBufferSlice>, offset: usize, length_in_u32s: usize, value: u32) {
    self.item.as_mut().unwrap().clear_storage_buffer(buffer, offset, length_in_u32s, value);
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

  pub(crate) fn handle(&self) -> &vk::CommandBuffer {
    self.item.as_ref().unwrap().handle()
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
    let _ = self.sender.send(item);
  }
}

fn barrier_sync_to_stage(sync: BarrierSync) -> vk::PipelineStageFlags2 {
  let mut stages = vk::PipelineStageFlags2::NONE;
  if sync.contains(BarrierSync::COMPUTE_SHADER) {
    stages |= vk::PipelineStageFlags2::COMPUTE_SHADER;
  }
  if sync.contains(BarrierSync::COPY) {
    stages |= vk::PipelineStageFlags2::TRANSFER;
  }
  if sync.contains(BarrierSync::EARLY_DEPTH) {
    stages |= vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS;
  }
  if sync.contains(BarrierSync::FRAGMENT_SHADER) {
    stages |= vk::PipelineStageFlags2::FRAGMENT_SHADER;
  }
  if sync.contains(BarrierSync::INDIRECT) {
    stages |= vk::PipelineStageFlags2::DRAW_INDIRECT;
  }
  if sync.contains(BarrierSync::LATE_DEPTH) {
    stages |= vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
  }
  if sync.contains(BarrierSync::RENDER_TARGET) {
    stages |= vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
  }
  if sync.contains(BarrierSync::RESOLVE) {
    stages |= vk::PipelineStageFlags2::RESOLVE;
  }
  if sync.contains(BarrierSync::VERTEX_INPUT) {
    stages |= vk::PipelineStageFlags2::VERTEX_ATTRIBUTE_INPUT;
  }
  if sync.contains(BarrierSync::INDEX_INPUT) {
    stages |= vk::PipelineStageFlags2::INDEX_INPUT;
  }
  if sync.contains(BarrierSync::VERTEX_SHADER) {
    stages |= vk::PipelineStageFlags2::VERTEX_SHADER;
  }
  if sync.contains(BarrierSync::HOST) {
    stages |= vk::PipelineStageFlags2::HOST;
  }
  if sync.contains(BarrierSync::ACCELERATION_STRUCTURE_BUILD) {
    stages |= vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR;
  }
  if sync.contains(BarrierSync::RAY_TRACING) {
    stages |= vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR;
  }
  stages
}

fn barrier_access_to_access(access: BarrierAccess) -> vk::AccessFlags2 {
  let mut vk_access = vk::AccessFlags2::empty();
  if access.contains(BarrierAccess::INDEX_READ) {
    vk_access |= vk::AccessFlags2::INDEX_READ;
  }
  if access.contains(BarrierAccess::INDIRECT_READ) {
    vk_access |= vk::AccessFlags2::INDIRECT_COMMAND_READ;
  }
  if access.contains(BarrierAccess::VERTEX_INPUT_READ) {
    vk_access |= vk::AccessFlags2::VERTEX_ATTRIBUTE_READ;
  }
  if access.contains(BarrierAccess::CONSTANT_READ) {
    vk_access |= vk::AccessFlags2::UNIFORM_READ;
  }
  if access.intersects(BarrierAccess::SAMPLING_READ) {
    vk_access |= vk::AccessFlags2::SHADER_SAMPLED_READ;
  }
  if access.intersects(BarrierAccess::STORAGE_READ) {
    vk_access |= vk::AccessFlags2::SHADER_STORAGE_READ;
  }
  if access.contains(BarrierAccess::STORAGE_WRITE) {
    vk_access |= vk::AccessFlags2::SHADER_STORAGE_WRITE;
  }
  if access.contains(BarrierAccess::COPY_READ) {
    vk_access |= vk::AccessFlags2::TRANSFER_READ;
  }
  if access.contains(BarrierAccess::COPY_WRITE) {
    vk_access |= vk::AccessFlags2::TRANSFER_WRITE;
  }
  if access.contains(BarrierAccess::RESOLVE_READ) {
    vk_access |= vk::AccessFlags2::TRANSFER_READ;
    // TODO: sync2
  }
  if access.contains(BarrierAccess::RESOLVE_WRITE) {
    vk_access |= vk::AccessFlags2::TRANSFER_WRITE;
  }
  if access.contains(BarrierAccess::DEPTH_STENCIL_READ) {
    vk_access |= vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;
  }
  if access.contains(BarrierAccess::DEPTH_STENCIL_WRITE) {
    vk_access |= vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE;
  }
  if access.contains(BarrierAccess::RENDER_TARGET_READ) {
    vk_access |= vk::AccessFlags2::COLOR_ATTACHMENT_READ;
  }
  if access.contains(BarrierAccess::RENDER_TARGET_WRITE) {
    vk_access |= vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
  }
  if access.contains(BarrierAccess::SHADER_READ) {
    vk_access |= vk::AccessFlags2::SHADER_READ;
  }
  if access.contains(BarrierAccess::SHADER_WRITE) {
    vk_access |= vk::AccessFlags2::SHADER_WRITE;
  }
  if access.contains(BarrierAccess::MEMORY_READ) {
    vk_access |= vk::AccessFlags2::MEMORY_READ;
  }
  if access.contains(BarrierAccess::MEMORY_WRITE) {
    vk_access |= vk::AccessFlags2::MEMORY_WRITE;
  }
  if access.contains(BarrierAccess::HOST_READ) {
    vk_access |= vk::AccessFlags2::HOST_READ;
  }
  if access.contains(BarrierAccess::HOST_WRITE) {
    vk_access |= vk::AccessFlags2::HOST_WRITE;
  }
  if access.contains(BarrierAccess::ACCELERATION_STRUCTURE_READ) {
    vk_access |= vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR;
  }
  if access.contains(BarrierAccess::ACCELERATION_STRUCTURE_WRITE) {
    vk_access |= vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR;
  }
  vk_access
}

fn texture_layout_to_image_layout(layout: TextureLayout) -> vk::ImageLayout {
  match layout {
    TextureLayout::CopyDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    TextureLayout::CopySrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
    TextureLayout::DepthStencilRead => vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
    TextureLayout::DepthStencilReadWrite => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    TextureLayout::General => vk::ImageLayout::GENERAL,
    TextureLayout::Sampled => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    TextureLayout::Storage => vk::ImageLayout::GENERAL,
    TextureLayout::RenderTarget => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    TextureLayout::ResolveSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
    TextureLayout::ResolveDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    TextureLayout::Undefined => vk::ImageLayout::UNDEFINED,
    TextureLayout::Present => vk::ImageLayout::PRESENT_SRC_KHR,
  }
}

pub(crate) fn index_format_to_vk(format: IndexFormat) -> vk::IndexType {
  match format {
    IndexFormat::U16 => vk::IndexType::UINT16,
    IndexFormat::U32 => vk::IndexType::UINT32,
  }
}

const WRITE_ACCESS_MASK: vk::AccessFlags2 = vk::AccessFlags2::from_raw(vk::AccessFlags2::HOST_WRITE.as_raw() | vk::AccessFlags2::MEMORY_WRITE.as_raw() | vk::AccessFlags2::SHADER_WRITE.as_raw() | vk::AccessFlags2::TRANSFER_WRITE.as_raw() | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw() | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw());
