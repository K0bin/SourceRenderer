use std::{
    cmp::min,
    ffi::CString,
    hash::Hash,
    sync::Arc,
};

use ash::vk;
use crossbeam_utils::atomic::AtomicCell;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::*;

use super::*;

const BINDLESS_TEXTURE_SET_INDEX: u32 = 3;

#[allow(clippy::vec_box)]
pub struct VkCommandPool {
    raw: Arc<RawVkCommandPool>,
    shared: Arc<VkShared>,
    flags: CommandPoolFlags,
    queue_family_index: u32,
}

impl VkCommandPool {
    pub fn new(device: &Arc<RawVkDevice>, queue_family_index: u32, flags: CommandPoolFlags, shared: &Arc<VkShared>) -> Self {
        let mut vk_flags = vk::CommandPoolCreateFlags::empty();
        if flags.contains(CommandPoolFlags::INDIVIDUAL_RESET) {
            vk_flags |= vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER;
        }
        if flags.contains(CommandPoolFlags::TRANSIENT) {
            vk_flags |= vk::CommandPoolCreateFlags::TRANSIENT;
        }

        let create_info = vk::CommandPoolCreateInfo {
            queue_family_index,
            flags: vk_flags,
            ..Default::default()
        };

        Self {
            raw: Arc::new(RawVkCommandPool::new(device, &create_info).unwrap()),
            shared: shared.clone(),
            flags,
            queue_family_index,
        }
    }
}

impl CommandPool<VkBackend> for VkCommandPool {
    unsafe fn create_command_buffer(
        &mut self,
        inner_info: Option<&VkInnerCommandBufferInfo>,
        frame: u64,
    ) -> VkCommandBuffer {
        let mut buffer = VkCommandBuffer::new(
            &self.raw.device,
            &self.raw,
            if inner_info.is_none() {
                CommandBufferType::Primary
            } else {
                CommandBufferType::Primary
            },
            self.queue_family_index,
            self.flags.contains(CommandPoolFlags::INDIVIDUAL_RESET),
            &self.shared
        );
        buffer.begin(None, frame);
        buffer
    }

