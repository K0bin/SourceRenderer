use std::{sync::{Arc, atomic::{AtomicU64, Ordering}}};
use log::warn;
use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, Device, GraphicsPipelineInfo, MemoryUsage, RenderPassInfo, SamplerInfo, TextureViewInfo, WHOLE_BUFFER};
use web_sys::{WebGl2RenderingContext, WebGlRenderingContext};
use crate::{GLThreadSender, WebGLBackend, WebGLBuffer, WebGLComputePipeline, WebGLFence, WebGLGraphicsPipeline, WebGLShader, WebGLSurface, WebGLTexture, WebGLTextureSamplingView, command::WebGLQueue, format_to_internal_gl, sync::WebGLSemaphore, texture::{WebGLDepthStencilView, WebGLRenderTargetView, WebGLSampler, WebGLUnorderedAccessView, format_to_gl, format_to_type}, thread::{BufferHandle, PipelineHandle, ShaderHandle, TextureHandle, WebGLThreadQueue}};

pub struct WebGLHandleAllocator {
  next_buffer_id: AtomicU64,
  next_texture_id: AtomicU64,
  next_shader_id: AtomicU64,
  next_pipeline_id: AtomicU64,
}

impl WebGLHandleAllocator {
  pub fn new_buffer_handle(&self) -> BufferHandle {
    self.next_buffer_id.fetch_add(1, Ordering::SeqCst) + 1
  }

  pub fn new_texture_handle(&self) -> TextureHandle {
    self.next_texture_id.fetch_add(1, Ordering::SeqCst) + 1
  }

  pub fn new_shader_handle(&self) -> ShaderHandle {
    self.next_shader_id.fetch_add(1, Ordering::SeqCst) + 1
  }

  pub fn new_pipeline_handle(&self) -> PipelineHandle {
    self.next_pipeline_id.fetch_add(1, Ordering::SeqCst) + 1
  }
}

pub struct WebGLDevice {
  handles: Arc<WebGLHandleAllocator>,
  queue: Arc<WebGLQueue>,
  thread_queue: GLThreadSender,
  _surface: Arc<WebGLSurface>,
}

impl WebGLDevice {
  pub fn new(surface: &Arc<WebGLSurface>) -> Self {
    let thread_queue = Arc::new(WebGLThreadQueue::new());
    let handles = Arc::new(
      WebGLHandleAllocator {
        next_buffer_id: AtomicU64::new(0),
        next_texture_id: AtomicU64::new(0),
        next_shader_id: AtomicU64::new(0),
        next_pipeline_id: AtomicU64::new(0),
      }
    );
    Self {
      queue: Arc::new(WebGLQueue::new(&thread_queue, &handles)),
      thread_queue: thread_queue,
      handles,
      _surface: surface.clone(),
    }
  }

  pub fn receiver(&self) -> &Arc<WebGLThreadQueue> {
    &self.thread_queue
  }

  pub fn sender(&self) -> &GLThreadSender {
    &self.thread_queue
  }

  pub fn handle_allocator(&self) -> &Arc<WebGLHandleAllocator> {
    &self.handles
  }
}

impl Device<WebGLBackend> for WebGLDevice {
  fn create_buffer(&self, info: &BufferInfo, memory_usage: MemoryUsage, _name: Option<&str>) -> Arc<WebGLBuffer> {
    let id = self.handles.new_buffer_handle();
    Arc::new(WebGLBuffer::new(id, info, memory_usage, &self.thread_queue))
  }

