use sourcerenderer_core::graphics::{Backend as GraphicsBackend, Barrier, BindingFrequency, Buffer, BufferInfo, BufferUsage, CommandBuffer, Device, ExternalOutput, ExternalProducerType, ExternalResource, InputUsage, MemoryUsage, Output, PassInfo, PassInput, PassType, PipelineBinding, PipelineStage, RenderPassCallbacks, ShaderType};
use sourcerenderer_core::{Matrix4, Platform};
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use crate::renderer::LateLatchCamera;
use sourcerenderer_core::platform::io::IO;

const PASS_NAME: &str = "LateLatch";
pub(super) const OUTPUT_CAMERA: &str = "Camera";
const EXTERNAL_RING_BUFFER: &str = "CameraRingBuffer";

pub(crate) fn build_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: PASS_NAME.to_string(),
    pass_type: PassType::Compute {
      inputs: vec![
        PassInput {
          name: EXTERNAL_RING_BUFFER.to_string(),
          usage: InputUsage::Storage,
          is_history: false,
          stage: PipelineStage::ComputeShader
        }
      ],
      outputs: vec![
        Output::Buffer {
          name: OUTPUT_CAMERA.to_string(),
          format: None,
          size: std::mem::size_of::<Matrix4>() as u32 * 2,
          clear: false
        }
      ]
    }
  }
}

pub(crate) fn build_pass<P: Platform>(device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>) -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  let copy_camera_compute_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("copy_camera.comp.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::ComputeShader, &bytes, Some("copy_camera.comp.spv"))
  };

  let copy_camera_pipeline = device.create_compute_pipeline(&copy_camera_compute_shader);
  (PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources, _frame_counter| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        command_buffer.set_pipeline(PipelineBinding::Compute(&copy_camera_pipeline));
        command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 0, graph_resources.get_buffer(EXTERNAL_RING_BUFFER, false).expect("Failed to get graph resource"));
        command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, graph_resources.get_buffer(OUTPUT_CAMERA, false).expect("Failed to get graph resource"));
        command_buffer.finish_binding();
        command_buffer.dispatch(1, 1, 1);
      })
    ]
  ))
}


pub(crate) fn external_resource_template() -> ExternalOutput {
  ExternalOutput::Buffer {
    name: EXTERNAL_RING_BUFFER.to_string(),
    producer_type: ExternalProducerType::Host
  }
}

pub(crate) fn external_resource<B: GraphicsBackend>(primary_camera: &Arc<LateLatchCamera<B>>) -> (String, ExternalResource<B>) {
  (EXTERNAL_RING_BUFFER.to_string(), ExternalResource::Buffer(primary_camera.buffer().clone()))
}


// =========================================0
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
