use std::{marker::PhantomData, sync::Arc};

use atomic_refcell::AtomicRefMut;
use crossbeam_channel::Sender;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, CommandBuffer as _, Buffer as _};

use super::*;

use super::{BottomLevelAccelerationStructureInfo, AccelerationStructure};

const DEBUG_FORCE_FAT_BARRIER: bool = false;

pub enum Barrier<'a> {
  RawTextureBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_layout: TextureLayout,
    new_layout: TextureLayout,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    texture: &'a active_gpu_backend::Texture,
    range: BarrierTextureRange,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
  TextureBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_layout: TextureLayout,
    new_layout: TextureLayout,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    texture: &'a super::Texture,
    range: BarrierTextureRange,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
  BufferBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    buffer: BufferRef<'a>,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
  GlobalBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
  }
}

#[derive(Clone)]
pub enum PipelineBinding<'a> {
    Graphics(&'a super::GraphicsPipeline),
    Compute(&'a super::ComputePipeline),
    RayTracing(&'a super::RayTracingPipeline),
}

pub struct CommandBuffer<'a> {
    context: AtomicRefMut<'a, FrameContext>,
    global_context: &'a GraphicsContext,
    cmd_buffer_handle: active_gpu_backend::CommandBuffer,
    active_query_range: Option<QueryRange>,
    is_secondary: bool,
    frame_context_entry: FrameContextCommandBufferEntry,
    no_send_sync: PhantomData<*mut u8>
}

pub struct FinishedCommandBuffer {
    pub(super) handle: active_gpu_backend::CommandBuffer,
    pub(super) sender: Sender<active_gpu_backend::CommandBuffer>,
    pub(super) frame_context_entry: FrameContextCommandBufferEntry,
}

pub enum BufferRef<'a> {
    Transient(&'a TransientBufferSlice),
    Regular(&'a Arc<BufferSlice>)
}

impl<'a> BufferRef<'a> {
    #[inline(always)]
    fn deconstruct(&self, frame: u64) -> BufferHandleRef<'a> {
        match self {
            BufferRef::Regular(b) => BufferHandleRef {
                handle: b.handle(),
                offset: b.offset(),
                length: b.length()
            },
            BufferRef::Transient(t) => BufferHandleRef {
                handle: t.handle(frame),
                offset: t.offset(),
                length: t.length()
            }
        }
    }
}

impl<'a> Clone for BufferRef<'a> {
    fn clone(&self) -> Self {
        match self {
            BufferRef::Regular(b) => BufferRef::Regular(b),
            BufferRef::Transient(t) => BufferRef::Transient(t)
        }
    }
}

struct BufferHandleRef<'a> {
    handle: &'a active_gpu_backend::Buffer,
    offset: u64,
    length: u64,
}

impl<'a> Copy for BufferRef<'a> {}

impl<'a> CommandBuffer<'a> {
    pub(super) fn new(
        global_context: &'a GraphicsContext,
        context: AtomicRefMut<'a, FrameContext>,
        handle: active_gpu_backend::CommandBuffer,
        frame_context_entry: FrameContextCommandBufferEntry,
        is_secondary: bool
    ) -> Self {
        Self {
            global_context,
            context,
            cmd_buffer_handle: handle,
            is_secondary,
            active_query_range: None,
            frame_context_entry,
            no_send_sync: PhantomData
        }
    }

    pub fn set_vertex_buffer(&mut self, index: u32, buffer: BufferRef, offset: u64) {
        let BufferHandleRef { handle: buffer_handle, offset: buffer_offset, length: _ } = buffer.deconstruct(self.frame());
        unsafe {
            self.cmd_buffer_handle.set_vertex_buffer(index, buffer_handle, buffer_offset + offset);
        }
    }

    pub fn set_index_buffer(&mut self, buffer: BufferRef, offset: u64, format: IndexFormat) {
        let BufferHandleRef { handle: buffer_handle, offset: buffer_offset, length: _ } = buffer.deconstruct(self.frame());
        unsafe {
            self.cmd_buffer_handle.set_index_buffer(buffer_handle, buffer_offset + offset, format);
        }
    }

