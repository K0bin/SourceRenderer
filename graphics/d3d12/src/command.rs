use std::sync::{Arc, Mutex};

use smallvec::SmallVec;

use sourcerenderer_core::gpu::ResourceHeapInfo;
use windows::core::GUID;
use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

pub struct D3D12CommandPool {
    allocator: D3D12::ID3D12CommandAllocator,
    command_list_type: D3D12::D3D12_COMMAND_LIST_TYPE
}

impl D3D12CommandPool {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, command_pool_type: gpu::CommandPoolType, flags: gpu::CommandPoolFlags) -> Self {
        let command_list_type = match command_pool_type {
            gpu::CommandPoolType::CommandBuffers => D3D12::D3D12_COMMAND_LIST_TYPE_DIRECT,
            gpu::CommandPoolType::InnerCommandBuffers => D3D12::D3D12_COMMAND_LIST_TYPE_BUNDLE,
        };
        let allocator = unsafe {
            device.CreateCommandAllocator(command_list_type).unwrap()
        };
        Self {
            allocator,
            command_list_type
        }
    }
}

impl gpu::CommandPool<D3D12Backend> for D3D12CommandPool {
    unsafe fn create_command_buffer(&mut self) -> <D3D12Backend as gpu::GPUBackend>::CommandBuffer {
        todo!()
    }

    unsafe fn reset(&mut self) {
        unsafe {
            self.allocator.Reset().unwrap();
        }
    }
}

pub struct D3D12CommandBuffer {
    list: D3D12::ID3D12CommandList
}

impl D3D12CommandBuffer {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, allocator: &D3D12::ID3D12CommandAllocator, command_list_type: D3D12::D3D12_COMMAND_LIST_TYPE) -> Self {
        let list = unsafe {
            device.CreateCommandList(0, command_list_type, allocator, None).unwrap()
        };
        Self {
            list
        }
    }
}

impl gpu::CommandBuffer<D3D12Backend> for D3D12CommandBuffer {
    unsafe fn set_pipeline(&mut self, pipeline: gpu::PipelineBinding<D3D12Backend>) {
        todo!()
    }

    unsafe fn set_vertex_buffer(&mut self, vertex_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, offset: u64) {
        todo!()
    }

    unsafe fn set_index_buffer(&mut self, index_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, offset: u64, format: gpu::IndexFormat) {
        todo!()
    }

    unsafe fn set_viewports(&mut self, viewports: &[ gpu::Viewport ]) {
        todo!()
    }

    unsafe fn set_scissors(&mut self, scissors: &[ gpu::Scissor ]) {
        todo!()
    }

