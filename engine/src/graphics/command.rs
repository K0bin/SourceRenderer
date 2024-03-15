use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{*, CommandBuffer as GPUCommandBuffer};

use sourcerenderer_core::gpu;

use super::*;

use super::{BottomLevelAccelerationStructureInfo, AccelerationStructure};

pub use sourcerenderer_core::gpu::{
    SubpassInfo,
    LoadOp,
    StoreOp,
    BarrierSync,
    BarrierAccess,
    IndexFormat,
    ShaderType,
    Viewport,
    Scissor,
    BindingFrequency
};

pub enum Barrier<'a, B: GPUBackend> {
  RawTextureBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_layout: TextureLayout,
    new_layout: TextureLayout,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    texture: &'a B::Texture,
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
    texture: &'a super::Texture<B>,
    range: BarrierTextureRange,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
  BufferBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    buffer: BufferRef<'a, B>,
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
pub enum PipelineBinding<'a, B: GPUBackend> {
    Graphics(&'a super::GraphicsPipeline<B>),
    Compute(&'a super::ComputePipeline<B>),
    RayTracing(&'a super::RayTracingPipeline<B>),
}

pub struct CommandBuffer<B: GPUBackend> {
    cmd_buffer: B::CommandBuffer,
    buffer_refs: Vec<Arc<BufferSlice<B>>>,
    device: Arc<B::Device>,
    global_buffer_allocator: Arc<BufferAllocator<B>>,
    transient_buffer_allocator: Arc<TransientBufferAllocator<B>>,
    destroyer: Arc<DeferredDestroyer<B>>,
    acceleration_structure_scratch: Option<TransientBufferSlice<B>>,
    acceleration_structure_scratch_offset: u64,
}

pub struct CommandBufferRecorder<B: GPUBackend> {
    inner: Box<CommandBuffer<B>>,
    sender: Sender<Box<CommandBuffer<B>>>,
    no_send_sync: PhantomData<*mut u8>
}

pub struct FinishedCommandBuffer<B: GPUBackend> {
    pub(super) inner: Box<CommandBuffer<B>>,
    pub(super) sender: Sender<Box<CommandBuffer<B>>>
}

pub enum BufferRef<'a, B: GPUBackend> {
    Transient(&'a TransientBufferSlice<B>),
    Regular(&'a Arc<BufferSlice<B>>)
}

impl<'a, B: GPUBackend> Clone for BufferRef<'a, B> {
    fn clone(&self) -> Self {
        match self {
            BufferRef::Regular(b) => BufferRef::Regular(b),
            BufferRef::Transient(t) => BufferRef::Transient(t)
        }
    }
}

impl<'a, B: GPUBackend> Copy for BufferRef<'a, B> {}

impl<B: GPUBackend> CommandBufferRecorder<B> {
    pub(super) fn new(
        cmd_buffer: Box<CommandBuffer<B>>,
        sender: Sender<Box<CommandBuffer<B>>>,
        ) -> Self {
        Self {
            inner: cmd_buffer,
            sender,
            no_send_sync: PhantomData
        }
    }

    pub fn set_vertex_buffer(&mut self, buffer: BufferRef<B>, offset: u64) {
        let buffer_handle: &B::Buffer;
        let buffer_offset: u64;

        match buffer {
            BufferRef::Transient(transient_buffer) => {
                buffer_handle = transient_buffer.handle();
                buffer_offset = transient_buffer.offset();
            }
            BufferRef::Regular(buffer) => {
                self.inner.buffer_refs.push(buffer.clone());
                buffer_handle = buffer.handle();
                buffer_offset = buffer.offset();
            }
        }
        unsafe {
            self.inner.cmd_buffer.set_vertex_buffer(buffer_handle, buffer_offset + offset);
        }
    }

    pub fn set_index_buffer(&mut self, buffer: BufferRef<B>, offset: u64, format: IndexFormat) {
        let buffer_handle: &B::Buffer;
        let buffer_offset: u64;

        match buffer {
            BufferRef::Transient(transient_buffer) => {
                buffer_handle = transient_buffer.handle();
                buffer_offset = transient_buffer.offset();
            }
            BufferRef::Regular(buffer) => {
                self.inner.buffer_refs.push(buffer.clone());
                buffer_handle = buffer.handle();
                buffer_offset = buffer.offset();
            }
        }
        unsafe {
            self.inner.cmd_buffer.set_index_buffer(buffer_handle, buffer_offset + offset, format);
        }
    }

    pub fn set_pipeline(&mut self, pipeline: PipelineBinding<B>) {
        unsafe {
            let gpu_pipeline_binding = match pipeline {
                PipelineBinding::Graphics(p) => gpu::PipelineBinding::Graphics(p.handle()),
                PipelineBinding::Compute(p) => gpu::PipelineBinding::Compute(p.handle()),
                PipelineBinding::RayTracing(p) => gpu::PipelineBinding::RayTracing(p.handle())
            };
            self.inner.cmd_buffer.set_pipeline(gpu_pipeline_binding);
        }
    }

    pub fn set_viewports(&mut self, viewports: &[Viewport]) {
        unsafe {
            self.inner.cmd_buffer.set_viewports(viewports);
        }
    }

    pub fn set_scissors(&mut self, scissors: &[Scissor]) {
        unsafe {
            self.inner.cmd_buffer.set_scissors(scissors);
        }
    }

    pub fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_stage: ShaderType)
        where T: 'static + Send + Sync + Sized + Clone
    {
        unsafe {
            self.inner.cmd_buffer.set_push_constant_data(data, visible_for_shader_stage);
        }
    }

    pub fn draw(&mut self, vertices: u32, offset: u32) {
        unsafe {
            self.inner.cmd_buffer.draw(vertices, offset);
        }
    }

    pub fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
        unsafe {
            self.inner.cmd_buffer.draw_indexed(instances, first_instance, indices, first_index, vertex_offset);
        }
    }

    pub fn draw_indexed_indirect(&mut self, draw_buffer: BufferRef<B>, draw_buffer_offset: u32, count_buffer: BufferRef<B>, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        unsafe {
            let draw_buffer_handle = match draw_buffer {
                BufferRef::Regular(b) => b.handle(),
                BufferRef::Transient(b) => b.handle()
            };
            let count_buffer_handle = match count_buffer {
                BufferRef::Regular(b) => b.handle(),
                BufferRef::Transient(b) => b.handle()
            };
            self.inner.cmd_buffer.draw_indexed_indirect(draw_buffer_handle, draw_buffer_offset, count_buffer_handle, count_buffer_offset, max_draw_count, stride);
        }
    }

    pub fn draw_indirect(&mut self, draw_buffer: BufferRef<B>, draw_buffer_offset: u32, count_buffer: BufferRef<B>, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        unsafe {
            let draw_buffer_handle = match draw_buffer {
                BufferRef::Regular(b) => b.handle(),
                BufferRef::Transient(b) => b.handle()
            };
            let count_buffer_handle = match count_buffer {
                BufferRef::Regular(b) => b.handle(),
                BufferRef::Transient(b) => b.handle()
            };
            self.inner.cmd_buffer.draw_indirect(draw_buffer_handle, draw_buffer_offset, count_buffer_handle, count_buffer_offset, max_draw_count, stride);
        }
    }

    pub fn bind_sampling_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &super::TextureView<B>) {
        unsafe {
            self.inner.cmd_buffer.bind_sampling_view(frequency, binding, texture.handle());
        }
    }

    pub fn bind_sampling_view_and_sampler(&mut self, frequency: BindingFrequency, binding: u32, texture: &super::TextureView<B>, sampler: &super::Sampler<B>) {
        unsafe {
            self.inner.cmd_buffer.bind_sampling_view_and_sampler(frequency, binding, texture.handle(), sampler.handle());
        }
    }

    pub fn bind_sampling_view_and_sampler_array(&mut self, frequency: BindingFrequency, binding: u32, textures_and_samplers: &[(&super::TextureView<B>, &super::Sampler<B>)]) {
        let handles: SmallVec<[(&B::TextureView, &B::Sampler); 4]> = textures_and_samplers.iter()
            .map(|(texture, sampler)| (texture.handle(), sampler.handle()))
            .collect();

        unsafe {
            self.inner.cmd_buffer.bind_sampling_view_and_sampler_array(frequency, binding, &handles);
        }
    }

    pub fn bind_storage_view_array(&mut self, frequency: BindingFrequency, binding: u32, textures: &[&super::TextureView<B>]) {
        let handles: SmallVec<[&B::TextureView; 4]> = textures.iter()
            .map(|texture| texture.handle())
            .collect();

        unsafe {
            self.inner.cmd_buffer.bind_storage_view_array(frequency, binding, &handles);
        }
    }

    pub fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: BufferRef<B>, offset: u64, length: u64) {
        let buffer_handle: &B::Buffer;
        let buffer_offset: u64;
        let buffer_length: u64;

        match buffer {
            BufferRef::Transient(transient_buffer) => {
                buffer_handle = transient_buffer.handle();
                buffer_offset = transient_buffer.offset();
                buffer_length = transient_buffer.length();
            }
            BufferRef::Regular(buffer) => {
                self.inner.buffer_refs.push(buffer.clone());
                buffer_handle = buffer.handle();
                buffer_offset = buffer.offset();
                buffer_length = buffer.length();
            }
        }

        unsafe {
            self.inner.cmd_buffer.bind_uniform_buffer(frequency, binding, buffer_handle, buffer_offset + offset, length.min(buffer_length - offset));
        }
    }

    pub fn bind_storage_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: BufferRef<B>, offset: u64, length: u64) {
        let buffer_handle: &B::Buffer;
        let buffer_offset: u64;
        let buffer_length: u64;

        match buffer {
            BufferRef::Transient(transient_buffer) => {
                buffer_handle = transient_buffer.handle();
                buffer_offset = transient_buffer.offset();
                buffer_length = transient_buffer.length();
            }
            BufferRef::Regular(buffer) => {
                self.inner.buffer_refs.push(buffer.clone());
                buffer_handle = buffer.handle();
                buffer_offset = buffer.offset();
                buffer_length = buffer.length();
            }
        }

        unsafe {
            self.inner.cmd_buffer.bind_storage_buffer(frequency, binding, buffer_handle, buffer_offset + offset, length.min(buffer_length - offset));
        }
    }

    pub fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &super::TextureView<B>) {
        unsafe {
            self.inner.cmd_buffer.bind_storage_texture(frequency, binding, texture.handle());
        }
    }

    pub fn bind_sampler(&mut self, frequency: BindingFrequency, binding: u32, sampler: &super::Sampler<B>) {
        unsafe {
            self.inner.cmd_buffer.bind_sampler(frequency, binding, sampler.handle());
        }
    }

    pub fn bind_acceleration_structure(&mut self, frequency: BindingFrequency, binding: u32, acceleration_structure: &AccelerationStructure<B>) {
        unsafe {
            self.inner.cmd_buffer.bind_acceleration_structure(frequency, binding, acceleration_structure.handle());
        }
    }

    pub fn finish_binding(&mut self) {
        unsafe {
            self.inner.cmd_buffer.finish_binding();
        }
    }

    pub fn begin_label(&mut self, label: &str) {
        unsafe {
            self.inner.cmd_buffer.begin_label(label);
        }
    }

    pub fn end_label(&mut self) {
        unsafe {
            self.inner.cmd_buffer.end_label();
        }
    }

    pub fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        unsafe {
            self.inner.cmd_buffer.dispatch(group_count_x, group_count_y, group_count_z);
        }
    }

    pub fn blit(&mut self, src_texture: &super::Texture<B>, src_array_layer: u32, src_mip_level: u32, dst_texture: &super::Texture<B>, dst_array_layer: u32, dst_mip_level: u32) {
        unsafe {
            self.inner.cmd_buffer.blit(src_texture.handle(), src_array_layer, src_mip_level, dst_texture.handle(), dst_array_layer, dst_mip_level);
        }
    }

    pub fn blit_to_handle(&mut self, src_texture: &super::Texture<B>, src_array_layer: u32, src_mip_level: u32, dst_texture_handle: &B::Texture, dst_array_layer: u32, dst_mip_level: u32) {
        unsafe {
            self.inner.cmd_buffer.blit(src_texture.handle(), src_array_layer, src_mip_level, dst_texture_handle, dst_array_layer, dst_mip_level);
        }
    }

    pub fn begin(&mut self, inheritance: Option<&<B::CommandBuffer as gpu::CommandBuffer<B>>::CommandBufferInheritance>) {
        unsafe {
            self.inner.cmd_buffer.begin(inheritance)
        }
    }

    pub fn inheritance(&self) -> &<B::CommandBuffer as gpu::CommandBuffer<B>>::CommandBufferInheritance {
        unsafe {
            self.inner.cmd_buffer.inheritance()
        }
    }

    pub fn finish(mut self) -> FinishedCommandBuffer<B> {
        unsafe {
            self.inner.cmd_buffer.finish();
        }

        let CommandBufferRecorder { inner, sender, no_send_sync: _ } = self;
        FinishedCommandBuffer { inner, sender }
    }

    pub fn clear_storage_texture(&mut self, view: &super::Texture<B>, array_layer: u32, mip_level: u32, values: [u32; 4]) {
        unsafe {
            self.inner.cmd_buffer.clear_storage_texture(view.handle(), array_layer, mip_level, values);
        }
    }

    pub fn clear_storage_buffer(&mut self, buffer: BufferRef<B>, offset: u64, length_in_u32s: u64, value: u32) {
        let buffer_handle: &B::Buffer;
        let buffer_offset: u64;
        let buffer_length: u64;

        match buffer {
            BufferRef::Transient(transient_buffer) => {
                buffer_handle = transient_buffer.handle();
                buffer_offset = transient_buffer.offset();
                buffer_length = transient_buffer.length();
            }
            BufferRef::Regular(buffer) => {
                self.inner.buffer_refs.push(buffer.clone());
                buffer_handle = buffer.handle();
                buffer_offset = buffer.offset();
                buffer_length = buffer.length();
            }
        }

        unsafe {
            self.inner.cmd_buffer.clear_storage_buffer(buffer_handle, offset + buffer_offset, length_in_u32s.min((buffer_length - offset) / 4), value);
        }
    }

    pub fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Result<TransientBufferSlice<B>, OutOfMemoryError>
    where T: 'static + Send + Sync + Sized + Clone {
        let required_size = std::mem::size_of_val(data) as u64;
        let size = align_up_64(required_size.max(64), 64);

        let buffer = self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
            size: size,
            usage,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::MappableGPUMemory, None)?;

        unsafe {
            let ptr_void = buffer.map(false).unwrap();

            if required_size < size {
                let ptr_u8 = (ptr_void as *mut u8).offset(required_size as isize);
                std::ptr::write_bytes(ptr_u8, 0u8, (size - required_size) as usize);
            }

            if required_size != 0 {
                let ptr = ptr_void as *mut T;
                ptr.copy_from(data.as_ptr(), data.len());
            }
            buffer.unmap(true);
        }
        Ok(buffer)
    }

    pub fn create_temporary_buffer(&mut self, info: &BufferInfo, usage: MemoryUsage) -> Result<TransientBufferSlice<B>, OutOfMemoryError> {
        self.inner.transient_buffer_allocator.get_slice(info, usage, None)
    }

    pub fn barrier(&mut self, barriers: &[Barrier<B>]) {
        let core_barriers: SmallVec::<[gpu::Barrier<B>; 4]> = barriers.iter().map(|b| {
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
                    let (buffer_handle, buffer_offset, buffer_length) = match buffer {
                        BufferRef::Regular(b) => (b.handle(), b.offset(), b.length()),
                        BufferRef::Transient(b) => (b.handle(), b.offset(), b.length())
                    };

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
            self.inner.cmd_buffer.barrier(&core_barriers);
        }
    }

    pub fn flush_barriers(&mut self) {
        // TODO batch barriers
    }

    pub fn begin_render_pass(&mut self, renderpass_info: &RenderPassBeginInfo<B>, recording_mode: RenderpassRecordingMode) {
        let attachments: SmallVec<[gpu::RenderPassAttachment<B>; 5]> = renderpass_info.attachments.iter().map(|a| gpu::RenderPassAttachment {
            view: match a.view {
                RenderPassAttachmentView::RenderTarget(rt) => gpu::RenderPassAttachmentView::RenderTarget(rt.handle()),
                RenderPassAttachmentView::DepthStencil(ds) => gpu::RenderPassAttachmentView::DepthStencil(ds.handle()),
            },
            load_op: a.load_op,
            store_op: a.store_op
        }).collect();

        unsafe {
            self.inner.cmd_buffer.begin_render_pass(&gpu::RenderPassBeginInfo {
                attachments: &attachments,
                subpasses: renderpass_info.subpasses
            }, recording_mode);
        }
    }

    pub fn advance_subpass(&mut self) {
        unsafe {
            self.inner.cmd_buffer.advance_subpass();
        }
    }

    pub fn end_render_pass(&mut self) {
        unsafe {
            self.inner.cmd_buffer.end_render_pass();
        }
    }

    pub fn preallocate_acceleration_structure_scratch_memory(&mut self, scratch_size: u64) {
        let scratch_result = self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
            size: scratch_size,
            usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD | BufferUsage::STORAGE,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::GPUMemory, None);

        if let Ok(scratch) = scratch_result {
            self.inner.acceleration_structure_scratch = Some(scratch);
            self.inner.acceleration_structure_scratch_offset = 0;
        }
    }

    pub fn create_bottom_level_acceleration_structure(&mut self, info: &BottomLevelAccelerationStructureInfo<B>, mut use_preallocated_scratch: bool) -> Option<AccelerationStructure<B>> {
        let core_info = gpu::BottomLevelAccelerationStructureInfo {
            index_format: info.index_format,
            vertex_position_offset: info.vertex_position_offset,
            vertex_buffer: info.vertex_buffer.handle(),
            vertex_buffer_offset: info.vertex_buffer.offset(),
            vertex_stride: info.vertex_stride,
            vertex_format: info.vertex_format,
            index_buffer: info.index_buffer.handle(),
            index_buffer_offset: info.index_buffer.offset(),
            opaque: info.opaque,
            mesh_parts: info.mesh_parts,
            max_vertex: info.max_vertex
        };

        let size = unsafe { self.inner.device.get_bottom_level_acceleration_structure_size(&core_info) };
        let buffer = self.inner.global_buffer_allocator.get_slice(
            &BufferInfo {
                size: size.size,
                usage: BufferUsage::ACCELERATION_STRUCTURE,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            None
        ).ok()?;

        if let Some(preallocated_scratch) = self.inner.acceleration_structure_scratch.as_ref() {
            use_preallocated_scratch = use_preallocated_scratch && preallocated_scratch.handle().info().size >= size.build_scratch_size;
        } else {
            use_preallocated_scratch = false;
        }

        let mut _owned_scratch = Option::<TransientBufferSlice<B>>::None;

        let (scratch, scratch_offset) = if use_preallocated_scratch {
            let preallocated_scratch = self.inner.acceleration_structure_scratch.as_ref().unwrap();
            let remaining_scratch: u64 = preallocated_scratch.handle().info().size - align_up_64(self.inner.acceleration_structure_scratch_offset, 256);
            if remaining_scratch < size.build_scratch_size {
                unsafe {
                    self.inner.cmd_buffer.barrier(&[
                        gpu::Barrier::BufferBarrier {
                            old_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            new_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ | BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            buffer: preallocated_scratch.handle(),
                            offset: preallocated_scratch.offset(),
                            length: preallocated_scratch.length(),
                            queue_ownership: None
                        }
                    ]);
                }
                self.inner.acceleration_structure_scratch_offset = 0;
            }
            let offset = align_up_64(self.inner.acceleration_structure_scratch_offset, 256);
            self.inner.acceleration_structure_scratch_offset = offset + size.build_scratch_size;
            (preallocated_scratch, offset)
        } else {
            _owned_scratch = Some(self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
                size: size.build_scratch_size,
                usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD | BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Exclusive
            }, MemoryUsage::GPUMemory, None).ok()?);
            (_owned_scratch.as_ref().unwrap(), 0)
        };

        let acceleration_structure = unsafe { self.inner.cmd_buffer.create_bottom_level_acceleration_structure(
            &core_info,
            size.size,
            buffer.handle(),
            buffer.offset(),
            scratch.handle(),
            scratch.offset() + scratch_offset
        )};

        Some(AccelerationStructure::new(acceleration_structure, buffer, &self.inner.destroyer))
    }

    pub fn create_top_level_acceleration_structure(&mut self, info: &super::rt::TopLevelAccelerationStructureInfo<B>, mut use_preallocated_scratch: bool) -> Option<AccelerationStructure<B>> {
        let core_instances: SmallVec::<[gpu::AccelerationStructureInstance<B>; 16]> = info.instances.iter().map(|i| gpu::AccelerationStructureInstance {
            acceleration_structure: i.acceleration_structure.handle(),
            transform: i.transform.clone(),
            front_face: i.front_face
        }).collect();

        let required_instances_buffer_size = self.inner.device.get_top_level_instances_buffer_size(&core_instances);
        let instances_buffer_size = required_instances_buffer_size.max(16);

        let instances_buffer = self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
            size: instances_buffer_size,
            usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::MappableGPUMemory, None).ok()?;
        if required_instances_buffer_size < instances_buffer_size {
            unsafe {
                let ptr = instances_buffer.map(false).unwrap();
                std::ptr::write_bytes(ptr as *mut u8, 0u8, (instances_buffer_size - required_instances_buffer_size) as usize);
                instances_buffer.unmap(true);
            }
        }

        if required_instances_buffer_size != 0 {
            unsafe { self.inner.cmd_buffer.upload_top_level_instances(&core_instances, instances_buffer.handle(),  instances_buffer.offset()); }
        }

        let core_info = gpu::TopLevelAccelerationStructureInfo {
            instances_buffer: instances_buffer.handle(),
            instances_buffer_offset: instances_buffer.offset(),
            instances_count: info.instances.len() as u32,
        };

        let size = unsafe { self.inner.device.get_top_level_acceleration_structure_size(&core_info) };
        let buffer = self.inner.global_buffer_allocator.get_slice(
            &BufferInfo {
                size: size.size,
                usage: BufferUsage::ACCELERATION_STRUCTURE,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            None
        ).ok()?;

        if let Some(preallocated_scratch) = self.inner.acceleration_structure_scratch.as_ref() {
            use_preallocated_scratch = use_preallocated_scratch && preallocated_scratch.handle().info().size >= size.build_scratch_size;
        } else {
            use_preallocated_scratch = false;
        }

        let mut _owned_scratch = Option::<TransientBufferSlice<B>>::None;

        let (scratch, scratch_offset) = if use_preallocated_scratch {
            let preallocated_scratch = self.inner.acceleration_structure_scratch.as_ref().unwrap();
            let remaining_scratch: u64 = preallocated_scratch.handle().info().size - self.inner.acceleration_structure_scratch_offset;
            if remaining_scratch < size.build_scratch_size {
                unsafe {
                    self.inner.cmd_buffer.barrier(&[
                        gpu::Barrier::BufferBarrier {
                            old_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            new_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                            old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ | BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                            buffer: preallocated_scratch.handle(),
                            offset: preallocated_scratch.offset(),
                            length: preallocated_scratch.length(),
                            queue_ownership: None
                        }
                    ]);
                }
                self.inner.acceleration_structure_scratch_offset = 0;
            }
            let offset = self.inner.acceleration_structure_scratch_offset;
            self.inner.acceleration_structure_scratch_offset += size.build_scratch_size;
            (preallocated_scratch, offset)
        } else {
            _owned_scratch = Some(self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
                size: size.build_scratch_size,
                usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD | BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Exclusive
            }, MemoryUsage::GPUMemory, None).ok()?);
            (_owned_scratch.as_ref().unwrap(), 0)
        };

        let acceleration_structure = unsafe { self.inner.cmd_buffer.create_top_level_acceleration_structure(
            &core_info,
            size.size,
            buffer.handle(),
            buffer.offset(),
            scratch.handle(),
            scratch.offset() + scratch_offset
        )};

        Some(AccelerationStructure::new(acceleration_structure, buffer, &self.inner.destroyer))
    }

    pub fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
        unsafe {
            self.inner.cmd_buffer.trace_ray(width, height, depth);
        }
    }

    pub fn execute_inner(&mut self, mut submission: Vec<FinishedCommandBuffer<B>>) {
        let raw_submissions: SmallVec<[&B::CommandBuffer; 16]> = submission.iter()
            .map(|c| c.inner.handle())
            .collect();
        unsafe {
            self.inner.cmd_buffer.execute_inner(&raw_submissions[..]);
        }
        std::mem::drop(raw_submissions);

        for s in submission.drain(..) {
            let FinishedCommandBuffer { inner, sender } = s;
            sender.send(inner).expect("Failed to reuse inner command buffer");
        }
    }
}

