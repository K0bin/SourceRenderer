use std::{collections::VecDeque, rc::Rc, sync::Arc};

use sourcerenderer_core::graphics::{BindingFrequency, Buffer, BufferInfo, BufferUsage, CommandBuffer, LoadOp, MemoryUsage, PipelineBinding, Queue, Scissor, ShaderType, Viewport, IndexFormat, WHOLE_BUFFER, Texture, RenderpassRecordingMode, TextureRenderTargetView};
use web_sys::{WebGl2RenderingContext, WebGlBuffer, WebGlRenderingContext};

use crate::{GLThreadSender, WebGLBackend, WebGLBuffer, WebGLFence, WebGLGraphicsPipeline, WebGLSwapchain, WebGLTexture, WebGLTextureSamplingView, device::WebGLHandleAllocator, sync::WebGLSemaphore, texture::{WebGLSampler, WebGLUnorderedAccessView, compare_func_to_gl}, thread::{TextureHandle, WebGLThreadBuffer, WebGLVBThreadBinding, WebGLTextureHandleView}, rt::WebGLAccelerationStructureStub, WebGLWork};

use bitflags::bitflags;

bitflags! {
  pub struct WebGLCommandBufferDirty: u32 {
    const VAO = 0b0001;
  }
}

pub struct WebGLVBBinding {
  buffer: Arc<WebGLBuffer>,
  offset: u64,
}

pub struct WebGLCommandBuffer {
  sender: GLThreadSender,
  pipeline: Option<Arc<WebGLGraphicsPipeline>>,
  commands: VecDeque<WebGLWork>,
  inline_buffer: Arc<WebGLBuffer>,
  handles: Arc<WebGLHandleAllocator>,
  dirty: WebGLCommandBufferDirty,
  vertex_buffer: Option<WebGLVBBinding>,
  index_buffer_offset: usize
}

impl WebGLCommandBuffer {
  pub fn new(sender: &GLThreadSender, handle_allocator: &Arc<WebGLHandleAllocator>) -> Self {
    let inline_buffer = Arc::new(WebGLBuffer::new(handle_allocator.new_buffer_handle(), &BufferInfo {
      size: 256,
      usage: BufferUsage::CONSTANT,
    }, MemoryUsage::UncachedRAM, sender));
    WebGLCommandBuffer {
      pipeline: None,
      commands: VecDeque::new(),
      sender: sender.clone(),
      handles: handle_allocator.clone(),
      inline_buffer,
      dirty: WebGLCommandBufferDirty::empty(),
      vertex_buffer: None,
      index_buffer_offset: 0,
    }
  }

  fn before_draw(&mut self) {
    if self.dirty.is_empty() {
      return;
    }
    assert!(self.pipeline.is_some());
    assert!(self.vertex_buffer.is_some());

    let dirty = self.dirty;
    let pipeline = self.pipeline.as_ref().unwrap();
    let pipeline_handle = pipeline.handle();
    let vbo = self.vertex_buffer.as_ref().unwrap();
    let vbo_handle = vbo.buffer.handle();
    let vbo_offset = vbo.offset;
    self.commands.push_back(Box::new(move |device| {
      let pipeline = device.pipeline(pipeline_handle);
      let vbo = device.buffer(vbo_handle);
      if dirty.contains(WebGLCommandBufferDirty::VAO) {
        let index_buffer: WebGlBuffer = device.get_parameter(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER_BINDING).unwrap().into();
        let mut vbs: [Option<WebGLVBThreadBinding>; 4] = Default::default();
        vbs[0] = Some(WebGLVBThreadBinding {
          buffer: vbo.clone(),
          offset: vbo_offset,
        });
        let vao = pipeline.get_vao(&vbs);
        device.bind_vertex_array(Some(&vao));
        device.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&index_buffer));
      }
    }));
    self.dirty = WebGLCommandBufferDirty::empty();
  }
}

