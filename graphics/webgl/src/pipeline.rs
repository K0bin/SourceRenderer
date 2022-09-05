use std::{hash::{Hash, Hasher}};

use sourcerenderer_core::graphics::{GraphicsPipelineInfo, Shader, ShaderType, ComputePipeline, BindingFrequency, BindingInfo};

use crate::{GLThreadSender, WebGLBackend, thread::{ShaderHandle, WebGLPipelineInfo}};

pub struct WebGLShader {
  handle: ShaderHandle,
  shader_type: ShaderType,
  sender: GLThreadSender
}

impl Hash for WebGLShader {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.handle.hash(state);
  }
}

impl PartialEq for WebGLShader {
  fn eq(&self, other: &Self) -> bool {
    self.handle == other.handle
  }
}

impl Eq for WebGLShader {}

impl WebGLShader {
  pub fn new(handle: ShaderHandle, shader_type: ShaderType, data: &[u8], sender: &GLThreadSender) -> Self {
    let data: Vec<u8> = data.iter().copied().collect();
    let boxed_data = data.into_boxed_slice();
    sender.send(Box::new(move |device| {
      device.create_shader(handle, shader_type, &boxed_data);
    }));
    Self {
      handle,
      shader_type,
      sender: sender.clone()
    }
  }

  pub fn handle(&self) -> ShaderHandle {
    self.handle
  }
}

impl Drop for WebGLShader {
  fn drop(&mut self) {
    let handle = self.handle;
    self.sender.send(Box::new(move |device| {
      device.remove_shader(handle);
    }));
  }
}

impl Shader for WebGLShader {
  fn shader_type(&self) -> ShaderType {
    self.shader_type
  }
}

pub struct WebGLGraphicsPipeline {
  handle: PipelineHandle,
  sender: GLThreadSender
}

impl WebGLGraphicsPipeline {
  pub fn new(handle: PipelineHandle, info: &GraphicsPipelineInfo<WebGLBackend>, sender: &GLThreadSender) -> Self {
    let info: WebGLPipelineInfo = info.into();
    sender.send(Box::new(move |device| {
      device.create_pipeline(handle, info);
    }));
    Self {
      handle,
      sender: sender.clone()
    }
  }

  pub fn handle(&self) -> PipelineHandle {
    self.handle
  }
}

impl Drop for WebGLGraphicsPipeline {
  fn drop(&mut self) {
    let handle = self.handle;
    self.sender.send(Box::new(move |device| {
      device.remove_pipeline(handle);
    }));
  }
}

pub struct WebGLComputePipeline {}

impl ComputePipeline for WebGLComputePipeline {
  fn binding_info(&self, _set: BindingFrequency, _slot: u32) -> Option<BindingInfo> {
    unimplemented!()
  }
}
