use js_sys::{wasm_bindgen::JsValue, Array};
use log::warn;
use sourcerenderer_core::gpu::{self, Buffer};
use web_sys::{GpuCommandBuffer, GpuCommandEncoder, GpuComputePassEncoder, GpuDevice, GpuIndexFormat, GpuRenderBundle, GpuRenderBundleDescriptor, GpuRenderBundleEncoder, GpuRenderPassEncoder, GpuTexelCopyTextureInfo};

use crate::{binding::{WebGPUBindingManager, WebGPUBoundResourceRef, WebGPUBufferBindingInfo, WebGPUHashableSampler, WebGPUHashableTextureView}, buffer::WebGPUBuffer, sampler::WebGPUSampler, stubs::WebGPUAccelerationStructure, texture::{WebGPUTexture, WebGPUTextureView}, WebGPUBackend};

enum WebGPUPassEncoder {
    None,
    Render(GpuRenderPassEncoder),
    Compute(GpuComputePassEncoder)
}

struct WebGPURecordingCommandBuffer {
    command_encoder: GpuCommandEncoder,
    pass_encoder: WebGPUPassEncoder
}

struct WebGPUFinishedCommandBuffer {
    command_buffer: GpuCommandBuffer
}

struct WebGPURenderBundleCommandBuffer {
    bundle: GpuRenderBundleEncoder
}

struct WebGPUFinishedRenderBundleCommandBuffer {
    bundle: GpuRenderBundle
}

struct WebGPURenderBundleInheritance {
    descriptor: GpuRenderBundleDescriptor
}

enum WebGPUCommandBufferHandle {
    Recording(WebGPURecordingCommandBuffer),
    Finished(WebGPUFinishedCommandBuffer),
    Secondary(WebGPURenderBundleCommandBuffer),
    SecondaryFinished(WebGPUFinishedRenderBundleCommandBuffer)

}

pub struct WebGPUCommandBuffer {
    handle: WebGPUCommandBufferHandle,
    binding_manager: WebGPUBindingManager,
    is_inner: bool,
    device: GpuDevice,
    frame: u64,
}

unsafe impl Send for WebGPUCommandBuffer {}
unsafe impl Sync for WebGPUCommandBuffer {}

unsafe impl Send for WebGPURenderBundleInheritance {}
unsafe impl Sync for WebGPURenderBundleInheritance {}

impl WebGPUCommandBuffer {
    fn get_recording(&self) -> &WebGPURecordingCommandBuffer {
        match &self.handle {
            WebGPUCommandBufferHandle::Recording(cmd_buffer) => cmd_buffer,
            WebGPUCommandBufferHandle::Finished(_cmd_buffer) => panic!("Command buffer is finished"),
            _ => panic!("Secondary command buffers aren't supported here")
        }
    }

    fn get_recording_mut(&mut self) -> &mut WebGPURecordingCommandBuffer {
        match &mut self.handle {
            WebGPUCommandBufferHandle::Recording(cmd_buffer) => cmd_buffer,
            WebGPUCommandBufferHandle::Finished(_cmd_buffer) => panic!("Command buffer is finished"),
            _ => panic!("Secondary command buffers aren't supported here")
        }
    }

    fn get_recording_inner(&self) -> &GpuRenderBundleEncoder {
        match &self.handle {
            WebGPUCommandBufferHandle::Secondary(cmd_buffer) => &cmd_buffer.bundle,
            WebGPUCommandBufferHandle::SecondaryFinished(_cmd_buffer) => panic!("Command buffer is finished"),
            _ => panic!("Primary command buffers aren't supported here")
        }
    }
}