impl CommandBuffer<WebGLBackend> for WebGLCommandBuffer {
  fn set_pipeline(&mut self, pipeline: PipelineBinding<WebGLBackend>) {
    match pipeline {
      PipelineBinding::Graphics(pipeline) => {
        self.pipeline = Some(pipeline.clone());
        let handle = pipeline.handle();
        self.dirty |= WebGLCommandBufferDirty::VAO;
        self.commands.push_back(Box::new(move |device| {
          let pipeline = device.pipeline(handle).clone();
          device.use_program(Some(pipeline.gl_program()));
          let info = pipeline.info();
          if info.depth_stencil.depth_test_enabled {
            device.enable(WebGl2RenderingContext::DEPTH_TEST);
          } else {
            device.disable(WebGl2RenderingContext::DEPTH_TEST);
          }
          device.depth_mask(info.depth_stencil.depth_write_enabled);
          device.depth_func(compare_func_to_gl(info.depth_stencil.depth_func));
          device.front_face(pipeline.gl_front_face());
          let cull_face = pipeline.gl_cull_face();
          if cull_face == 0 {
            device.disable(WebGl2RenderingContext::CULL_FACE);
          } else {
            device.enable(WebGl2RenderingContext::CULL_FACE);
            device.cull_face(cull_face);
          }
        }));
      },
      PipelineBinding::Compute(_) => panic!("WebGL does not support compute shaders"),
      PipelineBinding::RayTracing(_) => panic!("WebGL does not support ray tracing")
    }
  }

  fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<WebGLBuffer>, offset: usize) {
    self.vertex_buffer = Some(WebGLVBBinding {
      buffer: vertex_buffer.clone(),
      offset: offset as u64
    });
    self.dirty |= WebGLCommandBufferDirty::VAO;
  }

  fn set_index_buffer(&mut self, index_buffer: &Arc<WebGLBuffer>, offset: usize, index_format: IndexFormat) {
    // TODO: maybe track dirty and do before draw

    if index_format != IndexFormat::U32 {
      unimplemented!("16 bit indices are not implemented");
    }

    self.index_buffer_offset = offset;

    let handle = index_buffer.handle();
    self.commands.push_back(Box::new(move |device| {
      let buffer = device.buffer(handle).clone();
      device.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, Some(buffer.gl_buffer()));
    }));
  }

  fn set_viewports(&mut self, viewports: &[ Viewport ]) {
    // TODO: maybe track dirty and do before draw

    if viewports.len() == 0 {
      return;
    }
    debug_assert_eq!(viewports.len(), 1);
    let viewports: Vec<Viewport> = viewports.iter().cloned().collect();
    self.commands.push_back(Box::new(move |device| {
      let viewport = viewports.first().unwrap();
      device.viewport(viewport.position.x as i32, viewport.position.y as i32, viewport.extent.x as i32, viewport.extent.y as i32);
    }));
  }

  fn set_scissors(&mut self, scissors: &[ Scissor ]) {
    // TODO: maybe track dirty and do before draw

    if scissors.len() == 0 {
      return;
    }
    debug_assert_eq!(scissors.len(), 1);
    let scissors: Vec<Scissor> = scissors.iter().cloned().collect();
    self.commands.push_back(Box::new(move |device| {
      let scissor = scissors.first().unwrap();
      device.scissor(scissor.position.x as i32, scissor.position.y as i32, scissor.extent.x as i32, scissor.extent.y as i32);
    }));
  }

  fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Arc<WebGLBuffer>
  where T: 'static + Send + Sync + Sized + Clone {
    let buffer_handle = self.handles.new_buffer_handle();
    let buffer = Arc::new(WebGLBuffer::new(buffer_handle, &BufferInfo { size: std::mem::size_of_val(data), usage }, MemoryUsage::UncachedRAM, &self.sender));
    unsafe {
      let mapped = buffer.map_unsafe(false).unwrap();
      std::ptr::copy(data.as_ptr() as *const u8, mapped, std::mem::size_of_val(data));
      buffer.unmap_unsafe(true);
    }
    buffer
  }

  fn upload_dynamic_data_inline<T>(&mut self, data: &[T], _visible_for_shader_stage: ShaderType)
  where T: 'static + Send + Sync + Sized + Clone {
    assert!(self.pipeline.is_some());
    let pipeline = self.pipeline.as_ref().unwrap();
    unsafe {
      let mapped = self.inline_buffer.map_unsafe(false).unwrap();
      std::ptr::copy(data.as_ptr() as *const u8, mapped, std::mem::size_of_val(data));
      self.inline_buffer.unmap_unsafe(true);
    }
    let pipeline_handle = pipeline.handle();
    let buffer_handle = self.inline_buffer.handle();
    self.commands.push_back(Box::new(move |device| {
      let pipeline = device.pipeline(pipeline_handle);
      if let Some(info) = pipeline.push_constants_info() {
        let binding = info.binding;
        let buffer = device.buffer(buffer_handle);
        debug_assert!(buffer.info().size as u32 >= info.size);
        device.bind_buffer_base(WebGl2RenderingContext::UNIFORM_BUFFER, binding, Some(&buffer.gl_buffer()));
      }
    }));
  }

  fn draw(&mut self, vertices: u32, offset: u32) {
    self.before_draw();

    self.commands.push_back(Box::new(move |device| {
      device.draw_arrays(
        WebGlRenderingContext::TRIANGLES, // TODO: self.pipeline.as_ref().unwrap().gl_draw_mode(),
        offset as i32,
        vertices as i32
      );
    }));
  }

  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    self.before_draw();

    // TODO: support instancing with WebGL2
    assert_eq!(instances, 1);
    assert_eq!(first_instance, 0);
    assert_eq!(vertex_offset, 0);
    let pipeline_handle = self.pipeline.as_ref().unwrap().handle();
    let index_offset = self.index_buffer_offset as i32;
    self.commands.push_back(Box::new(move |device| {
      let pipeline = device.pipeline(pipeline_handle);
      device.draw_elements_with_i32(
        pipeline.gl_draw_mode(),
        indices as i32,
        WebGlRenderingContext::UNSIGNED_INT,
        first_index as i32 * std::mem::size_of::<u32>() as i32 + index_offset,
      );
    }));
  }

  fn bind_sampling_view(&mut self, _frequency: BindingFrequency, _binding: u32, _texture: &Arc<WebGLTextureSamplingView>) {
    panic!("WebGL only supports combined images and samplers")
  }

  fn bind_sampling_view_and_sampler(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<WebGLTextureSamplingView>, sampler: &Arc<WebGLSampler>) {
    let handle = texture.texture().handle();
    let info = texture.texture().info();
    let is_cubemap = info.array_length == 6;
    let target = if is_cubemap { WebGlRenderingContext::TEXTURE_CUBE_MAP } else { WebGlRenderingContext::TEXTURE_2D };
    let pipeline = self.pipeline.as_ref().expect("Can't bind texture without active pipeline.");
    let pipeline_handle = pipeline.handle();
    let sampler_handle = sampler.handle();

    self.commands.push_back(Box::new(move |device| {
      let pipeline = device.pipeline(pipeline_handle);
      let tex_uniform_info = pipeline.uniform_location(frequency, binding);
      if tex_uniform_info.is_none() {
        return;
      }
      let tex_uniform_info = tex_uniform_info.unwrap();
      let texture = device.texture(handle);
      let sampler = device.sampler(sampler_handle);
      device.active_texture(WebGlRenderingContext::TEXTURE0 + tex_uniform_info.texture_unit);
      device.bind_texture(target, Some(texture.gl_handle()));
      device.uniform1i(Some(&tex_uniform_info.uniform_location), tex_uniform_info.texture_unit as i32);
      device.bind_sampler(tex_uniform_info.texture_unit, Some(sampler.gl_handle()));
    }));
  }

  fn bind_uniform_buffer(&mut self, frequency: BindingFrequency, binding: u32, buffer: &Arc<WebGLBuffer>, offset: usize, length: usize) {
    assert!(self.pipeline.is_some());
    let pipeline = self.pipeline.as_ref().unwrap();
    let pipeline_handle = pipeline.handle();
    let buffer_handle = buffer.handle();
    self.commands.push_back(Box::new(move |device| {
      let buffer = device.buffer(buffer_handle);
      let pipeline = device.pipeline(pipeline_handle);
      let info = pipeline.ubo_info(frequency, binding);
      if let Some(info) = info {
        debug_assert!(buffer.info().size as u32 >= info.size);
        let binding_index = info.binding;
        let size = if length == WHOLE_BUFFER {
          info.size as i32
        } else {
          length as i32
        };
        device.bind_buffer_range_with_i32_and_i32(WebGl2RenderingContext::UNIFORM_BUFFER, binding_index, Some(buffer.gl_buffer()), offset as i32, size);
      }
    }));
  }

  fn bind_storage_buffer(&mut self, _frequency: BindingFrequency, _binding: u32, _buffer: &Arc<WebGLBuffer>, _offset: usize, _length: usize) {
    panic!("WebGL does not support storage buffers");
  }

  fn finish_binding(&mut self) {
    // nop
  }

  fn begin_label(&mut self, _label: &str) {}
  fn end_label(&mut self) {}

  fn dispatch(&mut self, _group_count_x: u32, _group_count_y: u32, _group_count_z: u32) {
    panic!("WebGL does not support compute shaders");
  }

  fn blit(&mut self, _src_texture: &Arc<WebGLTexture>, _src_array_layer: u32, _src_mip_level: u32, _dst_texture: &Arc<WebGLTexture>, _dst_array_layer: u32, _dst_mip_level: u32) {
    unimplemented!()
  }

  fn finish(self) -> WebGLCommandSubmission {
    // nop
    WebGLCommandSubmission {
      cmd_buffer: self
    }
  }

  fn bind_storage_texture(&mut self, _frequency: BindingFrequency, _binding: u32, _texture: &Arc<WebGLUnorderedAccessView>) {
    panic!("WebGL does not support storage textures")
  }

  fn begin_render_pass(&mut self, renderpass_info: &sourcerenderer_core::graphics::RenderPassBeginInfo<WebGLBackend>, recording_mode: RenderpassRecordingMode) {
    debug_assert_eq!(recording_mode, RenderpassRecordingMode::Commands);
    let mut clear_mask: u32 = 0;
    let mut color_attachments: [Option<WebGLTextureHandleView>; 8] = Default::default();
    let mut depth_attachment = Option::<WebGLTextureHandleView>::None;
    let subpass = &renderpass_info.subpasses[0];
    for (index, attachment_ref) in subpass.output_color_attachments.iter().enumerate() {
      let attachment = &renderpass_info.attachments[attachment_ref.index as usize];
      match &attachment.view {
        sourcerenderer_core::graphics::RenderPassAttachmentView::RenderTarget(rt) => {
          if attachment.load_op == LoadOp::Clear {
            clear_mask |= WebGl2RenderingContext::COLOR_BUFFER_BIT;
          }
          let info = rt.info();
          color_attachments[index] = Some(WebGLTextureHandleView {
            texture: rt.texture().handle(),
            array_layer: info.base_array_layer,
            mip: info.base_mip_level
          });
        },
        sourcerenderer_core::graphics::RenderPassAttachmentView::DepthStencil(ds) => {
          if attachment.load_op == LoadOp::Clear {
            clear_mask |= WebGl2RenderingContext::DEPTH_BUFFER_BIT;
          }
          let info = ds.info();
          depth_attachment = Some(WebGLTextureHandleView {
            texture: ds.texture().handle(),
            array_layer: info.base_array_layer,
            mip: info.base_mip_level
          });
        },
      }
    }

    self.commands.push_back(Box::new(move |context| {
      let fbo = context.get_framebuffer(&color_attachments, depth_attachment);
      context.bind_framebuffer(WebGl2RenderingContext::DRAW_FRAMEBUFFER, fbo.as_ref());
      context.clear_color(0f32, 0f32, 0f32, 1f32);
      context.clear(clear_mask);
    }));
  }

  fn advance_subpass(&mut self) {
  }

  fn end_render_pass(&mut self) {
  }

  fn barrier<'a>(&mut self, _barriers: &[sourcerenderer_core::graphics::Barrier<WebGLBackend>]) {
    // nop
  }

  fn flush_barriers(&mut self) {
    // nop
  }

  fn inheritance(&self) -> &Self::CommandBufferInheritance {
    panic!("WebGL does not support inner command buffers")
  }

  type CommandBufferInheritance = ();

  fn execute_inner(&mut self, _submission: Vec<WebGLCommandSubmission>) {
    panic!("WebGL does not support inner command buffers")
  }

  fn create_query_range(&mut self, _count: u32) -> Arc<()> {
    todo!()
  }

  fn begin_query(&mut self, _query_range: &Arc<()>, _query_index: u32) {
    todo!()
  }

  fn end_query(&mut self, _query_range: &Arc<()>, _query_index: u32) {
    todo!()
  }

  fn copy_query_results_to_buffer(&mut self, _query_range: &Arc<()>, _buffer: &Arc<WebGLBuffer>, _start_index: u32, _count: u32) {
    todo!()
  }

  fn create_temporary_buffer(&mut self, _info: &BufferInfo, _memory_usage: MemoryUsage) -> Arc<WebGLBuffer> {
    unimplemented!()
  }

  fn bind_sampler(&mut self, _frequency: BindingFrequency, _binding: u32, _sampler: &Arc<WebGLSampler>) {
    panic!("WebGL does not support separate samplers")
  }

  fn bind_acceleration_structure(&mut self, _frequency: BindingFrequency, _binding: u32, _acceleration_structure: &Arc<WebGLAccelerationStructureStub>) {
    panic!("WebGL does not support ray tracing")
  }

  fn create_bottom_level_acceleration_structure(&mut self, _info: &sourcerenderer_core::graphics::BottomLevelAccelerationStructureInfo<WebGLBackend>, _size: usize, _target_buffer: &Arc<WebGLBuffer>, _scratch_buffer: &Arc<WebGLBuffer>) -> Arc<WebGLAccelerationStructureStub> {
    panic!("WebGL does not support ray tracing")
  }

  fn upload_top_level_instances(&mut self, _instances: &[sourcerenderer_core::graphics::AccelerationStructureInstance<WebGLBackend>]) -> Arc<WebGLBuffer> {
    panic!("WebGL does not support ray tracing")
  }

  fn create_top_level_acceleration_structure(&mut self, _info: &sourcerenderer_core::graphics::TopLevelAccelerationStructureInfo<WebGLBackend>, _size: usize, _target_buffer: &Arc<WebGLBuffer>, _scratch_buffer: &Arc<WebGLBuffer>) -> Arc<WebGLAccelerationStructureStub> {
    panic!("WebGL does not support ray tracing")
  }

  fn trace_ray(&mut self, _width: u32, _height: u32, _depth: u32) {
    panic!("WebGL does not support ray tracing")
  }

  fn track_texture_view(&mut self, _texture_view: &Arc<WebGLTextureSamplingView>) {
    // nop
  }

  fn draw_indexed_indirect(&mut self, _draw_buffer: &Arc<WebGLBuffer>, _draw_buffer_offset: u32, _count_buffer: &Arc<WebGLBuffer>, _count_buffer_offset: u32, _max_draw_count: u32, _stride: u32) {
    panic!("WebGL does not support indirect rendering.");
  }

  fn draw_indirect(&mut self, _draw_buffer: &Arc<WebGLBuffer>, _draw_buffer_offset: u32, _count_buffer: &Arc<WebGLBuffer>, _count_buffer_offset: u32, _max_draw_count: u32, _stride: u32) {
    panic!("WebGL does not support indirect rendering.");
  }

  fn bind_sampling_view_and_sampler_array(&mut self, _frequency: BindingFrequency, _binding: u32, _textures_and_samplers: &[(&Arc<WebGLTextureSamplingView>, &Arc<WebGLSampler>)]) {
    panic!("No plans to support texture and sampler arrays on WebGL")
  }

  fn bind_storage_view_array(&mut self, _frequency: BindingFrequency, _binding: u32, _textures: &[&Arc<WebGLUnorderedAccessView>]) {
    panic!("WebGL doesnt support storage textures")
  }

  fn clear_storage_view(&mut self, _view: &Arc<WebGLUnorderedAccessView>, _values: [u32; 4]) {
    todo!()
  }

  fn clear_storage_buffer(&mut self, _buffer: &Arc<WebGLBuffer>, _offset: usize, _length_in_u32s: usize, _value: u32) {
    todo!()
  }
}

