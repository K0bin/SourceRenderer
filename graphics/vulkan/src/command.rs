use std::{
    cmp::min,
    ffi::{c_void, CString},
    hash::Hash,
    sync::Arc,
};

use ash::vk;
use crossbeam_utils::atomic::AtomicCell;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Buffer as _};

use super::*;

#[allow(clippy::vec_box)]
pub struct VkCommandPool {
    raw: Arc<RawVkCommandPool>,
    shared: Arc<VkShared>,
    flags: gpu::CommandPoolFlags,
    queue_family_index: u32,
    command_pool_type: gpu::CommandPoolType
}

impl VkCommandPool {
    pub(crate) fn new(device: &Arc<RawVkDevice>, queue_family_index: u32, flags: gpu::CommandPoolFlags, shared: &Arc<VkShared>, command_pool_type: gpu::CommandPoolType) -> Self {
        let mut vk_flags = vk::CommandPoolCreateFlags::empty();
        if flags.contains(gpu::CommandPoolFlags::INDIVIDUAL_RESET) {
            vk_flags |= vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER;
        }
        if flags.contains(gpu::CommandPoolFlags::TRANSIENT) {
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
            command_pool_type
        }
    }
}

impl gpu::CommandPool<VkBackend> for VkCommandPool {
    unsafe fn create_command_buffer(
        &mut self
    ) -> VkCommandBuffer {
        let buffer = VkCommandBuffer::new(
            &self.raw.device,
            &self.raw,
            if self.command_pool_type == gpu::CommandPoolType::InnerCommandBuffers {
                gpu::CommandBufferType::Secondary
            } else {
                gpu::CommandBufferType::Primary
            },
            self.queue_family_index,
            self.flags.contains(gpu::CommandPoolFlags::INDIVIDUAL_RESET),
            &self.shared
        );
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
pub(crate) enum VkCommandBufferState {
    Ready,
    Recording,
    Finished,
    Submitted,
}

pub struct VkSecondaryCommandBufferInheritance {
    pub(crate) rt_formats: SmallVec<[vk::Format; 8]>,
    pub(crate) depth_format: vk::Format,
    pub(crate) stencil_format: vk::Format,
    pub(crate) sample_count: vk::SampleCountFlags,
    pub(crate) query_pool: Option<vk::QueryPool>,
}

enum VkBoundPipeline {
    Graphics {
        pipeline_layout: Arc<VkPipelineLayout>,
        uses_bindless: bool
    },
    MeshGraphics {
        pipeline_layout: Arc<VkPipelineLayout>,
        uses_bindless: bool
    },
    Compute {
        pipeline_layout: Arc<VkPipelineLayout>,
        uses_bindless: bool
    },
    RayTracing {
        pipeline_layout: Arc<VkPipelineLayout>,
        raygen_sbt_region: vk::StridedDeviceAddressRegionKHR,
        closest_hit_sbt_region: vk::StridedDeviceAddressRegionKHR,
        miss_sbt_region: vk::StridedDeviceAddressRegionKHR,
        uses_bindless: bool
    },
    None,
}

impl VkBoundPipeline {
    #[inline(always)]
    fn is_graphics(&self) -> bool {
        if let VkBoundPipeline::Graphics {..} = self {
            true
        } else { false }
    }
    #[inline(always)]
    fn is_mesh_graphics(&self) -> bool {
        if let VkBoundPipeline::MeshGraphics {..} = self {
            true
        } else { false }
    }
    #[inline(always)]
    fn is_compute(&self) -> bool {
        if let VkBoundPipeline::Compute {..} = self {
            true
        } else { false }
    }
    #[allow(unused)]
    #[inline(always)]
    fn is_ray_tracing(&self) -> bool {
        if let VkBoundPipeline::RayTracing {..} = self {
            true
        } else { false }
    }
    #[allow(unused)]
    #[inline(always)]
    fn is_none(&self) -> bool {
        if let VkBoundPipeline::None = self {
            true
        } else { false }
    }
}

pub struct VkCommandBuffer {
    cmd_buffer: vk::CommandBuffer,
    _pool: Arc<RawVkCommandPool>,
    device: Arc<RawVkDevice>,
    state: AtomicCell<VkCommandBufferState>,
    command_buffer_type: gpu::CommandBufferType,
    shared: Arc<VkShared>,
    pipeline: VkBoundPipeline,
    descriptor_manager: VkBindingManager,
    frame: u64,
    reset_individually: bool,
    is_in_render_pass: bool,
    query_pool: Option<vk::QueryPool>,
}

impl VkCommandBuffer {
    pub(crate) fn new(
        device: &Arc<RawVkDevice>,
        pool: &Arc<RawVkCommandPool>,
        command_buffer_type: gpu::CommandBufferType,
        _queue_family_index: u32,
        reset_individually: bool,
        shared: &Arc<VkShared>,
    ) -> Self {
        let buffers_create_info = vk::CommandBufferAllocateInfo {
            command_pool: ***pool,
            level: if command_buffer_type == gpu::CommandBufferType::Primary {
                vk::CommandBufferLevel::PRIMARY
            } else {
                vk::CommandBufferLevel::SECONDARY
            },
            command_buffer_count: 1, // TODO: figure out how many buffers per pool (maybe create a new pool once we've run out?)
            ..Default::default()
        };
        let mut buffers = unsafe { device.allocate_command_buffers(&buffers_create_info) }.unwrap();
        VkCommandBuffer {
            cmd_buffer: buffers.pop().unwrap(),
            _pool: pool.clone(),
            device: device.clone(),
            command_buffer_type,
            pipeline: VkBoundPipeline::None,
            shared: shared.clone(),
            state: AtomicCell::new(VkCommandBufferState::Ready),
            descriptor_manager: VkBindingManager::new(device),
            frame: 0u64,
            reset_individually,
            is_in_render_pass: false,
            query_pool: None,
        }
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> vk::CommandBuffer {
        self.cmd_buffer
    }

    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn cmd_buffer_type(&self) -> gpu::CommandBufferType {
        self.command_buffer_type
    }

    #[inline(always)]
    pub(crate) fn mark_submitted(&self) {
        assert_eq!(self.state.swap(VkCommandBufferState::Submitted), VkCommandBufferState::Finished);
    }

    #[inline(always)]
    pub(crate) fn command_buffer_type(&self) -> gpu::CommandBufferType {
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

impl gpu::CommandBuffer<VkBackend> for VkCommandBuffer {
    type CommandBufferInheritance = VkSecondaryCommandBufferInheritance;

    unsafe fn set_pipeline(&mut self, pipeline: gpu::PipelineBinding<VkBackend>) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);

        match &pipeline {
            gpu::PipelineBinding::Graphics(graphics_pipeline) => {
                let vk_pipeline = graphics_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.cmd_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        vk_pipeline,
                    );
                }

                self.pipeline = VkBoundPipeline::Graphics {
                    pipeline_layout: graphics_pipeline.layout().clone(),
                    uses_bindless: graphics_pipeline.uses_bindless_texture_set()
                };

                if graphics_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features_12
                        .descriptor_indexing == vk::TRUE
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }
            }
            gpu::PipelineBinding::MeshGraphics(graphics_pipeline) => {
                let vk_pipeline = graphics_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.cmd_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        vk_pipeline,
                    );
                }

                self.pipeline = VkBoundPipeline::MeshGraphics {
                    pipeline_layout: graphics_pipeline.layout().clone(),
                    uses_bindless: graphics_pipeline.uses_bindless_texture_set()
                };

                if graphics_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features_12
                        .descriptor_indexing == vk::TRUE
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }
            }
            gpu::PipelineBinding::Compute(compute_pipeline) => {
                let vk_pipeline = compute_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.cmd_buffer,
                        vk::PipelineBindPoint::COMPUTE,
                        vk_pipeline,
                    );
                }

                self.pipeline = VkBoundPipeline::Compute {
                    pipeline_layout: compute_pipeline.layout().clone(),
                    uses_bindless: compute_pipeline.uses_bindless_texture_set()
                };

                if compute_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features_12
                        .descriptor_indexing == vk::TRUE
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }
            }
            gpu::PipelineBinding::RayTracing(rt_pipeline) => {
                let vk_pipeline = rt_pipeline.handle();
                unsafe {
                    self.device.cmd_bind_pipeline(
                        self.cmd_buffer,
                        vk::PipelineBindPoint::RAY_TRACING_KHR,
                        vk_pipeline,
                    );
                }

                self.pipeline = VkBoundPipeline::RayTracing {
                    pipeline_layout: rt_pipeline.layout().clone(),
                    miss_sbt_region: rt_pipeline.miss_sbt_region().clone(),
                    closest_hit_sbt_region: rt_pipeline.closest_hit_sbt_region().clone(),
                    raygen_sbt_region: rt_pipeline.raygen_sbt_region().clone(),
                    uses_bindless: rt_pipeline.uses_bindless_texture_set()
                };

                if rt_pipeline.uses_bindless_texture_set()
                    && !self
                        .device
                        .features_12
                        .descriptor_indexing == vk::TRUE
                {
                    panic!("Tried to use pipeline which uses bindless texture descriptor set. The current Vulkan device does not support this.");
                }
            }
        };
        self.descriptor_manager.mark_all_dirty();
    }

    unsafe fn end_render_pass(&mut self) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        self.pipeline = VkBoundPipeline::None;
        unsafe {
            self.device.cmd_end_rendering(self.cmd_buffer);
        }
        self.is_in_render_pass = false;
    }

    unsafe fn set_vertex_buffer(&mut self, index: u32, vertex_buffer: &VkBuffer, offset: u64) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        unsafe {
            self.device.cmd_bind_vertex_buffers(
                self.cmd_buffer,
                index,
                &[vertex_buffer.handle()],
                &[offset as u64],
            );
        }
    }

    unsafe fn set_index_buffer(
        &mut self,
        index_buffer: &VkBuffer,
        offset: u64,
        format: gpu::IndexFormat,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        unsafe {
            self.device.cmd_bind_index_buffer(
                self.cmd_buffer,
                index_buffer.handle(),
                offset,
                index_format_to_vk(format),
            );
        }
    }

    unsafe fn set_viewports(&mut self, viewports: &[gpu::Viewport]) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        unsafe {
            for (i, viewport) in viewports.iter().enumerate() {
                self.device.cmd_set_viewport(
                    self.cmd_buffer,
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

    unsafe fn set_scissors(&mut self, scissors: &[gpu::Scissor]) {
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
            self.device.cmd_set_scissor(self.cmd_buffer, 0, &vk_scissors);
        }
    }

    unsafe fn draw(&mut self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        unsafe {
            self.device.cmd_draw(self.cmd_buffer, vertex_count, instance_count, first_vertex, first_instance);
        }
    }

    unsafe fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        unsafe {
            self.device.cmd_draw_indexed(
                self.cmd_buffer,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    unsafe fn bind_sampling_view(
        &mut self,
        frequency: gpu::BindingFrequency,
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
        frequency: gpu::BindingFrequency,
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
        frequency: gpu::BindingFrequency,
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
                length: if length == gpu::WHOLE_BUFFER {
                    buffer.info().size - offset
                } else {
                    length
                },
            }),
        );
    }

    unsafe fn bind_storage_buffer(
        &mut self,
        frequency: gpu::BindingFrequency,
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
                length: if length == gpu::WHOLE_BUFFER {
                    buffer.info().size - offset
                } else {
                    length
                },
            }),
        );
    }

    unsafe fn bind_storage_texture(
        &mut self,
        frequency: gpu::BindingFrequency,
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
        frequency: gpu::BindingFrequency,
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

        let mut offsets = SmallVec::<[u32; 16]>::new();
        let mut descriptor_sets =
            SmallVec::<[vk::DescriptorSet; gpu::TOTAL_SET_COUNT as usize]>::new();
        let mut base_index = 0;

        let (pipeline_layout, bind_point, uses_bindless) = match &self.pipeline {
            VkBoundPipeline::Graphics { pipeline_layout, uses_bindless, .. } => (pipeline_layout, vk::PipelineBindPoint::GRAPHICS, *uses_bindless),
            VkBoundPipeline::MeshGraphics { pipeline_layout, uses_bindless, .. } => (pipeline_layout, vk::PipelineBindPoint::GRAPHICS, *uses_bindless),
            VkBoundPipeline::Compute { pipeline_layout, uses_bindless, .. } => (pipeline_layout, vk::PipelineBindPoint::COMPUTE, *uses_bindless),
            VkBoundPipeline::RayTracing { pipeline_layout, uses_bindless, .. } => (pipeline_layout, vk::PipelineBindPoint::RAY_TRACING_KHR, *uses_bindless),
            VkBoundPipeline::None => panic!("finish_binding must not be called without a bound pipeline.")
        };

        let finished_sets = self.descriptor_manager.finish(self.frame, pipeline_layout);
        for (index, set_option) in finished_sets.iter().enumerate() {
            match set_option {
                None => {
                    if !descriptor_sets.is_empty() {
                        unsafe {
                            self.device.cmd_bind_descriptor_sets(
                                self.cmd_buffer,
                                bind_point,
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
                    for offset in &set_binding.dynamic_offsets {
                        offsets.push(*offset as u32);
                    }
                }
            }
        }

        if !descriptor_sets.is_empty()
            && base_index + descriptor_sets.len() as u32 != gpu::NON_BINDLESS_SET_COUNT
        {
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    self.cmd_buffer,
                    bind_point,
                    pipeline_layout.handle(),
                    base_index,
                    &descriptor_sets,
                    &offsets,
                );
            }
            offsets.clear();
            descriptor_sets.clear();
            base_index = gpu::BINDLESS_TEXTURE_SET_INDEX;
        }

        if uses_bindless {
            let bindless_texture_descriptor_set =
                self.shared.bindless_texture_descriptor_set().expect("Shader requires support for bindless resources which device does not support.");
            descriptor_sets.push(bindless_texture_descriptor_set.descriptor_set_handle());
        }

        if !descriptor_sets.is_empty() {
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    self.cmd_buffer,
                    bind_point,
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
        if let Some(debug_utils) = self.device.debug_utils.as_ref() {
            debug_utils.cmd_begin_debug_utils_label(
                self.cmd_buffer,
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
        if let Some(debug_utils) = self.device.debug_utils.as_ref() {
            debug_utils
                .cmd_end_debug_utils_label(self.cmd_buffer);
        }
    }

    unsafe fn execute_inner(&mut self, submissions: &[&VkCommandBuffer], _inheritance: Self::CommandBufferInheritance) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        if submissions.is_empty() {
            return;
        }

        for submission in submissions.iter() {
            assert_eq!(
                submission.command_buffer_type(),
                gpu::CommandBufferType::Secondary
            );
        }
        let submission_handles: SmallVec<[vk::CommandBuffer; 16]> =
            submissions.iter().map(|s| s.handle()).collect();
        unsafe {
            self.device
                .cmd_execute_commands(self.cmd_buffer, &submission_handles);
        }
        for submission in submissions {
            submission.mark_submitted();
        }
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(!self.is_in_render_pass);
        debug_assert!(self.pipeline.is_compute());
        unsafe {
            self.device
                .cmd_dispatch(self.cmd_buffer, group_count_x, group_count_y, group_count_z);
        }
    }

    unsafe fn dispatch_indirect(&mut self, buffer: &VkBuffer, offset: u64) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(!self.is_in_render_pass);
        debug_assert!(self.pipeline.is_compute());
        unsafe {
            self.device
                .cmd_dispatch_indirect(self.cmd_buffer, buffer.handle(), offset);
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
        debug_assert!(!self.is_in_render_pass);
        let src_info = src_texture.info();
        let dst_info = dst_texture.info();

        unsafe {
            self.device.cmd_blit_image(
                self.cmd_buffer,
                src_texture.handle(),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_texture.handle(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit {
                    src_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: aspect_mask_from_format(src_info.format),
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
                        aspect_mask: aspect_mask_from_format(dst_info.format),
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

    unsafe fn barrier(&mut self, barriers: &[gpu::Barrier<VkBackend>]) {
        let mut pending_image_barriers =
            SmallVec::<[vk::ImageMemoryBarrier2; 4]>::with_capacity(barriers.len());
        let mut pending_buffer_barriers =
            SmallVec::<[vk::BufferMemoryBarrier2; 4]>::with_capacity(barriers.len());
        let mut pending_memory_barriers = <[vk::MemoryBarrier2; 2]>::default();

        for barrier in barriers {
            match barrier {
                gpu::Barrier::TextureBarrier {
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
                            gpu::QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            gpu::QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            gpu::QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                        };
                        dst_queue_family_index = match queue_transfer.to {
                            gpu::QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            gpu::QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            gpu::QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
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

                    let dst_stages = barrier_sync_to_stage(*new_sync) & self.device.supported_pipeline_stages;
                    let src_stages = barrier_sync_to_stage(*old_sync) & self.device.supported_pipeline_stages;
                    pending_image_barriers.push(vk::ImageMemoryBarrier2 {
                        src_stage_mask: src_stages,
                        dst_stage_mask: dst_stages,
                        src_access_mask: barrier_access_to_access(*old_access) & self.device.supported_access_flags,
                        dst_access_mask: barrier_access_to_access(*new_access) & self.device.supported_access_flags,
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
                gpu::Barrier::BufferBarrier {
                    old_sync,
                    new_sync,
                    old_access,
                    new_access,
                    buffer,
                    offset,
                    length,
                    queue_ownership
                } => {
                    let dst_stages = barrier_sync_to_stage(*new_sync) & self.device.supported_pipeline_stages;
                    let src_stages = barrier_sync_to_stage(*old_sync) & self.device.supported_pipeline_stages;

                    let mut src_queue_family_index: u32 = vk::QUEUE_FAMILY_IGNORED;
                    let mut dst_queue_family_index: u32 = vk::QUEUE_FAMILY_IGNORED;
                    if let Some(queue_transfer) = queue_ownership {
                        src_queue_family_index = match queue_transfer.from {
                            gpu::QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            gpu::QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            gpu::QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                        };
                        dst_queue_family_index = match queue_transfer.to {
                            gpu::QueueType::Graphics => self.device.graphics_queue_info.queue_family_index as u32,
                            gpu::QueueType::Compute => if let Some(info) = self.device.compute_queue_info.as_ref() {
                                info.queue_family_index as u32
                            } else {
                                vk::QUEUE_FAMILY_IGNORED
                            },
                            gpu::QueueType::Transfer => if let Some(info) = self.device.transfer_queue_info.as_ref() {
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
                        src_access_mask: barrier_access_to_access(*old_access) & self.device.supported_access_flags,
                        dst_access_mask: barrier_access_to_access(*new_access) & self.device.supported_access_flags,
                        src_queue_family_index: src_queue_family_index,
                        dst_queue_family_index: dst_queue_family_index,
                        buffer: buffer.handle(),
                        offset: *offset as u64,
                        size: (buffer.info().size - *offset).min(*length),
                        ..Default::default()
                    });
                }
                gpu::Barrier::GlobalBarrier {
                    old_sync,
                    new_sync,
                    old_access,
                    new_access,
                } => {
                    let dst_stages = barrier_sync_to_stage(*new_sync) & self.device.supported_pipeline_stages;
                    let src_stages = barrier_sync_to_stage(*old_sync) & self.device.supported_pipeline_stages;
                    let src_access = barrier_access_to_access(*old_access) & self.device.supported_access_flags;
                    let dst_access = barrier_access_to_access(*new_access) & self.device.supported_access_flags;

                    pending_memory_barriers[0].dst_stage_mask |= dst_stages
                        & !(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                            | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    pending_memory_barriers[0].src_stage_mask |= src_stages
                        & !(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR
                            | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_COPY_KHR);
                    pending_memory_barriers[0].src_access_mask |=
                        src_access
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
                    .cmd_pipeline_barrier2(self.cmd_buffer, &dependency_info);
            }
            pending_memory_barriers[0].src_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[0].dst_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[0].src_access_mask = vk::AccessFlags2::empty();
            pending_memory_barriers[0].dst_access_mask = vk::AccessFlags2::empty();
            pending_memory_barriers[1].src_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[1].dst_stage_mask = vk::PipelineStageFlags2::empty();
            pending_memory_barriers[1].src_access_mask = vk::AccessFlags2::empty();
            pending_memory_barriers[1].dst_access_mask = vk::AccessFlags2::empty();
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
            .cmd_pipeline_barrier2(self.cmd_buffer, &dependency_info);
    }

    unsafe fn begin_render_pass(
        &mut self,
        renderpass_begin_info: &gpu::RenderPassBeginInfo<VkBackend>,
        recording_mode: gpu::RenderpassRecordingMode,
    ) -> Option<Self::CommandBufferInheritance> {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        self.pipeline = VkBoundPipeline::None;

        begin_render_pass(self.device.as_ref(), self.cmd_buffer, renderpass_begin_info, recording_mode);
        self.is_in_render_pass = true;

        if let gpu::RenderpassRecordingMode::CommandBuffers(_) = recording_mode {
            let formats: SmallVec<[vk::Format; 8]> = renderpass_begin_info.render_targets.iter()
                .map(|rt| format_to_vk(rt.view.info().format.unwrap_or(rt.view.texture_info().format), false))
                .collect();

            let (depth_format, stencil_format) = renderpass_begin_info.depth_stencil.map(|dsv| {
                let format = dsv.view.info().format.unwrap_or(dsv.view.texture_info().format);
                let vk_format = format_to_vk(format, self.device.supports_d24);
                let depth_format = if format.is_depth() { vk_format } else { vk::Format::UNDEFINED };
                let stencil_format = if format.is_stencil() { vk_format } else { vk::Format::UNDEFINED };
                (depth_format, stencil_format)
            }).unwrap_or_default();
            let samples = if let Some(rt) = renderpass_begin_info.render_targets.first() {
                samples_to_vk(rt.view.texture_info().samples)
            } else if let Some(dsv) = renderpass_begin_info.depth_stencil.as_ref() {
                samples_to_vk(dsv.view.texture_info().samples)
            } else {
                panic!("Render pass must have either render target or depth stencil attachment")
            };
            Some(VkSecondaryCommandBufferInheritance {
                rt_formats: formats,
                depth_format,
                stencil_format,
                sample_count: samples,
                query_pool: self.query_pool.clone()
            })
        } else {
            None
        }
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        info: &gpu::BottomLevelAccelerationStructureInfo<VkBackend>,
        size: u64,
        target_buffer: &VkBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &VkBuffer,
        scratch_buffer_offset: u64
    ) -> VkAccelerationStructure {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(!self.is_in_render_pass);
        let acceleration_structure = VkAccelerationStructure::new_bottom_level(
            &self.device,
            info,
            size,
            target_buffer,
            target_buffer_offset,
            scratch_buffer,
            scratch_buffer_offset,
            &self.handle(),
        );
        acceleration_structure
    }

    unsafe fn upload_top_level_instances(
        &mut self,
        instances: &[gpu::AccelerationStructureInstance<VkBackend>],
        target_buffer: &VkBuffer,
        target_buffer_offset: u64
    ) {
        VkAccelerationStructure::upload_top_level_instances(target_buffer, target_buffer_offset, instances)
    }

    unsafe fn create_top_level_acceleration_structure(
        &mut self,
        info: &gpu::TopLevelAccelerationStructureInfo<VkBackend>,
        size: u64,
        target_buffer: &VkBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &VkBuffer,
        scratch_buffer_offset: u64
    ) -> VkAccelerationStructure {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(!self.is_in_render_pass);
        let acceleration_structure = VkAccelerationStructure::new_top_level(
            &self.device,
            info,
            size,
            target_buffer,
            target_buffer_offset,
            scratch_buffer,
            scratch_buffer_offset,
            &self.handle(),
        );
        acceleration_structure
    }

    unsafe fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(!self.is_in_render_pass);


        let raygen_sbt_region: &vk::StridedDeviceAddressRegionKHR;
        let miss_sbt_region: &vk::StridedDeviceAddressRegionKHR;
        let closest_hit_sbt_region: &vk::StridedDeviceAddressRegionKHR;

        if let VkBoundPipeline::RayTracing {
            raygen_sbt_region: pipeline_raygen_sbt_region,
            closest_hit_sbt_region: pipeline_closest_hit_sbt_region,
            miss_sbt_region: pipeline_miss_sbt_region,
            ..
        } = &self.pipeline {
            raygen_sbt_region = pipeline_raygen_sbt_region;
            miss_sbt_region = pipeline_miss_sbt_region;
            closest_hit_sbt_region = pipeline_closest_hit_sbt_region;
        } else {
            panic!("No RT pipeline bound.");
        };

        let rt = self.device.rt.as_ref().unwrap();
        let rt_pipelines_device = rt.rt_pipelines.as_ref().unwrap();
        unsafe {
            rt_pipelines_device.cmd_trace_rays(
                self.cmd_buffer,
                raygen_sbt_region,
                miss_sbt_region,
                closest_hit_sbt_region,
                &vk::StridedDeviceAddressRegionKHR::default(),
                width,
                height,
                depth,
            );
        }
    }

    unsafe fn bind_acceleration_structure(
        &mut self,
        frequency: gpu::BindingFrequency,
        binding: u32,
        acceleration_structure: &VkAccelerationStructure,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        self.descriptor_manager.bind(
            frequency,
            binding,
            VkBoundResourceRef::AccelerationStructure(acceleration_structure.handle()),
        );
    }

    unsafe fn bind_sampling_view_and_sampler_array(
        &mut self,
        frequency: gpu::BindingFrequency,
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
        frequency: gpu::BindingFrequency,
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
        draw_buffer_offset: u64,
        draw_count: u32,
        stride: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        unsafe {
            if self.device.features.multi_draw_indirect == vk::TRUE {
                self.device.cmd_draw_indexed_indirect(
                    self.cmd_buffer,
                    draw_buffer.handle(),
                    draw_buffer_offset as u64,
                    draw_count,
                    stride,
                );
            } else {
                for i in 0..(draw_count as u64) {
                    self.device.cmd_draw_indexed_indirect(
                        self.cmd_buffer,
                        draw_buffer.handle(),
                        draw_buffer_offset + (stride as u64) * i,
                        1,
                        stride,
                    );
                }
            }
        }
    }

    unsafe fn draw_indexed_indirect_count(
        &mut self,
        draw_buffer: &VkBuffer,
        draw_buffer_offset: u64,
        count_buffer: &VkBuffer,
        count_buffer_offset: u64,
        max_draw_count: u32,
        stride: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        debug_assert!(self.device.features_12.draw_indirect_count == vk::TRUE);
        unsafe {
            self.device.cmd_draw_indexed_indirect_count(
                self.cmd_buffer,
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
        draw_buffer_offset: u64,
        draw_count: u32,
        stride: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        unsafe {
            if self.device.features.multi_draw_indirect == vk::TRUE {
                self.device.cmd_draw_indirect(
                    self.cmd_buffer,
                    draw_buffer.handle(),
                    draw_buffer_offset as u64,
                    draw_count,
                    stride,
                );
            } else {
                for i in 0..(draw_count as u64) {
                    self.device.cmd_draw_indirect(
                        self.cmd_buffer,
                        draw_buffer.handle(),
                        draw_buffer_offset + (stride as u64) * i,
                        1,
                        stride,
                    );
                }
            }
        }
    }

    unsafe fn draw_indirect_count(
        &mut self,
        draw_buffer: &VkBuffer,
        draw_buffer_offset: u64,
        count_buffer: &VkBuffer,
        count_buffer_offset: u64,
        max_draw_count: u32,
        stride: u32,
    ) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        debug_assert!(self.device.features_12.draw_indirect_count == vk::TRUE);
        unsafe {
            self.device.cmd_draw_indirect_count(
                self.cmd_buffer,
                draw_buffer.handle(),
                draw_buffer_offset,
                count_buffer.handle(),
                count_buffer_offset,
                max_draw_count,
                stride,
            );
        }
    }

    unsafe fn draw_mesh_tasks(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_mesh_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        let mesh_shader_device = &self.device.mesh_shader.as_ref().unwrap().mesh_shader;
        mesh_shader_device.cmd_draw_mesh_tasks(self.cmd_buffer, group_count_x, group_count_y, group_count_z);
    }

    unsafe fn draw_mesh_tasks_indirect(&mut self, draw_buffer: &VkBuffer, draw_buffer_offset: u64, draw_count: u32, stride: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_mesh_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        let mesh_shader_device = &self.device.mesh_shader.as_ref().unwrap().mesh_shader;
        unsafe {
            if self.device.features.multi_draw_indirect == vk::TRUE {
                mesh_shader_device.cmd_draw_mesh_tasks_indirect(
                    self.cmd_buffer,
                    draw_buffer.handle(),
                    draw_buffer_offset as u64,
                    draw_count,
                    stride,
                );
            } else {
                for i in 0..(draw_count as u64) {
                    mesh_shader_device.cmd_draw_mesh_tasks_indirect(
                        self.cmd_buffer,
                        draw_buffer.handle(),
                        draw_buffer_offset + (stride as u64) * i,
                        1,
                        stride,
                    );
                }
            }
        }
    }

    unsafe fn draw_mesh_tasks_indirect_count(&mut self, draw_buffer: &VkBuffer, draw_buffer_offset: u64, count_buffer: &VkBuffer, count_buffer_offset: u64, max_draw_count: u32, stride: u32) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        debug_assert!(self.pipeline.is_mesh_graphics());
        debug_assert!(
            self.is_in_render_pass || self.command_buffer_type == gpu::CommandBufferType::Secondary
        );
        debug_assert!(self.device.features_12.draw_indirect_count == vk::TRUE);
        let mesh_shader_device = &self.device.mesh_shader.as_ref().unwrap().mesh_shader;
        unsafe {
            mesh_shader_device.cmd_draw_mesh_tasks_indirect_count(
                self.cmd_buffer,
                draw_buffer.handle(),
                draw_buffer_offset,
                count_buffer.handle(),
                count_buffer_offset,
                max_draw_count,
                stride,
            );
        }
    }

    unsafe fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_type: gpu::ShaderType)
    where
        T: 'static + Send + Sync + Sized + Clone,
    {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let pipeline_layout = match &self.pipeline {
            VkBoundPipeline::Graphics { pipeline_layout, .. } => pipeline_layout,
            VkBoundPipeline::MeshGraphics { pipeline_layout, .. } => pipeline_layout,
            VkBoundPipeline::Compute { pipeline_layout, .. } => pipeline_layout,
            VkBoundPipeline::RayTracing { pipeline_layout, .. } => pipeline_layout,
            VkBoundPipeline::None => panic!("Must not call set_push_constant_data without any pipeline bound"),
        };
        let range = pipeline_layout
            .push_constant_range(visible_for_shader_type)
            .expect("No push constants set up for shader");
        let size = std::mem::size_of_val(data);
        unsafe {
            self.device.cmd_push_constants(
                self.cmd_buffer,
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
        debug_assert!(!self.is_in_render_pass);

        let aspect_mask = aspect_mask_from_format(texture.info().format);

        let range = vk::ImageSubresourceRange {
            aspect_mask,
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
                    self.cmd_buffer,
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
                    self.cmd_buffer,
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
        debug_assert!(!self.is_in_render_pass);

        let actual_length_in_u32s = if length_in_u32s == gpu::WHOLE_BUFFER {
            debug_assert_eq!((buffer.info().size - offset) % 4, 0);
            (buffer.info().size - offset) / 4
        } else {
            length_in_u32s
        };
        let length_in_bytes = actual_length_in_u32s * 4;
        debug_assert!(buffer.info().size - offset >= length_in_bytes);

        #[allow(unused)]
        #[repr(packed)]
        struct MetaClearShaderData {
            length: u32,
            value: u32,
        }
        let push_data = MetaClearShaderData {
            length: length_in_bytes as u32,
            value: value,
        };

        let meta_pipeline = self.shared.get_clear_buffer_meta_pipeline();
        let binding_offsets = [offset as u32];
        let is_dynamic_binding = meta_pipeline
            .layout()
            .descriptor_set_layout(0)
            .unwrap()
            .is_dynamic_binding(0);
        let descriptor_set = self
            .descriptor_manager
            .get_or_create_set(
                0,
                meta_pipeline
                    .layout()
                    .descriptor_set_layout(0)
                    .as_ref()
                    .unwrap(),
                &[VkBoundResourceRef::StorageBuffer(VkBufferBindingInfo {
                    buffer: buffer.handle(),
                    offset,
                    length: length_in_bytes,
                })],
            )
            .unwrap();
        unsafe {
            self.device.cmd_bind_pipeline(
                self.cmd_buffer,
                vk::PipelineBindPoint::COMPUTE,
                meta_pipeline.handle(),
            );

            self.device.cmd_push_constants(
                self.cmd_buffer,
                meta_pipeline.layout().handle(),
                vk::ShaderStageFlags::COMPUTE,
                0,
                std::slice::from_raw_parts(
                    std::mem::transmute(&push_data as *const MetaClearShaderData),
                    std::mem::size_of::<MetaClearShaderData>(),
                ),
            );
            self.device.cmd_bind_descriptor_sets(
                self.cmd_buffer,
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
                .cmd_dispatch(self.cmd_buffer, (actual_length_in_u32s as u32 + 63) / 64, 1, 1);
        }
        self.descriptor_manager.mark_all_dirty();
    }

    unsafe fn begin(&mut self, frame: u64, inner_info: Option<&VkSecondaryCommandBufferInheritance>) {
        assert_eq!(self.state.load(), VkCommandBufferState::Ready);

        self.descriptor_manager.mark_all_dirty();
        self.state.store(VkCommandBufferState::Recording);
        self.frame = frame;

        let (flags, rendering_inhertiance_info) = if let Some(inner_info) = inner_info {
            self.query_pool = inner_info.query_pool.clone();
            (
                vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT
                    | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE,
                vk::CommandBufferInheritanceRenderingInfo {
                    color_attachment_count: inner_info.rt_formats.len() as u32,
                    p_color_attachment_formats: inner_info.rt_formats.as_ptr(),
                    depth_attachment_format: inner_info.depth_format,
                    stencil_attachment_format: inner_info.stencil_format,
                    rasterization_samples: inner_info.sample_count,
                    ..Default::default()
                },
            )
        } else {
            (
                vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                Default::default(),
            )
        };

        let inheritance_info = vk::CommandBufferInheritanceInfo {
            s_type: vk::StructureType::COMMAND_BUFFER_INHERITANCE_INFO,
            p_next: &rendering_inhertiance_info as *const vk::CommandBufferInheritanceRenderingInfo as *const c_void,
            render_pass: vk::RenderPass::null(),
            subpass: 0,
            framebuffer: vk::Framebuffer::null(),
            occlusion_query_enable: 0,
            query_flags: vk::QueryControlFlags::empty(),
            pipeline_statistics: vk::QueryPipelineStatisticFlags::empty(),
            ..Default::default()
        };

        unsafe {
            self.device
                .begin_command_buffer(
                    self.cmd_buffer,
                    &vk::CommandBufferBeginInfo {
                        flags,
                        p_inheritance_info: &inheritance_info,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
    }

    unsafe fn copy_buffer(&mut self, src: &VkBuffer, dst: &VkBuffer, region: &gpu::BufferCopyRegion) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        let copy = vk::BufferCopy {
            src_offset: region.src_offset,
            dst_offset: region.dst_offset,
            size: region.size
                .min(src.info().size - region.src_offset)
                .min(dst.info().size - region.dst_offset)
        };
        self.device.cmd_copy_buffer(self.cmd_buffer, src.handle(), dst.handle(), &[copy]);
    }

    unsafe fn copy_buffer_to_texture(&mut self, src: &VkBuffer, dst: &VkTexture, region: &gpu::BufferTextureCopyRegion) {
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
        self.device.cmd_copy_buffer_to_image(self.cmd_buffer, src.handle(), dst.handle(), vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[copy]);
    }

    unsafe fn finish(&mut self) {
        debug_assert_eq!(self.state.load(), VkCommandBufferState::Recording);
        if self.is_in_render_pass {
            self.end_render_pass();
        }

        self.state.store(VkCommandBufferState::Finished);
        self.device.end_command_buffer(self.cmd_buffer).unwrap();
    }

    unsafe fn reset(&mut self, frame: u64) {
        if self.reset_individually {
            self.device.reset_command_buffer(self.cmd_buffer, vk::CommandBufferResetFlags::empty()).unwrap();
        }
        self.descriptor_manager.reset(frame);
        self.state.store(VkCommandBufferState::Ready);
    }

    unsafe fn begin_query(&mut self, index: u32) {
        self.device.cmd_begin_query(self.cmd_buffer, self.query_pool.unwrap(), index, vk::QueryControlFlags::empty());
    }

    unsafe fn end_query(&mut self, index: u32) {
        self.device.cmd_end_query(self.cmd_buffer, self.query_pool.unwrap(), index);
    }

    unsafe fn copy_query_results_to_buffer(&mut self, query_pool: &VkQueryPool, start_index: u32, count: u32, buffer: &VkBuffer, buffer_offset: u64) {
        self.device.cmd_copy_query_pool_results(self.cmd_buffer, query_pool.handle(), start_index, count,
            buffer.handle(), buffer_offset as u64, std::mem::size_of::<u32>() as u64, vk::QueryResultFlags::WAIT | vk::QueryResultFlags::TYPE_64);
    }
}

pub(super) fn barrier_sync_to_stage(sync: gpu::BarrierSync) -> vk::PipelineStageFlags2 {
    let mut stages = vk::PipelineStageFlags2::NONE;
    if sync.contains(gpu::BarrierSync::COMPUTE_SHADER) {
        stages |= vk::PipelineStageFlags2::COMPUTE_SHADER;
    }
    if sync.contains(gpu::BarrierSync::COPY) {
        stages |= vk::PipelineStageFlags2::TRANSFER;
    }
    if sync.contains(gpu::BarrierSync::EARLY_DEPTH) {
        stages |= vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS;
    }
    if sync.contains(gpu::BarrierSync::FRAGMENT_SHADER) {
        stages |= vk::PipelineStageFlags2::FRAGMENT_SHADER;
    }
    if sync.contains(gpu::BarrierSync::INDIRECT) {
        stages |= vk::PipelineStageFlags2::DRAW_INDIRECT;
    }
    if sync.contains(gpu::BarrierSync::LATE_DEPTH) {
        stages |= vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
    }
    if sync.contains(gpu::BarrierSync::RENDER_TARGET) {
        stages |= vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
    }
    if sync.contains(gpu::BarrierSync::RESOLVE) {
        stages |= vk::PipelineStageFlags2::RESOLVE;
    }
    if sync.contains(gpu::BarrierSync::VERTEX_INPUT) {
        stages |= vk::PipelineStageFlags2::VERTEX_ATTRIBUTE_INPUT;
    }
    if sync.contains(gpu::BarrierSync::INDEX_INPUT) {
        stages |= vk::PipelineStageFlags2::INDEX_INPUT;
    }
    if sync.contains(gpu::BarrierSync::VERTEX_SHADER) {
        stages |= vk::PipelineStageFlags2::VERTEX_SHADER;
    }
    if sync.contains(gpu::BarrierSync::HOST) {
        stages |= vk::PipelineStageFlags2::HOST;
    }
    if sync.contains(gpu::BarrierSync::ACCELERATION_STRUCTURE_BUILD) {
        stages |= vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR;
    }
    if sync.contains(gpu::BarrierSync::RAY_TRACING) {
        stages |= vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR;
    }
    stages
}

fn barrier_access_to_access(access: gpu::BarrierAccess) -> vk::AccessFlags2 {
    let mut vk_access = vk::AccessFlags2::empty();
    if access.contains(gpu::BarrierAccess::INDEX_READ) {
        vk_access |= vk::AccessFlags2::INDEX_READ;
    }
    if access.contains(gpu::BarrierAccess::INDIRECT_READ) {
        vk_access |= vk::AccessFlags2::INDIRECT_COMMAND_READ;
    }
    if access.contains(gpu::BarrierAccess::VERTEX_INPUT_READ) {
        vk_access |= vk::AccessFlags2::VERTEX_ATTRIBUTE_READ;
    }
    if access.contains(gpu::BarrierAccess::CONSTANT_READ) {
        vk_access |= vk::AccessFlags2::UNIFORM_READ;
    }
    if access.intersects(gpu::BarrierAccess::SAMPLING_READ) {
        vk_access |= vk::AccessFlags2::SHADER_SAMPLED_READ;
    }
    if access.intersects(gpu::BarrierAccess::STORAGE_READ) {
        vk_access |= vk::AccessFlags2::SHADER_STORAGE_READ;
    }
    if access.contains(gpu::BarrierAccess::STORAGE_WRITE) {
        vk_access |= vk::AccessFlags2::SHADER_STORAGE_WRITE;
    }
    if access.contains(gpu::BarrierAccess::COPY_READ) {
        vk_access |= vk::AccessFlags2::TRANSFER_READ;
    }
    if access.contains(gpu::BarrierAccess::COPY_WRITE) {
        vk_access |= vk::AccessFlags2::TRANSFER_WRITE;
    }
    if access.contains(gpu::BarrierAccess::RESOLVE_READ) {
        vk_access |= vk::AccessFlags2::TRANSFER_READ;
        // TODO: sync2
    }
    if access.contains(gpu::BarrierAccess::RESOLVE_WRITE) {
        vk_access |= vk::AccessFlags2::TRANSFER_WRITE;
    }
    if access.contains(gpu::BarrierAccess::DEPTH_STENCIL_READ) {
        vk_access |= vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;
    }
    if access.contains(gpu::BarrierAccess::DEPTH_STENCIL_WRITE) {
        vk_access |= vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE;
    }
    if access.contains(gpu::BarrierAccess::RENDER_TARGET_READ) {
        vk_access |= vk::AccessFlags2::COLOR_ATTACHMENT_READ;
    }
    if access.contains(gpu::BarrierAccess::RENDER_TARGET_WRITE) {
        vk_access |= vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
    }
    if access.contains(gpu::BarrierAccess::SHADER_READ) {
        vk_access |= vk::AccessFlags2::SHADER_READ;
    }
    if access.contains(gpu::BarrierAccess::SHADER_WRITE) {
        vk_access |= vk::AccessFlags2::SHADER_WRITE;
    }
    if access.contains(gpu::BarrierAccess::MEMORY_READ) {
        vk_access |= vk::AccessFlags2::MEMORY_READ;
    }
    if access.contains(gpu::BarrierAccess::MEMORY_WRITE) {
        vk_access |= vk::AccessFlags2::MEMORY_WRITE;
    }
    if access.contains(gpu::BarrierAccess::HOST_READ) {
        vk_access |= vk::AccessFlags2::HOST_READ;
    }
    if access.contains(gpu::BarrierAccess::HOST_WRITE) {
        vk_access |= vk::AccessFlags2::HOST_WRITE;
    }
    if access.contains(gpu::BarrierAccess::ACCELERATION_STRUCTURE_READ) {
        vk_access |= vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR;
    }
    if access.contains(gpu::BarrierAccess::ACCELERATION_STRUCTURE_WRITE) {
        vk_access |= vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR;
    }
    vk_access
}

pub(crate) fn texture_layout_to_image_layout(layout: gpu::TextureLayout) -> vk::ImageLayout {
    match layout {
        gpu::TextureLayout::CopyDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        gpu::TextureLayout::CopySrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        gpu::TextureLayout::DepthStencilRead => vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
        gpu::TextureLayout::DepthStencilReadWrite => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        gpu::TextureLayout::General => vk::ImageLayout::GENERAL,
        gpu::TextureLayout::Sampled => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        gpu::TextureLayout::Storage => vk::ImageLayout::GENERAL,
        gpu::TextureLayout::RenderTarget => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        gpu::TextureLayout::ResolveSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        gpu::TextureLayout::ResolveDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        gpu::TextureLayout::Undefined => vk::ImageLayout::UNDEFINED,
        gpu::TextureLayout::Present => vk::ImageLayout::PRESENT_SRC_KHR,
    }
}

pub(crate) fn aspect_mask_from_format(format: gpu::Format) -> vk::ImageAspectFlags {
    let mut aspects= vk::ImageAspectFlags::empty();
    if format.is_stencil() {
        aspects |= vk::ImageAspectFlags::STENCIL;
    }
    if format.is_depth() {
        aspects |= vk::ImageAspectFlags::DEPTH;
    }
    if aspects.is_empty() {
        aspects = vk::ImageAspectFlags::COLOR;
    }
    aspects
}

pub(crate) fn index_format_to_vk(format: gpu::IndexFormat) -> vk::IndexType {
    match format {
        gpu::IndexFormat::U16 => vk::IndexType::UINT16,
        gpu::IndexFormat::U32 => vk::IndexType::UINT32,
    }
}
