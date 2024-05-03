use std::{ffi::c_void, sync::Arc};

use metal::{self, NSRange};

use smallvec::SmallVec;
use sourcerenderer_core::{align_up_32, gpu::{self, Texture}};

use super::*;

pub struct MTLCommandPool {
    queue: metal::CommandQueue,
    command_pool_type: gpu::CommandPoolType,
    meta_shaders: Arc<MTLMetaShaders>
}

impl MTLCommandPool {
    pub(crate) fn new(queue: &metal::CommandQueueRef, command_pool_type: gpu::CommandPoolType, meta_shaders: &Arc<MTLMetaShaders>) -> Self {
        Self {
            queue: queue.to_owned(),
            command_pool_type,
            meta_shaders: meta_shaders.clone()
        }
    }
}

impl gpu::CommandPool<MTLBackend> for MTLCommandPool {
    unsafe fn create_command_buffer(&mut self) -> MTLCommandBuffer {
        if self.command_pool_type == gpu::CommandPoolType::InnerCommandBuffers {
            return MTLCommandBuffer::new_without_cmd_buffer(&self.queue, &self.meta_shaders);
        }

        let cmd_buffer_handle_ref = self.queue.new_command_buffer_with_unretained_references();
        let cmd_buffer_handle: metal::CommandBuffer = cmd_buffer_handle_ref.to_owned();
        MTLCommandBuffer::new(&self.queue, cmd_buffer_handle, &self.meta_shaders)
    }

    unsafe fn reset(&mut self) {}
}

struct IndexBufferBinding {
    buffer: metal::Buffer,
    offset: u64,
    format: gpu::IndexFormat
}

fn index_format_to_mtl(index_format: gpu::IndexFormat) -> metal::MTLIndexType {
    match index_format {
        gpu::IndexFormat::U32 => metal::MTLIndexType::UInt32,
        gpu::IndexFormat::U16 => metal::MTLIndexType::UInt16
    }
}

struct MTLRenderPassState {
    render_encoder: Option<metal::RenderCommandEncoder>,
    parallel_encoder: Option<metal::ParallelRenderCommandEncoder>,
    render_pass: Vec<metal::RenderPassDescriptor>,
    subpass: u32,
}

pub struct MTLCommandBuffer {
    queue: metal::CommandQueue,
    command_buffer: Option<metal::CommandBuffer>,
    blit_encoder: Option<metal::BlitCommandEncoder>,
    render_pass: Option<MTLRenderPassState>,
    compute_encoder: Option<metal::ComputeCommandEncoder>,
    pre_event: metal::Event,
    post_event: metal::Event,
    index_buffer: Option<IndexBufferBinding>,
    primitive_type: metal::MTLPrimitiveType,
    resource_map: Option<Arc<PipelineResourceMap>>,
    binding: MTLBindingManager,
    meta_shaders: Arc<MTLMetaShaders>
}

impl MTLCommandBuffer {
    pub(crate) fn new(queue: &metal::CommandQueueRef, command_buffer: metal::CommandBuffer, meta_shaders: &Arc<MTLMetaShaders>) -> Self {
        Self {
            queue: queue.to_owned(),
            command_buffer: Some(command_buffer.clone()),
            render_pass: None,
            blit_encoder: None,
            compute_encoder: None,
            pre_event: queue.device().new_event(),
            post_event: queue.device().new_event(),
            index_buffer: None,
            primitive_type: metal::MTLPrimitiveType::Triangle,
            resource_map: None,
            binding: MTLBindingManager::new(),
            meta_shaders: meta_shaders.clone()
        }
    }

    pub(crate) fn new_without_cmd_buffer(queue: &metal::CommandQueueRef, meta_shaders: &Arc<MTLMetaShaders>) -> Self {
        Self {
            queue: queue.to_owned(),
            command_buffer: None,
            render_pass: None,
            blit_encoder: None,
            compute_encoder: None,
            pre_event: queue.device().new_event(),
            post_event: queue.device().new_event(),
            index_buffer: None,
            primitive_type: metal::MTLPrimitiveType::Triangle,
            resource_map: None,
            binding: MTLBindingManager::new(),
            meta_shaders: meta_shaders.clone()
        }
    }

