use std::{ffi::{c_void, CStr}, ptr::NonNull, sync::{Arc, Mutex}};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSInteger, NSRange, NSString, NSUInteger};
use objc2_metal::{self, MTLAccelerationStructureCommandEncoder as _, MTLBlitCommandEncoder, MTLBuffer as _, MTLCommandBuffer as _, MTLCommandEncoder as _, MTLCommandQueue as _, MTLComputeCommandEncoder as _, MTLDevice as _, MTLIndirectCommandBuffer as _, MTLParallelRenderCommandEncoder as _, MTLRenderCommandEncoder, MTLTexture as _};

use smallvec::SmallVec;
use sourcerenderer_core::{align_up_32, gpu::{self, Texture as _}};

use super::*;

pub struct MTLCommandPool {
    queue: Retained<ProtocolObject<dyn objc2_metal::MTLCommandQueue>>,
    command_pool_type: gpu::CommandPoolType,
    shared: Arc<MTLShared>
}

unsafe impl Send for MTLCommandPool {}
unsafe impl Sync for MTLCommandPool {}

impl MTLCommandPool {
    pub(crate) fn new(queue: &ProtocolObject<dyn objc2_metal::MTLCommandQueue>, command_pool_type: gpu::CommandPoolType, shared: &Arc<MTLShared>) -> Self {
        Self {
            queue: Retained::from(queue),
            command_pool_type,
            shared: shared.clone()
        }
    }
}

impl gpu::CommandPool<MTLBackend> for MTLCommandPool {
    unsafe fn create_command_buffer(&mut self) -> MTLCommandBuffer {
        if self.command_pool_type == gpu::CommandPoolType::InnerCommandBuffers {
            return MTLCommandBuffer::new_without_cmd_buffer(&self.queue, &self.shared);
        }

        let cmd_buffer_handle = self.queue.commandBufferWithUnretainedReferences().unwrap();
        MTLCommandBuffer::enable_error_tracking(&cmd_buffer_handle);
        MTLCommandBuffer::new(&self.queue, cmd_buffer_handle, &self.shared)
    }

    unsafe fn reset(&mut self) {}
}

struct IndexBufferBinding {
    buffer: Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    offset: u64,
    format: gpu::IndexFormat
}

pub(crate) fn index_format_to_mtl(index_format: gpu::IndexFormat) -> objc2_metal::MTLIndexType {
    match index_format {
        gpu::IndexFormat::U32 => objc2_metal::MTLIndexType::UInt32,
        gpu::IndexFormat::U16 => objc2_metal::MTLIndexType::UInt16
    }
}

pub(crate) fn index_format_size(index_format: gpu::IndexFormat) -> usize {
    match index_format {
        gpu::IndexFormat::U16 => 2,
        gpu::IndexFormat::U32 => 4
    }
}

pub(crate) fn format_to_mtl_attribute_format(format: gpu::Format) -> objc2_metal::MTLAttributeFormat {
    match format {
        gpu::Format::R32Float => objc2_metal::MTLAttributeFormat::Float,
        gpu::Format::RG32Float => objc2_metal::MTLAttributeFormat::Float2,
        gpu::Format::RGB32Float => objc2_metal::MTLAttributeFormat::Float3,
        gpu::Format::RGBA32Float => objc2_metal::MTLAttributeFormat::Float4,
        gpu::Format::R32UInt => objc2_metal::MTLAttributeFormat::UInt,
        gpu::Format::R8Unorm => objc2_metal::MTLAttributeFormat::UCharNormalized,
        _ => todo!("Unsupported format")
    }
}

#[allow(dead_code)] // Read by the GPU
struct MTLMDIParams {
    indirect_cmd_buffer: objc2_metal::MTLResourceID,
    draw_buffer: u64,
    count_buffer: u64,
    stride: usize,
    primitive_type: objc2_metal::MTLPrimitiveType
}

enum MTLEncoder {
    RenderPass {
        encoder: Retained<ProtocolObject<dyn objc2_metal::MTLRenderCommandEncoder>>,
        render_pass: Retained<objc2_metal::MTLRenderPassDescriptor>,
    },
    Parallel(Retained<ProtocolObject<dyn objc2_metal::MTLParallelRenderCommandEncoder>>),
    Compute(Retained<ProtocolObject<dyn objc2_metal::MTLComputeCommandEncoder>>),
    Blit(Retained<ProtocolObject<dyn objc2_metal::MTLBlitCommandEncoder>>),
    AccelerationStructure(Retained<ProtocolObject<dyn objc2_metal::MTLAccelerationStructureCommandEncoder>>),
    None
}

