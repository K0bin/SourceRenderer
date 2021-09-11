use std::{cell::{Ref, RefCell}, collections::{HashMap, HashSet}, hash::Hash, ops::Deref, rc::Rc, sync::Mutex};

use crossbeam_channel::{Receiver, Sender};
use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, GraphicsPipelineInfo, MappedBuffer, MemoryUsage, MutMappedBuffer, PrimitiveType, ShaderType, TextureInfo};

use web_sys::{Document, WebGl2RenderingContext, WebGlBuffer as WebGLBufferHandle, WebGlProgram, WebGlRenderingContext, WebGlShader, WebGlTexture};

use crate::{GLThreadReceiver, WebGLBackend, WebGLShader, WebGLSurface, WebGLTexture, raw_context::RawWebGLContext};

pub struct WebGLThreadTexture {
  texture: WebGlTexture,
  context: Rc<RawWebGLContext>,
  info: TextureInfo,
  is_cubemap: bool,
  target: u32,
}

impl WebGLThreadTexture {
  pub fn new(context: &Rc<RawWebGLContext>, info: &TextureInfo) -> Self {
    assert!(info.array_length == 6 || info.array_length == 1);
    let is_cubemap = info.array_length == 6;
    let target = if is_cubemap { WebGlRenderingContext::TEXTURE_CUBE_MAP } else { WebGlRenderingContext::TEXTURE_2D };
    let texture = context.create_texture().unwrap();
    Self {
      texture,
      context: context.clone(),
      info: info.clone(),
      is_cubemap,
      target
    }
  }

  pub fn info(&self) -> &TextureInfo {
    &self.info
  }

  pub fn is_cubemap(&self) -> bool {
    self.is_cubemap
  }

  pub fn target(&self) -> u32 {
    self.target
  }

  pub fn gl_handle(&self) -> &WebGlTexture {
    &self.texture
  }
}

impl Drop for WebGLThreadTexture {
  fn drop(&mut self) {
    self.context.delete_texture(Some(&self.texture));
  }
}

pub struct WebGLThreadBuffer {
  context: Rc<RawWebGLContext>,
  buffer: WebGLBufferHandle,
  info: BufferInfo,
  gl_usage: u32
}

impl WebGLThreadBuffer {
  pub fn new(
    context: &Rc<RawWebGLContext>,
    info: &BufferInfo,
    _memory_usage: MemoryUsage,
  ) -> Self {
    let buffer_usage = info.usage;
    let mut usage = WebGlRenderingContext::STATIC_DRAW;
    if buffer_usage.intersects(BufferUsage::COPY_DST) {
      if buffer_usage.intersects(BufferUsage::CONSTANT) {
        usage = WebGl2RenderingContext::STREAM_READ;
      } else {
        usage = WebGl2RenderingContext::STATIC_READ;
      }
    }
    if buffer_usage.intersects(BufferUsage::COPY_SRC) {
      if buffer_usage.intersects(BufferUsage::CONSTANT) {
        usage = WebGl2RenderingContext::STREAM_COPY;
      } else {
        usage = WebGl2RenderingContext::STATIC_COPY;
      }
    }
    let buffer = context.create_buffer().unwrap();
    Self {
      context: context.clone(),
      info: info.clone(),
      gl_usage: usage,
      buffer,
    }
  }

  pub fn gl_buffer(&self) -> &WebGLBufferHandle {
    &self.buffer
  }

  pub fn gl_usage(&self) -> u32 {
    self.gl_usage
  }

  pub fn info(&self) -> &BufferInfo {
    &self.info
  }
}

impl Drop for WebGLThreadBuffer {
  fn drop(&mut self) {
    self.context.delete_buffer(Some(&self.buffer));
  }
}

pub struct WebGLThreadShader {
  context: Rc<RawWebGLContext>,
  shader: WebGlShader,  
}

impl Drop for WebGLThreadShader {
  fn drop(&mut self) {
    self.context.delete_shader(Some(&self.shader));
  }
}

pub struct WebGLThreadPipeline {
  context: Rc<RawWebGLContext>,
  program: WebGlProgram,

  // graphics state
  gl_draw_mode: u32
}

impl WebGLThreadPipeline {
  pub fn gl_draw_mode(&self) -> u32 {
    self.gl_draw_mode
  }

  pub fn gl_program(&self) -> &WebGlProgram {
    &self.program
  }
}

impl Drop for WebGLThreadPipeline {
  fn drop(&mut self) {
    self.context.delete_program(Some(&self.program));
  }
}

pub struct WebGLThreadDevice {
  context: Rc<RawWebGLContext>,
  textures: HashMap<BufferHandle, Rc<WebGLThreadTexture>>,
  shaders: HashMap<ShaderHandle, Rc<WebGLThreadShader>>,
  pipelines: HashMap<PipelineHandle, Rc<WebGLThreadPipeline>>,
  buffers: HashMap<TextureHandle, Rc<WebGLThreadBuffer>>,
  receiver: Receiver<Box<dyn FnOnce(&mut Self) + Send>>
}

