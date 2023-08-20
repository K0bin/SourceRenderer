use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{*, CommandBuffer as GPUCommandBuffer};

use sourcerenderer_core::gpu;

use super::*;

use super::{BottomLevelAccelerationStructureInfo, AccelerationStructure, TopLevelAccelerationStructureInfo};

pub use sourcerenderer_core::gpu::{
    SubpassInfo,
    AttachmentRef,
    DepthStencilAttachmentRef,
    OutputAttachmentRef,
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

pub struct CommandBuffer<B: GPUBackend> {
    cmd_buffer: B::CommandBuffer,
    buffer_refs: Vec<Arc<BufferSlice<B>>>,
    device: Arc<B::Device>,
    global_buffer_allocator: Arc<BufferAllocator<B>>,
    transient_buffer_allocator: Arc<TransientBufferAllocator<B>>,
    destroyer: Arc<DeferredDestroyer<B>>
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

impl<B: GPUBackend> CommandBufferRecorder<B> {
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
            self.inner.cmd_buffer.set_pipeline(pipeline);
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

    pub fn draw_indexed_indirect(&mut self, draw_buffer: &B::Buffer, draw_buffer_offset: u32, count_buffer: &B::Buffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        unsafe {
            self.inner.cmd_buffer.draw_indexed_indirect(draw_buffer, draw_buffer_offset, count_buffer, count_buffer_offset, max_draw_count, stride);
        }
    }

    pub fn draw_indirect(&mut self, draw_buffer: &B::Buffer, draw_buffer_offset: u32, count_buffer: &B::Buffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        unsafe {
            self.inner.cmd_buffer.draw_indirect(draw_buffer, draw_buffer_offset, count_buffer, count_buffer_offset, max_draw_count, stride);
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

    pub fn begin(&mut self, inheritance: Option<&<B::CommandBuffer as sourcerenderer_core::gpu::CommandBuffer<B>>::CommandBufferInheritance>, frame: u64) {
        unsafe {
            self.inner.cmd_buffer.begin(inheritance, frame)
        }
    }

    pub fn finish(mut self) -> FinishedCommandBuffer<B> {
        unsafe {
            self.inner.cmd_buffer.finish();
        }

        let CommandBufferRecorder { inner, sender, no_send_sync: _ } = self;
        FinishedCommandBuffer { inner, sender }
    }

    pub fn reset(&mut self, frame: u64) {
        unsafe { self.inner.cmd_buffer.reset(frame); }
        self.inner.buffer_refs.clear();
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

    pub fn create_bottom_level_acceleration_structure(&mut self, info: &BottomLevelAccelerationStructureInfo<B>) -> Option<AccelerationStructure<B>> {
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

        let scratch = self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
            size: size.build_scratch_size,
            usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::GPUMemory, None).ok()?;

        let acceleration_structure = unsafe { self.inner.cmd_buffer.create_bottom_level_acceleration_structure(
            &core_info,
            size.size,
            buffer.handle(),
            buffer.offset(),
            scratch.handle(),
            scratch.offset()
        )};

        Some(AccelerationStructure::new(acceleration_structure, buffer, &self.inner.destroyer))
    }

    pub fn create_top_level_acceleration_structure(&mut self, info: &TopLevelAccelerationStructureInfo<B>) -> Option<AccelerationStructure<B>> {
        let core_instances: SmallVec::<[gpu::AccelerationStructureInstance<B>; 16]> = info.instances.iter().map(|i| gpu::AccelerationStructureInstance {
            acceleration_structure: i.acceleration_structure.handle(),
            transform: i.transform.clone(),
            front_face: i.front_face
        }).collect();

        let instances_buffer_size = self.inner.device.get_top_level_instances_buffer_size(&core_instances);

        let instances_buffer = self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
            size: instances_buffer_size,
            usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::GPUMemory, None).ok()?;

        unsafe { self.inner.cmd_buffer.upload_top_level_instances(&core_instances, instances_buffer.handle(), instances_buffer.offset()); }

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

        let scratch = self.inner.transient_buffer_allocator.get_slice(&BufferInfo {
            size: size.build_scratch_size,
            usage: BufferUsage::ACCELERATION_STRUCTURE_BUILD,
            sharing_mode: QueueSharingMode::Exclusive
        }, MemoryUsage::GPUMemory, None).ok()?;

        let acceleration_structure = unsafe { self.inner.cmd_buffer.create_top_level_acceleration_structure(
            &core_info,
            size.size,
            buffer.handle(),
            buffer.offset(),
            scratch.handle(),
            scratch.offset()
        )};

        Some(AccelerationStructure::new(acceleration_structure, buffer, &self.inner.destroyer))
    }
}

impl<B: GPUBackend> CommandBuffer<B> {
    pub(super) fn handle(&self) -> &B::CommandBuffer {
        &self.cmd_buffer
    }
    pub(super) fn handle_mut(&mut self) -> &mut B::CommandBuffer {
        &mut self.cmd_buffer
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