pub struct MTLInnerCommandBufferInheritance {
    encoders: Vec<Retained<ProtocolObject<dyn objc2_metal::MTLRenderCommandEncoder>>>,
    descriptor: Retained<objc2_metal::MTLRenderPassDescriptor>
}

unsafe impl Send for MTLInnerCommandBufferInheritance {}
unsafe impl Sync for MTLInnerCommandBufferInheritance {}

impl Drop for MTLInnerCommandBufferInheritance {
    fn drop(&mut self) {
        assert!(self.encoders.is_empty());
    }
}

pub struct MTLCommandBuffer {
    queue: Retained<ProtocolObject<dyn objc2_metal::MTLCommandQueue>>,
    command_buffer: Option<Retained<ProtocolObject<dyn objc2_metal::MTLCommandBuffer>>>,
    encoder: MTLEncoder,
    pre_event: Retained<ProtocolObject<dyn objc2_metal::MTLEvent>>,
    post_event: Retained<ProtocolObject<dyn objc2_metal::MTLEvent>>,
    index_buffer: Option<IndexBufferBinding>,
    primitive_type: objc2_metal::MTLPrimitiveType,
    resource_map: Option<Arc<PipelineResourceMap>>,
    binding: MTLBindingManager,
    shared: Arc<MTLShared>
}

unsafe impl Send for MTLCommandBuffer {}
unsafe impl Sync for MTLCommandBuffer {}

impl MTLCommandBuffer {
    pub(crate) fn new(queue: &ProtocolObject<dyn objc2_metal::MTLCommandQueue>, command_buffer: Retained<ProtocolObject<dyn objc2_metal::MTLCommandBuffer>>, shared: &Arc<MTLShared>) -> Self {
        Self {
            queue: Retained::from(queue),
            command_buffer: Some(command_buffer.clone()),
            encoder: MTLEncoder::None,
            pre_event: queue.device().newEvent().unwrap(),
            post_event: queue.device().newEvent().unwrap(),
            index_buffer: None,
            primitive_type: objc2_metal::MTLPrimitiveType::Triangle,
            resource_map: None,
            binding: MTLBindingManager::new(),
            shared: shared.clone()
        }
    }

    pub(crate) fn new_without_cmd_buffer(queue: &ProtocolObject<dyn objc2_metal::MTLCommandQueue>, shared: &Arc<MTLShared>) -> Self {
        Self {
            queue: Retained::from(queue),
            command_buffer: None,
            encoder: MTLEncoder::None,
            pre_event: queue.device().newEvent().unwrap(),
            post_event: queue.device().newEvent().unwrap(),
            index_buffer: None,
            primitive_type: objc2_metal::MTLPrimitiveType::Triangle,
            resource_map: None,
            binding: MTLBindingManager::new(),
            shared: shared.clone()
        }
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLCommandBuffer> {
        self.command_buffer.as_ref().expect("Secondary command buffer doesnt have a Metal command buffer")
    }

    pub(crate) fn pre_event_handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLEvent> {
        &self.pre_event
    }

    pub(crate) fn post_event_handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLEvent> {
        &self.post_event
    }

    fn get_blit_encoder(&mut self) -> &ProtocolObject<dyn objc2_metal::MTLBlitCommandEncoder> {
        let has_existing_encoder = if let MTLEncoder::Blit(_) = &self.encoder {
            true
        } else {
            false
        };

        if !has_existing_encoder {
            self.end_non_rendering_encoders();
            let encoder = self.handle().blitCommandEncoder().unwrap();
            self.encoder = MTLEncoder::Blit(encoder);
        }
        if let MTLEncoder::Blit(encoder) = &self.encoder {
            encoder
        } else {
            unreachable!()
        }
    }

    fn get_compute_encoder(&mut self) -> &ProtocolObject<dyn objc2_metal::MTLComputeCommandEncoder> {
        let has_existing_encoder = if let MTLEncoder::Compute(_) = &self.encoder {
            true
        } else {
            false
        };

        if !has_existing_encoder {
            self.end_non_rendering_encoders();
            let encoder = self.handle().computeCommandEncoderWithDispatchType(objc2_metal::MTLDispatchType::Concurrent).unwrap();
            let heap_list = self.shared.heap_list.read().unwrap();
            for heap in heap_list.iter() {
                encoder.useHeap(&heap);
            }
            self.encoder = MTLEncoder::Compute(encoder);
        }
        if let MTLEncoder::Compute(encoder) = &self.encoder {
            encoder
        } else {
            unreachable!()
        }
    }

    unsafe fn get_acceleration_structure_encoder(&mut self) -> &ProtocolObject<dyn objc2_metal::MTLAccelerationStructureCommandEncoder> {
        let has_existing_encoder = if let MTLEncoder::AccelerationStructure(_) = &self.encoder {
            true
        } else {
            false
        };

        if !has_existing_encoder {
            self.end_non_rendering_encoders();
            let encoder = self.handle().accelerationStructureCommandEncoder().unwrap();
            let heap_list = self.shared.heap_list.read().unwrap();
            for heap in heap_list.iter() {
                encoder.useHeap(&heap);
            }
            self.encoder = MTLEncoder::AccelerationStructure(encoder);
        }
        if let MTLEncoder::AccelerationStructure(encoder) = &self.encoder {
            encoder
        } else {
            unreachable!()
        }
    }

    fn render_encoder_use_all_heaps<'a>(encoder: &ProtocolObject<dyn objc2_metal::MTLRenderCommandEncoder>, shared: &Arc<MTLShared>) {
        let heap_list = shared.heap_list.read().unwrap();
        for heap in heap_list.iter() {
            encoder.useHeap_stages(&heap, objc2_metal::MTLRenderStages::Vertex | objc2_metal::MTLRenderStages::Fragment);
        }
    }

