use crate::{
    WebGPUBackend, WebGPUBindGroupBinding, WebGPULimits, WebGPUQueryPool,
    binding::{
        WebGPUBindingManager, WebGPUBoundResourceRef, WebGPUBufferBindingInfo,
        WebGPUHashableSampler, WebGPUHashableTextureView, WebGPUPipelineLayout,
    },
    buffer::WebGPUBuffer,
    sampler::WebGPUSampler,
    stubs::WebGPUAccelerationStructure,
    texture::{WebGPUTexture, WebGPUTextureView, format_to_webgpu},
};
use bytemuck::Pod;
use core::panic;
use js_sys::wasm_bindgen::JsCast;
use js_sys::{JsNullable, JsString, Uint32Array, wasm_bindgen::JsValue};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{
    Barrier, BarrierSync, BindingFrequency, BufferArrayEntry, SplitBarrierWait,
};
use sourcerenderer_core::{
    align_up_32,
    gpu::{self, Buffer as _, Texture as _, TextureView as _},
};
use std::collections::{HashSet, hash_set::Iter};
use std::marker::PhantomData;
use std::sync::Arc;
use web_sys::{
    GpuCommandBuffer, GpuCommandEncoder, GpuComputePassEncoder, GpuDevice, GpuExtent3dDict,
    GpuIndexFormat, GpuLoadOp, GpuRenderPassColorAttachment, GpuRenderPassDepthStencilAttachment,
    GpuRenderPassDescriptor, GpuRenderPassEncoder, GpuStoreOp, GpuTexelCopyBufferInfo,
    GpuTexelCopyTextureInfo,
};

enum WebGPUPassEncoder {
    None,
    Render(GpuRenderPassEncoder),
    Compute(GpuComputePassEncoder),
}

impl Drop for WebGPUPassEncoder {
    fn drop(&mut self) {
        match self {
            WebGPUPassEncoder::Render(encoder) => {
                encoder.end();
            }
            WebGPUPassEncoder::Compute(encoder) => {
                encoder.end();
            }
            _ => {}
        }
    }
}

enum WebGPUBoundPipeline {
    Graphics {
        pipeline_layout: Arc<WebGPUPipelineLayout>,
    },
    Compute {
        pipeline_layout: Arc<WebGPUPipelineLayout>,
    },
    None,
}

impl WebGPUBoundPipeline {
    #[inline(always)]
    fn is_graphics(&self) -> bool {
        if let WebGPUBoundPipeline::Graphics { .. } = self {
            true
        } else {
            false
        }
    }

    #[inline(always)]
    fn is_compute(&self) -> bool {
        if let WebGPUBoundPipeline::Compute { .. } = self {
            true
        } else {
            false
        }
    }
    #[allow(unused)]
    #[inline(always)]
    fn is_none(&self) -> bool {
        if let WebGPUBoundPipeline::None = self {
            true
        } else {
            false
        }
    }
}

struct WebGPUResetCommandBuffer {
    command_encoder: GpuCommandEncoder,
    binding_manager: WebGPUBindingManager,
    _p: PhantomData<*const std::ffi::c_void>,
}

struct WebGPURecordingCommandBuffer {
    command_encoder: GpuCommandEncoder,
    pass_encoder: WebGPUPassEncoder,
    bound_pipeline: WebGPUBoundPipeline,
    binding_manager: WebGPUBindingManager,
    _p: PhantomData<*const std::ffi::c_void>,
}

struct WebGPUFinishedCommandBuffer {
    command_buffer: GpuCommandBuffer,
    binding_manager: WebGPUBindingManager,
    _p: PhantomData<*const std::ffi::c_void>,
}

enum WebGPUCommandBufferHandle {
    Reset(WebGPUResetCommandBuffer),
    Recording(WebGPURecordingCommandBuffer),
    Finished(WebGPUFinishedCommandBuffer),
    Uninit,
}

#[derive(Clone)]
pub(crate) struct WebGPUReadbackBufferSync {
    pub(crate) src: web_sys::GpuBuffer,
    pub(crate) dst: Option<web_sys::GpuBuffer>,
    pub(crate) size: u32,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl std::hash::Hash for WebGPUReadbackBufferSync {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        WebGPUBuffer::handle_as_usize(&self.src).hash(state);
    }
}

impl PartialEq for WebGPUReadbackBufferSync {
    fn eq(&self, other: &Self) -> bool {
        self.src == other.src && self.dst == other.dst && self.size == other.size
    }
}

impl Eq for WebGPUReadbackBufferSync {}

pub struct WebGPUCommandBuffer {
    handle: WebGPUCommandBufferHandle,
    device: GpuDevice,
    frame: u64,
    readback_syncs: HashSet<WebGPUReadbackBufferSync>,
    _p: PhantomData<*const std::ffi::c_void>,
}