pub struct WebGLCommandSubmission {
  cmd_buffer: WebGLCommandBuffer
}

pub struct WebGLQueue {
  sender: GLThreadSender,
  handle_allocator: Arc<WebGLHandleAllocator>
}

impl WebGLQueue {
  pub fn new(sender: &GLThreadSender, handle_allocator: &Arc<WebGLHandleAllocator>) -> Self {
    Self {
      sender: sender.clone(),
      handle_allocator: handle_allocator.clone()
    }
  }
}

impl Queue<WebGLBackend> for WebGLQueue {
  fn create_command_buffer(&self) -> WebGLCommandBuffer {
    WebGLCommandBuffer::new(&self.sender, &self.handle_allocator)
  }

  fn create_inner_command_buffer(&self, _inheritance: &()) -> WebGLCommandBuffer {
    panic!("WebGL does not support inner command buffers")
  }

  fn submit(&self, mut submission: WebGLCommandSubmission, _fence: Option<&Arc<WebGLFence>>, _wait_semaphores: &[&Arc<WebGLSemaphore>], _signal_semaphores: &[&Arc<WebGLSemaphore>], _delay: bool) {
    while let Some(cmd) = submission.cmd_buffer.commands.pop_front() {
      self.sender.send(cmd);
    }
  }

  fn present(&self, swapchain: &Arc<WebGLSwapchain>, _wait_semaphores: &[&Arc<WebGLSemaphore>], _delay: bool) {
    swapchain.present();
    let c_swapchain = swapchain.clone();
    self.sender.send(Box::new(move |_context| {
      c_swapchain.bump_processed_frame();
    }));
  }

  fn process_submissions(&self) {
    // WebGL Queue isn't threaded right now
  }
}