    fn get_render_pass_encoder(&self) -> &ProtocolObject<dyn objc2_metal::MTLRenderCommandEncoder> {
        self.get_render_pass_encoder_opt().unwrap()
    }

    fn get_render_pass_encoder_opt(&self) -> Option<&ProtocolObject<dyn objc2_metal::MTLRenderCommandEncoder>> {
        match &self.encoder {
            MTLEncoder::RenderPass { encoder, .. } => Some(encoder),
            _ => None
        }
    }

    fn end_non_rendering_encoders(&mut self) {
        match std::mem::replace(&mut self.encoder, MTLEncoder::None) {
            MTLEncoder::Compute(encoder) => { encoder.endEncoding(); }
            MTLEncoder::Blit(encoder) => { encoder.endEncoding(); }
            MTLEncoder::AccelerationStructure(encoder) => { encoder.endEncoding(); }
            _ => { panic!("Rendering encoders need to be ended manually using end_render_pass.")}
        }
        self.binding.mark_all_dirty();
    }

    pub(crate) unsafe fn blit_rp(command_buffer: &ProtocolObject<dyn objc2_metal::MTLCommandBuffer>, shared: &Arc<MTLShared>, src_texture: &MTLTexture, src_array_layer: u32, src_mip_level: u32, dst_texture: &MTLTexture, dst_array_layer: u32, dst_mip_level: u32) {
        let new_view: Option<Retained<ProtocolObject<dyn objc2_metal::MTLTexture>>>;

        let descriptor = objc2_metal::MTLRenderPassDescriptor::new();
        let attachment = descriptor.colorAttachments().objectAtIndexedSubscript(0);
        attachment.setLoadAction(objc2_metal::MTLLoadAction::DontCare);
        attachment.setStoreAction(objc2_metal::MTLStoreAction::Store);
        attachment.setLevel(dst_mip_level as NSUInteger);
        attachment.setSlice(dst_array_layer as NSUInteger);
        attachment.setTexture(Some(dst_texture.handle()));
        let encoder = command_buffer.renderCommandEncoderWithDescriptor(&descriptor).unwrap();
        encoder.setRenderPipelineState(shared.blit_pipeline.handle());
        if src_array_layer == 0 && src_mip_level == 0 {
            encoder.setFragmentTexture_atIndex(Some(src_texture.handle()), 0);
        } else {
            new_view = src_texture.handle().newTextureViewWithPixelFormat_textureType_levels_slices(
                src_texture.handle().pixelFormat(), src_texture.handle().textureType(),
                NSRange::new(src_mip_level as NSUInteger, 1), NSRange::new(src_array_layer as NSUInteger, 1));
            assert!(new_view.is_some());
            encoder.setFragmentTexture_atIndex(new_view.as_ref().map(|v| v.as_ref()), 0);
        }
        encoder.setFragmentTexture_atIndex(Some(src_texture.handle()), 0);
        encoder.setFragmentSamplerState_atIndex(Some(&shared.linear_sampler), 0);
        encoder.drawPrimitives_vertexStart_vertexCount(objc2_metal::MTLPrimitiveType::Triangle, 0, 3);
        encoder.endEncoding();
    }

