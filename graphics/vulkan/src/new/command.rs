use std::cmp::min;
use std::ffi::CString;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

use ash::vk;
use crossbeam_channel::{
    unbounded,
    Receiver,
    Sender,
};
use smallvec::SmallVec;

use sourcerenderer_core::gpu::*;

use super::*;

const BINDLESS_TEXTURE_SET_INDEX: u32 = 3;

#[allow(clippy::vec_box)]
pub struct VkCommandPool {
    raw: Arc<RawVkCommandPool>,
    primary_buffers: Vec<Box<VkCommandBuffer>>,
    secondary_buffers: Vec<Box<VkCommandBuffer>>,
    receiver: Receiver<Box<VkCommandBuffer>>,
    sender: Sender<Box<VkCommandBuffer>>,
    shared: Arc<VkShared>,
    queue_family_index: u32,
}

impl VkCommandPool {
    pub fn new(
        device: &Arc<RawVkDevice>,
        queue_family_index: u32,
        shared: &Arc<VkShared>,
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
            queue_family_index
        }
    }

    pub fn get_command_buffer(&mut self, frame: u64) -> Box<VkCommandBuffer> {
        let mut buffer = self.primary_buffers.pop().unwrap_or_else(|| {
            Box::new(VkCommandBuffer::new(
                &self.raw.device,
                &self.raw,
                CommandBufferType::Primary,
                self.queue_family_index,
                &self.shared
            ))
        });
        buffer.begin(frame, None);
        buffer
    }

    pub fn get_inner_command_buffer(
        &mut self,
        frame: u64,
        inner_info: Option<&VkInnerCommandBufferInfo>,
    ) -> Box<VkCommandBuffer> {
        let mut buffer = self.secondary_buffers.pop().unwrap_or_else(|| {
            Box::new(VkCommandBuffer::new(
                &self.raw.device,
                &self.raw,
                CommandBufferType::Secondary,
                self.queue_family_index,
                &self.shared
            ))
        });
        buffer.begin(frame, inner_info);
        buffer
    }
}