impl WebGPURecordingCommandBuffer {
    fn get_compute_encoder(&mut self) -> &GpuComputePassEncoder {
        let mut has_active_compute_encoder = false;
        match &mut self.pass_encoder {
            WebGPUPassEncoder::Render(render) => { render.end(); },
            WebGPUPassEncoder::Compute(_compute) => { has_active_compute_encoder = true; },
            _ => {}
        }
        if !has_active_compute_encoder {
            self.pass_encoder = WebGPUPassEncoder::Compute(self.command_encoder.begin_compute_pass());
        }
        match &self.pass_encoder {
            WebGPUPassEncoder::Compute(compute) => return compute,
            _ => unreachable!()
        }
    }

    fn get_render_encoder(&mut self) -> &GpuRenderPassEncoder {
        match &self.pass_encoder {
            WebGPUPassEncoder::Render(render) => return render,
            _ => panic!("No active render pass")
        }
    }

    fn ensure_no_active_pass(&mut self) {
        match &self.pass_encoder {
            WebGPUPassEncoder::Compute(compute) => compute.end(),
            WebGPUPassEncoder::Render(render) => render.end(),
            _ => {}
        }
        self.pass_encoder = WebGPUPassEncoder::None;
    }
}

impl gpu::CommandBuffer<WebGPUBackend> for WebGPUCommandBuffer {
    unsafe fn set_pipeline(&mut self, pipeline: gpu::PipelineBinding<WebGPUBackend>) {
        let cmd_buffer = self.get_recording_mut();
        match pipeline {
            gpu::PipelineBinding::Graphics(graphics_pipeline) => {
                cmd_buffer.get_render_encoder().set_pipeline(graphics_pipeline.handle());
            },
            gpu::PipelineBinding::Compute(compute_pipeline) =>  {
                cmd_buffer.get_compute_encoder().set_pipeline(compute_pipeline.handle());
            },
            gpu::PipelineBinding::RayTracing(_) => panic!("WebGPU does not support ray tracing"),
        }
    }