fn load_op_color_to_webgpu(load_op: &gpu::LoadOpColor) -> (GpuLoadOp, &gpu::ClearColor) {
    match load_op {
        gpu::LoadOpColor::Load => (GpuLoadOp::Load, &gpu::ClearColor::BLACK),
        gpu::LoadOpColor::Clear(clear_color) => (GpuLoadOp::Clear, clear_color),
        gpu::LoadOpColor::DontCare => (GpuLoadOp::Clear, &gpu::ClearColor::BLACK), // why is there no DontCare. Let's just pick the one thats faster on tiled GPUs.
    }
}
fn load_op_ds_to_webgpu(
    load_op: &gpu::LoadOpDepthStencil,
) -> (GpuLoadOp, &gpu::ClearDepthStencilValue) {
    match load_op {
        gpu::LoadOpDepthStencil::Load => {
            (GpuLoadOp::Load, &gpu::ClearDepthStencilValue::DEPTH_ZERO)
        }
        gpu::LoadOpDepthStencil::Clear(clear_value) => (GpuLoadOp::Clear, &clear_value),
        gpu::LoadOpDepthStencil::DontCare => {
            (GpuLoadOp::Clear, &gpu::ClearDepthStencilValue::DEPTH_ZERO)
        } // why is there no DontCare. Let's just pick the one thats faster on tiled GPUs
    }
}
fn store_op_to_webgpu<'a>(
    store_op: &'a gpu::StoreOp<'a, WebGPUBackend>,
) -> (
    GpuStoreOp,
    Option<&'a gpu::ResolveAttachment<'a, WebGPUBackend>>,
) {
    match store_op {
        gpu::StoreOp::Store => (GpuStoreOp::Store, None),
        gpu::StoreOp::DontCare => (GpuStoreOp::Discard, None),
        gpu::StoreOp::Resolve(attachment) => (GpuStoreOp::Store, Some(attachment)),
    }
}

impl WebGPUCommandBuffer {
    fn new(device: &GpuDevice, limits: &WebGPULimits) -> Self {
        Self {
            device: device.clone(),
            handle: {
                let cmd_buffer = device.create_command_encoder();
                WebGPUCommandBufferHandle::Reset(WebGPUResetCommandBuffer {
                    command_encoder: cmd_buffer,
                    binding_manager: WebGPUBindingManager::new(device, limits),
                    _p: PhantomData,
                })
            },
            frame: 0u64,
            readback_syncs: HashSet::new(),
            _p: PhantomData,
        }
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> &GpuCommandBuffer {
        match &self.handle {
            WebGPUCommandBufferHandle::Finished(command_buffer) => &command_buffer.command_buffer,
            WebGPUCommandBufferHandle::Uninit => unreachable!(),
            _ => panic!("Invalid state for retrieving the command buffer"),
        }
    }

    #[inline(always)]
    fn get_recording(&self) -> &WebGPURecordingCommandBuffer {
        match &self.handle {
            WebGPUCommandBufferHandle::Recording(cmd_buffer) => cmd_buffer,
            WebGPUCommandBufferHandle::Finished(_cmd_buffer) => {
                panic!("Command buffer is finished")
            }
            WebGPUCommandBufferHandle::Reset(_cmd_buffer) => {
                panic!("Command buffer was not begun.")
            }
            WebGPUCommandBufferHandle::Uninit => unreachable!(),
            _ => panic!("Secondary command buffers aren't supported here"),
        }
    }

    #[inline(always)]
    fn get_recording_mut(&mut self) -> &mut WebGPURecordingCommandBuffer {
        match &mut self.handle {
            WebGPUCommandBufferHandle::Recording(cmd_buffer) => cmd_buffer,
            WebGPUCommandBufferHandle::Finished(_cmd_buffer) => {
                panic!("Command buffer is finished")
            }
            WebGPUCommandBufferHandle::Reset(_cmd_buffer) => {
                panic!("Command buffer was not begun.")
            }
            WebGPUCommandBufferHandle::Uninit => unreachable!(),
            _ => panic!("Secondary command buffers aren't supported here"),
        }
    }

    pub(crate) fn readback_syncs(&self) -> Iter<'_, WebGPUReadbackBufferSync> {
        self.readback_syncs.iter()
    }
}

impl WebGPURecordingCommandBuffer {
    fn get_compute_encoder(&mut self) -> &GpuComputePassEncoder {
        let has_existing_encoder = if let WebGPUPassEncoder::Compute(_) = &self.pass_encoder {
            true
        } else {
            false
        };

        if !has_existing_encoder {
            self.pass_encoder =
                WebGPUPassEncoder::Compute(self.command_encoder.begin_compute_pass());
        }
        if let WebGPUPassEncoder::Compute(encoder) = &self.pass_encoder {
            encoder
        } else {
            unreachable!()
        }
    }

    fn get_render_encoder(&mut self) -> &GpuRenderPassEncoder {
        match &self.pass_encoder {
            WebGPUPassEncoder::Render(encoder) => return encoder,
            _ => panic!("No active render pass"),
        }
    }

    fn end_non_rendering_encoders(&mut self) {
        match std::mem::replace(&mut self.pass_encoder, WebGPUPassEncoder::None) {
            WebGPUPassEncoder::Render(_) => {
                panic!("Render passes have to be ended manually using end_render_pass.")
            }
            WebGPUPassEncoder::Compute(_) => {
                self.bound_pipeline = WebGPUBoundPipeline::None;
                self.binding_manager.mark_all_dirty();
            }
            _ => {}
        };
    }
}

impl gpu::CommandBuffer<WebGPUBackend> for WebGPUCommandBuffer {
    unsafe fn set_pipeline(&mut self, pipeline: gpu::PipelineBinding<WebGPUBackend>) {
        let cmd_buffer = self.get_recording_mut();
        match pipeline {
            gpu::PipelineBinding::Graphics(graphics_pipeline) => {
                cmd_buffer.bound_pipeline = WebGPUBoundPipeline::Graphics {
                    pipeline_layout: graphics_pipeline.layout().clone(),
                };
                cmd_buffer
                    .get_render_encoder()
                    .set_pipeline(graphics_pipeline.handle());
                cmd_buffer.binding_manager.mark_all_dirty();
            }
            gpu::PipelineBinding::Compute(compute_pipeline) => {
                cmd_buffer.bound_pipeline = WebGPUBoundPipeline::Compute {
                    pipeline_layout: compute_pipeline.layout().clone(),
                };
                cmd_buffer
                    .get_compute_encoder()
                    .set_pipeline(compute_pipeline.handle());
                cmd_buffer.binding_manager.mark_all_dirty();
            }
            gpu::PipelineBinding::RayTracing(_) => {
                panic!("WebGPU does not support ray tracing")
            }
            gpu::PipelineBinding::MeshGraphics(_) => {
                panic!("WebGPU does not support mesh shaders")
            }
        }
    }