    pub(crate) fn handle(&self) -> &metal::CommandBufferRef {
        self.command_buffer.as_ref().expect("Secondary command buffer doesnt have a Metal command buffer")
    }

    pub(crate) fn pre_event_handle(&self) -> &metal::EventRef {
        &self.pre_event
    }

    pub(crate) fn post_event_handle(&self) -> &metal::EventRef {
        &self.post_event
    }

    fn get_blit_encoder(&mut self) -> &metal::BlitCommandEncoder {
        assert!(self.render_pass.is_none());
        if self.blit_encoder.is_none() {
            if let Some(encoder) = &self.compute_encoder {
                encoder.end_encoding();
                self.compute_encoder = None;
            }
            self.blit_encoder = Some(self.handle().new_blit_command_encoder().to_owned());
        }
        self.blit_encoder.as_ref().unwrap()
    }

    fn get_compute_encoder(&mut self) -> &metal::ComputeCommandEncoder {
        assert!(self.render_pass.is_none());
        if self.compute_encoder.is_none() {
            if let Some(encoder) = &self.blit_encoder {
                encoder.end_encoding();
                self.blit_encoder = None;
            }
            self.binding.dirty_all();
            self.compute_encoder = Some(self.handle().compute_command_encoder_with_dispatch_type(metal::MTLDispatchType::Concurrent).to_owned());
        }
        self.compute_encoder.as_ref().unwrap()
    }

    fn get_render_pass_encoder(&self) -> &metal::RenderCommandEncoder {
        let encoder = &self.render_pass.as_ref().and_then(|r| r.render_encoder.as_ref()).expect("Need to start a render pass first");
        encoder
    }

    fn get_render_pass_encoder_opt(&self) -> Option<&metal::RenderCommandEncoder> {
        self.render_pass.as_ref().and_then(|r| r.render_encoder.as_ref())
    }

    fn end_non_rendering_encoders(&mut self) {
        if let Some(encoder) = &self.blit_encoder {
            encoder.end_encoding();
        }
        if let Some(encoder) = &self.compute_encoder {
            encoder.end_encoding();
        }
        self.blit_encoder = None;
        self.compute_encoder = None;
    }

    pub(crate) fn blit_rp(command_buffer: &metal::CommandBufferRef, meta_shaders: &Arc<MTLMetaShaders>, src_texture: &MTLTexture, src_array_layer: u32, src_mip_level: u32, dst_texture: &MTLTexture, dst_array_layer: u32, dst_mip_level: u32) {
        let new_view: Option<metal::Texture>;

        let descriptor = metal::RenderPassDescriptor::new();
        let attachment = descriptor.color_attachments().object_at(0).unwrap();
        attachment.set_load_action(metal::MTLLoadAction::DontCare);
        attachment.set_store_action(metal::MTLStoreAction::Store);
        attachment.set_level(dst_mip_level as u64);
        attachment.set_slice(dst_array_layer as u64);
        attachment.set_texture(Some(dst_texture.handle()));
        let encoder = command_buffer.new_render_command_encoder(&descriptor);
        encoder.set_render_pipeline_state(meta_shaders.blit_pipeline.handle());
        if src_array_layer == 0 && src_mip_level == 0 {
            encoder.set_fragment_texture(0, Some(src_texture.handle()));
        } else {
            new_view = Some(src_texture.handle().new_texture_view_from_slice(
                src_texture.handle().pixel_format(), src_texture.handle().texture_type(),
                metal::NSRange::new(src_mip_level as u64, 1), NSRange::new(src_array_layer as u64, 1)));
            encoder.set_fragment_texture(0, new_view.as_ref().map(|v| v as &metal::TextureRef));
        }
        encoder.set_fragment_texture(0, Some(src_texture.handle()));
        encoder.set_fragment_sampler_state(0, Some(&meta_shaders.linear_sampler));
        encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, 3);
        encoder.end_encoding();
    }
}