    unsafe fn set_vertex_buffer(&mut self, vertex_buffer: &WebGPUBuffer, offset: u64) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            let render_pass_encoder = cmd_buffer.get_render_encoder();
            render_pass_encoder.set_vertex_buffer_with_u32_and_u32(0, Some(&vertex_buffer.handle()), offset as u32, vertex_buffer.info().size as u32 - offset as u32);
        } else {
            let render_bundle_encoder = self.get_recording_inner();
            render_bundle_encoder.set_vertex_buffer_with_u32_and_u32(0, Some(&vertex_buffer.handle()), offset as u32, vertex_buffer.info().size as u32 - offset as u32);
        }
    }

    unsafe fn set_index_buffer(&mut self, index_buffer: &WebGPUBuffer, offset: u64, format: gpu::IndexFormat) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            let render_pass_encoder = cmd_buffer.get_render_encoder();
            render_pass_encoder.set_index_buffer_with_u32_and_u32(
                &index_buffer.handle(),
                match format {
                    gpu::IndexFormat::U16 => GpuIndexFormat::Uint16,
                    gpu::IndexFormat::U32 => GpuIndexFormat::Uint32,
                },
                offset as u32,
                index_buffer.info().size as u32 - offset as u32);
        } else {
            let render_bundle_encoder = self.get_recording_inner();
            render_bundle_encoder.set_index_buffer_with_u32_and_u32(
                &index_buffer.handle(),
                match format {
                    gpu::IndexFormat::U16 => GpuIndexFormat::Uint16,
                    gpu::IndexFormat::U32 => GpuIndexFormat::Uint32,
                },
                offset as u32,
                index_buffer.info().size as u32 - offset as u32);
        }
    }

    unsafe fn set_viewports(&mut self, viewports: &[ gpu::Viewport ]) {
        if self.is_inner {
            panic!("Not supported in inner command buffer");
        }
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        assert_eq!(viewports.len(), 1);
        let viewport = &viewports[0];
        render_pass_encoder.set_viewport(viewport.position.x, viewport.position.y, viewport.extent.x, viewport.extent.y, viewport.min_depth, viewport.max_depth);
    }

    unsafe fn set_scissors(&mut self, scissors: &[ gpu::Scissor ]) {
        if self.is_inner {
            panic!("Not supported in inner command buffer");
        }
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        assert_eq!(scissors.len(), 1);
        let scissor = &scissors[0];
        render_pass_encoder.set_scissor_rect(scissor.position.x as u32, scissor.position.y as u32, scissor.extent.x, scissor.extent.y);
    }

    unsafe fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_stage: gpu::ShaderType)
        where T: 'static + Send + Sync + Sized + Clone {
        todo!()
    }

    unsafe fn draw(&mut self, vertices: u32, offset: u32) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            let render_pass_encoder = cmd_buffer.get_render_encoder();
            assert_eq!(offset, 0);
            render_pass_encoder.draw_with_instance_count_and_first_vertex(vertices, 1, offset);
        } else {
            let render_bundle_encoder = self.get_recording_inner();
            assert_eq!(offset, 0);
            render_bundle_encoder.draw_with_instance_count_and_first_vertex(vertices, 1, offset);
        }
    }

    unsafe fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            let render_pass_encoder = cmd_buffer.get_render_encoder();
            render_pass_encoder.draw_indexed_with_instance_count_and_first_index_and_base_vertex_and_first_instance(indices, instances, first_index, vertex_offset, first_instance);
        } else {
            let render_bundle_encoder = self.get_recording_inner();
            render_bundle_encoder.draw_indexed_with_instance_count_and_first_index_and_base_vertex_and_first_instance(indices, instances, first_index, vertex_offset, first_instance);
        }
    }

    unsafe fn draw_indexed_indirect(&mut self, draw_buffer: &WebGPUBuffer, draw_buffer_offset: u32, _count_buffer: &WebGPUBuffer, _count_buffer_offset: u32, _max_draw_count: u32, _stride: u32) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            let render_pass_encoder = cmd_buffer.get_render_encoder();
            warn!("WebGPU does not support multi draw indirect");
            render_pass_encoder.draw_indexed_indirect_with_u32(&draw_buffer.handle(), draw_buffer_offset);
        } else {
            let render_bundle_encoder = self.get_recording_inner();
            warn!("WebGPU does not support multi draw indirect");
            render_bundle_encoder.draw_indexed_indirect_with_u32(&draw_buffer.handle(), draw_buffer_offset);
        }
    }

    unsafe fn draw_indirect(&mut self, draw_buffer: &WebGPUBuffer, draw_buffer_offset: u32, _count_buffer: &WebGPUBuffer, _count_buffer_offset: u32, _max_draw_count: u32, _stride: u32) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            let render_pass_encoder = cmd_buffer.get_render_encoder();
            warn!("WebGPU does not support multi draw indirect");
            render_pass_encoder.draw_indirect_with_u32(&draw_buffer.handle(), draw_buffer_offset);
        } else {
            let render_bundle_encoder = self.get_recording_inner();
            warn!("WebGPU does not support multi draw indirect");
            render_bundle_encoder.draw_indirect_with_u32(&draw_buffer.handle(), draw_buffer_offset);
        }
    }

    unsafe fn bind_sampling_view(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &WebGPUTextureView) {
        self.binding_manager.bind(frequency, binding, WebGPUBoundResourceRef::SampledTexture(WebGPUHashableTextureView::from(texture)));
    }

    unsafe fn bind_sampling_view_and_sampler(&mut self, _frequency: gpu::BindingFrequency, _binding: u32, _texture: &WebGPUTextureView, _sampler: &WebGPUSampler) {
        panic!("WebGPU does not support combined textures and samplers");
    }

    unsafe fn bind_sampling_view_and_sampler_array(&mut self, _frequency: gpu::BindingFrequency, _binding: u32, _textures_and_samplers: &[(&WebGPUTextureView, &WebGPUSampler)]) {
        panic!("WebGPU does not support binding arrays");
    }

    unsafe fn bind_storage_view_array(&mut self, _frequency: gpu::BindingFrequency, _binding: u32, _textures: &[&WebGPUTextureView]) {
        panic!("WebGPU does not support binding arrays");
    }

    unsafe fn bind_uniform_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &WebGPUBuffer, offset: u64, length: u64) {
        self.binding_manager.bind(frequency, binding, WebGPUBoundResourceRef::UniformBuffer(WebGPUBufferBindingInfo {
            buffer: buffer.handle().clone(),
            offset,
            length,
        }));
    }

    unsafe fn bind_storage_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &WebGPUBuffer, offset: u64, length: u64) {
        self.binding_manager.bind(frequency, binding, WebGPUBoundResourceRef::StorageBuffer(WebGPUBufferBindingInfo {
            buffer: buffer.handle().clone(),
            offset,
            length,
        }));
    }

    unsafe fn bind_storage_texture(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &WebGPUTextureView) {
        self.binding_manager.bind(frequency, binding, WebGPUBoundResourceRef::StorageTexture(WebGPUHashableTextureView::from(texture)));
    }

    unsafe fn bind_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, sampler: &WebGPUSampler) {
        self.binding_manager.bind(frequency, binding, WebGPUBoundResourceRef::Sampler(WebGPUHashableSampler::from(sampler)));
    }

    unsafe fn bind_acceleration_structure(&mut self, _frequency: gpu::BindingFrequency, _binding: u32, _acceleration_structure: &WebGPUAccelerationStructure) {
        panic!("WebGPU does not support ray tracing");
    }

    unsafe fn finish_binding(&mut self) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            match &cmd_buffer.pass_encoder {
                WebGPUPassEncoder::None => {},
                WebGPUPassEncoder::Render(gpu_render_pass_encoder) => {
                    gpu_render_pass_encoder.get_
                },
                WebGPUPassEncoder::Compute(gpu_compute_pass_encoder) => todo!(),
            }
            match self.
            self.binding_manager.finish(self.frame, )
        }
    }

    unsafe fn begin_label(&mut self, label: &str) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            cmd_buffer.command_encoder.push_debug_group(label);
        } else {
            let encoder = self.get_recording_inner();
            encoder.push_debug_group(label);
        }
    }

    unsafe fn end_label(&mut self) {
        if !self.is_inner {
            let cmd_buffer = self.get_recording_mut();
            cmd_buffer.command_encoder.pop_debug_group();
        } else {
            let encoder = self.get_recording_inner();
            encoder.pop_debug_group();
        }
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        if self.is_inner {
            panic!("Not supported in inner command buffer");
        }
        let cmd_buffer = self.get_recording_mut();
        let compute_pass_encoder = cmd_buffer.get_compute_encoder();
        compute_pass_encoder.dispatch_workgroups_with_workgroup_count_y_and_workgroup_count_z(group_count_x, group_count_y, group_count_z);
    }

    unsafe fn blit(&mut self, src_texture: &WebGPUTexture, src_array_layer: u32, src_mip_level: u32, dst_texture: &WebGPUTexture, dst_array_layer: u32, dst_mip_level: u32) {
        if self.is_inner {
            panic!("Not supported in inner command buffer");
        }
        let cmd_buffer = self.get_recording_mut();
        cmd_buffer.ensure_no_active_pass();
        /*cmd_buffer.command_encoder.copy_texture_to_texture_with_gpu_extent_3d_dict(&GpuTexelCopyTextureInfo {
            obj: todo!(),
        }, destination, copy_size)*/
    }

    unsafe fn begin(&mut self, frame: u64, inheritance: Option<&Self::CommandBufferInheritance>) {
        todo!()
    }

    unsafe fn finish(&mut self) {
        if !self.is_inner {
            let cmd_buffer = {
                let cmd_encoder = self.get_recording_mut();
                cmd_encoder.ensure_no_active_pass();
                cmd_encoder.command_encoder.finish()
            };
            self.handle = WebGPUCommandBufferHandle::Finished(WebGPUFinishedCommandBuffer { command_buffer: cmd_buffer });
        } else {
            let render_bundle_encoder = self.get_recording_inner();
            let render_bundle = render_bundle_encoder.finish();
            self.handle = WebGPUCommandBufferHandle::SecondaryFinished(WebGPUFinishedRenderBundleCommandBuffer { bundle: render_bundle });
        }
    }

    unsafe fn copy_buffer_to_texture(&mut self, src: &WebGPUBuffer, dst: &WebGPUTexture, region: &gpu::BufferTextureCopyRegion) {
        todo!()
    }

    unsafe fn copy_buffer(&mut self, src: &WebGPUBuffer, dst: &WebGPUBuffer, region: &gpu::BufferCopyRegion) {
        todo!()
    }

    unsafe fn clear_storage_texture(&mut self, view: &WebGPUTexture, array_layer: u32, mip_level: u32, values: [u32; 4]) {
        todo!()
    }

    unsafe fn clear_storage_buffer(&mut self, buffer: &WebGPUBuffer, offset: u64, length_in_u32s: u64, value: u32) {
        todo!()
    }

    unsafe fn begin_render_pass(&mut self, renderpass_info: &gpu::RenderPassBeginInfo<WebGPUBackend>, recording_mode: gpu::RenderpassRecordingMode) {
        todo!()
    }

    unsafe fn end_render_pass(&mut self) {
        todo!()
    }

    unsafe fn barrier(&mut self, barriers: &[gpu::Barrier<WebGPUBackend>]) {
        todo!()
    }

    unsafe fn inheritance(&self) -> &Self::CommandBufferInheritance {
        todo!()
    }

    type CommandBufferInheritance = WebGPURenderBundleInheritance;

    unsafe fn execute_inner(&mut self, submission: &[&WebGPUCommandBuffer]) {
        let cmd_buffer = self.get_recording_mut();
        let render_pass_encoder = cmd_buffer.get_render_encoder();
        let array = Array::new_with_length(submission.len() as u32);
        for i in 0..submission.len() {
            let cmd_buffer_handle = &submission[i].handle;
            match cmd_buffer_handle {
                WebGPUCommandBufferHandle::Recording(_) => panic!("execute_inner can only execute inner command buffers"),
                WebGPUCommandBufferHandle::Finished(_) => panic!("execute_inner can only execute inner command buffers"),
                WebGPUCommandBufferHandle::Secondary(_) => panic!("Inner command buffer is not finished yet."),
                WebGPUCommandBufferHandle::SecondaryFinished(inner) => array.set(i as u32, JsValue::from(&inner.bundle)),
            }
        }
        render_pass_encoder.execute_bundles(&array);
    }

    unsafe fn reset(&mut self, frame: u64) {
        if !self.is_inner {
            let encoder = self.device.create_command_encoder();
            self.handle = WebGPUCommandBufferHandle::Recording(WebGPURecordingCommandBuffer { command_encoder: encoder, pass_encoder: WebGPUPassEncoder::None });
        } else {
            let encoder = self.device.create_render_bundle_encoder(self.)
        }
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        _info: &gpu::BottomLevelAccelerationStructureInfo<WebGPUBackend>,
        _size: u64,
        _target_buffer: &WebGPUBuffer,
        _target_buffer_offset: u64,
        _scratch_buffer: &WebGPUBuffer,
        _scratch_buffer_offset: u64
      ) -> WebGPUAccelerationStructure {
        panic!("WebGPU does not support ray tracing.");
    }

    unsafe fn upload_top_level_instances(
        &mut self,
        _instances: &[gpu::AccelerationStructureInstance<WebGPUBackend>],
        _target_buffer: &WebGPUBuffer,
        _target_buffer_offset: u64
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
        _scratch_buffer_offset: u64
      ) -> WebGPUAccelerationStructure {
        panic!("WebGPU does not support ray tracing.");
    }

    unsafe fn trace_ray(&mut self, _width: u32, _height: u32, _depth: u32) {
        panic!("WebGPU does not support ray tracing.");
    }
}