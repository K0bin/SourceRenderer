use sourcerenderer_core::graphics::{PassInfo, PassType, PassInput, Output, RenderPassCallbacks, CommandBuffer, PipelineBinding, BindingFrequency, ShaderType, Backend as GraphicsBackend, Device, ExternalResource, ExternalOutput, ExternalProducerType, PipelineStage};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;
use crate::renderer::passes::desktop::geometry::OUTPUT_IMAGE;
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;

const PASS_NAME: &str = "TAA";

pub(crate) fn build_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: PASS_NAME.to_string(),
    pass_type: PassType::Compute {
      inputs: vec![
        PassInput {
          name: OUTPUT_IMAGE.to_string(),
          is_local: false,
          is_history: false,
          stage: PipelineStage::ComputeShader
        },
        PassInput {
          name: OUTPUT_IMAGE.to_string(),
          is_local: false,
          is_history: true,
          stage: PipelineStage::ComputeShader
        },
      ],
      outputs: vec![
        Output::Backbuffer {
          clear: false
        }
      ]
    }
  }
}

pub(crate) fn build_pass<P: Platform>(device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>) -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  let taa_compute_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("taa.comp.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::ComputeShader, &bytes, Some("taa.comp.spv"))
  };

  let copy_camera_pipeline = device.create_compute_pipeline(&taa_compute_shader);
  (PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        command_buffer.set_pipeline(PipelineBinding::Compute(&copy_camera_pipeline));
        command_buffer.bind_storage_texture(BindingFrequency::PerDraw, 0, graph_resources.get_texture_uav(OUTPUT_IMAGE, false).expect("Failed to get graph resource"));
        command_buffer.bind_storage_texture(BindingFrequency::PerDraw, 1, graph_resources.get_texture_uav(OUTPUT_IMAGE, true).expect("Failed to get graph resource"));
        command_buffer.bind_storage_texture(BindingFrequency::PerDraw, 2, graph_resources.get_texture_uav(BACK_BUFFER_ATTACHMENT_NAME, true).expect("Failed to get graph resource"));
        command_buffer.finish_binding();

        let dimensions = graph_resources.texture_dimensions(OUTPUT_IMAGE).unwrap();
        command_buffer.dispatch(dimensions.width, dimensions.height, 1);
      })
    ]
  ))
}