    pub fn set_pipeline(&mut self, pipeline: PipelineBinding) {
        unsafe {
            let gpu_pipeline_binding = match pipeline {
                PipelineBinding::Graphics(p) => gpu::PipelineBinding::Graphics(p.handle()),
                PipelineBinding::Compute(p) => gpu::PipelineBinding::Compute(p.handle()),
                PipelineBinding::RayTracing(p) => gpu::PipelineBinding::RayTracing(p.handle())
            };
            self.cmd_buffer_handle.set_pipeline(gpu_pipeline_binding);
        }
    }

    pub fn set_viewports(&mut self, viewports: &[Viewport]) {
        unsafe {
            self.cmd_buffer_handle.set_viewports(viewports);
        }
    }

    pub fn set_scissors(&mut self, scissors: &[Scissor]) {
        unsafe {
            self.cmd_buffer_handle.set_scissors(scissors);
        }
    }

    pub fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_stage: ShaderType)
        where T: 'static + Send + Sync + Sized + Clone
    {
        unsafe {
            self.cmd_buffer_handle.set_push_constant_data(data, visible_for_shader_stage);
        }
    }

    pub fn draw(&mut self, vertices: u32, offset: u32) {
        unsafe {
            self.cmd_buffer_handle.draw(vertices, offset);
        }
    }

    pub fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
        unsafe {
            self.cmd_buffer_handle.draw_indexed(instances, first_instance, indices, first_index, vertex_offset);
        }
    }

    pub fn draw_indexed_indirect(&mut self, draw_buffer: BufferRef, draw_buffer_offset: u32, count_buffer: BufferRef, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        let BufferHandleRef { handle: draw_buffer_handle, offset: _, length: _ } = draw_buffer.deconstruct(self.frame());
        let BufferHandleRef { handle: count_buffer_handle, offset: _, length: _ } = count_buffer.deconstruct(self.frame());
        unsafe {
            self.cmd_buffer_handle.draw_indexed_indirect(draw_buffer_handle, draw_buffer_offset, count_buffer_handle, count_buffer_offset, max_draw_count, stride);
        }
    }

    pub fn draw_indirect(&mut self, draw_buffer: BufferRef, draw_buffer_offset: u32, count_buffer: BufferRef, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        let BufferHandleRef { handle: draw_buffer_handle, offset: _, length: _ } = draw_buffer.deconstruct(self.frame());
        let BufferHandleRef { handle: count_buffer_handle, offset: _, length: _ } = count_buffer.deconstruct(self.frame());
        unsafe {
            self.cmd_buffer_handle.draw_indirect(draw_buffer_handle, draw_buffer_offset, count_buffer_handle, count_buffer_offset, max_draw_count, stride);
        }
    }

    pub fn bind_sampling_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &super::TextureView) {
        unsafe {
            self.cmd_buffer_handle.bind_sampling_view(frequency, binding, texture.handle());
        }
    }

    pub fn bind_sampling_view_and_sampler(&mut self, frequency: BindingFrequency, binding: u32, texture: &super::TextureView, sampler: &super::Sampler) {
        unsafe {
            self.cmd_buffer_handle.bind_sampling_view_and_sampler(frequency, binding, texture.handle(), sampler.handle());
        }
    }

    pub fn bind_sampling_view_and_sampler_array(&mut self, frequency: BindingFrequency, binding: u32, textures_and_samplers: &[(&super::TextureView, &super::Sampler)]) {
        let handles: SmallVec<[(&active_gpu_backend::TextureView, &active_gpu_backend::Sampler); 4]> = textures_and_samplers.iter()
            .map(|(texture, sampler)| (texture.handle(), sampler.handle()))
            .collect();

        unsafe {
            self.cmd_buffer_handle.bind_sampling_view_and_sampler_array(frequency, binding, &handles);
        }
    }

    pub fn bind_storage_view_array(&mut self, frequency: BindingFrequency, binding: u32, textures: &[&super::TextureView]) {
        let handles: SmallVec<[&active_gpu_backend::TextureView; 4]> = textures.iter()
            .map(|texture| texture.handle())
            .collect();

        unsafe {
            self.cmd_buffer_handle.bind_storage_view_array(frequency, binding, &handles);
        }
    }

    pub fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: BufferRef, offset: u64, length: u64) {
        let BufferHandleRef { handle: buffer_handle, offset: buffer_offset, length: buffer_length } = buffer.deconstruct(self.frame());
        unsafe {
            self.cmd_buffer_handle.bind_uniform_buffer(frequency, binding, buffer_handle, buffer_offset + offset, length.min(buffer_length - offset));
        }
    }

    pub fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: BufferRef, offset: u64, length: u64) {
        let BufferHandleRef { handle: buffer_handle, offset: buffer_offset, length: buffer_length } = buffer.deconstruct(self.frame());
        unsafe {
            self.cmd_buffer_handle.bind_storage_buffer(frequency, binding, buffer_handle, buffer_offset + offset, length.min(buffer_length - offset));
        }
    }

    pub fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &super::TextureView) {
        unsafe {
            self.cmd_buffer_handle.bind_storage_texture(frequency, binding, texture.handle());
        }
    }

    pub fn bind_sampler(&mut self, frequency: BindingFrequency, binding: u32, sampler: &super::Sampler) {
        unsafe {
            self.cmd_buffer_handle.bind_sampler(frequency, binding, sampler.handle());
        }
    }

    pub fn bind_acceleration_structure(&mut self, frequency: BindingFrequency, binding: u32, acceleration_structure: &AccelerationStructure) {
        unsafe {
            self.cmd_buffer_handle.bind_acceleration_structure(frequency, binding, acceleration_structure.handle());
        }
    }

    pub fn finish_binding(&mut self) {
        unsafe {
            self.cmd_buffer_handle.finish_binding();
        }
    }

    pub fn begin_label(&mut self, label: &str) {
        unsafe {
            self.cmd_buffer_handle.begin_label(label);
        }
    }

    pub fn end_label(&mut self) {
        unsafe {
            self.cmd_buffer_handle.end_label();
        }
    }

    pub fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        if DEBUG_FORCE_FAT_BARRIER {
            self.fat_barrier();
        }

        unsafe {
            self.cmd_buffer_handle.dispatch(group_count_x, group_count_y, group_count_z);
        }

        if DEBUG_FORCE_FAT_BARRIER {
            self.fat_barrier();
        }
    }

    pub fn blit(&mut self, src_texture: &super::Texture, src_array_layer: u32, src_mip_level: u32, dst_texture: &super::Texture, dst_array_layer: u32, dst_mip_level: u32) {
        unsafe {
            self.cmd_buffer_handle.blit(src_texture.handle(), src_array_layer, src_mip_level, dst_texture.handle(), dst_array_layer, dst_mip_level);
        }
    }

    pub fn blit_to_handle(&mut self, src_texture: &super::Texture, src_array_layer: u32, src_mip_level: u32, dst_texture_handle: &active_gpu_backend::Texture, dst_array_layer: u32, dst_mip_level: u32) {
        unsafe {
            self.cmd_buffer_handle.blit(src_texture.handle(), src_array_layer, src_mip_level, dst_texture_handle, dst_array_layer, dst_mip_level);
        }
    }

    pub fn begin(&mut self, frame: u64, inheritance: Option<&active_gpu_backend::CommandBufferInheritance>) {
        unsafe {
            self.cmd_buffer_handle.begin(frame, inheritance)
        }
    }

    pub fn finish(mut self) -> FinishedCommandBuffer {
        unsafe {
            self.cmd_buffer_handle.finish();
        }

        let CommandBuffer { context, global_context: _, cmd_buffer_handle, is_secondary, active_query_range: _, frame_context_entry, no_send_sync: _ } = self;
        FinishedCommandBuffer {
            handle: cmd_buffer_handle,
            sender: context.sender(is_secondary).clone(),
            frame_context_entry,
        }
    }

    pub fn clear_storage_texture(&mut self, view: &super::Texture, array_layer: u32, mip_level: u32, values: [u32; 4]) {
        unsafe {
            self.cmd_buffer_handle.clear_storage_texture(view.handle(), array_layer, mip_level, values);
        }
    }

    pub fn clear_storage_buffer(&mut self, buffer: BufferRef, offset: u64, length_in_u32s: u64, value: u32) {
        let BufferHandleRef { handle: buffer_handle, offset: buffer_offset, length: buffer_length } = buffer.deconstruct(self.frame());
        unsafe {
            self.cmd_buffer_handle.clear_storage_buffer(buffer_handle, offset + buffer_offset, length_in_u32s.min((buffer_length - offset) / 4), value);
        }
    }

    pub fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Result<TransientBufferSlice, OutOfMemoryError>
    where T: 'static + Send + Sync + Sized + Clone {
        let required_size = std::mem::size_of_val(data);
        let size = align_up(required_size.max(64), 64);

        let buffer = self.context.transient_buffer_allocator().get_slice(&BufferInfo {
            size: size as u64,
            usage,
            sharing_mode: QueueSharingMode::Exclusive,
        }, MemoryUsage::MappableGPUMemory, self.frame(), None)?;

        unsafe {
            let ptr_void = buffer.map(self.frame(), false).unwrap();

            if required_size < size {
                let ptr_u8 = (ptr_void as *mut u8).offset(required_size as isize);
                std::ptr::write_bytes(ptr_u8, 0u8, size - required_size);
            }

            if required_size != 0 {
                let ptr = ptr_void as *mut u8;
                ptr.copy_from(data.as_ptr() as *const u8, required_size);
            }

            buffer.unmap(self.frame(), true);
        }
        Ok(buffer)
    }

    pub fn create_temporary_buffer(&mut self, info: &BufferInfo, usage: MemoryUsage) -> Result<TransientBufferSlice, OutOfMemoryError> {
        self.context.transient_buffer_allocator().get_slice(info, usage, self.frame(), None)
    }

    fn fat_barrier(&mut self) {
        let fat_core_barrier = [
            gpu::Barrier::GlobalBarrier { old_sync: gpu::BarrierSync::all(), new_sync: gpu::BarrierSync::all(), old_access: gpu::BarrierAccess::MEMORY_WRITE, new_access: gpu::BarrierAccess::MEMORY_READ | gpu::BarrierAccess::MEMORY_WRITE }
        ];

        unsafe {
            self.cmd_buffer_handle.barrier(&fat_core_barrier);
        }
    }

    pub fn barrier(&mut self, barriers: &[Barrier]) {
        if DEBUG_FORCE_FAT_BARRIER {
            self.fat_barrier();
        }

        let core_barriers: SmallVec::<[active_gpu_backend::Barrier; 4]> = barriers.iter().map(|b| {
            match b {
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
                } => gpu::Barrier::TextureBarrier {
                    old_sync: *old_sync,
                    new_sync: *new_sync,
                    old_layout: *old_layout,
                    new_layout: *new_layout,
                    old_access: *old_access,
                    new_access: *new_access,
                    texture: texture.handle(),
                    range: range.clone(),
                    queue_ownership: queue_ownership.clone()
                },
                Barrier::BufferBarrier {
                    old_sync,
                    new_sync,
                    old_access,
                    new_access,
                    buffer,
                    queue_ownership
                } => {
                    let BufferHandleRef { handle: buffer_handle, offset: buffer_offset, length: buffer_length } = buffer.deconstruct(self.frame());
                    gpu::Barrier::BufferBarrier {
                        old_sync: *old_sync,
                        new_sync: *new_sync,
                        old_access: *old_access,
                        new_access: *new_access,
                        buffer: buffer_handle,
                        offset: buffer_offset,
                        length: buffer_length,
                        queue_ownership: queue_ownership.clone()
                    }
                }
                Barrier::GlobalBarrier {
                    old_sync,
                    new_sync,
                    old_access,
                    new_access
                } => gpu::Barrier::GlobalBarrier {
                    old_sync: *old_sync,
                    new_sync: *new_sync,
                    old_access: *old_access,
                    new_access: *new_access,
                },
                Barrier::RawTextureBarrier {
                    old_sync,
                    new_sync,
                    old_layout,
                    new_layout,
                    old_access,
                    new_access,
                    texture,
                    range,
                    queue_ownership
                } => gpu::Barrier::TextureBarrier {
                    old_sync: *old_sync,
                    new_sync: *new_sync,
                    old_layout: *old_layout,
                    new_layout: *new_layout,
                    old_access: *old_access,
                    new_access: *new_access,
                    texture: *texture,
                    range: range.clone(),
                    queue_ownership: queue_ownership.clone()
                },
            }
        }).collect();

        unsafe {
            self.cmd_buffer_handle.barrier(&core_barriers);
        }
    }

    pub fn flush_barriers(&mut self) {
        // TODO batch barriers
    }

    pub fn begin_render_pass(&mut self, renderpass_info: &RenderPassBeginInfo) {
        let _ = self.begin_render_pass_impl(renderpass_info, RenderpassRecordingMode::Commands);
    }

    fn begin_render_pass_impl(&mut self, renderpass_info: &RenderPassBeginInfo, recording_mode: RenderpassRecordingMode) -> Option<<active_gpu_backend::CommandBuffer as gpu::CommandBuffer<active_gpu_backend::Backend>>::CommandBufferInheritance> {
        if DEBUG_FORCE_FAT_BARRIER {
            self.fat_barrier();
        }

        let attachments: SmallVec<[active_gpu_backend::RenderTarget; 5]> = renderpass_info.render_targets.iter().map(|a| gpu::RenderTarget {
            view: a.view.handle(),
            load_op: a.load_op,
            store_op: match &a.store_op {
                StoreOp::Store => gpu::StoreOp::Store,
                StoreOp::DontCare => gpu::StoreOp::DontCare,
                StoreOp::Resolve(resolve_attachment) => gpu::StoreOp::Resolve(gpu::ResolveAttachment {
                    view: resolve_attachment.view.handle(),
                    mode: resolve_attachment.mode
                }),
            },
        }).collect();

        let depth_stencil = renderpass_info.depth_stencil.as_ref().map(|a| gpu::DepthStencilAttachment {
            view: a.view.handle(),
            load_op: a.load_op,
            store_op: match &a.store_op {
                StoreOp::Store => gpu::StoreOp::Store,
                StoreOp::DontCare => gpu::StoreOp::DontCare,
                StoreOp::Resolve(resolve_attachment) => gpu::StoreOp::Resolve(gpu::ResolveAttachment {
                    view: resolve_attachment.view.handle(),
                    mode: resolve_attachment.mode
                }),
            }
        });

        self.active_query_range = renderpass_info.query_range.clone();
        unsafe {
            self.cmd_buffer_handle.begin_render_pass(&gpu::RenderPassBeginInfo {
                render_targets: &attachments,
                depth_stencil: depth_stencil.as_ref(),
                query_pool: renderpass_info.query_range.as_ref().map(|q| q.pool_handle(self.frame())),
            }, recording_mode)
        }
    }

    pub fn end_render_pass(&mut self) {
        unsafe {
            self.cmd_buffer_handle.end_render_pass();
        }

        if DEBUG_FORCE_FAT_BARRIER {
            self.fat_barrier();
        }
    }

    pub fn preallocate_acceleration_structure_scratch_memory(&mut self, scratch_size: u64) {
        let scratch_result = self.context.transient_buffer_allocator().get_slice(&BufferInfo {
            size: scratch_size,
            usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD | BufferUsage::STORAGE,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::GPUMemory, self.frame(), None);

        if let Ok(scratch) = scratch_result {
            self.context.acceleration_structure_scratch = Some(scratch);
            self.context.acceleration_structure_scratch_offset = 0;
        }
    }

    pub fn create_bottom_level_acceleration_structure(&mut self, info: &BottomLevelAccelerationStructureInfo, mut use_preallocated_scratch: bool) -> Option<AccelerationStructure> {
        assert_ne!(info.mesh_parts.len(), 0);
        let core_info = gpu::BottomLevelAccelerationStructureInfo {
            index_format: info.index_format,
            vertex_position_offset: info.vertex_position_offset,
            vertex_buffer: info.vertex_buffer.handle(),
            vertex_buffer_offset: info.vertex_buffer.offset() + info.vertex_buffer_offset as u64,
            vertex_stride: info.vertex_stride,
            vertex_format: info.vertex_format,
            index_buffer: info.index_buffer.handle(),
            index_buffer_offset: info.index_buffer.offset() + info.index_buffer_offset as u64,
            opaque: info.opaque,
            mesh_parts: info.mesh_parts,
            max_vertex: info.max_vertex
        };

        let size = unsafe { self.context.device().get_bottom_level_acceleration_structure_size(&core_info) };
        let buffer = self.context.global_buffer_allocator().get_slice(
            &BufferInfo {
                size: size.size,
                usage: BufferUsage::ACCELERATION_STRUCTURE,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            None
        ).ok()?;

        let reset_scratch_bump_alloc: bool;
        if let Some(preallocated_scratch) = self.context.acceleration_structure_scratch.as_ref() {
            // Does the required scratch fit into the entire preallocated scratch buffer?
            // If not, we need to create a one-off buffer.
            use_preallocated_scratch = use_preallocated_scratch && preallocated_scratch.handle(self.frame()).info().size >= size.build_scratch_size;

            // Does the required scratch fit into the remaining preallocated scratch buffer space?
            // If not, we need to insert a barrier to make the entire space available again.
            let remaining_scratch_with_aligned_offset = preallocated_scratch.handle(self.frame()).info().size - align_up_64(self.context.acceleration_structure_scratch_offset, 256);
            reset_scratch_bump_alloc = use_preallocated_scratch
                && remaining_scratch_with_aligned_offset < size.build_scratch_size;
        } else {
            use_preallocated_scratch = false;
            reset_scratch_bump_alloc = false;
        }

        let mut _owned_scratch = Option::<TransientBufferSlice>::None;

        let (scratch, scratch_offset) = if use_preallocated_scratch {
            let preallocated_scratch: &TransientBufferSlice;
            let offset: u64;
            if reset_scratch_bump_alloc {
                self.context.acceleration_structure_scratch_offset = 0;
                offset = size.build_scratch_size;
                preallocated_scratch = self.context.acceleration_structure_scratch.as_ref().unwrap();
                unsafe {
                    self.cmd_buffer_handle.barrier(&[
                        gpu::Barrier::BufferBarrier {
                            old_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            new_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ | BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            buffer: preallocated_scratch.handle(self.frame()),
                            offset: preallocated_scratch.offset(),
                            length: preallocated_scratch.length(),
                            queue_ownership: None
                        }
                    ]);
                }
            } else {
                offset = align_up_64(self.context.acceleration_structure_scratch_offset, 256);
                self.context.acceleration_structure_scratch_offset = offset + size.build_scratch_size;
                preallocated_scratch = self.context.acceleration_structure_scratch.as_ref().unwrap();
            }
            (preallocated_scratch, offset)
        } else {
            _owned_scratch = Some(self.context.transient_buffer_allocator().get_slice(&BufferInfo {
                size: size.build_scratch_size,
                usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD | BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Exclusive
            }, MemoryUsage::GPUMemory, self.frame(), None).ok()?);
            (_owned_scratch.as_ref().unwrap(), 0)
        };

        let acceleration_structure = unsafe { self.cmd_buffer_handle.create_bottom_level_acceleration_structure(
            &core_info,
            size.size,
            buffer.handle(),
            buffer.offset(),
            scratch.handle(self.frame()),
            scratch.offset() + scratch_offset
        )};

        Some(AccelerationStructure::new(acceleration_structure, buffer, self.context.destroyer()))
    }

    pub fn create_top_level_acceleration_structure(&mut self, info: &super::rt::TopLevelAccelerationStructureInfo, mut use_preallocated_scratch: bool) -> Option<AccelerationStructure> {
        let core_instances: SmallVec::<[active_gpu_backend::AccelerationStructureInstance; 16]> = info.instances.iter().map(|i| gpu::AccelerationStructureInstance {
            acceleration_structure: i.acceleration_structure.handle(),
            transform: i.transform.clone(),
            front_face: i.front_face,
            id: i.id
        }).collect();

        let required_instances_buffer_size = self.context.device().get_top_level_instances_buffer_size(&core_instances);
        let instances_buffer_size = required_instances_buffer_size.max(16);

        let instances_buffer = self.context.transient_buffer_allocator().get_slice(&BufferInfo {
            size: instances_buffer_size,
            usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::MappableGPUMemory, self.frame(), None).ok()?;
        if required_instances_buffer_size < instances_buffer_size {
            unsafe {
                let ptr = instances_buffer.map(self.frame(), false).unwrap();
                std::ptr::write_bytes(ptr as *mut u8, 0u8, (instances_buffer_size - required_instances_buffer_size) as usize);
                instances_buffer.unmap(self.frame(), true);
            }
        }

        let instance_buffer_handle = instances_buffer.handle(self.frame());
        if required_instances_buffer_size != 0 {
            unsafe { self.cmd_buffer_handle.upload_top_level_instances(&core_instances, instance_buffer_handle,  instances_buffer.offset()); }
        }

        let core_info = gpu::TopLevelAccelerationStructureInfo {
            instances_buffer: instance_buffer_handle,
            instances_buffer_offset: instances_buffer.offset(),
            instances_count: info.instances.len() as u32,
        };

        let size = unsafe { self.context.device().get_top_level_acceleration_structure_size(&core_info) };
        let buffer = self.context.global_buffer_allocator().get_slice(
            &BufferInfo {
                size: size.size,
                usage: BufferUsage::ACCELERATION_STRUCTURE,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            None
        ).ok()?;

        let reset_scratch_bump_alloc: bool;
        if let Some(preallocated_scratch) = self.context.acceleration_structure_scratch.as_ref() {
            // Does the required scratch fit into the entire preallocated scratch buffer?
            // If not, we need to create a one-off buffer.
            use_preallocated_scratch = use_preallocated_scratch && preallocated_scratch.handle(self.frame()).info().size >= size.build_scratch_size;

            // Does the required scratch fit into the remaining preallocated scratch buffer space?
            // If not, we need to insert a barrier to make the entire space available again.
            let remaining_scratch_with_aligned_offset = preallocated_scratch.handle(self.frame()).info().size - align_up_64(self.context.acceleration_structure_scratch_offset, 256);
            reset_scratch_bump_alloc = use_preallocated_scratch
                && remaining_scratch_with_aligned_offset < size.build_scratch_size;
        } else {
            use_preallocated_scratch = false;
            reset_scratch_bump_alloc = false;
        }

        let mut _owned_scratch = Option::<TransientBufferSlice>::None;

        let (scratch, scratch_offset) = if use_preallocated_scratch {
            let preallocated_scratch: &TransientBufferSlice;
            let offset: u64;
            if reset_scratch_bump_alloc {
                self.context.acceleration_structure_scratch_offset = 0;
                offset = size.build_scratch_size;
                preallocated_scratch = self.context.acceleration_structure_scratch.as_ref().unwrap();
                unsafe {
                    self.cmd_buffer_handle.barrier(&[
                        gpu::Barrier::BufferBarrier {
                            old_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            new_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ | BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            buffer: preallocated_scratch.handle(self.frame()),
                            offset: preallocated_scratch.offset(),
                            length: preallocated_scratch.length(),
                            queue_ownership: None
                        }
                    ]);
                }
            } else {
                offset = align_up_64(self.context.acceleration_structure_scratch_offset, 256);
                self.context.acceleration_structure_scratch_offset = offset + size.build_scratch_size;
                preallocated_scratch = self.context.acceleration_structure_scratch.as_ref().unwrap();
            }
            (preallocated_scratch, offset)
        } else {
            _owned_scratch = Some(self.context.transient_buffer_allocator().get_slice(&BufferInfo {
                size: size.build_scratch_size,
                usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD | BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Exclusive
            }, MemoryUsage::GPUMemory, self.frame(), None).ok()?);
            (_owned_scratch.as_ref().unwrap(), 0)
        };

        let acceleration_structure = unsafe { self.cmd_buffer_handle.create_top_level_acceleration_structure(
            &core_info,
            size.size,
            buffer.handle(),
            buffer.offset(),
            scratch.handle(self.frame()),
            scratch.offset() + scratch_offset
        )};

        Some(AccelerationStructure::new(acceleration_structure, buffer, self.context.destroyer()))
    }

    pub fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
        unsafe {
            self.cmd_buffer_handle.trace_ray(width, height, depth);
        }
    }

    pub fn split_render_pass_with_chunks<F, T>(self, renderpass_info: &RenderPassBeginInfo, elements: &[T], chunk_size_hint: u32, thread_func: F, ) -> Self
    where F: for<'secondary_lt> Fn(&mut CommandBuffer<'a>, u32, u32, &[T]) + Send + Sync,
        T: Sync {
        let thread_count_hint = if chunk_size_hint == 0 { 0 } else {
            ((elements.len() as u32) + (chunk_size_hint - 1)) / chunk_size_hint
        };
        self.split_render_pass(renderpass_info, thread_count_hint, |cmd_buffer, thread_index, threads_count| {
            let chunk_index = thread_index;
            let chunk_size = ((elements.len() as u32) + (threads_count - 1)) / threads_count;
            let element_index = thread_index * chunk_size;
            if element_index >= elements.len() as u32 {
                return;
            }
            let element_count = chunk_size.min((elements.len() as u32) - element_index);
            let chunk = &elements[(element_index as usize)..((element_index + element_count) as usize)];
            thread_func(cmd_buffer, chunk_index, chunk_size, chunk)
        })
    }

    pub fn split_render_pass<F>(mut self, renderpass_info: &RenderPassBeginInfo, thread_count_hint: u32, thread_func: F) -> Self
    where F: for<'secondary_lt> Fn(&mut CommandBuffer<'a>, u32, u32) + Send + Sync {
        assert!(!self.is_secondary);

        let mut new_self = if cfg!(target_arch = "wasm32") {
            // WebGPU does not support multithreading.
            // The bevy_tasks implementation runs tasks in a Web Microtask
            // which is executed once the canvas frame callback is done.
            // That's too late. Besides that, WebGPU bundles just add
            // unnecessary work when we cannot use multithreading.
            self.begin_render_pass_impl(renderpass_info, RenderpassRecordingMode::Commands);
            thread_func(&mut self, 0, 1);
            self
        } else {
            // We need to dissolve the command buffer wrapper here, so we can drop the frame context reference.
            // The frame context reference is a mutable atomic refcell reference, so keeping that would
            // potentially explode when bevy_tasks runs one of the tasks on the thread this function gets
            // called on.
            let task_pool = bevy_tasks::ComputeTaskPool::get();
            let task_count = if thread_count_hint == 0 { task_pool.thread_num() as u32 } else { thread_count_hint.max(task_pool.thread_num() as u32) };
            let inheritance = self.begin_render_pass_impl(renderpass_info, RenderpassRecordingMode::CommandBuffers(task_count)).unwrap();
            let CommandBuffer { global_context, context, mut cmd_buffer_handle, is_secondary: _, active_query_range, frame_context_entry, no_send_sync: _ } = self;
            let secondary_recycle_sender = context.sender(true).clone();
            let frame = context.frame();
            std::mem::drop(context);
            let cmd_buffers: Vec<active_gpu_backend::CommandBuffer> = task_pool.scope(
                |scope| {
                    for i in 0..task_count {
                        let c_inheritance_ref = &inheritance;
                        let c_func_ref = &thread_func;
                        scope.spawn(async move {
                            let mut inner_cmd_buffer = global_context.get_inner_command_buffer(c_inheritance_ref);
                            c_func_ref(&mut inner_cmd_buffer, i, task_count);
                            let finished_inner_cmd_buffer = inner_cmd_buffer.finish();
                            finished_inner_cmd_buffer.handle
                        });
                    }
                }
            );
            {
                let cmd_buffer_refs: Vec<&active_gpu_backend::CommandBuffer> = cmd_buffers.iter().map(|cmd_buffer| cmd_buffer).collect();
                unsafe { cmd_buffer_handle.execute_inner(&cmd_buffer_refs, inheritance); }
            }
            for cmd_buffer in cmd_buffers {
                secondary_recycle_sender.send(cmd_buffer).unwrap();
            }

            let new_frame_context = global_context.get_thread_frame_context(frame);
            CommandBuffer::<'a> {
                context: new_frame_context,
                global_context,
                cmd_buffer_handle,
                is_secondary: false,
                active_query_range: active_query_range,
                frame_context_entry,
                no_send_sync: PhantomData,
            }
        };
        new_self.end_render_pass();
        new_self
    }

    pub fn begin_query(&mut self, query_index: u32) {
        let query_range = self.active_query_range.as_ref().unwrap();
        unsafe {
            self.cmd_buffer_handle.begin_query(query_range.query_index(query_index));
        }
    }

    pub fn end_query(&mut self, query_index: u32) {
        let query_range = self.active_query_range.as_ref().unwrap();
        unsafe {
            self.cmd_buffer_handle.end_query(query_range.query_index(query_index));
        }
    }

    #[inline(always)]
    pub fn frame(&self) -> u64 {
        self.context.frame()
    }

    pub fn get_queries(&mut self, query_count: u32) -> Result<QueryRange, OutOfQueriesError> {
        let frame = self.frame();
        self.context.query_allocator().get_queries(frame, query_count)
    }
}

pub enum StoreOp<'a> {
  Store,
  DontCare,
  Resolve(ResolveAttachment<'a>)
}

pub struct ResolveAttachment<'a> {
    pub view: &'a super::TextureView,
    pub mode: ResolveMode
  }

  pub struct RenderTarget<'a> {
    pub view: &'a super::TextureView,
    pub load_op: LoadOpColor,
    pub store_op: StoreOp<'a>,
  }

  pub struct DepthStencilAttachment<'a> {
    pub view: &'a super::TextureView,
    pub load_op: LoadOpDepthStencil,
    pub store_op: StoreOp<'a>,
  }

  pub struct RenderPassBeginInfo<'a> {
    pub render_targets: &'a [RenderTarget<'a>],
    pub depth_stencil: Option<&'a DepthStencilAttachment<'a>>,
    pub query_range: Option<QueryRange>,
  }