    unsafe fn set_vertex_buffer(&mut self, index: u32, vertex_buffer: &WebGPUBuffer, offset: u64) {
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        render_pass_encoder.set_vertex_buffer_with_u32_and_u32(
            index,
            Some(&vertex_buffer.handle()),
            offset as u32,
            vertex_buffer.info().size as u32 - offset as u32,
        );
    }

    unsafe fn set_index_buffer(
        &mut self,
        index_buffer: &WebGPUBuffer,
        offset: u64,
        format: gpu::IndexFormat,
    ) {
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        render_pass_encoder.set_index_buffer_with_u32_and_u32(
            &index_buffer.handle(),
            match format {
                gpu::IndexFormat::U16 => GpuIndexFormat::Uint16,
                gpu::IndexFormat::U32 => GpuIndexFormat::Uint32,
            },
            offset as u32,
            index_buffer.info().size as u32 - offset as u32,
        );
    }

    unsafe fn set_viewports(&mut self, viewports: &[gpu::Viewport]) {
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        assert_eq!(viewports.len(), 1);
        let viewport = &viewports[0];
        render_pass_encoder.set_viewport(
            viewport.position.x,
            viewport.position.y,
            viewport.extent.x,
            viewport.extent.y,
            viewport.min_depth,
            viewport.max_depth,
        );
    }

    unsafe fn set_scissors(&mut self, scissors: &[gpu::Scissor]) {
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        assert_eq!(scissors.len(), 1);
        let scissor = &scissors[0];
        render_pass_encoder.set_scissor_rect(
            scissor.position.x as u32,
            scissor.position.y as u32,
            scissor.extent.x,
            scissor.extent.y,
        );
    }

    unsafe fn set_push_constant_data<T>(
        &mut self,
        data: &[T],
        visible_for_shader_stage: gpu::ShaderType,
    ) where
        T: 'static + Pod,
    {
        let cmd_buffer = self.get_recording_mut();
        cmd_buffer
            .binding_manager
            .set_push_constant_data(data, visible_for_shader_stage);
    }

    unsafe fn draw(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        let cmd_buffer = self.get_recording_mut();
        debug_assert!(cmd_buffer.bound_pipeline.is_graphics());
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        render_pass_encoder.draw_with_instance_count_and_first_vertex_and_first_instance(
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        );
    }

    unsafe fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        let cmd_buffer = self.get_recording_mut();
        debug_assert!(cmd_buffer.bound_pipeline.is_graphics());
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        render_pass_encoder
            .draw_indexed_with_instance_count_and_first_index_and_base_vertex_and_first_instance(
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
    }

    unsafe fn draw_indexed_indirect_count(
        &mut self,
        _draw_buffer: &WebGPUBuffer,
        _draw_buffer_offset: u64,
        _count_buffer: &WebGPUBuffer,
        _count_buffer_offset: u64,
        _max_draw_count: u32,
        _stride: u32,
    ) {
        panic!("WebGPU does not support multi draw indirect");
    }

    unsafe fn draw_indirect(
        &mut self,
        draw_buffer: &WebGPUBuffer,
        draw_buffer_offset: u64,
        draw_count: u32,
        stride: u32,
    ) {
        let cmd_buffer = self.get_recording_mut();
        debug_assert!(cmd_buffer.bound_pipeline.is_graphics());
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        for i in 0..draw_count {
            render_pass_encoder.draw_indexed_indirect_with_u32(
                &draw_buffer.handle(),
                (draw_buffer_offset as u32) + i * stride,
            );
        }
    }

    unsafe fn draw_indexed_indirect(
        &mut self,
        draw_buffer: &WebGPUBuffer,
        draw_buffer_offset: u64,
        draw_count: u32,
        stride: u32,
    ) {
        let cmd_buffer = self.get_recording_mut();
        debug_assert!(cmd_buffer.bound_pipeline.is_graphics());
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        for i in 0..draw_count {
            render_pass_encoder.draw_indirect_with_u32(
                &draw_buffer.handle(),
                (draw_buffer_offset as u32) + i * stride,
            );
        }
    }

    unsafe fn draw_indirect_count(
        &mut self,
        _draw_buffer: &WebGPUBuffer,
        _draw_buffer_offset: u64,
        _count_buffer: &WebGPUBuffer,
        _count_buffer_offset: u64,
        _max_draw_count: u32,
        _stride: u32,
    ) {
        panic!("WebGPU does not support multi draw indirect");
    }

    unsafe fn draw_mesh_tasks(
        &mut self,
        _group_count_x: u32,
        _group_count_y: u32,
        _group_count_z: u32,
    ) {
        panic!("WebGPU does not support mesh shaders");
    }

    unsafe fn draw_mesh_tasks_indirect(
        &mut self,
        _draw_buffer: &WebGPUBuffer,
        _draw_buffer_offset: u64,
        _draw_count: u32,
        _stride: u32,
    ) {
        panic!("WebGPU does not support mesh shaders");
    }