    unsafe fn multi_draw_indirect(&mut self, indexed: bool, draw_buffer: &MTLBuffer, draw_buffer_offset: u32, count_buffer: &MTLBuffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        {
            let render_encoder = self.get_render_pass_encoder();
            render_encoder.endEncoding();
        }
        let descriptor = objc2_metal::MTLIndirectCommandBufferDescriptor::new();
        descriptor.setInheritBuffers(true);
        descriptor.setInheritPipelineState(true);
        descriptor.setCommandTypes(if indexed { objc2_metal::MTLIndirectCommandType::DrawIndexed } else { objc2_metal::MTLIndirectCommandType::Draw });
        let icb = self.shared.device.newIndirectCommandBufferWithDescriptor_maxCommandCount_options(&descriptor, max_draw_count as NSUInteger, objc2_metal::MTLResourceOptions::StorageModeShared).unwrap();
        {
            let compute_encoder = self.command_buffer.as_ref().expect("Draw indirect is not supported in secondary command buffers.")
                .computeCommandEncoder().unwrap();
            compute_encoder.setComputePipelineState(&self.shared.mdi_pipeline);
            let resource_id = icb.gpuResourceID();
            let params = MTLMDIParams {
                indirect_cmd_buffer: resource_id,
                draw_buffer: draw_buffer.handle().gpuAddress() + draw_buffer_offset as u64,
                count_buffer: count_buffer.handle().gpuAddress() + count_buffer_offset as u64,
                stride: stride as usize,
                primitive_type: self.primitive_type
            };
            compute_encoder.setBytes_length_atIndex(NonNull::new_unchecked(std::mem::transmute::<*const MTLMDIParams, *mut c_void>(&params as *const MTLMDIParams)), std::mem::size_of_val(&params) as NSUInteger, 0);
            compute_encoder.dispatchThreads_threadsPerThreadgroup(objc2_metal::MTLSize { width: max_draw_count as NSUInteger, height: 1, depth: 1 }, objc2_metal::MTLSize { width: 32, height: 1, depth: 1 });
        }
        {
            match &mut self.encoder {
                MTLEncoder::RenderPass { encoder, render_pass } => {
                    *encoder = self.command_buffer.as_ref().unwrap().renderCommandEncoderWithDescriptor(render_pass).unwrap();
                    Self::render_encoder_use_all_heaps(encoder, &self.shared);
                    encoder.executeCommandsInBuffer_withRange(&icb, NSRange { location: 0, length: max_draw_count as NSUInteger});
                },
                MTLEncoder::Parallel { .. } => panic!("Cannot use draw indirect inside of a parallel render pass"),
                _ => panic!("Cannot use draw indirect outside of a render pass"),
            }
        }
    }

    fn enable_error_tracking(_command_buffer: &ProtocolObject<dyn objc2_metal::MTLCommandBuffer>) {
        //let _: ()  = unsafe { msg_send![command_buffer, setErrorOptions: 1] };
    }

    fn print_error(command_buffer: &ProtocolObject<dyn objc2_metal::MTLCommandBuffer>) {
        unsafe {
            let error_opt = command_buffer.error();
            if error_opt.is_none() {
                return;
            }
            let error = error_opt.unwrap();
            let desc = error.localizedDescription();
            let message = CStr::from_ptr(desc.UTF8String()).to_string_lossy().into_owned();
            println!("error {}", message);
        }
    }
}

impl gpu::CommandBuffer<MTLBackend> for MTLCommandBuffer {
    unsafe fn set_pipeline(&mut self, pipeline: gpu::PipelineBinding<MTLBackend>) {
        self.binding.mark_all_dirty();
        match pipeline {
            gpu::PipelineBinding::Graphics(pipeline) => {
                self.primitive_type = pipeline.primitive_type();
                let encoder = self.get_render_pass_encoder();
                encoder.setRenderPipelineState(pipeline.handle());
                encoder.setCullMode(pipeline.rasterizer_state().cull_mode);
                encoder.setFrontFacingWinding(pipeline.rasterizer_state().front_face);
                encoder.setTriangleFillMode(pipeline.rasterizer_state().fill_mode);
                encoder.setDepthStencilState(Some(pipeline.depth_stencil_state()));
                self.resource_map = Some(pipeline.resource_map().clone());
            },
            gpu::PipelineBinding::Compute(pipeline) => {
                let encoder = self.get_compute_encoder();
                encoder.setComputePipelineState(pipeline.handle());
                self.resource_map = Some(pipeline.resource_map().clone());
            },
            _ => unimplemented!()
        }
    }

    unsafe fn set_vertex_buffer(&mut self, index: u32, vertex_buffer: &MTLBuffer, offset: u64) {
        let encoder = self.get_render_pass_encoder();
        encoder.setVertexBuffer_offset_atIndex(Some(vertex_buffer.handle()), offset as NSUInteger, index as NSUInteger);
    }