pub type BufferHandle = u64;
pub type TextureHandle = u64;
pub type ShaderHandle = u64;
pub type PipelineHandle = u64;

impl WebGLThreadDevice {
  pub fn new(receiver: &GLThreadReceiver, surface: &WebGLSurface, document: &Document) -> Self {
    Self {
      context: Rc::new(RawWebGLContext::new(document, surface)),
      textures: HashMap::new(),
      shaders: HashMap::new(),
      pipelines: HashMap::new(),
      buffers: HashMap::new(),
      receiver: receiver.clone()
    }
  }

  pub fn create_buffer(&mut self, id: BufferHandle, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) {
    let buffer = WebGLThreadBuffer::new(&self.context, info, memory_usage);
    self.buffers.insert(id, Rc::new(buffer));
  }

  pub fn remove_buffer(&mut self, id: BufferHandle) {
    self.buffers.remove(&id).expect("Buffer didnt exist");
  }

  pub fn buffer(&self, id: BufferHandle) -> &Rc<WebGLThreadBuffer> {
    self.buffers.get(&id).expect("Cant find buffer")
  }

  pub fn create_shader(&mut self, id: ShaderHandle, shader_type: ShaderType, data: &[u8]) {    
    let gl_shader_type = match shader_type {
      ShaderType::VertexShader => WebGl2RenderingContext::VERTEX_SHADER,
      ShaderType::FragmentShader => WebGl2RenderingContext::FRAGMENT_SHADER,
      _ => panic!("Shader type is not supported by WebGL")
    };
    let shader = self.context.create_shader(gl_shader_type).unwrap();
    let source = String::from_utf8(data.iter().copied().collect()).unwrap();
    self.context.shader_source(&shader, source.as_str());
    self.context.compile_shader(&shader);
    self.shaders.insert(id, Rc::new(WebGLThreadShader {
      context: self.context.clone(),
      shader: shader,
    }));
  }

  pub fn shader(&self, id: ShaderHandle) -> &Rc<WebGLThreadShader> {
    self.shaders.get(&id).expect("Shader does not exist")
  }

  pub fn remove_shader(&mut self, id: ShaderHandle) {
    self.shaders.remove(&id).expect("Shader does not exist");
  }

  pub fn create_pipeline(&mut self, id: PipelineHandle, info: &GraphicsPipelineInfo<WebGLBackend>) {
    let vs = self.shader(info.vs.handle()).clone();
    let fs = info.fs.as_ref().map(|fs| self.shader(fs.handle()).clone());

    let program = self.context.create_program().unwrap();
    self.context.attach_shader(&program, &vs.shader);
    if let Some(fs) = &fs {
      self.context.attach_shader(&program, &fs.shader);
    }
    self.context.link_program(&program);

    let attrib_count = self.context.get_program_parameter(&program, WebGlRenderingContext::ACTIVE_ATTRIBUTES).as_f64().unwrap() as u32;
    for i in 0..attrib_count {
      let attrib_info = self.context.get_active_attrib(&program, i).unwrap();
    }

    let gl_draw_mode = match &info.primitive_type {
        PrimitiveType::Triangles => WebGl2RenderingContext::TRIANGLES,
        PrimitiveType::TriangleStrip => WebGl2RenderingContext::TRIANGLE_STRIP,
        PrimitiveType::Lines => WebGl2RenderingContext::LINES,
        PrimitiveType::LineStrip => WebGl2RenderingContext::LINE_STRIP,
        PrimitiveType::Points => WebGl2RenderingContext::POINTS,
    };
    self.pipelines.insert(id, Rc::new(WebGLThreadPipeline {
      program,
      context: self.context.clone(),
      gl_draw_mode
    }));
  }

  pub fn pipeline(&self, id: PipelineHandle) -> &Rc<WebGLThreadPipeline> {
    self.pipelines.get(&id).expect("Pipeline does not exist")
  }

  pub fn remove_pipeline(&mut self, id: PipelineHandle) {
    self.pipelines.remove(&id).expect("Pipeline does not exist");
  }

  pub fn create_texture(&mut self, id: TextureHandle, info: &TextureInfo) {
    let texture = WebGLThreadTexture::new(&self.context, info);
    self.textures.insert(id, Rc::new(texture));
  }

  pub fn texture(&self, id: TextureHandle) -> &WebGLThreadTexture {
    self.textures.get(&id).expect("Texture does not exist")
  }

  pub fn remove_texture(&mut self, id: TextureHandle) {
    self.textures.remove(&id).expect("Texture does not exist");
  }

  pub fn process(&mut self) {
    let mut cmd_res = self.receiver.try_recv();
    while cmd_res.is_ok() {
      let cmd = cmd_res.unwrap();
      cmd(self);
      cmd_res = self.receiver.try_recv();
    }
  }
}

impl Deref for WebGLThreadDevice {
  type Target = WebGl2RenderingContext;

  fn deref(&self) -> &Self::Target {
    &self.context
  }
}