    unsafe fn draw_mesh_tasks_indirect_count(
        &mut self,
        _draw_buffer: &WebGPUBuffer,
        _draw_buffer_offset: u64,
        _count_buffer: &WebGPUBuffer,
        _count_buffer_offset: u64,
        _max_draw_count: u32,
        _stride: u32,
    ) {
        panic!("WebGPU does not support mesh shaders");
    }

    unsafe fn bind_sampling_view(
        &mut self,
        frequency: gpu::BindingFrequency,
        binding: u32,
        texture: &WebGPUTextureView,
    ) {
        let binding_manager = &mut self.get_recording_mut().binding_manager;
        binding_manager.bind(
            frequency,
            binding,
            WebGPUBoundResourceRef::SampledTexture(WebGPUHashableTextureView::from(texture)),
        );
    }

    unsafe fn bind_sampling_view_and_sampler(
        &mut self,
        frequency: gpu::BindingFrequency,
        binding: u32,
        texture: &WebGPUTextureView,
        sampler: &WebGPUSampler,
    ) {
        let binding_manager = &mut self.get_recording_mut().binding_manager;
        binding_manager.bind(
            frequency,
            binding,
            WebGPUBoundResourceRef::SampledTextureAndSampler(
                WebGPUHashableTextureView::from(texture),
                WebGPUHashableSampler::from(sampler),
            ),
        );
    }

    unsafe fn bind_sampling_view_and_sampler_array(
        &mut self,
        _frequency: gpu::BindingFrequency,
        _binding: u32,
        _textures_and_samplers: &[(&WebGPUTextureView, &WebGPUSampler)],
    ) {
        panic!("WebGPU does not support binding arrays");
    }

    unsafe fn bind_storage_view_array(
        &mut self,
        _frequency: gpu::BindingFrequency,
        _binding: u32,
        _textures: &[&WebGPUTextureView],
    ) {
        panic!("WebGPU does not support binding arrays");
    }

    unsafe fn bind_uniform_buffer(
        &mut self,
        frequency: gpu::BindingFrequency,
        binding: u32,
        buffer: &WebGPUBuffer,
        offset: u64,
        length: u64,
    ) {
        let binding_manager = &mut self.get_recording_mut().binding_manager;
        binding_manager.bind(
            frequency,
            binding,
            WebGPUBoundResourceRef::UniformBuffer(WebGPUBufferBindingInfo {
                buffer: buffer.handle().clone(),
                offset,
                length,
                _p: PhantomData,
            }),
        );
    }

    unsafe fn bind_storage_buffer(
        &mut self,
        frequency: gpu::BindingFrequency,
        binding: u32,
        buffer: &WebGPUBuffer,
        offset: u64,
        length: u64,
    ) {
        let identical: bool;
        {
            let binding_manager = &mut self.get_recording_mut().binding_manager;
            identical = binding_manager.bind(
                frequency,
                binding,
                WebGPUBoundResourceRef::StorageBuffer(WebGPUBufferBindingInfo {
                    buffer: buffer.handle().clone(),
                    offset,
                    length,
                    _p: PhantomData,
                }),
            );
        }
        if !identical && buffer.is_mappable() && buffer.info().usage.gpu_writable() {
            self.readback_syncs.insert(WebGPUReadbackBufferSync {
                src: buffer.handle().clone(),
                dst: buffer.readback_handle().map(|h| (*h).clone()),
                size: buffer.info().size as u32,
                _p: PhantomData,
            });
        }
    }

    unsafe fn bind_storage_texture(
        &mut self,
        frequency: gpu::BindingFrequency,
        binding: u32,
        texture: &WebGPUTextureView,
    ) {
        let binding_manager = &mut self.get_recording_mut().binding_manager;
        binding_manager.bind(
            frequency,
            binding,
            WebGPUBoundResourceRef::StorageTexture(WebGPUHashableTextureView::from(texture)),
        );
    }

    unsafe fn bind_sampler(
        &mut self,
        frequency: gpu::BindingFrequency,
        binding: u32,
        sampler: &WebGPUSampler,
    ) {
        let binding_manager = &mut self.get_recording_mut().binding_manager;
        binding_manager.bind(
            frequency,
            binding,
            WebGPUBoundResourceRef::Sampler(WebGPUHashableSampler::from(sampler)),
        );
    }

    unsafe fn bind_acceleration_structure(
        &mut self,
        _frequency: gpu::BindingFrequency,
        _binding: u32,
        _acceleration_structure: &WebGPUAccelerationStructure,
    ) {
        panic!("WebGPU does not support ray tracing");
    }