    unsafe fn reset(&mut self) {
        self.raw
            .device
            .reset_command_pool(**self.raw, vk::CommandPoolResetFlags::empty())
            .unwrap();
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
#[repr(u32)]
pub enum VkCommandBufferState {
    Ready,
    Recording,
    Finished,
    Submitted,
}

pub struct VkInnerCommandBufferInfo {
    pub render_pass: Arc<VkRenderPass>,
    pub sub_pass: u32,
    pub frame_buffer: Arc<VkFrameBuffer>,
}

pub struct BoundPipeline {
    pipeline: vk::Pipeline,
    bind_point: vk::PipelineBindPoint,
    pipeline_layout: Arc<VkPipelineLayout>,
}

pub struct VkCommandBuffer {
    buffer: vk::CommandBuffer,
    pool: Arc<RawVkCommandPool>,
    device: Arc<RawVkDevice>,
    state: AtomicCell<VkCommandBufferState>,
    command_buffer_type: CommandBufferType,
    shared: Arc<VkShared>,
    render_pass: Option<Arc<VkRenderPass>>,
    pipeline: Option<BoundPipeline>,
    sub_pass: u32,
    queue_family_index: u32,
    descriptor_manager: VkBindingManager,
    inheritance: Option<VkInnerCommandBufferInfo>,
    frame: u64,
    reset_individually: bool
}

impl VkCommandBuffer {
    pub(crate) fn new(
        device: &Arc<RawVkDevice>,
        pool: &Arc<RawVkCommandPool>,
        command_buffer_type: CommandBufferType,
        queue_family_index: u32,
        reset_individually: bool,
        shared: &Arc<VkShared>,
    ) -> Self {
        let buffers_create_info = vk::CommandBufferAllocateInfo {
            command_pool: ***pool,
            level: if command_buffer_type == CommandBufferType::Primary {
                vk::CommandBufferLevel::PRIMARY
            } else {
                vk::CommandBufferLevel::SECONDARY
            },
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
            state: AtomicCell::new(VkCommandBufferState::Ready),
            queue_family_index,
            descriptor_manager: VkBindingManager::new(device),
            inheritance: None,
            frame: 0u64,
            reset_individually
        }
    }

    pub fn handle(&self) -> vk::CommandBuffer {
        self.buffer
    }

    pub fn cmd_buffer_type(&self) -> CommandBufferType {
        self.command_buffer_type
    }

    pub(crate) fn mark_submitted(&self) {
        assert_eq!(self.state.swap(VkCommandBufferState::Submitted), VkCommandBufferState::Finished);
    }

    pub(crate) fn command_buffer_type(&self) -> CommandBufferType {
        self.command_buffer_type
    }
}

impl Drop for VkCommandBuffer {
    fn drop(&mut self) {
        if self.state.load() == VkCommandBufferState::Submitted {
            self.device.wait_for_idle();
        }
    }
}

impl CommandBuffer<VkBackend> for VkCommandBuffer {
    type CommandBufferInheritance = VkInnerCommandBufferInfo;

    unsafe fn set_pipeline(&mut self, pipeline: PipelineBinding<VkBackend>) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);

        match &pipeline {
            PipelineBinding::Graphics(graphics_pipeline) => {
                let vk_pipeline = graphics_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        vk_pipeline,
                    );
                }

                self.pipeline = Some(BoundPipeline {
                    pipeline: vk_pipeline,
                    bind_point: vk::PipelineBindPoint::GRAPHICS,
                    pipeline_layout: graphics_pipeline.layout().clone(),
                });

                /*if graphics_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features
                        .contains(VkFeatures::DESCRIPTOR_INDEXING)
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }*/
            }
            PipelineBinding::Compute(compute_pipeline) => {
                let vk_pipeline = compute_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.buffer,
                        vk::PipelineBindPoint::COMPUTE,
                        vk_pipeline,
                    );
                }

                self.pipeline = Some(BoundPipeline {
                    pipeline: vk_pipeline,
                    bind_point: vk::PipelineBindPoint::COMPUTE,
                    pipeline_layout: compute_pipeline.layout().clone(),
                });

                /*if compute_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features
                        .contains(VkFeatures::DESCRIPTOR_INDEXING)
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }*/
            }
            PipelineBinding::RayTracing(rt_pipeline) => {
                let vk_pipeline = rt_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.buffer,
                        vk::PipelineBindPoint::RAY_TRACING_KHR,
                        vk_pipeline,
                    );
                }

                self.pipeline = Some(BoundPipeline {
                    pipeline: vk_pipeline,
                    bind_point: vk::PipelineBindPoint::RAY_TRACING_KHR,
                    pipeline_layout: rt_pipeline.layout().clone(),
                });

                /*if rt_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features
                        .contains(VkFeatures::DESCRIPTOR_INDEXING)
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }*/
            }
        };
        self.descriptor_manager.mark_all_dirty();
    }

    unsafe fn end_render_pass(&mut self) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(
            self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary
        );
        unsafe {
            self.device.cmd_end_render_pass(self.buffer);
        }
        self.render_pass = None;
    }

    unsafe fn advance_subpass(&mut self) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(
            self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary
        );
        unsafe {
            self.device
                .cmd_next_subpass(self.buffer, vk::SubpassContents::INLINE);
        }
        self.sub_pass += 1;
    }

    unsafe fn set_vertex_buffer(&mut self, vertex_buffer: &VkBuffer, offset: u64) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        unsafe {
            self.device.cmd_bind_vertex_buffers(
                self.buffer,
                0,
                &[vertex_buffer.handle()],
                &[offset as u64],
            );
        }
    }

    unsafe fn set_index_buffer(
        &mut self,
        index_buffer: &VkBuffer,
        offset: u64,
        format: IndexFormat,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        unsafe {
            self.device.cmd_bind_index_buffer(
                self.buffer,
                index_buffer.handle(),
                offset,
                index_format_to_vk(format),
            );
        }
    }

    unsafe fn set_viewports(&mut self, viewports: &[Viewport]) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        unsafe {
            for (i, viewport) in viewports.iter().enumerate() {
                self.device.cmd_set_viewport(
                    self.buffer,
                    i as u32,
                    &[vk::Viewport {
                        x: viewport.position.x,
                        y: viewport.extent.y - viewport.position.y,
                        width: viewport.extent.x,
                        height: -viewport.extent.y,
                        min_depth: viewport.min_depth,
                        max_depth: viewport.max_depth,
                    }],
                );
            }
        }
    }

    unsafe fn set_scissors(&mut self, scissors: &[Scissor]) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        unsafe {
            let vk_scissors: Vec<vk::Rect2D> = scissors
                .iter()
                .map(|scissor| vk::Rect2D {
                    offset: vk::Offset2D {
                        x: scissor.position.x,
                        y: scissor.position.y,
                    },
                    extent: vk::Extent2D {
                        width: scissor.extent.x,
                        height: scissor.extent.y,
                    },
                })
                .collect();
            self.device.cmd_set_scissor(self.buffer, 0, &vk_scissors);
        }
    }

    unsafe fn draw(&mut self, vertices: u32, offset: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(
            self.pipeline.as_ref().unwrap().bind_point == vk::PipelineBindPoint::GRAPHICS
        );
        debug_assert!(
            self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary
        );
        unsafe {
            self.device.cmd_draw(self.buffer, vertices, 1, offset, 0);
        }
    }

    unsafe fn draw_indexed(
        &mut self,
        instances: u32,
        first_instance: u32,
        indices: u32,
        first_index: u32,
        vertex_offset: i32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(
            self.pipeline.as_ref().unwrap().bind_point == vk::PipelineBindPoint::GRAPHICS
        );
        debug_assert!(
            self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary
        );
        unsafe {
            self.device.cmd_draw_indexed(
                self.buffer,
                indices,
                instances,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    unsafe fn bind_sampling_view(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        texture: &VkTextureView,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::SampledTexture(texture.view_handle()),
        );
    }

    unsafe fn bind_sampling_view_and_sampler(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        texture: &VkTextureView,
        sampler: &VkSampler,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::SampledTextureAndSampler(texture.view_handle(), sampler.handle()),
        );
    }

    unsafe fn bind_uniform_buffer(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        buffer: &VkBuffer,
        offset: u64,
        length: u64,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert_ne!(length, 0);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::UniformBuffer(VkBufferBindingInfo {
                buffer: buffer.handle(),
                offset,
                length: if length == WHOLE_BUFFER {
                    buffer.info().size - offset
                } else {
                    length
                },
            }),
        );
    }

    unsafe fn bind_storage_buffer(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        buffer: &VkBuffer,
        offset: u64,
        length: u64,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert_ne!(length, 0);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::StorageBuffer(VkBufferBindingInfo {
                buffer: buffer.handle(),
                offset,
                length: if length == WHOLE_BUFFER {
                    buffer.info().size - offset
                } else {
                    length
                },
            }),
        );
    }

    unsafe fn bind_storage_texture(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        texture: &VkTextureView,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::StorageTexture(texture.view_handle()),
        );
    }

    unsafe fn bind_sampler(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        sampler: &VkSampler,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::Sampler(sampler.handle()),
        );
    }

    unsafe fn finish_binding(&mut self) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());

        let mut offsets = SmallVec::<[u32; PER_SET_BINDINGS]>::new();
        let mut descriptor_sets =
            SmallVec::<[vk::DescriptorSet; (BINDLESS_TEXTURE_SET_INDEX + 1) as usize]>::new();
        let mut base_index = 0;

        let pipeline = self.pipeline.as_ref().expect("No pipeline bound");
        let pipeline_layout = &pipeline.pipeline_layout;

        let finished_sets = self.descriptor_manager.finish(0, pipeline_layout);
        for (index, set_option) in finished_sets.iter().enumerate() {
            match set_option {
                None => {
                    if !descriptor_sets.is_empty() {
                        unsafe {
                            self.device.cmd_bind_descriptor_sets(
                                self.buffer,
                                pipeline.bind_point,
                                pipeline_layout.handle(),
                                base_index,
                                &descriptor_sets,
                                &offsets,
                            );
                            offsets.clear();
                            descriptor_sets.clear();
                        }
                    }
                    base_index = index as u32 + 1;
                }
                Some(set_binding) => {
                    descriptor_sets.push(set_binding.set.handle());
                    for i in 0..set_binding.dynamic_offset_count as usize {
                        offsets.push(set_binding.dynamic_offsets[i] as u32);
                    }
                }
            }
        }

        if !descriptor_sets.is_empty()
            && base_index + descriptor_sets.len() as u32 != BINDLESS_TEXTURE_SET_INDEX
        {
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    self.buffer,
                    pipeline.bind_point,
                    pipeline_layout.handle(),
                    base_index,
                    &descriptor_sets,
                    &offsets,
                );
            }
            offsets.clear();
            descriptor_sets.clear();
            base_index = BINDLESS_TEXTURE_SET_INDEX;
        }

        /*if pipeline.uses_bindless_texture_set() {
            let bindless_texture_descriptor_set =
                self.shared.bindless_texture_descriptor_set().unwrap();
            descriptor_sets.push(bindless_texture_descriptor_set.descriptor_set_handle());
        }*/

        if !descriptor_sets.is_empty() {
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    self.buffer,
                    pipeline.bind_point,
                    pipeline_layout.handle(),
                    base_index,
                    &descriptor_sets,
                    &offsets,
                );
            }
        }
    }

    unsafe fn begin_label(&mut self, label: &str) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let label_cstring = CString::new(label).unwrap();
        if let Some(debug_utils) = self.device.instance.debug_utils.as_ref() {
            debug_utils.debug_utils_loader.cmd_begin_debug_utils_label(
                self.buffer,
                &vk::DebugUtilsLabelEXT {
                    p_label_name: label_cstring.as_ptr(),
                    color: [0.0f32; 4],
                    ..Default::default()
                },
            );
        }
    }

    unsafe fn end_label(&mut self) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        if let Some(debug_utils) = self.device.instance.debug_utils.as_ref() {
            debug_utils
                .debug_utils_loader
                .cmd_end_debug_utils_label(self.buffer);
        }
    }

    unsafe fn execute_inner(&mut self, submissions: &mut [VkCommandBuffer]) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        if submissions.is_empty() {
            return;
        }

        for submission in submissions.iter() {
            assert_eq!(
                submission.command_buffer_type(),
                CommandBufferType::Secondary
            );
        }
        let submission_handles: SmallVec<[vk::CommandBuffer; 16]> =
            submissions.iter().map(|s| s.handle()).collect();
        unsafe {
            self.device
                .cmd_execute_commands(self.buffer, &submission_handles);
        }
        for submission in submissions {
            submission.mark_submitted();
        }
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());
        debug_assert!(self.pipeline.is_some());
        debug_assert!(self.pipeline.as_ref().unwrap().bind_point == vk::PipelineBindPoint::COMPUTE);
        unsafe {
            self.device
                .cmd_dispatch(self.buffer, group_count_x, group_count_y, group_count_z);
        }
    }

    unsafe fn blit(
        &mut self,
        src_texture: &VkTexture,
        src_array_layer: u32,
        src_mip_level: u32,
        dst_texture: &VkTexture,
        dst_array_layer: u32,
        dst_mip_level: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());
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
            self.device.cmd_blit_image(
                self.buffer,
                src_texture.handle(),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_texture.handle(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit {
                    src_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: src_aspect,
                        mip_level: src_mip_level,
                        base_array_layer: src_array_layer,
                        layer_count: 1,
                    },
                    src_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: src_info.width as i32,
                            y: src_info.height as i32,
                            z: src_info.depth as i32,
                        },
                    ],
                    dst_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: dst_aspect,
                        mip_level: dst_mip_level,
                        base_array_layer: dst_array_layer,
                        layer_count: 1,
                    },
                    dst_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: dst_info.width as i32,
                            y: dst_info.height as i32,
                            z: dst_info.depth as i32,
                        },
                    ],
                }],
                vk::Filter::LINEAR,
            );
        }
    }

    unsafe fn barrier(&mut self, barriers: &[Barrier<VkBackend>]) {
        let mut pending_image_barriers =
            SmallVec::<[vk::ImageMemoryBarrier2; 4]>::with_capacity(barriers.len());
        let mut pending_buffer_barriers =
            SmallVec::<[vk::BufferMemoryBarrier2; 4]>::with_capacity(barriers.len());
        let mut pending_memory_barriers = <[vk::MemoryBarrier2; 2]>::default();

        for barrier in barriers {
            match barrier {
                Barrier::TextureBarrier {
                    old_sync,
                    new_sync,
                    old_layout,
                    new_layout,
                    old_access,
                    new_access,
                    texture,
                    range,
                    queue_ownership
                } => {
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

                    let mut src_queue_family_index: u32 = vk::QUEUE_FAMILY_IGNORED;
                    let mut dst_queue_family_index: u32 = vk::QUEUE_FAMILY_IGNORED;
                    if let Some(queue_transfer) = queue_ownership {
                        src_queue_family_index = match queue_transfer.from {
                            QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                        };
                        dst_queue_family_index = match queue_transfer.to {
                            QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                        };
                    }
                    if src_queue_family_index == vk::QUEUE_FAMILY_IGNORED || dst_queue_family_index == vk::QUEUE_FAMILY_IGNORED {
                        src_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
                        dst_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
                    }

                    let dst_stages = barrier_sync_to_stage(*new_sync);
                    let src_stages = barrier_sync_to_stage(*old_sync);
                    pending_image_barriers.push(vk::ImageMemoryBarrier2 {
                        src_stage_mask: src_stages,
                        dst_stage_mask: dst_stages,
                        src_access_mask: barrier_access_to_access(*old_access),
                        dst_access_mask: barrier_access_to_access(*new_access),
                        old_layout: texture_layout_to_image_layout(*old_layout),
                        new_layout: texture_layout_to_image_layout(*new_layout),
                        src_queue_family_index: src_queue_family_index,
                        dst_queue_family_index: dst_queue_family_index,
                        image: texture.handle(),
                        subresource_range: vk::ImageSubresourceRange {
                            aspect_mask,
                            base_array_layer: range.base_array_layer,
                            base_mip_level: range.base_mip_level,
                            level_count: range.mip_level_length,
                            layer_count: range.array_layer_length,
                        },
                        ..Default::default()
                    });
                }
                Barrier::BufferBarrier {
                    old_sync,
                    new_sync,
                    old_access,
                    new_access,
                    buffer,
                    offset,
                    length,
                    queue_ownership
                } => {
                    let dst_stages = barrier_sync_to_stage(*new_sync);
                    let src_stages = barrier_sync_to_stage(*old_sync);

                    let mut src_queue_family_index: u32 = vk::QUEUE_FAMILY_IGNORED;
                    let mut dst_queue_family_index: u32 = vk::QUEUE_FAMILY_IGNORED;
                    if let Some(queue_transfer) = queue_ownership {
                        src_queue_family_index = match queue_transfer.from {
                            QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                        };
                        dst_queue_family_index = match queue_transfer.to {
                            QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                        };
                    }
                    if src_queue_family_index == vk::QUEUE_FAMILY_IGNORED || dst_queue_family_index == vk::QUEUE_FAMILY_IGNORED {
                        src_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
                        dst_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
                    }

                    pending_buffer_barriers.push(vk::BufferMemoryBarrier2 {
                        src_stage_mask: src_stages,
                        dst_stage_mask: dst_stages,
                        src_access_mask: barrier_access_to_access(*old_access),
                        dst_access_mask: barrier_access_to_access(*new_access),
                        src_queue_family_index: src_queue_family_index,
                        dst_queue_family_index: dst_queue_family_index,
                        buffer: buffer.handle(),
                        offset: *offset as u64,
                        size: (buffer.info().size - *offset).min(*length),
                        ..Default::default()
                    });
                }
                Barrier::GlobalBarrier {
                    old_sync,
                    new_sync,
                    old_access,
                    new_access,
                } => {
                    let dst_stages = barrier_sync_to_stage(*new_sync);
                    let src_stages = barrier_sync_to_stage(*old_sync);
                    let src_access = barrier_access_to_access(*old_access);
                    let dst_access = barrier_access_to_access(*new_access);

                    pending_memory_barriers[0].dst_stage_mask |= dst_stages
                        & !(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                            | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    pending_memory_barriers[0].src_stage_mask |= src_stages
                        & !(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                            | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    pending_memory_barriers[0].src_access_mask |=
                        barrier_access_to_access(*old_access)
                            & !(vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);
                    pending_memory_barriers[0].dst_access_mask |= dst_access
                        & !(vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR
                            | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);

                    pending_memory_barriers[1].dst_access_mask |= dst_access
                        & (vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR
                            | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);

                    if !pending_memory_barriers[1].dst_access_mask.is_empty() {
                        /*
                        VUID-VkMemoryBarrier2-dstAccessMask-06256
                        If the rayQuery feature is not enabled and dstAccessMask includes VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR,
                        dstStageMask must not include any of the VK_PIPELINESTAGE*_SHADER_BIT stages except VK_PIPELINE_STAGE_2_RAY_TRACING_SHADER_BIT_KHR

                        So we need to handle RT barriers separately.
                        */
                        pending_memory_barriers[1].src_stage_mask = src_stages
                            & (vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                                | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                        pending_memory_barriers[1].src_access_mask =
                            src_access & (vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);
                        pending_memory_barriers[1].dst_stage_mask = dst_stages
                            & (vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR
                                | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                                | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    }
                }
            }
        }

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
                self.device
                    .synchronization2
                    .cmd_pipeline_barrier2(self.buffer, &dependency_info);
            }
            pending_memory_barriers[0].src_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[0].dst_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[0].src_access_mask = vk::AccessFlags2::empty();
            pending_memory_barriers[0].dst_access_mask = vk::AccessFlags2::empty();
            pending_memory_barriers[1].src_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[1].dst_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[1].src_access_mask = vk::AccessFlags2::empty();
            pending_memory_barriers[1].dst_access_mask = vk::AccessFlags2::empty();
            pending_image_barriers.clear();
            pending_buffer_barriers.clear();
            return;
        }

        let has_pending_barriers = !pending_image_barriers.is_empty()
            || !pending_buffer_barriers.is_empty()
            || !pending_memory_barriers[0].src_stage_mask.is_empty()
            || !pending_memory_barriers[0].dst_stage_mask.is_empty()
            || !pending_memory_barriers[1].src_stage_mask.is_empty()
            || !pending_memory_barriers[1].dst_stage_mask.is_empty();

        if !has_pending_barriers {
            return;
        }

        let dependency_info = vk::DependencyInfo {
            image_memory_barrier_count: pending_image_barriers.len() as u32,
            p_image_memory_barriers: pending_image_barriers.as_ptr(),
            buffer_memory_barrier_count: pending_buffer_barriers.len() as u32,
            p_buffer_memory_barriers: pending_buffer_barriers.as_ptr(),
            memory_barrier_count: if pending_memory_barriers[0].src_stage_mask.is_empty()
                && pending_memory_barriers[0].dst_stage_mask.is_empty()
                && pending_memory_barriers[1].src_stage_mask.is_empty()
                && pending_memory_barriers[1].dst_stage_mask.is_empty()
            {
                0
            } else if pending_memory_barriers[1].src_stage_mask.is_empty()
                && pending_memory_barriers[1].dst_stage_mask.is_empty()
            {
                1
            } else {
                2
            },
            p_memory_barriers: &pending_memory_barriers as *const vk::MemoryBarrier2,
            ..Default::default()
        };

        self.device
            .synchronization2
            .cmd_pipeline_barrier2(self.buffer, &dependency_info);
    }

    unsafe fn begin_render_pass(
        &mut self,
        renderpass_begin_info: &RenderPassBeginInfo<VkBackend>,
        recording_mode: RenderpassRecordingMode,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());

        let mut attachment_infos = SmallVec::<[VkAttachmentInfo; 16]>::with_capacity(
            renderpass_begin_info.attachments.len(),
        );
        let mut width = 0u32;
        let mut height = 0u32;
        let mut attachment_views =
            SmallVec::<[&VkTextureView; 8]>::with_capacity(renderpass_begin_info.attachments.len());
        let mut clear_values =
            SmallVec::<[vk::ClearValue; 8]>::with_capacity(renderpass_begin_info.attachments.len());

        for attachment in renderpass_begin_info.attachments {
            let view = match &attachment.view {
                RenderPassAttachmentView::RenderTarget(view) => *view,
                RenderPassAttachmentView::DepthStencil(view) => *view,
            };

            let info = view.texture_info();
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
                    },
                }
            } else {
                vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0f32; 4] },
                }
            });
        }

        let renderpass_info = VkRenderPassInfo {
            attachments: attachment_infos,
            subpasses: renderpass_begin_info
                .subpasses
                .iter()
                .map(|sp| VkSubpassInfo {
                    input_attachments: sp.input_attachments.iter().cloned().collect(),
                    output_color_attachments: sp.output_color_attachments.iter().cloned().collect(),
                    depth_stencil_attachment: sp.depth_stencil_attachment.clone(),
                })
                .collect(),
        };

        let renderpass = self.shared.get_render_pass(renderpass_info);
        let framebuffer = self.shared.get_framebuffer(&renderpass, &attachment_views);

        // TODO: begin info fields
        unsafe {
            let begin_info = vk::RenderPassBeginInfo {
                framebuffer: framebuffer.handle(),
                render_pass: renderpass.handle(),
                render_area: vk::Rect2D {
                    offset: vk::Offset2D { x: 0i32, y: 0i32 },
                    extent: vk::Extent2D { width, height },
                },
                clear_value_count: clear_values.len() as u32,
                p_clear_values: clear_values.as_ptr(),
                ..Default::default()
            };
            self.device.cmd_begin_render_pass(
                self.buffer,
                &begin_info,
                if recording_mode == RenderpassRecordingMode::Commands {
                    vk::SubpassContents::INLINE
                } else {
                    vk::SubpassContents::SECONDARY_COMMAND_BUFFERS
                },
            );
        }
        self.sub_pass = 0;
        self.render_pass = Some(renderpass.clone());
        self.inheritance = Some(VkInnerCommandBufferInfo {
            render_pass: renderpass,
            sub_pass: 0,
            frame_buffer: framebuffer,
        });
    }

    unsafe fn inheritance(&self) -> &VkInnerCommandBufferInfo {
        self.inheritance.as_ref().unwrap()
    }

    /*unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        info: &BottomLevelAccelerationStructureInfo<VkBackend>,
        size: usize,
        target_buffer: &VkBuffer,
        scratch_buffer: &VkBuffer,
    ) -> VkAccelerationStructure {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());
        let acceleration_structure = Arc::new(VkAccelerationStructure::new_bottom_level(
            &self.device,
            info,
            size,
            target_buffer,
            scratch_buffer,
            self.handle(),
        ));
        acceleration_structure
    }

    fn upload_top_level_instances(
        &mut self,
        instances: &[AccelerationStructureInstance<VkBackend>],
    ) -> VkBuffer {
        unimplemented!()
        //VkAccelerationStructure::upload_top_level_instances(self, instances)
    }

    fn create_top_level_acceleration_structure(
        &mut self,
        info: &TopLevelAccelerationStructureInfo<VkBackend>,
        size: usize,
        target_buffer: &VkBuffer,
        scratch_buffer: &VkBuffer,
    ) -> VkAccelerationStructure {
        /*debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());
        for instance in info.instances {
            self.trackers
                .track_acceleration_structure(instance.acceleration_structure);
        }
        let acceleration_structure = Arc::new(VkAccelerationStructure::new_top_level(
            &self.device,
            info,
            size,
            target_buffer,
            scratch_buffer,
            self.handle(),
        ));
        acceleration_structure*/
        unimplemented!()
    }

    unsafe fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());
        debug_assert!(
            self.pipeline.as_ref().unwrap().bind_point == vk::PipelineBindPoint::RAY_TRACING_KHR
        );

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
                depth,
            );
        }
    }

    unsafe fn bind_acceleration_structure(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        acceleration_structure: &VkAccelerationStructure,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::AccelerationStructure(acceleration_structure.handle()),
        );
    }*/

    unsafe fn bind_sampling_view_and_sampler_array(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        textures_and_samplers: &[(&VkTextureView, &VkSampler)],
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let handles: SmallVec<[(vk::ImageView, vk::Sampler); 8]> = textures_and_samplers
            .iter()
            .map(|(tv, s)| (tv.view_handle(), s.handle()))
            .collect();
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::SampledTextureAndSamplerArray(&handles),
        );
    }

    unsafe fn bind_storage_view_array(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        textures: &[&VkTextureView],
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let handles: SmallVec<[vk::ImageView; 8]> =
            textures.iter().map(|tv| tv.view_handle()).collect();
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::StorageTextureArray(&handles),
        );
    }

    unsafe fn draw_indexed_indirect(
        &mut self,
        draw_buffer: &VkBuffer,
        draw_buffer_offset: u32,
        count_buffer: &VkBuffer,
        count_buffer_offset: u32,
        max_draw_count: u32,
        stride: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(
            self.pipeline.as_ref().unwrap().bind_point == vk::PipelineBindPoint::GRAPHICS
        );
        debug_assert!(
            self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary
        );
        unsafe {
            self.device
                .indirect_count
                .as_ref()
                .unwrap()
                .cmd_draw_indexed_indirect_count(
                    self.buffer,
                    draw_buffer.handle(),
                    draw_buffer_offset as u64,
                    count_buffer.handle(),
                    count_buffer_offset as u64,
                    max_draw_count,
                    stride,
                );
        }
    }

    unsafe fn draw_indirect(
        &mut self,
        draw_buffer: &VkBuffer,
        draw_buffer_offset: u32,
        count_buffer: &VkBuffer,
        count_buffer_offset: u32,
        max_draw_count: u32,
        stride: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(
            self.pipeline.as_ref().unwrap().bind_point == vk::PipelineBindPoint::GRAPHICS
        );
        debug_assert!(
            self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary
        );
        unsafe {
            self.device
                .indirect_count
                .as_ref()
                .unwrap()
                .cmd_draw_indirect_count(
                    self.buffer,
                    draw_buffer.handle(),
                    draw_buffer_offset as u64,
                    count_buffer.handle(),
                    count_buffer_offset as u64,
                    max_draw_count,
                    stride,
                );
        }
    }

    unsafe fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_type: ShaderType)
    where
        T: 'static + Send + Sync + Sized + Clone,
    {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let pipeline = self.pipeline.as_ref().expect("No pipeline bound");
        let pipeline_layout = &pipeline.pipeline_layout;
        let range = pipeline_layout
            .push_constant_range(visible_for_shader_type)
            .expect("No push constants set up for shader");
        let size = std::mem::size_of_val(data);
        unsafe {
            self.device.cmd_push_constants(
                self.buffer,
                pipeline_layout.handle(),
                shader_type_to_vk(visible_for_shader_type),
                range.offset,
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    min(size, range.size as usize),
                ),
            );
        }
    }

    unsafe fn clear_storage_texture(
        &mut self,
        texture: &VkTexture,
        array_layer: u32,
        mip_level: u32,
        values: [u32; 4],
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());

        let format = texture.info().format;
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
            base_mip_level: mip_level,
            level_count: 1,
            base_array_layer: array_layer,
            layer_count: 1,
        };

        unsafe {
            if aspect_mask.intersects(vk::ImageAspectFlags::DEPTH)
                || aspect_mask.intersects(vk::ImageAspectFlags::STENCIL)
            {
                self.device.cmd_clear_depth_stencil_image(
                    self.buffer,
                    texture.handle(),
                    vk::ImageLayout::GENERAL,
                    &vk::ClearDepthStencilValue {
                        depth: values[0] as f32,
                        stencil: values[1],
                    },
                    &[range],
                );
            } else {
                self.device.cmd_clear_color_image(
                    self.buffer,
                    texture.handle(),
                    vk::ImageLayout::GENERAL,
                    &vk::ClearColorValue { uint32: values },
                    &[range],
                );
            }
        }
    }

    unsafe fn clear_storage_buffer(
        &mut self,
        buffer: &VkBuffer,
        offset: u64,
        length_in_u32s: u64,
        value: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());

        let actual_length_in_u32s = if length_in_u32s == WHOLE_BUFFER {
            debug_assert_eq!((buffer.info().size - offset) % 4, 0);
            (buffer.info().size - offset) / 4
        } else {
            length_in_u32s
        };
        let length_in_bytes = actual_length_in_u32s * 4;
        debug_assert!(buffer.info().size - offset >= length_in_bytes);

        #[repr(packed)]
        struct MetaClearShaderData {
            length: u32,
            value: u32,
        }
        let push_data = MetaClearShaderData {
            length: length_in_bytes as u32,
            value: value,
        };

        let meta_pipeline = self.shared.get_clear_buffer_meta_pipeline().clone();
        let mut bindings = <[VkBoundResourceRef; PER_SET_BINDINGS]>::default();
        let binding_offsets = [offset as u32];
        let is_dynamic_binding = meta_pipeline
            .layout()
            .descriptor_set_layout(0)
            .unwrap()
            .is_dynamic_binding(0);
        bindings[0] = VkBoundResourceRef::StorageBuffer(VkBufferBindingInfo {
            buffer: buffer.handle(),
            offset,
            length: length_in_bytes,
        });
        let descriptor_set = self
            .descriptor_manager
            .get_or_create_set(
                0,
                meta_pipeline
                    .layout()
                    .descriptor_set_layout(0)
                    .as_ref()
                    .unwrap(),
                &bindings,
            )
            .unwrap();
        unsafe {
            self.device.cmd_bind_pipeline(
                self.buffer,
                vk::PipelineBindPoint::COMPUTE,
                meta_pipeline.handle(),
            );

            self.device.cmd_push_constants(
                self.buffer,
                meta_pipeline.layout().handle(),
                vk::ShaderStageFlags::COMPUTE,
                0,
                std::slice::from_raw_parts(
                    std::mem::transmute(&push_data as *const MetaClearShaderData),
                    std::mem::size_of::<MetaClearShaderData>(),
                ),
            );
            self.device.cmd_bind_descriptor_sets(
                self.buffer,
                vk::PipelineBindPoint::COMPUTE,
                meta_pipeline.layout().handle(),
                0,
                &[descriptor_set.handle()],
                if is_dynamic_binding {
                    &binding_offsets
                } else {
                    &[]
                },
            );
            self.device
                .cmd_dispatch(self.buffer, (actual_length_in_u32s as u32 + 63) / 64, 1, 1);
        }
        self.descriptor_manager.mark_all_dirty();
    }

    unsafe fn begin(&mut self, inner_info: Option<&VkInnerCommandBufferInfo>, frame: u64) {
        assert_eq!(self.state.load(), VkCommandBufferState::Ready);
        assert!(self.frame < frame);
        self.frame = frame;

        self.state.store(VkCommandBufferState::Recording);

        let (flags, inhertiance_info) = if let Some(inner_info) = inner_info {
            (
                vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT
                    | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE,
                vk::CommandBufferInheritanceInfo {
                    render_pass: inner_info.render_pass.handle(),
                    subpass: inner_info.sub_pass,
                    framebuffer: inner_info.frame_buffer.handle(),
                    ..Default::default()
                },
            )
        } else {
            (
                vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                Default::default(),
            )
        };

        unsafe {
            self.device
                .begin_command_buffer(
                    self.buffer,
                    &vk::CommandBufferBeginInfo {
                        flags,
                        p_inheritance_info: &inhertiance_info
                            as *const vk::CommandBufferInheritanceInfo,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
    }

    unsafe fn copy_buffer(&mut self, src: &VkBuffer, dst: &VkBuffer, region: &BufferCopyRegion) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let copy = vk::BufferCopy {
            src_offset: region.src_offset,
            dst_offset: region.dst_offset,
            size: region.size
                .min(src.info().size - region.src_offset)
                .min(dst.info().size - region.dst_offset)
        };
        self.device.cmd_copy_buffer(self.buffer, src.handle(), dst.handle(), &[copy]);
    }

    unsafe fn copy_buffer_to_texture(&mut self, src: &VkBuffer, dst: &VkTexture, region: &BufferTextureCopyRegion) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let format = dst.info().format;
        let texels_width = if region.buffer_row_pitch != 0 {
            (region.buffer_row_pitch as u32) * format.block_size().x / format.element_size()
        } else {
            0
        };
        let texels_height = if region.buffer_slice_pitch != 0 {
            (region.buffer_slice_pitch as u32) / texels_width * format.block_size().y / format.element_size()
        } else {
            0
        };

        let copy = vk::BufferImageCopy {
            image_subresource: texture_subresource_to_vk_layers(&region.texture_subresource, format, 1),
            buffer_offset: region.buffer_offset,
            buffer_row_length: texels_width,
            buffer_image_height: texels_height,
            image_offset: vk::Offset3D {
                x: region.texture_offset.x as i32,
                y: region.texture_offset.y as i32,
                z: region.texture_offset.z as i32
            },
            image_extent: vk::Extent3D {
                width: region.texture_extent.x,
                height: region.texture_extent.y,
                depth: region.texture_extent.z,
            }
        };
        self.device.cmd_copy_buffer_to_image(self.buffer, src.handle(), dst.handle(), vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[copy]);
    }

    unsafe fn finish(&mut self) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        if self.render_pass.is_some() {
            self.end_render_pass();
        }

        self.state.store(VkCommandBufferState::Finished);
        self.device.end_command_buffer(self.buffer).unwrap();
    }

    unsafe fn reset(&mut self, frame: u64) {
        if self.reset_individually {
            self.device.reset_command_buffer(self.buffer, vk::CommandBufferResetFlags::empty()).unwrap();
        }
        self.descriptor_manager.reset(frame);
    }
}

pub(super) fn barrier_sync_to_stage(sync: BarrierSync) -> vk::PipelineStageFlags2 {
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

const WRITE_ACCESS_MASK: vk::AccessFlags2 = vk::AccessFlags2::from_raw(
    vk::AccessFlags2::HOST_WRITE.as_raw()
        | vk::AccessFlags2::MEMORY_WRITE.as_raw()
        | vk::AccessFlags2::SHADER_WRITE.as_raw()
        | vk::AccessFlags2::TRANSFER_WRITE.as_raw()
        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw()
        | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw(),
);
