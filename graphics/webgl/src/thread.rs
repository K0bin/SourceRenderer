use std::{cell::RefCell, collections::{HashMap}, hash::Hash, ops::Deref, rc::Rc};

use crossbeam_channel::Receiver;
use log::warn;
use sourcerenderer_core::graphics::{BindingFrequency, BufferInfo, BufferUsage, GraphicsPipelineInfo, InputRate, MemoryUsage, PrimitiveType, ShaderType, TextureInfo};

use web_sys::{Document, WebGl2RenderingContext, WebGlBuffer as WebGLBufferHandle, WebGlFramebuffer, WebGlProgram, WebGlRenderingContext, WebGlShader, WebGlTexture, WebGlVertexArrayObject};

use crate::{GLThreadReceiver, WebGLBackend, WebGLSurface, raw_context::RawWebGLContext};

#[derive(Hash, PartialEq, Eq, Debug)]
struct FboKey {
  rts: [Option<TextureHandle>; 8],
  ds: Option<TextureHandle>
}

pub struct WebGLThreadTexture {
  texture: WebGlTexture,
  context: Rc<RawWebGLContext>,
  info: TextureInfo,
  is_cubemap: bool,
  target: u32
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
  gl_usage: u32,
  buffer_handle: BufferHandle
}

impl WebGLThreadBuffer {
  pub fn new(
    context: &Rc<RawWebGLContext>,
    info: &BufferInfo,
    buffer_handle: BufferHandle,
    _memory_usage: MemoryUsage,
  ) -> Self {
    let buffer_usage = info.usage;

    if buffer_usage.contains(BufferUsage::INDEX) && buffer_usage != BufferUsage::INDEX {
      if buffer_usage == BufferUsage::INDEX | BufferUsage::COPY_DST {
        warn!("WebGL does not allow using index buffers for anything else. Buffer copies will be handled on the CPU.");
      } else {
        panic!("WebGL does not allow using index buffers for anything else.");
      }
    }

    let mut usage = WebGlRenderingContext::STATIC_DRAW;
    if buffer_usage.intersects(BufferUsage::COPY_DST) {
      if buffer_usage.intersects(BufferUsage::CONSTANT) {
        usage = WebGl2RenderingContext::STREAM_READ;
      } else {
        usage = WebGl2RenderingContext::STATIC_READ;
      }
    }
    if buffer_usage.intersects(BufferUsage::COPY_SRC) {
      /*if buffer_usage.intersects(BufferUsage::CONSTANT) {
        usage = WebGl2RenderingContext::STREAM_COPY;
      } else {
        usage = WebGl2RenderingContext::STATIC_COPY;
      }*/
      usage = WebGl2RenderingContext::STREAM_READ;
    }
    let buffer = context.create_buffer().unwrap();
    let target = crate::buffer::buffer_usage_to_target(info.usage);
    context.bind_buffer(target, Some(&buffer));
    context.buffer_data_with_i32(target, info.size as i32, usage);
    Self {
      context: context.clone(),
      info: info.clone(),
      gl_usage: usage,
      buffer,
      buffer_handle
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

  pub fn handle(&self) -> BufferHandle {
    self.buffer_handle
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

pub struct WebGLBlockInfo {
  pub name: String,
  pub binding: u32,
  pub size: u32
}

pub struct WebGLThreadPipeline {
  context: Rc<RawWebGLContext>,
  program: WebGlProgram,
  ubo_infos: HashMap<(BindingFrequency, u32), WebGLBlockInfo>,
  push_constants_info: Option<WebGLBlockInfo>,
  vao_cache: RefCell<HashMap<[Option<BufferHandle>; 4], WebGlVertexArrayObject>>,
  info: GraphicsPipelineInfo<WebGLBackend>,

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

  pub fn get_vao(&self, vertex_buffers: &[Option<Rc<WebGLThreadBuffer>>; 4]) -> WebGlVertexArrayObject {
    let mut key: [Option<BufferHandle>; 4] = Default::default();
    for i in 0..vertex_buffers.len() {
      key[i] = vertex_buffers[i].as_ref().map(|b| b.handle());
    }
    {
      let cache = self.vao_cache.borrow();
      if let Some(cached) = cache.get(&key) {
        return cached.clone();
      }
    }

    let mut cache_mut = self.vao_cache.borrow_mut();
    let attrib_count = self.context.get_program_parameter(&self.program, WebGl2RenderingContext::ACTIVE_ATTRIBUTES).as_f64().unwrap() as u32;
    for i in 0..attrib_count {
      let attrib_info = self.context.get_active_attrib(&self.program, i).unwrap();
      let name = attrib_info.name();
      let mut name_parts = name.split("_"); // name should be like this: "vs_input_X"
      name_parts.next();
      name_parts.next();
      let attrib_index = name_parts.next().unwrap().parse::<u32>().unwrap();
      self.context.bind_attrib_location(&self.program, attrib_index, &attrib_info.name());
    }

    let vao = self.context.create_vertex_array().unwrap();
    self.context.bind_vertex_array(Some(&vao));
    for ia_element in &self.info.vertex_layout.input_assembler {
      let input = self.info.vertex_layout.shader_inputs.iter().find(|a| a.input_assembler_binding == ia_element.binding).unwrap();
      let gl_attrib_index = input.location_vk_mtl;

      let buffer = vertex_buffers[ia_element.binding as usize].as_ref();
      if buffer.is_none() {
        warn!("Vertex buffer {} not bound", ia_element.binding);
        continue;
      }
      let buffer = buffer.unwrap();

      self.context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(buffer.gl_buffer()));
      self.context.enable_vertex_attrib_array(gl_attrib_index);
      self.context.vertex_attrib_divisor(gl_attrib_index,  if ia_element.input_rate == InputRate::PerVertex { 0 } else { 1 });
      self.context.vertex_attrib_pointer_with_i32(gl_attrib_index, input.format.element_size() as i32 / std::mem::size_of::<f32>() as i32, WebGl2RenderingContext::FLOAT, false, ia_element.stride as i32, input.offset as i32);
    }
    cache_mut.insert(key, vao.clone());
    vao
  }

  pub fn push_constants_info(&self) -> Option<&WebGLBlockInfo> {
    self.push_constants_info.as_ref()
  }

  pub fn ubo_info(&self, frequency: BindingFrequency, binding: u32) -> Option<&WebGLBlockInfo> {
    self.ubo_infos.get(&(frequency, binding))
  }
}

impl Drop for WebGLThreadPipeline {
  fn drop(&mut self) {
    let mut cache = self.vao_cache.borrow_mut();
    for (_key, vao) in cache.drain() {
      self.context.delete_vertex_array(Some(&vao));
    }

    self.context.delete_program(Some(&self.program));
  }
}

pub struct WebGLThreadDevice {
  context: Rc<RawWebGLContext>,
  textures: HashMap<TextureHandle, Rc<WebGLThreadTexture>>,
  shaders: HashMap<ShaderHandle, Rc<WebGLThreadShader>>,
  pipelines: HashMap<PipelineHandle, Rc<WebGLThreadPipeline>>,
  buffers: HashMap<BufferHandle, Rc<WebGLThreadBuffer>>,
  receiver: Receiver<Box<dyn FnOnce(&mut Self) + Send>>,
  fbo_cache: HashMap<FboKey, WebGlFramebuffer>
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
      receiver: receiver.clone(),
      fbo_cache: HashMap::new()
    }
  }

  pub fn create_buffer(&mut self, id: BufferHandle, info: &BufferInfo, memory_usage: MemoryUsage, name: Option<&str>) {
    let buffer = WebGLThreadBuffer::new(&self.context, info, id, memory_usage);
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
    let info = self.context.get_shader_info_log(&shader);
    if let Some(info) = info {
      if !info.is_empty() {
        warn!("Shader info: {}", info);
      }
    }
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
    if !self.context.get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS).as_bool().unwrap() {
      panic!("Linking shader failed.");
    }

    let mut push_constants_info = Option::<WebGLBlockInfo>::None;
    let mut ubo_infos = HashMap::<(BindingFrequency, u32), WebGLBlockInfo>::new();
    let ubo_count = self.context.get_program_parameter(&program, WebGl2RenderingContext::ACTIVE_UNIFORM_BLOCKS).as_f64().unwrap() as u32;
    for i in 0..ubo_count {
      let binding = i + 1;
      self.context.uniform_block_binding(&program, i, binding);
      let size = self.context.get_active_uniform_block_parameter(&program, i, WebGl2RenderingContext::UNIFORM_BLOCK_DATA_SIZE).unwrap().as_f64().unwrap() as u32;
      let ubo_name = self.context.get_active_uniform_block_name(&program, i).unwrap();
      if ubo_name == "push_constants_t" {
        push_constants_info = Some(WebGLBlockInfo {
          name: ubo_name,
          size,
          binding: binding
        });
        continue;
      }
      let mut ubo_name_parts = ubo_name.split("_"); // name should be like this: "res_X_X_t"
      ubo_name_parts.next();
      let set = ubo_name_parts.next().unwrap();
      let descriptor_set_binding = ubo_name_parts.next().unwrap();
      let frequency = match set.parse::<u32>().unwrap() {
        0 => BindingFrequency::PerDraw,
        1 => BindingFrequency::PerMaterial,
        2 => BindingFrequency::PerFrame,
        3 => BindingFrequency::Rarely,
        _ => panic!("Invalid binding frequency")
      };
      ubo_infos.insert((frequency, descriptor_set_binding.parse::<u32>().unwrap()), WebGLBlockInfo {
        name: ubo_name,
        size,
        binding: binding
      });
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
      gl_draw_mode,
      ubo_infos,
      push_constants_info,
      vao_cache: RefCell::new(HashMap::new()),
      info: info.clone()
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

  pub fn get_framebuffer(&mut self, rts: &[Option<TextureHandle>; 8], ds: Option<TextureHandle>) -> Option<WebGlFramebuffer> {
    let mut use_internal_fbo = Option::<bool>::None;
    for rt in rts {
      if rt.is_none() {
        continue;
      }
      let rt = rt.unwrap();
      if rt == 1 {
        if let Some(use_internal_fbo) = use_internal_fbo {
          if !use_internal_fbo {
            panic!("Cannot mix internal fbo texture and manually created textures");
          }
        } else {
          use_internal_fbo = Some(true);
        }
      } else {
        if let Some(use_internal_fbo) = use_internal_fbo {
          if use_internal_fbo {
            panic!("Cannot mix internal fbo texture and manually created textures");
          }
        }
        use_internal_fbo = Some(false);
      }
    }
    if let Some(ds) = ds {
      if ds == 1 {
        if let Some(use_internal_fbo) = use_internal_fbo {
          if !use_internal_fbo {
            panic!("Cannot mix internal fbo texture and manually created textures");
          }
        } else {
          use_internal_fbo = Some(true);
        }
      } else {
        if let Some(use_internal_fbo) = use_internal_fbo {
          if use_internal_fbo {
            panic!("Cannot mix internal fbo texture and manually created textures");
          }
        }
        use_internal_fbo = Some(false);
      }
    }

    if use_internal_fbo.expect("Empty frame buffer") {
      return None;
    }

    let key = FboKey {
      rts: rts.clone(),
      ds: ds.clone()
    };

    let fbo = self.fbo_cache.get(&key);
    if let Some(fbo) = fbo {
      return Some(fbo.clone());
    }

    let fbo = self.context.create_framebuffer().unwrap();
    self.context.bind_framebuffer(WebGl2RenderingContext::DRAW_FRAMEBUFFER, Some(&fbo));
    for (index, rt) in rts.iter().enumerate() {
      if rt.is_none() {
        continue;
      }
      let rt = rt.unwrap();
      let rt_texture = self.texture(rt);
      self.context.framebuffer_texture_2d(WebGl2RenderingContext::DRAW_FRAMEBUFFER, WebGl2RenderingContext::COLOR_ATTACHMENT0 + index as u32, WebGl2RenderingContext::TEXTURE_2D, Some(&rt_texture.texture), 0);
    }

    if let Some(ds) = ds {
      let ds_texture = self.texture(ds);
      self.context.framebuffer_texture_2d(WebGl2RenderingContext::DRAW_FRAMEBUFFER, WebGl2RenderingContext::DEPTH_STENCIL_ATTACHMENT, WebGl2RenderingContext::TEXTURE_2D, Some(&ds_texture.texture), 0);
    }

    assert!(self.context.is_framebuffer(Some(&fbo)));
    assert_eq!(self.context.check_framebuffer_status(WebGl2RenderingContext::DRAW_FRAMEBUFFER), WebGl2RenderingContext::FRAMEBUFFER_COMPLETE);
    self.fbo_cache.insert(key, fbo.clone());
    Some(fbo)
  }
}

impl Deref for WebGLThreadDevice {
  type Target = WebGl2RenderingContext;

  fn deref(&self) -> &Self::Target {
    &self.context
  }
}
