use std::{rc::Rc, sync::Arc};

use sourcerenderer_core::graphics::{BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, MemoryUsage, PipelineBinding, Queue, Scissor, ShaderType, Texture, Viewport};
use wasm_bindgen::JsCast;
use web_sys::{WebGlRenderingContext, WebGlTexture};

use crate::{RawWebGLContext, WebGLBackend, WebGLBuffer, WebGLFence, WebGLGraphicsPipeline, WebGLSwapchain, WebGLTexture, WebGLTextureShaderResourceView, address_mode_to_gl, format_to_internal_gl, max_filter_to_gl, min_filter_to_gl, sync::WebGLSemaphore, texture::{WebGLSampler, WebGLUnorderedAccessView}};

pub struct WebGLCommandBuffer {
  context: Rc<RawWebGLContext>,
  pipeline: Option<WebGLGraphicsPipeline>
}

impl CommandBuffer<WebGLBackend> for WebGLCommandBuffer {
  fn set_pipeline(&mut self, pipeline: PipelineBinding<WebGLBackend>) {
    match pipeline {
      PipelineBinding::Graphics(pipeline) => {
        self.context.use_program(Some(pipeline.gl_program()));
      },
      PipelineBinding::Compute(_) => panic!("WebGL does not support compute shaders")
    }
  }

  fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<WebGLBuffer>) {
    self.context.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, vertex_buffer.gl_buffer());
  }

  fn set_index_buffer(&mut self, index_buffer: &Arc<WebGLBuffer>) {
    self.context.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, index_buffer.gl_buffer());
  }

  fn set_viewports(&mut self, viewports: &[ Viewport ]) {
    if viewports.len() == 0 {
      return;
    }
    debug_assert_eq!(viewports.len(), 1);
    let viewport = viewports.first().unwrap();
    self.context.viewport(viewport.position.x as i32, viewport.position.y as i32, viewport.extent.x as i32, viewport.extent.y as i32);
  }

  fn set_scissors(&mut self, scissors: &[ Scissor ]) {
    if scissors.len() == 0 {
      return;
    }
    debug_assert_eq!(scissors.len(), 1);
    let scissor = scissors.first().unwrap();
    self.context.scissor(scissor.position.x as i32, scissor.position.y as i32, scissor.extent.x as i32, scissor.extent.y as i32);
  }

  fn init_texture_mip_level(&mut self, src_buffer: &Arc<WebGLBuffer>, texture: &Arc<WebGLTexture>, mip_level: u32, array_layer: u32) {
    let info = texture.get_info();
    let data_ref = src_buffer.data();
    let data = data_ref.as_ref().unwrap();
    let target = texture.target();
    let bind_texture = self.context.get_parameter(target).unwrap();
    self.context.bind_texture(target, Some(texture.handle()));
    if info.format.is_compressed() {
      self.context.compressed_tex_image_2d_with_u8_array(
        if texture.is_cubemap() { WebGlRenderingContext::TEXTURE_CUBE_MAP_POSITIVE_X + array_layer } else { target },
        mip_level as i32,
        format_to_internal_gl(info.format),
        info.width as i32,
        info.height as i32,
        0,
        &data[..]
      );
    } else {
      self.context.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
        if texture.is_cubemap() { WebGlRenderingContext::TEXTURE_CUBE_MAP_POSITIVE_X + array_layer } else { target },
        mip_level as i32,
        format_to_internal_gl(info.format) as i32,
        info.width as i32,
        info.height as i32,
        0,
        format_to_internal_gl(info.format), // TODO: change for Webgl 2
        WebGlRenderingContext::UNSIGNED_BYTE,
        Some(&data[..])
      ).unwrap();
    }
    if !bind_texture.is_null() {
      let bind_texture = bind_texture.unchecked_into::<WebGlTexture>();
      self.context.bind_texture(target, Some(&bind_texture));
    }
  }

  fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Arc<WebGLBuffer>
  where T: 'static + Send + Sync + Sized + Clone {
    Arc::new(WebGLBuffer::new(&self.context, &BufferInfo { size: std::mem::size_of_val(data), usage }, MemoryUsage::CpuOnly))
  }

  fn upload_dynamic_data_inline<T>(&mut self, data: &[T], _visible_for_shader_stage: ShaderType)
  where T: 'static + Send + Sync + Sized + Clone {
    let size = std::mem::size_of_val(data);
    assert_eq!(size % std::mem::size_of::<f32>(), 0);
    let float_count = size / std::mem::size_of::<f32>();
    for _i in 0..float_count {
      //self.context.uniform4f()
    }
    todo!()
  }

  fn draw(&mut self, vertices: u32, offset: u32) {
    assert!(self.pipeline.is_none());
    self.context.draw_arrays(
      self.pipeline.as_ref().unwrap().gl_draw_mode(),
      offset as i32,
      vertices as i32
    );
  }

  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    assert!(self.pipeline.is_none());

    // TODO: support instancing with WebGL2
    assert_eq!(instances, 0);
    assert_eq!(first_instance, 0);
    assert_eq!(vertex_offset, 0);

    self.context.draw_elements_with_i32(
      self.pipeline.as_ref().unwrap().gl_draw_mode(),
      indices as i32,
      WebGlRenderingContext::UNSIGNED_INT,
      first_index as i32 * std::mem::size_of::<u32>() as i32,
    );
  }

  fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<WebGLTextureShaderResourceView>, sampler: &Arc<WebGLSampler>) {
    assert_eq!(frequency, BindingFrequency::PerDraw);
    let gl_texture = texture.texture().handle();
    let view_info = texture.info();
    let info = texture.texture().get_info();
    let is_cubemap = info.array_length == 6;
    let target = if is_cubemap { WebGlRenderingContext::TEXTURE_BINDING_CUBE_MAP } else { WebGlRenderingContext::TEXTURE_BINDING_2D };
    let bind_texture = self.context.get_parameter(target).unwrap();
    self.context.active_texture(WebGlRenderingContext::TEXTURE0 + binding);
    self.context.bind_texture(target, Some(gl_texture));
    {
      // TODO: optimize state changes
      /*self.context.tex_parameteri(target, WebGlRenderingContext::TEXTURE_WRAP_S, address_mode_to_gl(view_info.address_mode_u) as i32);
      self.context.tex_parameteri(target, WebGlRenderingContext::TEXTURE_WRAP_T, address_mode_to_gl(view_info.address_mode_v) as i32);
      self.context.tex_parameteri(target, WebGlRenderingContext::TEXTURE_MIN_FILTER, min_filter_to_gl(view_info.min_filter, view_info.mip_filter) as i32);
      self.context.tex_parameteri(target, WebGlRenderingContext::TEXTURE_MAG_FILTER, max_filter_to_gl(view_info.mag_filter) as i32);*/
    }
    self.context.active_texture(WebGlRenderingContext::TEXTURE0 + binding);
    //self.context.uniform1i(LOCATION, 0);

    if !bind_texture.is_null() {
      let bind_texture = bind_texture.unchecked_into::<WebGlTexture>();
      self.context.bind_texture(target, Some(&bind_texture));
    }
  }

  fn bind_uniform_buffer(&mut self, _frequency: BindingFrequency, _binding: u32, _buffer: &Arc<WebGLBuffer>) {
    unimplemented!()
  }

  fn bind_storage_buffer(&mut self, _frequency: BindingFrequency, _binding: u32, _buffer: &Arc<WebGLBuffer>) {
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
    WebGLCommandSubmission {}
  }

  fn bind_storage_texture(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<WebGLUnorderedAccessView>) {
    panic!("WebGL does not support storage textures")
  }

  fn begin_render_pass_1(&mut self, renderpass_info: &sourcerenderer_core::graphics::RenderPassBeginInfo<WebGLBackend>, recording_mode: sourcerenderer_core::graphics::RenderpassRecordingMode) {
    todo!()
  }

  fn advance_subpass(&mut self) {
    todo!()
  }

  fn end_render_pass(&mut self) {
    todo!()
  }

  fn barrier<'a>(&mut self, barriers: &[sourcerenderer_core::graphics::Barrier<WebGLBackend>]) {
    // nop
  }

  fn flush_barriers(&mut self) {
    // nop
  }

  fn inheritance(&self) -> &Self::CommandBufferInheritance {
    panic!("WebGL does not support inner command buffers")
  }

  type CommandBufferInheritance = ();

  fn execute_inner(&mut self, submission: Vec<WebGLCommandSubmission>) {
    panic!("WebGL does not support inner command buffers")
  }
}

pub struct WebGLCommandSubmission {}

pub struct WebGLQueue {
  context: Rc<RawWebGLContext>,
}

impl WebGLQueue {
  pub fn new(context: &Rc<RawWebGLContext>) -> Self {
    Self {
      context: context.clone()
    }
  }
}

impl Queue<WebGLBackend> for WebGLQueue {
  fn create_command_buffer(&self) -> WebGLCommandBuffer {
    WebGLCommandBuffer {
      context: self.context.clone(),
      pipeline: None
    }
  }

  fn create_inner_command_buffer(&self, inheritance: &()) -> WebGLCommandBuffer {
    panic!("WebGL does not support inner command buffers")
  }

  fn submit(&self, submission: WebGLCommandSubmission, fence: Option<&Arc<WebGLFence>>, wait_semaphores: &[&Arc<WebGLSemaphore>], signal_semaphores: &[&Arc<WebGLSemaphore>]) {
    // nop
  }

  fn present(&self, swapchain: &Arc<WebGLSwapchain>, wait_semaphores: &[&Arc<WebGLSemaphore>]) {
    // nop in WebGL
  }
}

// doesnt matter anyway for WebGL
unsafe impl Send for WebGLQueue {}
unsafe impl Sync for WebGLQueue {}