  fn upload_data<T>(&self, data: &[T], memory_usage: sourcerenderer_core::graphics::MemoryUsage, usage: sourcerenderer_core::graphics::BufferUsage) -> Arc<WebGLBuffer> where T: 'static + Send + Sync + Sized + Clone {
    let data = data.clone();
    let id = self.handles.new_buffer_handle();
    let buffer = Arc::new(WebGLBuffer::new(id, &BufferInfo { size: std::mem::size_of_val(data), usage }, memory_usage, &self.thread_queue));
    unsafe {
      let ptr = buffer.map_unsafe(true).unwrap();
      std::ptr::copy(data.as_ptr(), ptr as *mut T, data.len());
      buffer.unmap_unsafe(true);
    }
    buffer
  }

  fn create_shader(&self, shader_type: sourcerenderer_core::graphics::ShaderType, bytecode: &[u8], _name: Option<&str>) -> Arc<WebGLShader> {
    let id = self.handles.new_shader_handle();
    Arc::new(WebGLShader::new(id, shader_type, bytecode, &self.thread_queue))
  }

  fn create_texture(&self, info: &sourcerenderer_core::graphics::TextureInfo, _name: Option<&str>) -> Arc<WebGLTexture> {
    let id = self.handles.new_texture_handle();
    Arc::new(WebGLTexture::new(id, info, &self.thread_queue))
  }

  fn create_sampling_view(&self, texture: &Arc<WebGLTexture>, info: &sourcerenderer_core::graphics::TextureViewInfo, _name: Option<&str>) -> Arc<WebGLTextureSamplingView> {
    Arc::new(WebGLTextureSamplingView::new(texture, info))
  }

  fn create_graphics_pipeline(&self, info: &GraphicsPipelineInfo<WebGLBackend>, _pass_info: &RenderPassInfo, _subpass_index: u32, _name: Option<&str>) -> Arc<WebGLGraphicsPipeline> {
    let id = self.handles.new_pipeline_handle();
    Arc::new(WebGLGraphicsPipeline::new(id, info, &self.thread_queue))
  }

  fn create_compute_pipeline(&self, _shader: &Arc<WebGLShader>, _name: Option<&str>) -> Arc<WebGLComputePipeline> {
    panic!("WebGL does not support compute shaders");
  }

  fn wait_for_idle(&self) {
    // Can't implement that but it's also not our problem
  }

  fn init_texture(&self, texture: &Arc<WebGLTexture>, buffer: &Arc<WebGLBuffer>, mip_level: u32, array_layer: u32, src_buffer_offset: usize) {
    return;
    let buffer_id = buffer.handle();
    let texture_id = texture.handle();
    self.thread_queue.send(Box::new(move |device| {
      let buffer = device.buffer(buffer_id);
      let texture = device.texture(texture_id);
      let target = texture.target();
      let info = texture.info();

      device.bind_buffer(WebGl2RenderingContext::PIXEL_UNPACK_BUFFER, Some(buffer.gl_buffer()));
      device.bind_texture(target, Some(texture.gl_handle()));
      if !info.format.is_compressed() {
        device.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_i32(
          if texture.is_cubemap() { WebGlRenderingContext::TEXTURE_CUBE_MAP_POSITIVE_X + array_layer } else { target },
          mip_level as i32,
          format_to_internal_gl(info.format) as i32,
          info.width as i32,
          info.height as i32,
          0,
          format_to_gl(info.format),
          format_to_type(info.format),
          src_buffer_offset as i32
        ).unwrap();
      } else {
        device.compressed_tex_image_2d_with_i32_and_i32(
          if texture.is_cubemap() { WebGlRenderingContext::TEXTURE_CUBE_MAP_POSITIVE_X + array_layer } else { target },
          mip_level as i32,
          format_to_internal_gl(info.format),
          info.width as i32,
          info.height as i32,
          0,
          buffer.info().size as i32,
          src_buffer_offset as i32
        );
      }
    }));
  }

  fn init_texture_async(&self, texture: &Arc<WebGLTexture>, buffer: &Arc<WebGLBuffer>, mip_level: u32, array_layer: u32, buffer_offset: usize) -> Option<Arc<WebGLFence>> {
    self.init_texture(texture, buffer, mip_level, array_layer, buffer_offset);
    Some(Arc::new(WebGLFence::new()))
  }

  fn init_buffer(&self, src_buffer: &Arc<WebGLBuffer>, dst_buffer: &Arc<WebGLBuffer>, src_buffer_offset: usize, dst_buffer_offset: usize, length: usize) {
    let src_buffer_id = src_buffer.handle();
    let dst_buffer_id = dst_buffer.handle();
    self.thread_queue.send(Box::new(move |device| {
      let src_buffer = device.buffer(src_buffer_id);
      let dst_buffer = device.buffer(dst_buffer_id);

      if dst_buffer.info().usage.contains(BufferUsage::INDEX) {
        // WebGL does not allow using the index buffer for anything else, so we have to do the copy on the CPU
        let mut read_data = vec![0; if length == WHOLE_BUFFER { src_buffer.info().size } else { length }];
        device.bind_buffer(WebGl2RenderingContext::COPY_READ_BUFFER, Some(src_buffer.gl_buffer()));
        device.get_buffer_sub_data_with_i32_and_u8_array(WebGl2RenderingContext::COPY_READ_BUFFER, src_buffer_offset as i32, &mut read_data[..dst_buffer.info().size.min(length)]);
        device.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(dst_buffer.gl_buffer()));
        device.buffer_sub_data_with_i32_and_u8_array(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, dst_buffer_offset as i32, &read_data);
      } else {
        device.bind_buffer(WebGl2RenderingContext::COPY_READ_BUFFER, Some(src_buffer.gl_buffer()));
        device.bind_buffer(WebGl2RenderingContext::COPY_WRITE_BUFFER, Some(dst_buffer.gl_buffer()));
        device.copy_buffer_sub_data_with_i32_and_i32_and_i32(WebGl2RenderingContext::COPY_READ_BUFFER, WebGl2RenderingContext::COPY_WRITE_BUFFER, src_buffer_offset as i32, dst_buffer_offset as i32, if length == WHOLE_BUFFER { dst_buffer.info().size.min(src_buffer.info().size) as i32 } else { length as i32 });
      }
    }));
  }

  fn flush_transfers(&self) {
    // nop
  }

  fn free_completed_transfers(&self) {
    // nop
  }

  fn create_render_target_view(&self, texture: &Arc<WebGLTexture>, info: &TextureViewInfo, _name: Option<&str>) -> Arc<WebGLRenderTargetView> {
    Arc::new(WebGLRenderTargetView::new(texture, info))
  }

  fn create_storage_view(&self, _texture: &Arc<WebGLTexture>, _info: &TextureViewInfo, _name: Option<&str>) -> Arc<WebGLUnorderedAccessView> {
    panic!("WebGL does not support storage textures")
  }

  fn create_depth_stencil_view(&self, texture: &Arc<WebGLTexture>, info: &TextureViewInfo, _name: Option<&str>) -> Arc<WebGLDepthStencilView> {
    Arc::new(WebGLDepthStencilView::new(texture, info))
  }

  fn create_sampler(&self, _info: &SamplerInfo) -> Arc<WebGLSampler> {
    warn!("Samplers are unimplemented");
    Arc::new(WebGLSampler {})
  }

  fn create_fence(&self) -> Arc<WebGLFence> {
    Arc::new(WebGLFence {})
  }

  fn create_semaphore(&self) -> Arc<WebGLSemaphore> {
    Arc::new(WebGLSemaphore {})
  }

  fn graphics_queue(&self) -> &Arc<WebGLQueue> {
    &self.queue
  }

  fn prerendered_frames(&self) -> u32 {
    3
  }

  fn supports_bindless(&self) -> bool {
    false
  }

  fn insert_texture_into_bindless_heap(&self, _texture: &Arc<WebGLTextureSamplingView>) -> u32 {
    panic!("WebGL does not support bindless resources")
  }

  fn get_bottom_level_acceleration_structure_size(&self, _info: &sourcerenderer_core::graphics::BottomLevelAccelerationStructureInfo<WebGLBackend>) -> sourcerenderer_core::graphics::AccelerationStructureSizes {
    panic!("WebGL does not support ray tracing")
  }

  fn get_top_level_acceleration_structure_size(&self, _info: &sourcerenderer_core::graphics::TopLevelAccelerationStructureInfo<WebGLBackend>) -> sourcerenderer_core::graphics::AccelerationStructureSizes {
    panic!("WebGL does not support ray tracing")
  }

  fn create_raytracing_pipeline(&self, _info: &sourcerenderer_core::graphics::RayTracingPipelineInfo<WebGLBackend>) -> Arc<<WebGLBackend as sourcerenderer_core::graphics::Backend>::RayTracingPipeline> {
    panic!("WebGL does not support ray tracing")
  }

  fn supports_ray_tracing(&self) -> bool {
    false
  }

  fn supports_indirect(&self) -> bool {
    false
  }

  fn supports_min_max_filter(&self) -> bool {
    false
  }
}
