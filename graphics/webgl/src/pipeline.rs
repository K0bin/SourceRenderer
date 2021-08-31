use std::{hash::Hash, rc::Rc};

use js_sys::JsString;
use sourcerenderer_core::graphics::{GraphicsPipelineInfo, PrimitiveType, Shader, ShaderType};
use web_sys::{WebGl2RenderingContext as WebGLContext, WebGlProgram, WebGlRenderingContext, WebGlShader};

use crate::{WebGLBackend, RawWebGLContext};

pub struct WebGLShader {
  context: Rc<RawWebGLContext>,
  shader: WebGlShader,
  shader_type: ShaderType
}

unsafe impl Send for WebGLShader {}
unsafe impl Sync for WebGLShader {}

impl Hash for WebGLShader {
  fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {
    unimplemented!()
  }
}

impl PartialEq for WebGLShader {
  fn eq(&self, other: &Self) -> bool {
    self.shader == other.shader
  }
}

impl Eq for WebGLShader {}

impl WebGLShader {
  pub fn new(context: &Rc<RawWebGLContext>, shader_type: ShaderType, data: &[u8]) -> Self {
    let gl_shader_type = match shader_type {
      ShaderType::VertexShader => WebGLContext::VERTEX_SHADER,
      ShaderType::FragmentShader => WebGLContext::FRAGMENT_SHADER,
      _ => panic!("Shader type is not supported by WebGL")
    };
    let shader = context.create_shader(gl_shader_type).unwrap();
    let source = String::from_utf8(data.iter().copied().collect()).unwrap();
    context.shader_source(&shader, source.as_str());
    context.compile_shader(&shader);
    Self {
      context: context.clone(),
      shader,
      shader_type
    }
  }

  pub fn handle(&self) -> &WebGlShader {
    &self.shader
  }
}

impl Drop for WebGLShader {
  fn drop(&mut self) {
    self.context.delete_shader(Some(&self.shader));
  }
}

impl Shader for WebGLShader {
  fn get_shader_type(&self) -> ShaderType {
    self.shader_type
  }
}

pub struct WebGLGraphicsPipeline {
  context: Rc<RawWebGLContext>,
  program: WebGlProgram,

  gl_draw_mode: u32
}

unsafe impl Send for WebGLGraphicsPipeline {}
unsafe impl Sync for WebGLGraphicsPipeline {}

impl WebGLGraphicsPipeline {
  pub fn new(context: &Rc<RawWebGLContext>, info: &GraphicsPipelineInfo<WebGLBackend>) -> Self {
    let program = context.create_program().unwrap();
    context.attach_shader(&program, info.vs.handle());
    context.attach_shader(&program, &info.fs.as_ref().unwrap().handle());
    context.link_program(&program);

    let attrib_count = context.get_program_parameter(&program, WebGlRenderingContext::ACTIVE_ATTRIBUTES).as_f64().unwrap() as u32;
    for i in 0..attrib_count {
      let attrib_info = context.get_active_attrib(&program, i).unwrap();
    }

    let gl_draw_mode = match &info.primitive_type {
        PrimitiveType::Triangles => WebGLContext::TRIANGLES,
        PrimitiveType::TriangleStrip => WebGLContext::TRIANGLE_STRIP,
        PrimitiveType::Lines => WebGLContext::LINES,
        PrimitiveType::LineStrip => WebGLContext::LINE_STRIP,
        PrimitiveType::Points => WebGLContext::POINTS,
    };
    Self {
      program,
      context: context.clone(),
      gl_draw_mode
    }
  }

  pub fn gl_draw_mode(&self) -> u32 {
    self.gl_draw_mode
  }

  pub fn gl_program(&self) -> &WebGlProgram {
    &self.program
  }
}

impl Drop for WebGLGraphicsPipeline {
  fn drop(&mut self) {
    self.context.delete_program(Some(&self.program));
  }
}

pub struct WebGLComputePipeline {}