impl<B: GPUBackend> CommandBuffer<B> {
    pub(super) fn new(
        cmd_buffer: B::CommandBuffer,
        device: &Arc<B::Device>,
        transient_buffer_allocator: &Arc<TransientBufferAllocator<B>>,
        global_buffer_allocator: &Arc<BufferAllocator<B>>,
        destroyer: &Arc<DeferredDestroyer<B>>,
        ) -> Self {
        Self {
            cmd_buffer,
            buffer_refs: Vec::new(),
            device: device.clone(),
            global_buffer_allocator: global_buffer_allocator.clone(),
            transient_buffer_allocator: transient_buffer_allocator.clone(),
            destroyer: destroyer.clone(),
            acceleration_structure_scratch: None,
            acceleration_structure_scratch_offset: 0u64
        }
    }

    pub(super) fn handle(&self) -> &B::CommandBuffer {
        &self.cmd_buffer
    }
    pub(super) fn handle_mut(&mut self) -> &mut B::CommandBuffer {
        &mut self.cmd_buffer
    }


    pub fn reset(&mut self, frame: u64) {
        unsafe { self.cmd_buffer.reset(frame); }
        self.buffer_refs.clear();
        self.acceleration_structure_scratch = None;
        self.acceleration_structure_scratch_offset = 0;
    }
}

pub enum RenderPassAttachmentView<'a, B: GPUBackend> {
  RenderTarget(&'a super::TextureView<B>),
  DepthStencil(&'a super::TextureView<B>)
}

pub struct RenderPassAttachment<'a, B: GPUBackend> {
  pub view: RenderPassAttachmentView<'a, B>,
  pub load_op: LoadOp,
  pub store_op: StoreOp
}

pub struct RenderPassBeginInfo<'a, B: GPUBackend> {
  pub attachments: &'a [RenderPassAttachment<'a, B>],
  pub subpasses: &'a [SubpassInfo<'a>]
}