impl gpu::CommandBuffer<MTLBackend> for MTLCommandBuffer {
    unsafe fn set_pipeline(&mut self, pipeline: gpu::PipelineBinding<MTLBackend>) {
        match pipeline {
            gpu::PipelineBinding::Graphics(pipeline) => {
                self.primitive_type = pipeline.primitive_type();
                let encoder = self.get_render_pass_encoder();
                encoder.set_render_pipeline_state(pipeline.handle());
                encoder.set_cull_mode(pipeline.rasterizer_state().cull_mode);
                encoder.set_front_facing_winding(pipeline.rasterizer_state().front_face);
                encoder.set_triangle_fill_mode(pipeline.rasterizer_state().fill_mode);
                encoder.set_depth_stencil_state(pipeline.depth_stencil_state());
                self.resource_map = Some(pipeline.resource_map().clone());
            },
            gpu::PipelineBinding::Compute(pipeline) => {
                let encoder = self.get_compute_encoder();
                encoder.set_compute_pipeline_state(pipeline.handle());
                self.resource_map = Some(pipeline.resource_map().clone());
            },
            _ => unimplemented!()
        }
    }

    unsafe fn set_vertex_buffer(&mut self, vertex_buffer: &MTLBuffer, offset: u64) {
        let encoder = self.get_render_pass_encoder();
        encoder.set_vertex_buffer(0, Some(vertex_buffer.handle()), offset);
    }

    unsafe fn set_index_buffer(&mut self, index_buffer: &MTLBuffer, offset: u64, format: gpu::IndexFormat) {
        self.index_buffer = Some(IndexBufferBinding {
            buffer: index_buffer.handle().to_owned(),
            offset,
            format
        });
    }

    unsafe fn set_viewports(&mut self, viewports: &[ gpu::Viewport ]) {
        assert_eq!(viewports.len(), 1);
        let viewport = &viewports[0];
        let encoder = self.get_render_pass_encoder();
        encoder
            .set_viewport(metal::MTLViewport {
                originX: viewport.position.x as f64,
                originY: viewport.position.y as f64,
                width: viewport.extent.x as f64,
                height: viewport.extent.y as f64,
                znear: viewport.min_depth as f64,
                zfar: viewport.max_depth as f64,
            });
    }

    unsafe fn set_scissors(&mut self, scissors: &[ gpu::Scissor ]) {
        assert_eq!(scissors.len(), 1);
        let scissor = &scissors[0];
        let encoder = self.get_render_pass_encoder();
        encoder
            .set_scissor_rect(metal::MTLScissorRect {
                x: scissor.position.x as u64,
                y: scissor.position.y as u64,
                width: scissor.extent.x as u64,
                height: scissor.extent.y as u64
            });
    }