    unsafe fn finish_binding(&mut self) {
        let frame = self.frame;
        let pipeline_layout = match &self.get_recording().bound_pipeline {
            WebGPUBoundPipeline::Graphics { pipeline_layout } => pipeline_layout.clone(),
            WebGPUBoundPipeline::Compute { pipeline_layout } => pipeline_layout.clone(),
            WebGPUBoundPipeline::None => {
                panic!("Must not call finish_binding without a pipeline bound")
            }
        };
        let dynamic_offsets_js =
            Uint32Array::new_with_length(gpu::PER_SET_BINDINGS * gpu::NON_BINDLESS_SET_COUNT);
        let binding_infos: [Option<WebGPUBindGroupBinding>; gpu::NON_BINDLESS_SET_COUNT as usize];
        {
            let binding_manager = &mut self.get_recording_mut().binding_manager;
            binding_infos = binding_manager.finish(frame, &pipeline_layout);

            for (set_index, binding) in binding_infos.iter().enumerate() {
                if binding.is_none() {
                    continue;
                }
                let binding = binding.as_ref().unwrap();
                for (offset_index, offset) in binding.dynamic_offsets.iter().enumerate() {
                    dynamic_offsets_js.set_index(
                        (set_index as u32) * gpu::PER_SET_BINDINGS + offset_index as u32,
                        *offset as u32,
                    );
                }
            }
        }

        let cmd_buffer = self.get_recording_mut();

        match &cmd_buffer.pass_encoder {
            WebGPUPassEncoder::None => {}
            WebGPUPassEncoder::Render(gpu_render_pass_encoder) => {
                for (set_index, binding) in binding_infos.iter().enumerate() {
                    if binding.is_none() {
                        continue;
                    }
                    let binding = binding.as_ref().unwrap();
                    gpu_render_pass_encoder
                        .set_bind_group_with_u32_array_and_f64_and_dynamic_offsets_data_length(
                            set_index as u32,
                            Some(binding.set.handle()),
                            &dynamic_offsets_js,
                            (gpu::PER_SET_BINDINGS * (set_index as u32)) as f64,
                            binding.dynamic_offsets.len() as u32,
                        )
                        .unwrap();
                }
            }
            WebGPUPassEncoder::Compute(gpu_compute_pass_encoder) => {
                for (set_index, binding) in binding_infos.iter().enumerate() {
                    if binding.is_none() {
                        continue;
                    }
                    let binding = binding.as_ref().unwrap();
                    gpu_compute_pass_encoder
                        .set_bind_group_with_u32_array_and_f64_and_dynamic_offsets_data_length(
                            set_index as u32,
                            Some(binding.set.handle()),
                            &dynamic_offsets_js,
                            (gpu::PER_SET_BINDINGS * (set_index as u32)) as f64,
                            binding.dynamic_offsets.len() as u32,
                        )
                        .unwrap();
                }
            }
        }
    }

    unsafe fn begin_label(&mut self, label: &str) {
        let cmd_buffer = self.get_recording_mut();
        cmd_buffer.command_encoder.push_debug_group(label);
    }

    unsafe fn end_label(&mut self) {
        let cmd_buffer = self.get_recording_mut();
        cmd_buffer.command_encoder.pop_debug_group();
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        let cmd_buffer = self.get_recording_mut();
        debug_assert!(cmd_buffer.bound_pipeline.is_compute());
        let compute_pass_encoder = cmd_buffer.get_compute_encoder();
        compute_pass_encoder.dispatch_workgroups_with_workgroup_count_y_and_workgroup_count_z(
            group_count_x,
            group_count_y,
            group_count_z,
        );
    }

    unsafe fn dispatch_indirect(&mut self, buffer: &WebGPUBuffer, offset: u64) {
        let cmd_buffer = self.get_recording_mut();
        debug_assert!(cmd_buffer.bound_pipeline.is_compute());
        let compute_pass_encoder = cmd_buffer.get_compute_encoder();
        compute_pass_encoder.dispatch_workgroups_indirect_with_u32(&buffer.handle(), offset as u32);
    }

    unsafe fn set_stencil_reference(&mut self, reference: u32) {
        let cmd_buffer = self.get_recording_mut();
        cmd_buffer
            .get_render_encoder()
            .set_stencil_reference(reference);
    }

    unsafe fn blit(
        &mut self,
        src_texture: &WebGPUTexture,
        src_array_layer: u32,
        src_mip_level: u32,
        dst_texture: &WebGPUTexture,
        dst_array_layer: u32,
        dst_mip_level: u32,
    ) {
        let cmd_buffer = self.get_recording_mut();
        cmd_buffer.end_non_rendering_encoders();

        let src_info = GpuTexelCopyTextureInfo::new(src_texture.handle());
        src_info.set_mip_level(src_mip_level);
        let mut src_origin = [
            js_sys::Number::from(0),
            js_sys::Number::from(0),
            js_sys::Number::from(0),
        ];
        if src_texture.info().dimension == gpu::TextureDimension::Dim3D {
            assert_eq!(src_array_layer, 0);
        } else {
            src_origin[2] = js_sys::Number::from(src_array_layer);
        }
        src_info.set_origin(&src_origin);

        let dst_info = GpuTexelCopyTextureInfo::new(dst_texture.handle());
        dst_info.set_mip_level(dst_mip_level);
        let mut dst_origin = [
            js_sys::Number::from(0),
            js_sys::Number::from(0),
            js_sys::Number::from(0),
        ];
        if dst_texture.info().dimension == gpu::TextureDimension::Dim3D {
            assert_eq!(dst_array_layer, 0);
        } else {
            dst_origin[2] = js_sys::Number::from(dst_array_layer);
        }
        dst_info.set_origin(&dst_origin);

        assert_eq!(
            (src_texture.info().width >> src_mip_level).max(1),
            (dst_texture.info().width >> dst_mip_level).max(1)
        );
        assert_eq!(
            (src_texture.info().height >> src_mip_level).max(1),
            (dst_texture.info().height >> dst_mip_level).max(1)
        );
        assert_eq!(
            (src_texture.info().depth >> src_mip_level).max(1),
            (dst_texture.info().depth >> dst_mip_level).max(1)
        );

        let copy_size = GpuExtent3dDict::new((src_texture.info().width >> src_mip_level).max(1));
        copy_size.set_height((src_texture.info().height >> src_mip_level).max(1));
        if src_texture.info().dimension == gpu::TextureDimension::Dim3D {
            copy_size.set_depth_or_array_layers((src_texture.info().depth >> src_mip_level).max(1));
            assert_eq!(dst_array_layer, 0);
        } else {
            copy_size.set_depth_or_array_layers(1);
            assert_eq!(src_texture.info().depth, 1);
        }

        cmd_buffer
            .command_encoder
            .copy_texture_to_texture_with_gpu_extent_3d_dict(&src_info, &dst_info, &copy_size)
            .unwrap();
    }