    unsafe fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_stage: gpu::ShaderType)
        where T: 'static + Send + Sync + Sized + Clone {
        todo!()
    }

    unsafe fn draw(&mut self, vertices: u32, offset: u32) {
        todo!()
    }

    unsafe fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
        todo!()
    }

    unsafe fn draw_indexed_indirect(&mut self, draw_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, draw_buffer_offset: u32, count_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        todo!()
    }

    unsafe fn draw_indirect(&mut self, draw_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, draw_buffer_offset: u32, count_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        todo!()
    }

    unsafe fn bind_sampling_view(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &<D3D12Backend as gpu::GPUBackend>::TextureView) {
        todo!()
    }

    unsafe fn bind_sampling_view_and_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &<D3D12Backend as gpu::GPUBackend>::TextureView, sampler: &<D3D12Backend as gpu::GPUBackend>::Sampler) {
        todo!()
    }

    unsafe fn bind_sampling_view_and_sampler_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures_and_samplers: &[(&<D3D12Backend as gpu::GPUBackend>::TextureView, &<D3D12Backend as gpu::GPUBackend>::Sampler)]) {
        todo!()
    }

    unsafe fn bind_storage_view_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures: &[&<D3D12Backend as gpu::GPUBackend>::TextureView]) {
        todo!()
    }

    unsafe fn bind_uniform_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, offset: u64, length: u64) {
        todo!()
    }

    unsafe fn bind_storage_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, offset: u64, length: u64) {
        todo!()
    }

    unsafe fn bind_storage_texture(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &<D3D12Backend as gpu::GPUBackend>::TextureView) {
        todo!()
    }

    unsafe fn bind_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, sampler: &<D3D12Backend as gpu::GPUBackend>::Sampler) {
        todo!()
    }

    unsafe fn bind_acceleration_structure(&mut self, frequency: gpu::BindingFrequency, binding: u32, acceleration_structure: &<D3D12Backend as gpu::GPUBackend>::AccelerationStructure) {
        todo!()
    }

    unsafe fn finish_binding(&mut self) {
        todo!()
    }

    unsafe fn begin_label(&mut self, label: &str) {
        todo!()
    }

    unsafe fn end_label(&mut self) {
        todo!()
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        todo!()
    }

    unsafe fn blit(&mut self, src_texture: &<D3D12Backend as gpu::GPUBackend>::Texture, src_array_layer: u32, src_mip_level: u32, dst_texture: &<D3D12Backend as gpu::GPUBackend>::Texture, dst_array_layer: u32, dst_mip_level: u32) {
        todo!()
    }

    unsafe fn begin(&mut self, inheritance: Option<&Self::CommandBufferInheritance>) {
        todo!()
    }

    unsafe fn finish(&mut self) {
        todo!()
    }

    unsafe fn copy_buffer_to_texture(&mut self, src: &<D3D12Backend as gpu::GPUBackend>::Buffer, dst: &<D3D12Backend as gpu::GPUBackend>::Texture, region: &gpu::BufferTextureCopyRegion) {
        todo!()
    }

    unsafe fn copy_buffer(&mut self, src: &<D3D12Backend as gpu::GPUBackend>::Buffer, dst: &<D3D12Backend as gpu::GPUBackend>::Buffer, region: &gpu::BufferCopyRegion) {
        todo!()
    }

    unsafe fn clear_storage_texture(&mut self, view: &<D3D12Backend as gpu::GPUBackend>::Texture, array_layer: u32, mip_level: u32, values: [u32; 4]) {
        todo!()
    }

    unsafe fn clear_storage_buffer(&mut self, buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer, offset: u64, length_in_u32s: u64, value: u32) {
        todo!()
    }

    unsafe fn begin_render_pass(&mut self, renderpass_info: &gpu::RenderPassBeginInfo<D3D12Backend>, recording_mode: gpu::RenderpassRecordingMode) {
        todo!()
    }

    unsafe fn advance_subpass(&mut self) {
        todo!()
    }

    unsafe fn end_render_pass(&mut self) {
        todo!()
    }

    unsafe fn barrier(&mut self, barriers: &[gpu::Barrier<D3D12Backend>]) {
        todo!()
    }

    unsafe fn inheritance(&self) -> &Self::CommandBufferInheritance {
        todo!()
    }

    type CommandBufferInheritance = ();

    unsafe fn execute_inner(&mut self, submission: &[&<D3D12Backend as gpu::GPUBackend>::CommandBuffer]) {
        todo!()
    }

    unsafe fn reset(&mut self, frame: u64) {
        todo!()
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        info: &gpu::BottomLevelAccelerationStructureInfo<D3D12Backend>,
        size: u64,
        target_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer,
        target_buffer_offset: u64,
        scratch_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer,
        scratch_buffer_offset: u64
      ) -> <D3D12Backend as gpu::GPUBackend>::AccelerationStructure {
        todo!()
    }

    unsafe fn upload_top_level_instances(
        &mut self,
        instances: &[gpu::AccelerationStructureInstance<D3D12Backend>],
        target_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer,
        target_buffer_offset: u64
      ) {
        todo!()
    }

    unsafe fn create_top_level_acceleration_structure(
        &mut self,
        info: &gpu::TopLevelAccelerationStructureInfo<D3D12Backend>,
        size: u64,
        target_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer,
        target_buffer_offset: u64,
        scratch_buffer: &<D3D12Backend as gpu::GPUBackend>::Buffer,
        scratch_buffer_offset: u64
      ) -> <D3D12Backend as gpu::GPUBackend>::AccelerationStructure {
        todo!()
    }

    unsafe fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
        todo!()
    }
}