impl Resettable for VkCommandPool {
    fn reset(&mut self) {
        unsafe {
            self.raw
                .device
                .reset_command_pool(**self.raw, vk::CommandPoolResetFlags::empty())
                .unwrap();
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
    Submitted,
}

pub struct VkInnerCommandBufferInfo {
    pub render_pass: Arc<VkRenderPass>,
    pub sub_pass: u32,
    pub frame_buffer: Arc<VkFrameBuffer>,
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
    queue_family_index: u32,
    descriptor_manager: VkBindingManager,
    frame: u64,
    inheritance: Option<VkInnerCommandBufferInfo>,
}

impl VkCommandBuffer {
    pub(crate) fn new(
        device: &Arc<RawVkDevice>,
        pool: &Arc<RawVkCommandPool>,
        command_buffer_type: CommandBufferType,
        queue_family_index: u32,
        shared: &Arc<VkShared>
    ) -> Self {
        let buffers_create_info = vk::CommandBufferAllocateInfo {
            command_pool: ***pool,
            level: if command_buffer_type == CommandBufferType::Primary {
                vk::CommandBufferLevel::PRIMARY
            } else {
                vk::CommandBufferLevel::SECONDARY
            }, // TODO: support secondary command buffers / bundles
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
            queue_family_index,
            descriptor_manager: VkBindingManager::new(device),
            frame: 0,
            inheritance: None,
        }
    }

    pub fn handle(&self) -> vk::CommandBuffer {
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
        debug_assert!(frame >= self.frame);

        self.state = VkCommandBufferState::Recording;
        self.frame = frame;

        let (flags, inhertiance_info) = if let Some(inner_info) = inner_info {
            (
                vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT
                    | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE,
                vk::CommandBufferInheritanceInfo {
                    render_pass: *inner_info.render_pass.handle(),
                    subpass: inner_info.sub_pass,
                    framebuffer: *inner_info.frame_buffer.handle(),
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

    pub(crate) unsafe fn end(&mut self) {
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
}

impl Drop for VkCommandBuffer {
    fn drop(&mut self) {
        if self.state == VkCommandBufferState::Submitted {
            self.device.wait_for_idle();
        }
    }
}

impl CommandBuffer<VkBackend> for VkCommandBuffer {
    unsafe fn set_pipeline(&mut self, pipeline: PipelineBinding<VkBackend>) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);

        match &pipeline {
            PipelineBinding::Graphics(graphics_pipeline) => {
                let vk_pipeline = graphics_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        *vk_pipeline,
                    );
                }

                self.trackers.track_pipeline(*graphics_pipeline);
                if graphics_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features
                        .contains(VkFeatures::DESCRIPTOR_INDEXING)
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }
                self.pipeline = Some((*graphics_pipeline).clone())
            }
            PipelineBinding::Compute(compute_pipeline) => {
                let vk_pipeline = compute_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.buffer,
                        vk::PipelineBindPoint::COMPUTE,
                        *vk_pipeline,
                    );
                }
                self.trackers.track_pipeline(*compute_pipeline);
                if compute_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features
                        .contains(VkFeatures::DESCRIPTOR_INDEXING)
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }
                self.pipeline = Some((*compute_pipeline).clone())
            }
            PipelineBinding::RayTracing(rt_pipeline) => {
                let vk_pipeline = rt_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.buffer,
                        vk::PipelineBindPoint::RAY_TRACING_KHR,
                        *vk_pipeline,
                    );
                }
                self.trackers.track_pipeline(*rt_pipeline);
                if rt_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features
                        .contains(VkFeatures::DESCRIPTOR_INDEXING)
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }
                self.pipeline = Some((*rt_pipeline).clone())
            }
        };
        self.descriptor_manager.mark_all_dirty();
    }

    unsafe fn end_render_pass(&mut self) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(
            self.render_pass.is_some() || self.command_buffer_type == CommandBufferType::Secondary
        );
        unsafe {
            self.device.cmd_end_render_pass(self.buffer);
        }
        self.render_pass = None;
    }

    unsafe fn advance_subpass(&mut self) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        unsafe {
            self.device.cmd_bind_vertex_buffers(
                self.buffer,
                0,
                &[vertex_buffer.buffer().handle()],
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        unsafe {
            self.device.cmd_bind_index_buffer(
                self.buffer,
                index_buffer.buffer().handle(),
                offset,
                index_format_to_vk(format),
            );
        }
    }

    unsafe fn set_viewports(&mut self, viewports: &[Viewport]) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
        debug_assert!(!self.has_pending_barrier());
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
        debug_assert!(!self.has_pending_barrier());
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            &VkBoundResource::SampledTexture(texture.handle()),
        );
    }

    unsafe fn bind_sampling_view_and_sampler(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        texture: &VkTextureView,
        sampler: &VkSampler,
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            &VkBoundResource::SampledTextureAndSampler(texture.view_handle(), sampler.handle()),
        );
    }

    unsafe fn bind_uniform_buffer(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        buffer: &VkBuffer,
        offset: usize,
        length: usize,
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert_ne!(length, 0);
        self.descriptor_manager.bind(
            frequency,
            binding,
            &VkBoundResource::UniformBuffer(VkBufferBindingInfo {
                buffer: buffer.handle(),
                offset,
                length: if length == WHOLE_BUFFER {
                    buffer.length() - offset
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
        offset: usize,
        length: usize,
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert_ne!(length, 0);
        self.descriptor_manager.bind(
            frequency,
            binding,
            &VkBoundResource::StorageBuffer(VkBufferBindingInfo {
                buffer: buffer.handle(),
                offset,
                length: if length == WHOLE_BUFFER {
                    buffer.length() - offset
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            &VkBoundResource::StorageTexture(texture.view_handle()),
        );
    }

    unsafe fn bind_sampler(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        sampler: &VkSampler,
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        self.descriptor_manager
            .bind(frequency, binding, &VkBoundResource::Sampler(sampler.handle()));
    }

    unsafe fn finish_binding(&mut self) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());

        self.flush_barriers();

        let mut offsets = SmallVec::<[u32; PER_SET_BINDINGS]>::new();
        let mut descriptor_sets =
            SmallVec::<[vk::DescriptorSet; (BINDLESS_TEXTURE_SET_INDEX + 1) as usize]>::new();
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
                                    VkPipelineType::RayTracing => {
                                        vk::PipelineBindPoint::RAY_TRACING_KHR
                                    }
                                },
                                *pipeline_layout.handle(),
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
                    descriptor_sets.push(*set_binding.set.handle());
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
                    match pipeline.pipeline_type() {
                        VkPipelineType::Graphics => vk::PipelineBindPoint::GRAPHICS,
                        VkPipelineType::Compute => vk::PipelineBindPoint::COMPUTE,
                        VkPipelineType::RayTracing => vk::PipelineBindPoint::RAY_TRACING_KHR,
                    },
                    *pipeline_layout.handle(),
                    base_index,
                    &descriptor_sets,
                    &offsets,
                );
            }
            offsets.clear();
            descriptor_sets.clear();
            base_index = BINDLESS_TEXTURE_SET_INDEX;
        }

        if pipeline.uses_bindless_texture_set() {
            let bindless_texture_descriptor_set =
                self.shared.bindless_texture_descriptor_set().unwrap();
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
                    &offsets,
                );
            }
        }
    }

    pub(crate) fn begin_label(&self, label: &str) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        let label_cstring = CString::new(label).unwrap();
        if let Some(debug_utils) = self.device.instance.debug_utils.as_ref() {
            unsafe {
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
    }

    pub(crate) fn end_label(&self) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        if let Some(debug_utils) = self.device.instance.debug_utils.as_ref() {
            unsafe {
                debug_utils
                    .debug_utils_loader
                    .cmd_end_debug_utils_label(self.buffer);
            }
        }
    }

    pub(crate) fn execute_inner(&mut self, mut submissions: &[CommandBuffer]) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        if submissions.is_empty() {
            return;
        }

        for submission in &submissions {
            assert_eq!(
                submission.command_buffer_type(),
                CommandBufferType::Secondary
            );
        }
        let submission_handles: SmallVec<[vk::CommandBuffer; 16]> =
            submissions.iter().map(|s| *s.handle()).collect();
        unsafe {
            self.device
                .cmd_execute_commands(self.buffer, &submission_handles);
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
            self.device
                .cmd_dispatch(self.buffer, group_count_x, group_count_y, group_count_z);
        }
    }

    pub(crate) fn blit(
        &mut self,
        src_texture: &Arc<VkTexture>,
        src_array_layer: u32,
        src_mip_level: u32,
        dst_texture: &Arc<VkTexture>,
        dst_array_layer: u32,
        dst_mip_level: u32,
    ) {
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
            self.device.cmd_blit_image(
                self.buffer,
                *src_texture.handle(),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                *dst_texture.handle(),
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

        self.trackers.track_texture(src_texture);
        self.trackers.track_texture(dst_texture);
    }

    pub(crate) fn barrier(&mut self, barriers: &[Barrier<VkBackend>]) {
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
                            layer_count: range.array_layer_length,
                        },
                        ..Default::default()
                    });
                    self.trackers.track_texture(texture);
                }
                Barrier::BufferBarrier {
                    old_sync,
                    new_sync,
                    old_access,
                    new_access,
                    buffer,
                } => {
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

                    self.pending_memory_barriers[0].dst_stage_mask |= dst_stages & !(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    self.pending_memory_barriers[0].src_stage_mask |= src_stages & !(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    self.pending_memory_barriers[0].src_access_mask |=
                        barrier_access_to_access(*old_access) & !(vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);
                    self.pending_memory_barriers[0].dst_access_mask |=
                        dst_access & !(vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);

                    self.pending_memory_barriers[1].dst_access_mask |=
                        dst_access & (vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR | vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);

                    if !self.pending_memory_barriers[1].dst_access_mask.is_empty() {
                        /*
                        VUID-VkMemoryBarrier2-dstAccessMask-06256
                        If the rayQuery feature is not enabled and dstAccessMask includes VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR,
                        dstStageMask must not include any of the VK_PIPELINESTAGE*_SHADER_BIT stages except VK_PIPELINE_STAGE_2_RAY_TRACING_SHADER_BIT_KHR

                        So we need to handle RT barriers separately.
                        */
                        self.pending_memory_barriers[1].src_stage_mask = src_stages & (vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                        self.pending_memory_barriers[1].src_access_mask = src_access & (vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR);
                        self.pending_memory_barriers[1].dst_stage_mask = dst_stages & (vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    }
                }
            }
        }
    }

    unsafe fn begin_render_pass(
        &mut self,
        renderpass_begin_info: &RenderPassBeginInfo<VkBackend>,
        recording_mode: RenderpassRecordingMode,
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());

        self.flush_barriers();

        let mut attachment_infos = SmallVec::<[VkAttachmentInfo; 16]>::with_capacity(
            renderpass_begin_info.attachments.len(),
        );
        let mut width = 0u32;
        let mut height = 0u32;
        let mut attachment_views = SmallVec::<[&Arc<VkTextureView>; 8]>::with_capacity(
            renderpass_begin_info.attachments.len(),
        );
        let mut clear_values =
            SmallVec::<[vk::ClearValue; 8]>::with_capacity(renderpass_begin_info.attachments.len());

        for attachment in renderpass_begin_info.attachments {
            let view = match &attachment.view {
                sourcerenderer_core::graphics::RenderPassAttachmentView::RenderTarget(view) => {
                    *view
                }
                sourcerenderer_core::graphics::RenderPassAttachmentView::DepthStencil(view) => {
                    *view
                }
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
                framebuffer: *framebuffer.handle(),
                render_pass: *renderpass.handle(),
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
        self.trackers.track_frame_buffer(&framebuffer);
        self.trackers.track_render_pass(&renderpass);
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

    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        info: &BottomLevelAccelerationStructureInfo<VkBackend>,
        size: usize,
        target_buffer: &VkBuffer,
        scratch_buffer: &VkBuffer,
    ) -> VkAccelerationStructure {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());
        self.trackers.track_buffer(scratch_buffer);
        self.trackers.track_buffer(target_buffer);
        self.trackers.track_buffer(info.vertex_buffer);
        self.trackers.track_buffer(info.index_buffer);
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
        info: &sourcerenderer_core::graphics::TopLevelAccelerationStructureInfo<VkBackend>,
        size: usize,
        target_buffer: &VkBuffer,
        scratch_buffer: &VkBuffer,
    ) -> VkAccelerationStructure {
        /*debug_assert_eq!(self.state, VkCommandBufferState::Recording);
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.render_pass.is_none());
        debug_assert!(
            self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::RayTracing
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResource::AccelerationStructure(acceleration_structure.handle()),
        );
        self.trackers
            .track_acceleration_structure(acceleration_structure);
    }

    fn bind_sampling_view_and_sampler_array(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        textures_and_samplers: &[(&VkTextureView, &VkSampler)],
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResource::SampledTextureAndSamplerArray((textures_and_samplers.0.handle(), )),
        );
        for (texture, samplers) in textures_and_samplers {
            self.trackers.track_texture_view(*texture);
            self.trackers.track_sampler(*samplers);
        }
    }

    fn bind_storage_view_array(
        &mut self,
        frequency: BindingFrequency,
        binding: u32,
        textures: &[&Arc<VkTextureView>],
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::StorageTextureArray(textures),
        );
        for texture in textures {
            self.trackers.track_texture_view(*texture);
        }
    }

    fn track_texture_view(&mut self, texture_view: &Arc<VkTextureView>) {
        self.trackers.track_texture_view(texture_view);
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
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
        debug_assert!(!self.has_pending_barrier());
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
                    draw_buffer.buffer().handle(),
                    draw_buffer_offset as u64,
                    count_buffer.buffer().handle(),
                    count_buffer_offset as u64,
                    max_draw_count,
                    stride,
                );
        }
    }

    fn draw_indirect(
        &mut self,
        draw_buffer: &VkBuffer,
        draw_buffer_offset: u32,
        count_buffer: &VkBuffer,
        count_buffer_offset: u32,
        max_draw_count: u32,
        stride: u32,
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_some());
        debug_assert!(self.pipeline.as_ref().unwrap().pipeline_type() == VkPipelineType::Graphics);
        debug_assert!(!self.has_pending_barrier());
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
                    draw_buffer.buffer().handle(),
                    draw_buffer_offset as u64,
                    count_buffer.buffer().handle(),
                    count_buffer_offset as u64,
                    max_draw_count,
                    stride,
                );
        }
    }

    fn clear_storage_texture(
        &mut self,
        texture: &Arc<VkTexture>,
        array_layer: u32,
        mip_level: u32,
        values: [u32; 4],
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(!self.has_pending_barrier());
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
                    *texture.handle(),
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
                    *texture.handle(),
                    vk::ImageLayout::GENERAL,
                    &vk::ClearColorValue { uint32: values },
                    &[range],
                );
            }
        }
    }

    fn clear_storage_buffer(
        &mut self,
        buffer: &Arc<VkBufferSlice>,
        offset: usize,
        length_in_u32s: usize,
        value: u32,
    ) {
        debug_assert_eq!(self.state, VkCommandBufferState::Recording);
        debug_assert!(!self.has_pending_barrier());
        debug_assert!(self.render_pass.is_none());

        let actual_length_in_u32s = if length_in_u32s == WHOLE_BUFFER {
            debug_assert_eq!((buffer.length() - offset) % 4, 0);
            (buffer.length() - offset) / 4
        } else {
            length_in_u32s
        };
        let length_in_bytes = actual_length_in_u32s * 4;
        debug_assert!(buffer.length() - offset >= length_in_bytes);

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
        let binding_offsets = [(buffer.offset() + offset) as u32];
        let is_dynamic_binding = meta_pipeline
            .layout()
            .descriptor_set_layout(0)
            .unwrap()
            .is_dynamic_binding(0);
        bindings[0] = VkBoundResourceRef::StorageBuffer(VkBufferBindingInfoRef {
            buffer,
            offset,
            length: length_in_bytes,
        });
        let descriptor_set = self
            .descriptor_manager
            .get_or_create_set(
                self.frame,
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
                *meta_pipeline.handle(),
            );

            self.device.cmd_push_constants(
                self.buffer,
                *meta_pipeline.layout().handle(),
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
                *meta_pipeline.layout().handle(),
                0,
                &[*descriptor_set.handle()],
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

const WRITE_ACCESS_MASK: vk::AccessFlags2 = vk::AccessFlags2::from_raw(
    vk::AccessFlags2::HOST_WRITE.as_raw()
        | vk::AccessFlags2::MEMORY_WRITE.as_raw()
        | vk::AccessFlags2::SHADER_WRITE.as_raw()
        | vk::AccessFlags2::TRANSFER_WRITE.as_raw()
        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw()
        | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw(),
);