    unsafe fn begin(&mut self, frame: u64) {
        if let &WebGPUCommandBufferHandle::Reset(_) = &self.handle {
        } else {
            panic!("Command buffer was not reset.");
        }

        self.frame = frame;

        let handle = std::mem::replace(&mut self.handle, WebGPUCommandBufferHandle::Uninit);
        if let WebGPUCommandBufferHandle::Reset(mut cmd_buffer) = handle {
            cmd_buffer.binding_manager.mark_all_dirty();
            self.handle = WebGPUCommandBufferHandle::Recording(WebGPURecordingCommandBuffer {
                command_encoder: cmd_buffer.command_encoder,
                pass_encoder: WebGPUPassEncoder::None,
                bound_pipeline: WebGPUBoundPipeline::None,
                binding_manager: cmd_buffer.binding_manager,
                _p: PhantomData,
            });
        } else {
            unreachable!()
        }
    }

    unsafe fn finish(&mut self) {
        if !self.readback_syncs.is_empty() {
            // Copy all buffers that were written to their readback buffers.
            let mut copies = SmallVec::<[WebGPUReadbackBufferSync; 8]>::new();
            for sync in &self.readback_syncs {
                if sync.dst.is_some() {
                    copies.push(sync.clone());
                }
            }

            let recording = self.get_recording_mut();
            recording.end_non_rendering_encoders();
            for sync in copies {
                let dst = sync.dst.clone().unwrap();
                recording
                    .command_encoder
                    .copy_buffer_to_buffer_with_u32_and_u32_and_u32(
                        &sync.src, 0, &dst, 0, sync.size,
                    )
                    .unwrap();
            }
        }

        let handle = std::mem::replace(&mut self.handle, WebGPUCommandBufferHandle::Uninit);
        let (cmd_buffer, binding_manager) = match handle {
            WebGPUCommandBufferHandle::Recording(mut cmd_buffer) => {
                cmd_buffer.end_non_rendering_encoders();
                (
                    cmd_buffer.command_encoder.finish(),
                    cmd_buffer.binding_manager,
                )
            }
            _ => unreachable!(),
        };

        self.handle = WebGPUCommandBufferHandle::Finished(WebGPUFinishedCommandBuffer {
            command_buffer: cmd_buffer,
            binding_manager,
            _p: PhantomData,
        });
    }

    unsafe fn copy_buffer_to_texture(
        &mut self,
        src: &WebGPUBuffer,
        dst: &WebGPUTexture,
        region: &gpu::BufferTextureCopyRegion,
    ) {
        let recording = self.get_recording_mut();
        recording.end_non_rendering_encoders();

        let src_info = GpuTexelCopyBufferInfo::new(&src.handle());
        src_info.set_offset(region.buffer_offset as u32);

        let format = dst.info().format;
        let row_pitch = if region.buffer_row_pitch != 0 {
            region.buffer_row_pitch
        } else {
            (align_up_32(region.texture_extent.x, format.block_size().x) / format.block_size().x
                * format.element_size()) as u64
        };
        let slice_pitch = if region.buffer_slice_pitch != 0 {
            region.buffer_slice_pitch
        } else {
            (align_up_32(region.texture_extent.y, format.block_size().y) / format.block_size().y)
                as u64
                * row_pitch
        };
        assert_eq!(slice_pitch % row_pitch, 0);

        src_info.set_bytes_per_row(row_pitch as u32);
        src_info.set_rows_per_image((slice_pitch / row_pitch) as u32);
        let dst_info = GpuTexelCopyTextureInfo::new(dst.handle());
        dst_info.set_mip_level(region.texture_subresource.mip_level);
        let mut origin = [
            js_sys::Number::from(region.texture_offset.x),
            js_sys::Number::from(region.texture_offset.y),
            js_sys::Number::from(0),
        ];
        let copy_size = GpuExtent3dDict::new(region.texture_extent.x);
        copy_size.set_height(region.texture_extent.y);
        assert!(
            dst.info().array_length == 0 || dst.info().dimension != gpu::TextureDimension::Dim3D
        );
        if dst.info().dimension == gpu::TextureDimension::Dim3D {
            assert_eq!(region.texture_subresource.array_layer, 0);
            copy_size.set_depth_or_array_layers(region.texture_extent.z);
            origin[2] = js_sys::Number::from(region.texture_offset.z);
        } else {
            assert_eq!(region.texture_extent.z, 1);
            assert_eq!(region.texture_offset.z, 0);
            copy_size.set_depth_or_array_layers(1);
            origin[2] = js_sys::Number::from(region.texture_subresource.array_layer);
        }
        dst_info.set_origin(&origin);
        recording
            .command_encoder
            .copy_buffer_to_texture_with_gpu_extent_3d_dict(&src_info, &dst_info, &copy_size)
            .unwrap();
    }

