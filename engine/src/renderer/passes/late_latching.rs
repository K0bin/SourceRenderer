use sourcerenderer_core::graphics::{PassInfo, PassType, PassInput, ComputeOutput, RenderPassCallbacks, CommandBuffer, PipelineBinding, BindingFrequency, ShaderType, Backend as GraphicsBackend, Device, ExternalResource, ExternalOutput, ExternalProducerType};
use sourcerenderer_core::{Matrix4, Platform};
use std::sync::Arc;
use std::fs::File;
use std::path::Path;
use std::io::Read;
use crate::renderer::LateLatchCamera;

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
          is_local: false
        }
      ],
      outputs: vec![
        ComputeOutput::Buffer {
          name: OUTPUT_CAMERA.to_string(),
          format: None,
          size: std::mem::size_of::<Matrix4>() as u32,
          clear: false
        }
      ]
    }
  }
}

pub(crate) fn build_pass<B: GraphicsBackend>(device: &Arc<B::Device>) -> (String, RenderPassCallbacks<B>) {
  let copy_camera_compute_shader = {
    let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("shaders")).join(Path::new("copy_camera.comp.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::ComputeShader, &bytes, Some("copy_camera.comp.spv"))
  };

  let copy_camera_pipeline = device.create_compute_pipeline(&copy_camera_compute_shader);
  (PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources| {
        let command_buffer = command_buffer_a as &mut B::CommandBuffer;
        command_buffer.set_pipeline(PipelineBinding::Compute(&copy_camera_pipeline));
        command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, graph_resources.get_buffer(EXTERNAL_RING_BUFFER).expect("Failed to get graph resource"));
        command_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, graph_resources.get_buffer(OUTPUT_CAMERA).expect("Failed to get graph resource"));
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