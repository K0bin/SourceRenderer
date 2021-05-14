use sourcerenderer_core::graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, InputUsage, Output, PassInfo, PassInput, PassType, PipelineBinding, PipelineStage, RenderPassCallbacks, RenderPassTextureExtent, ShaderType, TextureUnorderedAccessView};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;
use crate::renderer::passes::desktop::geometry::OUTPUT_IMAGE;
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;

const PASS_NAME: &str = "TAA";
const BLIT_PASS_NAME: &str = "TAA_blit";
const HISTORY_BUFFER_NAME: &str = "TAA_buffer";

pub(crate) fn build_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: PASS_NAME.to_string(),
    pass_type: PassType::Compute {
      inputs: vec![
        PassInput {
          name: OUTPUT_IMAGE.to_string(),
          stage: PipelineStage::ComputeShader,
          usage: InputUsage::Sampled,
          is_history: false,
        },
        PassInput {
          name: HISTORY_BUFFER_NAME.to_string(),
          stage: PipelineStage::ComputeShader,
          usage: InputUsage::Sampled,
          is_history: true,
        },
      ],
      outputs: vec![
        Output::RenderTarget {
          name: HISTORY_BUFFER_NAME.to_string(),
          format: Format::RGBA8,
          samples: sourcerenderer_core::graphics::SampleCount::Samples1,
          extent: RenderPassTextureExtent::RelativeToSwapchain {
            width: 1.0f32,
            height: 1.0f32,
          },
          depth: 1,
          levels: 1,
          external: false,
          clear: false
        }
      ]
    }
  }
}

pub(crate) fn build_blit_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: BLIT_PASS_NAME.to_string(),
    pass_type: PassType::Copy {
      inputs: vec![
        PassInput {
          name: HISTORY_BUFFER_NAME.to_string(),
          stage: PipelineStage::ComputeShader,
          usage: InputUsage::Sampled,
          is_history: false,
        }
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
        command_buffer.bind_texture_view(BindingFrequency::PerDraw, 0, graph_resources.get_texture_srv(OUTPUT_IMAGE, false).expect("Failed to get graph resource"));
        command_buffer.bind_texture_view(BindingFrequency::PerDraw, 1, graph_resources.get_texture_srv(HISTORY_BUFFER_NAME, true).expect("Failed to get graph resource"));
        command_buffer.bind_storage_texture(BindingFrequency::PerDraw, 2, graph_resources.get_texture_uav(HISTORY_BUFFER_NAME, false).expect("Failed to get graph resource"));
        command_buffer.finish_binding();

        let dimensions = graph_resources.texture_dimensions(OUTPUT_IMAGE).unwrap();
        command_buffer.dispatch(dimensions.width, dimensions.height, 1);
      })
    ]
  ))
}

pub(crate) fn build_blit_pass<P: Platform>() -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  (BLIT_PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        let history_view = graph_resources.get_texture_uav(HISTORY_BUFFER_NAME, false).expect("Failed to get graph resource");
        let history_texture = history_view.texture();
        let backbuffer_view = graph_resources.get_texture_uav(BACK_BUFFER_ATTACHMENT_NAME, false).expect("Failed to get graph resource");
        let backbuffer_texture = backbuffer_view.texture();
        command_buffer.blit(history_texture, 0, 0, backbuffer_texture, 0, 0);
      })
    ]
  ))
}