    unsafe fn copy_buffer(
        &mut self,
        src: &WebGPUBuffer,
        dst: &WebGPUBuffer,
        region: &gpu::BufferCopyRegion,
    ) {
        if dst.is_mappable() {
            self.readback_syncs.insert(WebGPUReadbackBufferSync {
                src: dst.handle().clone(),
                dst: dst.readback_handle().map(|h| (*h).clone()),
                size: dst.info().size as u32,
                _p: PhantomData,
            });
        }
        let recording = self.get_recording_mut();
        recording.end_non_rendering_encoders();
        recording
            .command_encoder
            .copy_buffer_to_buffer_with_u32_and_u32_and_u32(
                &src.handle(),
                region.src_offset as u32,
                &dst.handle(),
                region.dst_offset as u32,
                region.size as u32,
            )
            .unwrap();
    }

    unsafe fn clear_storage_texture(
        &mut self,
        _view: &WebGPUTexture,
        _array_layer: u32,
        _mip_level: u32,
        _values: [u32; 4],
    ) {
        todo!("TODO: Write a compute shader to clear storage textures")
    }

    unsafe fn clear_storage_buffer(
        &mut self,
        buffer: &WebGPUBuffer,
        offset: u64,
        length_in_u32s: u64,
        value: u32,
    ) {
        if buffer.is_mappable() {
            self.readback_syncs.insert(WebGPUReadbackBufferSync {
                src: buffer.handle().clone(),
                dst: buffer.readback_handle().map(|h| (*h).clone()),
                size: buffer.info().size as u32,
                _p: PhantomData,
            });
        }

        if value != 0 {
            todo!(
                "clear_storage_buffer is only implemented for value 0. TODO: Write a compute shader to clear buffers."
            )
        } else {
            let recording: &mut WebGPURecordingCommandBuffer = self.get_recording_mut();
            recording.end_non_rendering_encoders();
            recording.command_encoder.clear_buffer_with_u32_and_u32(
                &buffer.handle(),
                offset as u32,
                length_in_u32s as u32 * 4,
            );
        }
    }

    unsafe fn begin_render_pass(
        &mut self,
        renderpass_info: &gpu::RenderPassBeginInfo<WebGPUBackend>,
    ) {
        let mut color_attachments =
            SmallVec::<[JsNullable<GpuRenderPassColorAttachment>; 4]>::with_capacity(
                renderpass_info.render_targets.len(),
            );
        let mut color_formats = SmallVec::<[JsNullable<JsString>; 4]>::with_capacity(
            renderpass_info.render_targets.len(),
        );
        let mut color = [
            js_sys::Number::from(0),
            js_sys::Number::from(0),
            js_sys::Number::from(0),
            js_sys::Number::from(0),
        ];
        for color_rt in renderpass_info.render_targets.iter() {
            let (load_op, clear_color) = load_op_color_to_webgpu(&color_rt.load_op);
            let (store_op, resolve_attachment) = store_op_to_webgpu(&color_rt.store_op);
            for i in 0..4 {
                color[i] = js_sys::Number::from(clear_color.as_u32()[i]);
            }
            let descriptor = GpuRenderPassColorAttachment::new_with_gpu_texture_view(
                load_op,
                store_op,
                color_rt.view.handle(),
            );
            descriptor.set_clear_value(&color);
            if let Some(resolve_attachment) = resolve_attachment {
                descriptor.set_resolve_target_gpu_texture_view(resolve_attachment.view.handle());
            }
            color_attachments.push(JsNullable::wrap(descriptor));
            color_formats.push(JsNullable::wrap(
                JsValue::from(format_to_webgpu(
                    color_rt
                        .view
                        .info()
                        .format
                        .unwrap_or(color_rt.view.texture_info().format),
                ))
                .unchecked_into::<JsString>(),
            ));
        }
        let descriptor = GpuRenderPassDescriptor::new(&color_attachments);
        if let Some(depth_stencil) = renderpass_info.depth_stencil {
            let dsv_format = depth_stencil
                .view
                .info()
                .format
                .unwrap_or_else(|| depth_stencil.view.texture_info().format);

            let attachment = GpuRenderPassDepthStencilAttachment::new_with_gpu_texture_view(
                depth_stencil.view.handle(),
            );
            let (load_op, clear_value) = load_op_ds_to_webgpu(&depth_stencil.load_op);
            let (store_op, resolve_attachment) = store_op_to_webgpu(&depth_stencil.store_op);
            assert!(resolve_attachment.is_none());
            descriptor.set_depth_stencil_attachment(&attachment);
            let mut read_only = true;
            match &depth_stencil.store_op {
                gpu::StoreOp::Store => read_only = false,
                gpu::StoreOp::Resolve(_) => read_only = false,
                _ => {}
            }
            match &depth_stencil.load_op {
                gpu::LoadOpDepthStencil::Clear(_) => read_only = false,
                gpu::LoadOpDepthStencil::DontCare => read_only = false,
                _ => {}
            }
            if dsv_format.is_stencil() {
                attachment.set_stencil_clear_value(clear_value.stencil);
                attachment.set_stencil_load_op(load_op);
                attachment.set_stencil_store_op(store_op);
                attachment.set_stencil_read_only(read_only);
            }
            if dsv_format.is_depth() {
                attachment.set_depth_clear_value(clear_value.depth);
                attachment.set_depth_load_op(load_op);
                attachment.set_depth_store_op(store_op);
                attachment.set_depth_read_only(read_only);
            }
        }
        let recording = self.get_recording_mut();
        recording.end_non_rendering_encoders();
        recording.pass_encoder = WebGPUPassEncoder::Render(
            recording
                .command_encoder
                .begin_render_pass(&descriptor)
                .unwrap(),
        );
    }

