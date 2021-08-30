use sourcerenderer_core::graphics::{Backend as GraphicsBackend, Barrier, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, MemoryUsage, PipelineBinding, ShaderType};
use sourcerenderer_core::{Matrix4, Platform};
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

pub struct LateLatchingPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>,
  camera_buffer: Arc<B::Buffer>,
  camera_buffer_b: Arc<B::Buffer>
}

impl<B: GraphicsBackend> LateLatchingPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>) -> Self {
    let copy_camera_compute_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("copy_camera.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("copy_camera.comp.spv"))
    };
  
    let copy_camera_pipeline = device.create_compute_pipeline(&copy_camera_compute_shader);
    let buffer_info = BufferInfo {
      size: std::mem::size_of::<Matrix4>() * 2,
      usage: BufferUsage::COMPUTE_SHADER_CONSTANT | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::FRAGMENT_SHADER_CONSTANT
        | BufferUsage::COMPUTE_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_STORAGE_READ | BufferUsage::FRAGMENT_SHADER_STORAGE_READ
        | BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
    };
    let camera_buffer = device.create_buffer(&&buffer_info, MemoryUsage::GpuOnly, Some("Camera"));
    let camera_buffer_b = device.create_buffer(&buffer_info, MemoryUsage::GpuOnly, Some("Camera_b"));
    Self {
      pipeline: copy_camera_pipeline,
      camera_buffer,
      camera_buffer_b
    }
  }

  pub fn execute(&mut self, command_buffer: &mut B::CommandBuffer, camera_ring_buffer: &Arc<B::Buffer>) {
    command_buffer.barrier(&[
      Barrier::BufferBarrier {
        old_primary_usage: BufferUsage::VERTEX_SHADER_CONSTANT,
        new_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        old_usages: BufferUsage::COMPUTE_SHADER_CONSTANT | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::FRAGMENT_SHADER_CONSTANT
          | BufferUsage::COMPUTE_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_STORAGE_READ | BufferUsage::FRAGMENT_SHADER_STORAGE_READ,
        new_usages: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        buffer: &self.camera_buffer,
      }
    ]);

    command_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 0, camera_ring_buffer);
    command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, &self.camera_buffer);
    command_buffer.finish_binding();
    command_buffer.dispatch(1, 1, 1);
  }

  pub fn swap_history_resources(&mut self) {
    std::mem::swap(&mut self.camera_buffer, &mut self.camera_buffer_b);    
  }

  pub fn camera_buffer(&self) -> &Arc<B::Buffer> {
    &self.camera_buffer
  }

  pub fn camera_buffer_history(&self) -> &Arc<B::Buffer> {
    &self.camera_buffer_b
  }
}
