use metal;

use sourcerenderer_core::gpu;

use super::*;

pub struct MTLCommandPool {
    queue: metal::CommandQueue
}

impl MTLCommandPool {
    pub(crate) fn new(queue: &metal::CommandQueueRef) -> Self {
        Self {
            queue: queue.to_owned()
        }
    }
}

impl gpu::CommandPool<MTLBackend> for MTLCommandPool {
    unsafe fn create_command_buffer(&mut self) -> MTLCommandBuffer {
        let cmd_buffer_handle_ref = self.queue.new_command_buffer_with_unretained_references();
        let cmd_buffer_handle: metal::CommandBuffer = cmd_buffer_handle_ref.to_owned();
        MTLCommandBuffer::new(&self.queue, cmd_buffer_handle)
    }

    unsafe fn reset(&mut self) {
        todo!()
    }
}

struct IndexBufferBinding {
    buffer: metal::Buffer,
    offset: u64
}

pub struct MTLCommandBuffer {
    queue: metal::CommandQueue,
    command_buffer: metal::CommandBuffer,
    render_encoder: Option<metal::RenderCommandEncoder>,
    blit_encoder: Option<metal::BlitCommandEncoder>,
    compute_encoder: Option<metal::ComputeCommandEncoder>,
    pre_event: metal::Event,
    post_event: metal::Event,
    index_buffer: Option<IndexBufferBinding>
}

impl MTLCommandBuffer {
    pub(crate) fn new(queue: &metal::CommandQueueRef, command_buffer: metal::CommandBuffer) -> Self {
        Self {
            queue: queue.to_owned(),
            command_buffer: command_buffer,
            render_encoder: None,
            blit_encoder: None,
            compute_encoder: None,
            pre_event: queue.device().new_event(),
            post_event: queue.device().new_event(),
            index_buffer: None
        }
    }

    pub(crate) fn handle(&self) -> &metal::CommandBufferRef {
        &self.command_buffer
    }

    pub(crate) fn pre_event_handle(&self) -> &metal::EventRef {
        &self.pre_event
    }

    pub(crate) fn post_event_handle(&self) -> &metal::EventRef {
        &self.post_event
    }
}

impl gpu::CommandBuffer<MTLBackend> for MTLCommandBuffer {
    unsafe fn set_pipeline(&mut self, pipeline: gpu::PipelineBinding<MTLBackend>) {
        todo!()
    }

    unsafe fn set_vertex_buffer(&mut self, vertex_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, offset: u64) {
        todo!()
    }

    unsafe fn set_index_buffer(&mut self, index_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, offset: u64, format: gpu::IndexFormat) {
        todo!()
    }

    unsafe fn set_viewports(&mut self, viewports: &[ gpu::Viewport ]) {
        assert_eq!(viewports.len(), 1);
        let viewport = &viewports[0];
        self.render_encoder
            .expect("Viewports can only be set after starting a render pass.")
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
        self.render_encoder
            .expect("Scissor can only be set after starting a render pass.")
            .set_scissor_rect(metal::MTLScissorRect {
                x: scissor.position.x as u64,
                y: scissor.position.y as u64,
                width: scissor.extent.x as u64,
                height: scissor.extent.y as u64
            });
    }