    unsafe fn end_render_pass(&mut self) {
        let recording = self.get_recording_mut();
        recording.bound_pipeline = WebGPUBoundPipeline::None;
        recording.binding_manager.mark_all_dirty();
        let previous_encoder =
            std::mem::replace(&mut recording.pass_encoder, WebGPUPassEncoder::None);
        match &previous_encoder {
            WebGPUPassEncoder::Render(_) => {}
            _ => panic!("No active render pass."),
        };
    }

    unsafe fn barrier(&mut self, _barriers: &[gpu::Barrier<WebGPUBackend>]) {
        // Handled by the WebGPU implementation
    }

    unsafe fn reset(&mut self, frame: u64) {
        self.readback_syncs.clear();
        let handle = std::mem::replace(&mut self.handle, WebGPUCommandBufferHandle::Uninit);
        let mut binding_manager = match handle {
            WebGPUCommandBufferHandle::Finished(cmd_buffer) => cmd_buffer.binding_manager,
            WebGPUCommandBufferHandle::Reset(cmd_buffer) => cmd_buffer.binding_manager,
            WebGPUCommandBufferHandle::Recording(cmd_buffer) => cmd_buffer.binding_manager,
            _ => unreachable!(),
        };
        binding_manager.reset(frame);
        let encoder = self.device.create_command_encoder();
        self.handle = WebGPUCommandBufferHandle::Reset(WebGPUResetCommandBuffer {
            command_encoder: encoder,
            binding_manager,
            _p: PhantomData,
        });
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        _info: &gpu::BottomLevelAccelerationStructureInfo<WebGPUBackend>,
        _size: u64,
        _target_buffer: &WebGPUBuffer,
        _target_buffer_offset: u64,
        _scratch_buffer: &WebGPUBuffer,
        _scratch_buffer_offset: u64,
    ) -> WebGPUAccelerationStructure {
        panic!("WebGPU does not support ray tracing.");
    }

    unsafe fn upload_top_level_instances(
        &mut self,
        _instances: &[gpu::AccelerationStructureInstance<WebGPUBackend>],
        _target_buffer: &WebGPUBuffer,
        _target_buffer_offset: u64,
    ) {
        panic!("WebGPU does not support ray tracing.");
    }

    unsafe fn create_top_level_acceleration_structure(
        &mut self,
        _info: &gpu::TopLevelAccelerationStructureInfo<WebGPUBackend>,
        _size: u64,
        _target_buffer: &WebGPUBuffer,
        _target_buffer_offset: u64,
        _scratch_buffer: &WebGPUBuffer,
        _scratch_buffer_offset: u64,
    ) -> WebGPUAccelerationStructure {
        panic!("WebGPU does not support ray tracing.");
    }

    unsafe fn trace_ray(&mut self, _width: u32, _height: u32, _depth: u32) {
        panic!("WebGPU does not support ray tracing.");
    }

    unsafe fn begin_query(&mut self, query_index: u32) {
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        render_pass_encoder.begin_occlusion_query(query_index);
    }

    unsafe fn end_query(&mut self, _query_index: u32) {
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        render_pass_encoder.end_occlusion_query();
    }

    unsafe fn copy_query_results_to_buffer(
        &mut self,
        query_pool: &WebGPUQueryPool,
        start_index: u32,
        count: u32,
        buffer: &WebGPUBuffer,
        buffer_offset: u64,
    ) {
        if buffer.is_mappable() {
            self.readback_syncs.insert(WebGPUReadbackBufferSync {
                src: buffer.handle().clone(),
                dst: buffer.readback_handle().map(|h| (*h).clone()),
                size: buffer.info().size as u32,
                _p: PhantomData,
            });
        }

        let cmd_buffer = self.get_recording_mut();
        cmd_buffer.command_encoder.resolve_query_set_with_u32(
            &query_pool.handle(),
            start_index,
            count,
            &buffer.handle(),
            buffer_offset as u32,
        );
    }

    unsafe fn bind_storage_buffer_array(
        &mut self,
        _frequency: BindingFrequency,
        _binding: u32,
        _buffers: &[BufferArrayEntry<WebGPUBackend>],
    ) {
        panic!("WebGPU does not support binding buffer arrays")
    }

    unsafe fn bind_uniform_buffer_array(
        &mut self,
        _frequency: BindingFrequency,
        _binding: u32,
        _buffers: &[BufferArrayEntry<WebGPUBackend>],
    ) {
        panic!("WebGPU does not support binding buffer arrays")
    }

    // Barriers in WebGPU are handled automatically by the WebGPU implementation
    unsafe fn split_barrier_reset(&mut self, _split_barrier: &(), _after: BarrierSync) {}
    unsafe fn split_barrier_signal(
        &mut self,
        _split_barrier: &(),
        _barrier: Barrier<WebGPUBackend>,
    ) {
    }
    unsafe fn split_barrier_wait(&mut self, _waits: &[SplitBarrierWait<WebGPUBackend>]) {}
}

pub struct WebGPUCommandPool {
    device: GpuDevice,
    limits: WebGPULimits,
    _p: PhantomData<*const std::ffi::c_void>,
}

impl WebGPUCommandPool {
    pub(crate) fn new(device: &GpuDevice, limits: &WebGPULimits) -> Self {
        Self {
            device: device.clone(),
            limits: limits.clone(),
            _p: PhantomData,
        }
    }
}

impl gpu::CommandPool<WebGPUBackend> for WebGPUCommandPool {
    unsafe fn create_command_buffer(&mut self) -> WebGPUCommandBuffer {
        WebGPUCommandBuffer::new(&self.device, &self.limits)
    }

    unsafe fn reset(&mut self) {}
}