    unsafe fn set_index_buffer(&mut self, index_buffer: &MTLBuffer, offset: u64, format: gpu::IndexFormat) {
        self.index_buffer = Some(IndexBufferBinding {
            buffer: Retained::from(index_buffer.handle()),
            offset,
            format
        });
    }

    unsafe fn set_viewports(&mut self, viewports: &[ gpu::Viewport ]) {
        assert_eq!(viewports.len(), 1);
        let viewport = &viewports[0];
        let encoder = self.get_render_pass_encoder();
        encoder
            .setViewport(objc2_metal::MTLViewport {
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
            .setScissorRect(objc2_metal::MTLScissorRect {
                x: scissor.position.x as NSUInteger,
                y: scissor.position.y as NSUInteger,
                width: scissor.extent.x as NSUInteger,
                height: scissor.extent.y as NSUInteger
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
                encoder.setVertexBytes_length_atIndex(NonNull::new_unchecked(std::mem::transmute::<*const T, *mut c_void>(data.as_ptr())), data_size as NSUInteger, push_constant_info.binding as NSUInteger);
            } else if visible_for_shader_stage == gpu::ShaderType::FragmentShader {
                encoder.setFragmentBytes_length_atIndex(NonNull::new_unchecked(std::mem::transmute::<*const T, *mut c_void>(data.as_ptr())), data_size as NSUInteger, push_constant_info.binding as NSUInteger);
            } else {
                panic!("Can only set vertex or fragment push constant data while in a render pass");
            }
        } else if visible_for_shader_stage == gpu::ShaderType::ComputeShader {
            let resource_map = self.resource_map.as_ref().expect("Cannot set push constant data before binding a shader");
            let push_constant_info = resource_map.push_constants.get(&visible_for_shader_stage).expect("Shader does not have push constants").clone();
            let encoder = self.get_compute_encoder();
            let data_size = std::mem::size_of_val(data);
            assert!(data_size <= push_constant_info.size as usize);
            encoder.setBytes_length_atIndex(NonNull::new_unchecked(std::mem::transmute::<*const T, *mut c_void>(data.as_ptr())), data_size as NSUInteger, push_constant_info.binding as NSUInteger);
        } else {
            unimplemented!()
        }
    }

    unsafe fn draw(&mut self, vertices: u32, offset: u32) {
        self.get_render_pass_encoder()
            .drawPrimitives_vertexStart_vertexCount(self.primitive_type, offset as NSUInteger, vertices as NSUInteger);
    }

    unsafe fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
        let index_buffer = self.index_buffer.as_ref()
            .expect("No index buffer bound");

        if instances != 1 || first_instance != 0 || vertex_offset != 0 {
            self.get_render_pass_encoder()
                .drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset_instanceCount_baseVertex_baseInstance(
                    self.primitive_type,
                    indices as NSUInteger,
                    index_format_to_mtl(index_buffer.format),
                    &index_buffer.buffer,
                    index_buffer.offset as NSUInteger + first_index as NSUInteger * index_format_size(index_buffer.format) as NSUInteger,
                    instances as NSUInteger,
                    vertex_offset as NSInteger,
                    first_instance as NSUInteger);
        } else {
            self.get_render_pass_encoder()
                .drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    self.primitive_type,
                    indices as NSUInteger,
                    index_format_to_mtl(index_buffer.format),
                    &index_buffer.buffer,
                    index_buffer.offset as NSUInteger + first_index as NSUInteger * index_format_size(index_buffer.format) as NSUInteger
                );
        }
    }

    unsafe fn draw_indexed_indirect(&mut self, draw_buffer: &MTLBuffer, draw_buffer_offset: u32, count_buffer: &MTLBuffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
       self.multi_draw_indirect(true, draw_buffer, draw_buffer_offset, count_buffer, count_buffer_offset, max_draw_count, stride);
    }

    unsafe fn draw_indirect(&mut self, draw_buffer: &MTLBuffer, draw_buffer_offset: u32, count_buffer: &MTLBuffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        self.multi_draw_indirect(false, draw_buffer, draw_buffer_offset, count_buffer, count_buffer_offset, max_draw_count, stride);
    }

