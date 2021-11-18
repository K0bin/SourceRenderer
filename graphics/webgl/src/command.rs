use std::{collections::VecDeque, sync::Arc};

use crossbeam_channel::Sender;
use sourcerenderer_core::graphics::{BindingFrequency, BufferUsage, CommandBuffer, LoadOp, PipelineBinding, Queue, Scissor, ShaderType, Viewport};
use web_sys::{WebGl2RenderingContext, WebGlRenderingContext};

use crate::{WebGLBackend, WebGLBuffer, WebGLFence, WebGLGraphicsPipeline, WebGLSwapchain, WebGLTexture, WebGLTextureShaderResourceView, sync::WebGLSemaphore, texture::{WebGLSampler, WebGLUnorderedAccessView}, thread::TextureHandle};

pub struct WebGLCommandBuffer {
  pipeline: Option<WebGLGraphicsPipeline>,
  commands: VecDeque<Box<dyn FnOnce(&mut crate::thread::WebGLThreadDevice) + Send>>
}

impl CommandBuffer<WebGLBackend> for WebGLCommandBuffer {
  fn set_pipeline(&mut self, pipeline: PipelineBinding<WebGLBackend>) {
    match pipeline {
      PipelineBinding::Graphics(pipeline) => {
        let handle = pipeline.handle();
        self.commands.push_back(Box::new(move |device| {
          let pipeline = device.pipeline(handle).clone();
          device.use_program(Some(pipeline.gl_program()));
        }));
      },
      PipelineBinding::Compute(_) => panic!("WebGL does not support compute shaders")
    }
  }

  fn set_vertex_buffer(&mut self, vertex_buffer: &Arc<WebGLBuffer>) {
    // TODO: maybe track dirty and do before draw

    let handle = vertex_buffer.handle();
    self.commands.push_back(Box::new(move |device| {
      let buffer = device.buffer(handle).clone();
      device.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(buffer.gl_buffer()));
    }));
  }

  fn set_index_buffer(&mut self, index_buffer: &Arc<WebGLBuffer>) {
    // TODO: maybe track dirty and do before draw

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

  fn init_texture_mip_level(&mut self, src_buffer: &Arc<WebGLBuffer>, texture: &Arc<WebGLTexture>, mip_level: u32, array_layer: u32) {
    /*let info = texture.get_info();
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
    }*/
    unimplemented!()
  }

  fn upload_dynamic_data<T>(&mut self, data: &[T], usage: BufferUsage) -> Arc<WebGLBuffer>
  where T: 'static + Send + Sync + Sized + Clone {
    //Arc::new(WebGLBuffer::new(&self.context, &BufferInfo { size: std::mem::size_of_val(data), usage }, MemoryUsage::CpuOnly))
    unimplemented!()
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
    self.commands.push_back(Box::new(move |device| {
      device.draw_arrays(
        WebGlRenderingContext::TRIANGLES, // TODO: self.pipeline.as_ref().unwrap().gl_draw_mode(),
        offset as i32,
        vertices as i32
      );
    }));
  }

  fn draw_indexed(&mut self, instances: u32, first_instance: u32, indices: u32, first_index: u32, vertex_offset: i32) {
    assert!(self.pipeline.is_none());

    // TODO: support instancing with WebGL2
    assert_eq!(instances, 0);
    assert_eq!(first_instance, 0);
    assert_eq!(vertex_offset, 0);
    self.commands.push_back(Box::new(move |device| {
      device.draw_elements_with_i32(
        WebGlRenderingContext::TRIANGLES, // TODO: self.pipeline.as_ref().unwrap().gl_draw_mode(),
        indices as i32,
        WebGlRenderingContext::UNSIGNED_INT,
        first_index as i32 * std::mem::size_of::<u32>() as i32,
      );
    }));
  }

  fn bind_texture_view(&mut self, frequency: BindingFrequency, binding: u32, texture: &Arc<WebGLTextureShaderResourceView>, sampler: &Arc<WebGLSampler>) {
    /*assert_eq!(frequency, BindingFrequency::PerDraw);
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
    }*/
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
    WebGLCommandSubmission {
      cmd_buffer: self
    }
  }

  fn bind_storage_texture(&mut self, _frequency: BindingFrequency, _binding: u32, _texture: &Arc<WebGLUnorderedAccessView>) {
    panic!("WebGL does not support storage textures")
  }

  fn begin_render_pass_1(&mut self, renderpass_info: &sourcerenderer_core::graphics::RenderPassBeginInfo<WebGLBackend>, recording_mode: sourcerenderer_core::graphics::RenderpassRecordingMode) {
    let mut clear_mask: u32 = 0;
    let mut color_attachments: [Option<TextureHandle>; 8] = Default::default();
    let mut depth_attachment = Option::<TextureHandle>::None;
    let subpass = &renderpass_info.subpasses[0];
    for (index, attachment_ref) in subpass.output_color_attachments.iter().enumerate() {
      let attachment = &renderpass_info.attachments[attachment_ref.index as usize];
      match &attachment.view {
        sourcerenderer_core::graphics::RenderPassAttachmentView::RenderTarget(rt) => {
          if attachment.load_op == LoadOp::Clear {
            clear_mask |= WebGl2RenderingContext::COLOR_BUFFER_BIT;
          }
          color_attachments[index] = Some(rt.texture().handle());
        },
        sourcerenderer_core::graphics::RenderPassAttachmentView::DepthStencil(ds) => {
          if attachment.load_op == LoadOp::Clear {
            clear_mask |= WebGl2RenderingContext::DEPTH_BUFFER_BIT;
          }
          depth_attachment = Some(ds.texture().handle());
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

  fn execute_inner(&mut self, submission: Vec<WebGLCommandSubmission>) {
    panic!("WebGL does not support inner command buffers")
  }
}

pub struct WebGLCommandSubmission {
  cmd_buffer: WebGLCommandBuffer
}

pub struct WebGLQueue {
  sender: Sender<Box<dyn FnOnce(&mut crate::thread::WebGLThreadDevice) + Send>>
}

impl WebGLQueue {
  pub fn new(sender: &Sender<Box<dyn FnOnce(&mut crate::thread::WebGLThreadDevice) + Send>>) -> Self {
    Self {
      sender: sender.clone()
    }
  }
}

impl Queue<WebGLBackend> for WebGLQueue {
  fn create_command_buffer(&self) -> WebGLCommandBuffer {
    WebGLCommandBuffer {
      pipeline: None,
      commands: VecDeque::new()
    }
  }

  fn create_inner_command_buffer(&self, _inheritance: &()) -> WebGLCommandBuffer {
    panic!("WebGL does not support inner command buffers")
  }

  fn submit(&self, mut submission: WebGLCommandSubmission, _fence: Option<&Arc<WebGLFence>>, _wait_semaphores: &[&Arc<WebGLSemaphore>], _signal_semaphores: &[&Arc<WebGLSemaphore>]) {
    for cmd in submission.cmd_buffer.commands.drain(..) {
      self.sender.send(cmd).unwrap();
    }
  }

  fn present(&self, swapchain: &Arc<WebGLSwapchain>, _wait_semaphores: &[&Arc<WebGLSemaphore>]) {
    // nop in WebGL
    swapchain.bump_frame();
  }
}