    unsafe fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_stage: gpu::ShaderType)
        where T: 'static + Send + Sync + Sized + Clone {
        if let Some(encoder) = self.get_render_pass_encoder_opt() {
            let resource_map = self.resource_map.as_ref().expect("Cannot set push constant data before binding a shader");
            let push_constant_info = resource_map.push_constants.get(&visible_for_shader_stage).expect("Shader does not have push constants");
            let data_size = std::mem::size_of_val(data);
            assert!(data_size <= push_constant_info.size as usize);
            if visible_for_shader_stage == gpu::ShaderType::VertexShader {
                encoder.set_vertex_bytes(push_constant_info.binding as u64, data_size as u64, data.as_ptr() as *const c_void);
            } else if visible_for_shader_stage == gpu::ShaderType::FragmentShader {
                encoder.set_fragment_bytes(push_constant_info.binding as u64, data_size as u64, data.as_ptr() as *const c_void);
            } else {
                panic!("Can only set vertex or fragment push constant data while in a render pass");
            }
        } else if visible_for_shader_stage == gpu::ShaderType::ComputeShader {
            let resource_map = self.resource_map.as_ref().expect("Cannot set push constant data before binding a shader");
            let push_constant_info = resource_map.push_constants.get(&visible_for_shader_stage).expect("Shader does not have push constants").clone();
            let encoder = self.get_compute_encoder();
            let data_size = std::mem::size_of_val(data);
            assert!(data_size <= push_constant_info.size as usize);
            encoder.set_bytes(push_constant_info.binding as u64, data_size as u64, data.as_ptr() as *const c_void);
        } else {
            unimplemented!()
        }
    }

    unsafe fn draw(&mut self, vertices: u32, offset: u32) {
        self.get_render_pass_encoder()
            .draw_primitives(self.primitive_type, offset as u64, vertices as u64);
    }

    unsafe fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
        let index_buffer = self.index_buffer.as_ref()
            .expect("No index buffer bound");

        self.get_render_pass_encoder()
            .draw_indexed_primitives(self.primitive_type, indices as u64, index_format_to_mtl(index_buffer.format), &index_buffer.buffer, index_buffer.offset as u64);
    }

    unsafe fn draw_indexed_indirect(&mut self, draw_buffer: &MTLBuffer, draw_buffer_offset: u32, count_buffer: &MTLBuffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        todo!()
    }

    unsafe fn draw_indirect(&mut self, draw_buffer: &MTLBuffer, draw_buffer_offset: u32, count_buffer: &MTLBuffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        todo!()
    }

    unsafe fn bind_sampling_view(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &MTLTextureView) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::SampledTexture(texture.handle()));
    }

    unsafe fn bind_sampling_view_and_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &MTLTextureView, sampler: &MTLSampler) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::SampledTextureAndSampler(texture.handle(), sampler.handle()));
    }

    unsafe fn bind_sampling_view_and_sampler_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures_and_samplers: &[(&MTLTextureView, &MTLSampler)]) {
        let handles: SmallVec<[(&metal::TextureRef, &metal::SamplerStateRef); 8]> = textures_and_samplers
            .iter()
            .map(|(tv, s)| (tv.handle(), s.handle()))
            .collect();
        self.binding.bind(frequency, binding, MTLBoundResourceRef::SampledTextureAndSamplerArray(&handles));
    }

    unsafe fn bind_storage_view_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures: &[&MTLTextureView]) {
        let handles: SmallVec<[(&metal::TextureRef); 8]> = textures
            .iter()
            .map(|tv| tv.handle())
            .collect();
        self.binding.bind(frequency, binding, MTLBoundResourceRef::StorageTextureArray(&handles));
    }

    unsafe fn bind_uniform_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &MTLBuffer, offset: u64, length: u64) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::UniformBuffer(MTLBufferBindingInfoRef {
            buffer: buffer.handle(), offset: offset, length: length
        }));
    }

    unsafe fn bind_storage_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &MTLBuffer, offset: u64, length: u64) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::UniformBuffer(MTLBufferBindingInfoRef {
            buffer: buffer.handle(), offset: offset, length: length
        }));
    }

    unsafe fn bind_storage_texture(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &MTLTextureView) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::SampledTexture(texture.handle()));
    }

    unsafe fn bind_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, sampler: &MTLSampler) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::Sampler(sampler.handle()));
    }

    unsafe fn bind_acceleration_structure(&mut self, frequency: gpu::BindingFrequency, binding: u32, acceleration_structure: &MTLAccelerationStructure) {
        unimplemented!()
    }

    unsafe fn finish_binding(&mut self) {
        if let Some(encoder) = self.compute_encoder.as_ref() {
            self.binding.finish(MTLEncoderRef::Compute(encoder), self.resource_map.as_ref().expect("Need to bind a shader before finishing binding."));
        }

        if let Some(rp) = self.render_pass.as_ref().and_then(|r| r.render_encoder.as_ref()) {
            self.binding.finish(MTLEncoderRef::Graphics(rp), self.resource_map.as_ref().expect("Need to bind a shader before finishing binding."));
        }
    }

    unsafe fn begin_label(&mut self, label: &str) {
        self.handle().push_debug_group(label);
    }

    unsafe fn end_label(&mut self) {
        self.handle().pop_debug_group();
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        let compute_encoder = self.get_compute_encoder();
        compute_encoder.dispatch_thread_groups(metal::MTLSize::new(group_count_x as u64, group_count_y as u64, group_count_z as u64), metal::MTLSize::new(8, 8, 1));
    }

    unsafe fn blit(&mut self, src_texture: &MTLTexture, src_array_layer: u32, src_mip_level: u32, dst_texture: &MTLTexture, dst_array_layer: u32, dst_mip_level: u32) {
        if dst_texture.info().usage.contains(gpu::TextureUsage::COPY_DST) {
            let encoder = self.get_blit_encoder();
            encoder.copy_from_texture(
                src_texture.handle(),
                src_array_layer as u64,
                src_mip_level as u64,
                metal::MTLOrigin { x: 0u64, y: 0u64, z: 0u64 },
                metal::MTLSize { width: (src_texture.info().width >> src_mip_level) as u64, height: (src_texture.info().height >> src_mip_level) as u64, depth: (src_texture.info().depth >> src_mip_level) as u64 },
                dst_texture.handle(),
                dst_array_layer as u64,
                dst_mip_level as u64,
                metal::MTLOrigin { x: 0u64, y: 0u64, z: 0u64 }
            );
        } else if dst_texture.info().usage.contains(gpu::TextureUsage::RENDER_TARGET) {
            Self::blit_rp(self.command_buffer.as_ref().unwrap(), &self.meta_shaders, src_texture, src_array_layer, src_mip_level, dst_texture, dst_array_layer, dst_mip_level);
        }
    }

    unsafe fn begin(&mut self, inheritance: Option<&Self::CommandBufferInheritance>) {
        if let Some(handle) = self.command_buffer.as_ref() {
            handle.encode_wait_for_event(&self.pre_event, 1);
        }
        if let Some(inheritance) = inheritance {
            self.render_pass = Some(MTLRenderPassState {
                render_encoder: Some(inheritance.render_command_encoder().to_owned()),
                parallel_encoder: None,
                render_pass: Vec::new(),
                subpass: 0
            });
        }
    }

    unsafe fn finish(&mut self) {
        self.end_non_rendering_encoders();
    }

    unsafe fn copy_buffer_to_texture(&mut self, src: &MTLBuffer, dst: &MTLTexture, region: &gpu::BufferTextureCopyRegion) {
        let blit_encoder = self.get_blit_encoder();
        let format = dst.info().format;
        let row_pitch = if region.buffer_row_pitch != 0 {
            region.buffer_row_pitch
        } else {
            (align_up_32(region.texture_extent.x, format.block_size().x) / format.block_size().x * format.element_size()) as u64
        };
        let slice_pitch = if region.buffer_slice_pitch != 0 {
            region.buffer_slice_pitch
        } else {
            (align_up_32(region.texture_extent.y, format.block_size().y) / format.block_size().y) as u64 * row_pitch
        };

        blit_encoder.copy_from_buffer_to_texture(
            src.handle(),
            region.buffer_offset,
            row_pitch,
            slice_pitch,
            metal::MTLSize {
                width: region.texture_extent.x as u64,
                height: region.texture_extent.y as u64,
                depth: region.texture_extent.z as u64
            },
            dst.handle(),
            region.texture_subresource.array_layer as u64,
            region.texture_subresource.mip_level as u64,
            metal::MTLOrigin {
                x: region.texture_offset.x as u64,
                y: region.texture_offset.y as u64,
                z: region.texture_offset.z as u64
            },
            metal::MTLBlitOption::empty()
        );
    }

    unsafe fn copy_buffer(&mut self, src: &MTLBuffer, dst: &MTLBuffer, region: &gpu::BufferCopyRegion) {
        let blit_encoder = self.get_blit_encoder();
        blit_encoder.copy_from_buffer(src.handle(), region.src_offset, dst.handle(), region.dst_offset, region.size);
    }

    unsafe fn clear_storage_texture(&mut self, view: &MTLTexture, array_layer: u32, mip_level: u32, values: [u32; 4]) {
        todo!()
    }

    unsafe fn clear_storage_buffer(&mut self, buffer: &MTLBuffer, offset: u64, length_in_u32s: u64, value: u32) {
        todo!()
    }

    unsafe fn begin_render_pass(&mut self, renderpass_info: &gpu::RenderPassBeginInfo<MTLBackend>, recording_mode: gpu::RenderpassRecordingMode) {
        assert!(self.render_pass.is_none());
        self.end_non_rendering_encoders();
        let descriptors = render_pass_to_descriptors(renderpass_info);
        let first_descriptor = descriptors[0].clone();
        if recording_mode == gpu::RenderpassRecordingMode::Commands {
            self.binding.dirty_all();
            self.render_pass = Some(MTLRenderPassState {
                render_pass: descriptors,
                parallel_encoder: None,
                render_encoder: Some(self.handle().new_render_command_encoder(&first_descriptor).to_owned()),
                subpass: 0,
            });
        } else {
            assert_eq!(descriptors.len(), 1);
            self.binding.dirty_all();
            let parallel_encoder = self.handle().new_parallel_render_command_encoder(&first_descriptor).to_owned();
            self.render_pass = Some(MTLRenderPassState {
                render_pass: descriptors,
                parallel_encoder: Some(parallel_encoder),
                render_encoder: None,
                subpass: 0,
            });
        }
    }

    unsafe fn advance_subpass(&mut self) {
        let renderpass_state = self.render_pass.as_mut().unwrap();
        let subpass = renderpass_state.subpass + 1;
        renderpass_state.subpass = subpass;
        renderpass_state.render_encoder = Some(self.command_buffer.as_ref().unwrap().new_render_command_encoder(&renderpass_state.render_pass[subpass as usize]).to_owned());
    }

    unsafe fn end_render_pass(&mut self) {
        if let Some(encoder) = self.get_render_pass_encoder_opt() {
            encoder.end_encoding();
        }
        if let Some(encoder) = self.render_pass.as_ref().and_then(|r| r.parallel_encoder.as_ref()) {
            encoder.end_encoding();
        }
        self.render_pass = None;
    }

    unsafe fn barrier(&mut self, _barriers: &[gpu::Barrier<MTLBackend>]) {
        // No-op, all writable resources are tracked by the Metal driver
    }

    unsafe fn inheritance(&self) -> &Self::CommandBufferInheritance {
        self.render_pass.as_ref().and_then(|r| r.parallel_encoder.as_ref()).expect("Need to start render pass first")
    }

    type CommandBufferInheritance = metal::ParallelRenderCommandEncoder;

    unsafe fn execute_inner(&mut self, _submission: &[&MTLCommandBuffer]) {
        // Done automatically
    }

    unsafe fn reset(&mut self, frame: u64) {
        self.end_non_rendering_encoders();
        assert!(self.render_pass.is_none());
        assert!(self.compute_encoder.is_none());
        assert!(self.blit_encoder.is_none());
        if let Some(command_buffer) = self.command_buffer.as_mut() {
            assert_eq!(command_buffer.status(), metal::MTLCommandBufferStatus::Completed);
            *command_buffer = self.queue.new_command_buffer_with_unretained_references().to_owned();
        }

        self.pre_event = self.queue.device().new_event();
        self.post_event = self.queue.device().new_event();
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>,
        size: u64,
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &MTLBuffer,
        scratch_buffer_offset: u64
      ) -> MTLAccelerationStructure {
        todo!()
    }

    unsafe fn upload_top_level_instances(
        &mut self,
        instances: &[gpu::AccelerationStructureInstance<MTLBackend>],
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64
      ) {
        todo!()
    }

    unsafe fn create_top_level_acceleration_structure(
        &mut self,
        info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>,
        size: u64,
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &MTLBuffer,
        scratch_buffer_offset: u64
      ) -> MTLAccelerationStructure {
        todo!()
    }

    unsafe fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
        todo!()
    }
}

impl Drop for MTLCommandBuffer {
    fn drop(&mut self) {
        if let Some(encoder) = self.get_render_pass_encoder_opt() {
            encoder.end_encoding();
        }
        if let Some(encoder) = self.render_pass.as_ref().and_then(|r| r.parallel_encoder.as_ref()) {
            encoder.end_encoding();
        }
        if let Some(command_buffer) = self.command_buffer.as_ref() {
            assert_eq!(command_buffer.status(), metal::MTLCommandBufferStatus::Completed);
        }
        self.render_pass = None;
        self.end_non_rendering_encoders();
    }
}