    unsafe fn bind_sampling_view(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &MTLTextureView) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::SampledTexture(texture.handle()));
    }

    unsafe fn bind_sampling_view_and_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &MTLTextureView, sampler: &MTLSampler) {
        self.binding.bind(frequency, binding, MTLBoundResourceRef::SampledTextureAndSampler(texture.handle(), sampler.handle()));
    }

    unsafe fn bind_sampling_view_and_sampler_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures_and_samplers: &[(&MTLTextureView, &MTLSampler)]) {
        let handles: SmallVec<[(&ProtocolObject<dyn objc2_metal::MTLTexture>, &ProtocolObject<dyn objc2_metal::MTLSamplerState>); 8]> = textures_and_samplers
            .iter()
            .map(|(tv, s)| (tv.handle(), s.handle()))
            .collect();
        self.binding.bind(frequency, binding, MTLBoundResourceRef::SampledTextureAndSamplerArray(&handles));
    }

    unsafe fn bind_storage_view_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures: &[&MTLTextureView]) {
        let handles: SmallVec<[&ProtocolObject<dyn objc2_metal::MTLTexture>; 8]> = textures
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
        self.binding.bind(frequency, binding, MTLBoundResourceRef::StorageBuffer(MTLBufferBindingInfoRef {
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
        self.binding.bind(frequency, binding, MTLBoundResourceRef::AccelerationStructure(acceleration_structure.handle()));
    }

    unsafe fn finish_binding(&mut self) {
        match &mut self.encoder {
            MTLEncoder::RenderPass { encoder: rp, .. } => {
                self.binding.finish(MTLEncoderRef::Graphics(rp), self.resource_map.as_ref().expect("Need to bind a shader before finishing binding."));
                let bindless_map = &self.resource_map.as_ref().unwrap().bindless_argument_buffer_binding;
                if let Some(bindless_binding) = bindless_map.get(&gpu::ShaderType::VertexShader) {
                    rp.setVertexBuffer_offset_atIndex(Some(self.shared.bindless.handle()), *bindless_binding as NSUInteger, 0);
                }
                if let Some(bindless_binding) = bindless_map.get(&gpu::ShaderType::FragmentShader) {
                    rp.setVertexBuffer_offset_atIndex(Some(self.shared.bindless.handle()), *bindless_binding as NSUInteger, 0);
                }
            }
            MTLEncoder::Compute(encoder) => {
                self.binding.finish(MTLEncoderRef::Compute(encoder), self.resource_map.as_ref().expect("Need to bind a shader before finishing binding."));
                let bindless_map = &self.resource_map.as_ref().unwrap().bindless_argument_buffer_binding;
                if let Some(bindless_binding) = bindless_map.get(&gpu::ShaderType::ComputeShader) {
                    encoder.setBuffer_offset_atIndex(Some(self.shared.bindless.handle()), 0, *bindless_binding as NSUInteger);
                }
            }
            _ => {}
        }
    }

    unsafe fn begin_label(&mut self, label: &str) {
        self.handle().pushDebugGroup(&NSString::from_str(label));
    }

    unsafe fn end_label(&mut self) {
        self.handle().popDebugGroup();
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        let compute_encoder = self.get_compute_encoder();
        compute_encoder.dispatchThreadgroups_threadsPerThreadgroup(objc2_metal::MTLSize {
            width: group_count_x as NSUInteger, height: group_count_y as NSUInteger, depth: group_count_z as NSUInteger
        }, objc2_metal::MTLSize { width: 8, height: 8, depth: 1 });
    }

    unsafe fn blit(&mut self, src_texture: &MTLTexture, src_array_layer: u32, src_mip_level: u32, dst_texture: &MTLTexture, dst_array_layer: u32, dst_mip_level: u32) {
        if dst_texture.info().usage.contains(gpu::TextureUsage::COPY_DST) {
            let encoder = self.get_blit_encoder();
            encoder.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                src_texture.handle(),
                src_array_layer as NSUInteger,
                src_mip_level as NSUInteger,
                objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                objc2_metal::MTLSize { width: (src_texture.info().width >> src_mip_level) as NSUInteger, height: (src_texture.info().height >> src_mip_level) as NSUInteger, depth: (src_texture.info().depth >> src_mip_level) as NSUInteger },
                dst_texture.handle(),
                dst_array_layer as NSUInteger,
                dst_mip_level as NSUInteger,
                objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 }
            );
        } else if dst_texture.info().usage.contains(gpu::TextureUsage::RENDER_TARGET) {
            Self::blit_rp(self.command_buffer.as_ref().unwrap(), &self.shared, src_texture, src_array_layer, src_mip_level, dst_texture, dst_array_layer, dst_mip_level);
        }
    }

    unsafe fn begin(&mut self, _frame: u64, inheritance: Option<&Self::CommandBufferInheritance>) {
        self.binding.mark_all_dirty();
        if let Some(handle) = self.command_buffer.as_ref() {
            handle.encodeWaitForEvent_value(&self.pre_event, 1);
        }
        if let Some(inheritance) = inheritance {
            let mut guard = inheritance.lock().unwrap();
            let encoder = guard.encoders.pop().expect("Ran out of inner encoders.");
            self.encoder = MTLEncoder::RenderPass {
                encoder,
                render_pass: guard.descriptor.clone()
            }
        }
    }

    unsafe fn finish(&mut self) {
        self.end_non_rendering_encoders();
        self.end_render_pass();
    }

    unsafe fn copy_buffer_to_texture(&mut self, src: &MTLBuffer, dst: &MTLTexture, region: &gpu::BufferTextureCopyRegion) {
        let blit_encoder = self.get_blit_encoder();
        let format = dst.info().format;
        let row_pitch = if region.buffer_row_pitch != 0 {
            region.buffer_row_pitch as NSUInteger
        } else {
            (align_up_32(region.texture_extent.x, format.block_size().x) / format.block_size().x * format.element_size()) as NSUInteger
        };
        let slice_pitch = if region.buffer_slice_pitch != 0 {
            region.buffer_slice_pitch as NSUInteger
        } else {
            (align_up_32(region.texture_extent.y, format.block_size().y) / format.block_size().y) as NSUInteger * row_pitch
        };

        blit_encoder.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
            src.handle(),
            region.buffer_offset as NSUInteger,
            row_pitch,
            slice_pitch,
            objc2_metal::MTLSize {
                width: region.texture_extent.x as NSUInteger,
                height: region.texture_extent.y as NSUInteger,
                depth: region.texture_extent.z as NSUInteger
            },
            dst.handle(),
            region.texture_subresource.array_layer as NSUInteger,
            region.texture_subresource.mip_level as NSUInteger,
            objc2_metal::MTLOrigin {
                x: region.texture_offset.x as NSUInteger,
                y: region.texture_offset.y as NSUInteger,
                z: region.texture_offset.z as NSUInteger
            }
        );
    }

    unsafe fn copy_buffer(&mut self, src: &MTLBuffer, dst: &MTLBuffer, region: &gpu::BufferCopyRegion) {
        let blit_encoder = self.get_blit_encoder();
        blit_encoder.copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(src.handle(), region.src_offset as NSUInteger, dst.handle(), region.dst_offset as NSUInteger, region.size as NSUInteger);
    }

    unsafe fn clear_storage_texture(&mut self, _view: &MTLTexture, _array_layer: u32, _mip_level: u32, _values: [u32; 4]) {
        todo!()
    }

    unsafe fn clear_storage_buffer(&mut self, buffer: &MTLBuffer, offset: u64, length_in_u32s: u64, value: u32) {
        assert_eq!(value & 0xFF, value & 0x00FF);
        assert_eq!(value & 0xFF, value & 0x0000FF);
        assert_eq!(value & 0xFF, value & 0x000000FF); // Write compute shader fallback

        let blit_encoder = self.get_blit_encoder();
        blit_encoder.fillBuffer_range_value(
            buffer.handle(),
            NSRange::new(offset as NSUInteger, (length_in_u32s / 4) as NSUInteger),
            value as u8
        );
    }

    unsafe fn begin_render_pass(&mut self, renderpass_info: &gpu::RenderPassBeginInfo<MTLBackend>, recording_mode: gpu::RenderpassRecordingMode) -> Option<Self::CommandBufferInheritance> {
        self.end_non_rendering_encoders();
        let descriptor = render_pass_to_descriptors(renderpass_info);
        if let gpu::RenderpassRecordingMode::CommandBuffers(count) = recording_mode {
            let parallel_encoder = self.handle().parallelRenderCommandEncoderWithDescriptor(&descriptor).unwrap();
            let mut encoders = Vec::new();
            for _ in 0..count {
                let encoder = parallel_encoder.renderCommandEncoder().unwrap();
                Self::render_encoder_use_all_heaps(&encoder, &self.shared);
                encoders.push(encoder);
            }
            self.encoder = MTLEncoder::Parallel(parallel_encoder);
            Some(Arc::new(Mutex::new(MTLInnerCommandBufferInheritance {
                descriptor: descriptor.clone(),
                encoders
            })))
        } else {
            let encoder = self.handle().renderCommandEncoderWithDescriptor(&descriptor).unwrap();
            Self::render_encoder_use_all_heaps(&encoder, &self.shared);
            self.encoder = MTLEncoder::RenderPass {
                encoder: encoder,
                render_pass: descriptor,
            };
            None
        }
    }

    unsafe fn end_render_pass(&mut self) {
        match &self.encoder {
            MTLEncoder::RenderPass { encoder, .. } => {
                encoder.endEncoding();
            },
            MTLEncoder::Parallel(encoder) => {
                encoder.endEncoding();
            },
            _ => {}
        }

        self.encoder = MTLEncoder::None;
        self.binding.mark_all_dirty();
    }

    unsafe fn barrier(&mut self, _barriers: &[gpu::Barrier<MTLBackend>]) {
        // No-op, all writable resources are tracked by the Metal driver
    }

    type CommandBufferInheritance = Arc<Mutex<MTLInnerCommandBufferInheritance>>;

    unsafe fn execute_inner(&mut self, _submission: &[&MTLCommandBuffer], inheritance: Self::CommandBufferInheritance) {
        let mut inheritance_guard = inheritance.lock().unwrap();
        for encoder in inheritance_guard.encoders.iter() {
            encoder.endEncoding();
        }
        inheritance_guard.encoders.clear();

        // They are automatically appended to the command buffer when we call endEncoding.
    }

    unsafe fn reset(&mut self, _frame: u64) {
        self.end_non_rendering_encoders();
        if let Some(command_buffer) = self.command_buffer.as_mut() {
            assert!(command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Completed || command_buffer.status() == objc2_metal::MTLCommandBufferStatus::NotEnqueued || command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Error);
            if command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Error {
                log::error!("COMMAND BUFFER ERROR");
                Self::print_error(command_buffer);
            }
            *command_buffer = self.queue.commandBufferWithUnretainedReferences().unwrap();
            Self::enable_error_tracking(command_buffer);
        }

        self.pre_event = self.queue.device().newEvent().unwrap();
        self.post_event = self.queue.device().newEvent().unwrap();
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
        let encoder = Retained::from(self.get_acceleration_structure_encoder());
        MTLAccelerationStructure::new_bottom_level(
            &encoder,
            &self.shared,
            size,
            target_buffer,
            target_buffer_offset,
            scratch_buffer,
            scratch_buffer_offset,
            info,
            self.command_buffer.as_ref().unwrap()
        )
    }

    unsafe fn upload_top_level_instances(
        &mut self,
        instances: &[gpu::AccelerationStructureInstance<MTLBackend>],
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64
      ) {
        MTLAccelerationStructure::upload_top_level_instances(&self.shared, target_buffer, target_buffer_offset, instances)
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
        let encoder = Retained::from(self.get_acceleration_structure_encoder());
        MTLAccelerationStructure::new_top_level(&encoder, &self.shared, size, target_buffer, target_buffer_offset, scratch_buffer, scratch_buffer_offset, info, self.command_buffer.as_ref().unwrap())
    }

    unsafe fn trace_ray(&mut self, _width: u32, _height: u32, _depth: u32) {
        panic!("Metal does not support ray tracing pipelines")
    }

    unsafe fn begin_query(&mut self, query_index: u32) {
        let encoder = self.get_render_pass_encoder();
        encoder.setVisibilityResultMode_offset(objc2_metal::MTLVisibilityResultMode::Counting, (query_index as NSUInteger) * 8);
    }

    unsafe fn end_query(&mut self, query_index: u32) {
        let encoder = self.get_render_pass_encoder();
        encoder.setVisibilityResultMode_offset(objc2_metal::MTLVisibilityResultMode::Disabled, (query_index as NSUInteger) * 8);
    }

    unsafe fn copy_query_results_to_buffer(&mut self, query_pool: &MTLQueryPool, start_index: u32, count: u32, buffer: &MTLBuffer, buffer_offset: u64) {
        let blit_encoder = self.get_blit_encoder();
        blit_encoder.copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
            query_pool.handle(),
            start_index as NSUInteger * 8,
            buffer.handle(),
            buffer_offset as NSUInteger,
            count as NSUInteger * 8
        );
    }
}

impl Drop for MTLCommandBuffer {
    fn drop(&mut self) {
        match &self.encoder {
            MTLEncoder::RenderPass { encoder, .. } => {
                encoder.endEncoding();
            }
            MTLEncoder::Parallel(encoder) => {
                encoder.endEncoding();
            },
            _ => {}
        }

        if let Some(command_buffer) = self.command_buffer.as_ref() {
            assert!(command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Completed || command_buffer.status() == objc2_metal::MTLCommandBufferStatus::NotEnqueued || command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Error);
            if command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Error {
                println!("COMMAND BUFFER ERROR");
                Self::print_error(command_buffer);
            }
        }
        self.end_non_rendering_encoders();
    }
}