    unsafe fn set_push_constant_data<T>(&mut self, data: &[T], visible_for_shader_stage: gpu::ShaderType)
        where T: 'static + Send + Sync + Sized + Clone {
        todo!()
    }

    unsafe fn draw(&mut self, vertices: u32, offset: u32) {
        todo!("primitive_type");
        self.render_encoder
            .expect("Draws can only be done after starting a render pass.")
            .draw_primitives(primitive_type, offset as u64, vertices as u64);
    }

    unsafe fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
        todo!("primitive_type & index_type");
        let index_buffer = self.index_buffer
            .expect("No index buffer bound");

        self.render_encoder
            .expect("Draws can only be done after starting a render pass.")
            .draw_indexed_primitives(primitive_type, indices as u64, index_type, &index_buffer.buffer, index_buffer.offset as u64);
    }

    unsafe fn draw_indexed_indirect(&mut self, draw_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, draw_buffer_offset: u32, count_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        todo!()
    }

    unsafe fn draw_indirect(&mut self, draw_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, draw_buffer_offset: u32, count_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, count_buffer_offset: u32, max_draw_count: u32, stride: u32) {
        todo!()
    }

    unsafe fn bind_sampling_view(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &<MTLBackend as gpu::GPUBackend>::TextureView) {
        todo!()
    }

    unsafe fn bind_sampling_view_and_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &<MTLBackend as gpu::GPUBackend>::TextureView, sampler: &<MTLBackend as gpu::GPUBackend>::Sampler) {
        todo!()
    }

    unsafe fn bind_sampling_view_and_sampler_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures_and_samplers: &[(&<MTLBackend as gpu::GPUBackend>::TextureView, &<MTLBackend as gpu::GPUBackend>::Sampler)]) {
        todo!()
    }

    unsafe fn bind_storage_view_array(&mut self, frequency: gpu::BindingFrequency, binding: u32, textures: &[&<MTLBackend as gpu::GPUBackend>::TextureView]) {
        todo!()
    }

    unsafe fn bind_uniform_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, offset: u64, length: u64) {
        todo!()
    }

    unsafe fn bind_storage_buffer(&mut self, frequency: gpu::BindingFrequency, binding: u32, buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, offset: u64, length: u64) {
        todo!()
    }

    unsafe fn bind_storage_texture(&mut self, frequency: gpu::BindingFrequency, binding: u32, texture: &<MTLBackend as gpu::GPUBackend>::TextureView) {
        todo!()
    }

    unsafe fn bind_sampler(&mut self, frequency: gpu::BindingFrequency, binding: u32, sampler: &<MTLBackend as gpu::GPUBackend>::Sampler) {
        todo!()
    }

    unsafe fn bind_acceleration_structure(&mut self, frequency: gpu::BindingFrequency, binding: u32, acceleration_structure: &<MTLBackend as gpu::GPUBackend>::AccelerationStructure) {
        todo!()
    }

    unsafe fn finish_binding(&mut self) {
        todo!()
    }

    unsafe fn begin_label(&mut self, label: &str) {
        self.command_buffer.push_debug_group(label);
    }

    unsafe fn end_label(&mut self) {
        self.command_buffer.pop_debug_group();
    }

    unsafe fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        if self.compute_encoder.is_none() {
            self.compute_encoder = Some(self.command_buffer.compute_command_encoder_with_dispatch_type(metal::MTLDispatchType::Concurrent).to_owned());
        }
        self.compute_encoder.unwrap().dispatch_thread_groups(metal::MTLSize::new(group_count_x, group_count_y, group_count_z), threads_per_threadgroup);
    }

    unsafe fn blit(&mut self, src_texture: &<MTLBackend as gpu::GPUBackend>::Texture, src_array_layer: u32, src_mip_level: u32, dst_texture: &<MTLBackend as gpu::GPUBackend>::Texture, dst_array_layer: u32, dst_mip_level: u32) {
        todo!()
    }

    unsafe fn begin(&mut self, inheritance: Option<&Self::CommandBufferInheritance>) {
        self.command_buffer.encode_wait_for_event(&self.pre_event, 1);
    }

    unsafe fn finish(&mut self) {}

    unsafe fn copy_buffer_to_texture(&mut self, src: &<MTLBackend as gpu::GPUBackend>::Buffer, dst: &<MTLBackend as gpu::GPUBackend>::Texture, region: &gpu::BufferTextureCopyRegion) {
        todo!()
    }

    unsafe fn copy_buffer(&mut self, src: &<MTLBackend as gpu::GPUBackend>::Buffer, dst: &<MTLBackend as gpu::GPUBackend>::Buffer, region: &gpu::BufferCopyRegion) {
        todo!()
    }

    unsafe fn clear_storage_texture(&mut self, view: &<MTLBackend as gpu::GPUBackend>::Texture, array_layer: u32, mip_level: u32, values: [u32; 4]) {
        todo!()
    }

    unsafe fn clear_storage_buffer(&mut self, buffer: &<MTLBackend as gpu::GPUBackend>::Buffer, offset: u64, length_in_u32s: u64, value: u32) {
        todo!()
    }

    unsafe fn begin_render_pass(&mut self, renderpass_info: &gpu::RenderPassBeginInfo<MTLBackend>, recording_mode: gpu::RenderpassRecordingMode) {
        let descriptor = metal::RenderPassDescriptor::new();
        //descriptor.
        self.command_buffer.new_parallel_render_command_encoder(descriptor);
    }

    unsafe fn advance_subpass(&mut self) {
        todo!()
    }

    unsafe fn end_render_pass(&mut self) {
        assert!(std::mem::replace(&mut self.render_encoder, None).is_none());
    }

    unsafe fn barrier(&mut self, barriers: &[gpu::Barrier<MTLBackend>]) {
        // No-op, all writable resources are tracked by the Metal driver
    }

    unsafe fn inheritance(&self) -> &Self::CommandBufferInheritance {
        todo!()
    }

    type CommandBufferInheritance = ();

    unsafe fn execute_inner(&mut self, submission: &[&<MTLBackend as gpu::GPUBackend>::CommandBuffer]) {
        todo!()
    }

    unsafe fn reset(&mut self, frame: u64) {
        assert!(self.render_encoder.is_none());
        assert!(self.compute_encoder.is_none());
        assert!(self.blit_encoder.is_none());
        self.command_buffer = self.queue.new_command_buffer_with_unretained_references().to_owned();;
    }

    unsafe fn create_bottom_level_acceleration_structure(
        &mut self,
        info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>,
        size: u64,
        target_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer,
        target_buffer_offset: u64,
        scratch_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer,
        scratch_buffer_offset: u64
      ) -> <MTLBackend as gpu::GPUBackend>::AccelerationStructure {
        todo!()
    }

    unsafe fn upload_top_level_instances(
        &mut self,
        instances: &[gpu::AccelerationStructureInstance<MTLBackend>],
        target_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer,
        target_buffer_offset: u64
      ) {
        todo!()
    }

    unsafe fn create_top_level_acceleration_structure(
        &mut self,
        info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>,
        size: u64,
        target_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer,
        target_buffer_offset: u64,
        scratch_buffer: &<MTLBackend as gpu::GPUBackend>::Buffer,
        scratch_buffer_offset: u64
      ) -> <MTLBackend as gpu::GPUBackend>::AccelerationStructure {
        todo!()
    }

    unsafe fn trace_ray(&mut self, width: u32, height: u32, depth: u32) {
        todo!()
    }
}
