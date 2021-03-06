use sourcerenderer_core::{graphics::{AddressMode, Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Filter, Format, InputUsage, Output, PassInfo, PassInput, PassType, PipelineBinding, PipelineStage, RenderPassCallbacks, RenderPassTextureExtent, SamplerInfo, ShaderType, TextureUnorderedAccessView}};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;
use crate::renderer::passes::desktop::taa::HISTORY_BUFFER_NAME;
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;

const PASS_NAME: &str = "SHARPEN";
pub(crate) const SHARPEN_OUTPUT_NAME: &str = "sharpen";

pub(crate) fn build_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: PASS_NAME.to_string(),
    pass_type: PassType::Compute {
      inputs: vec![
        PassInput {
          name: HISTORY_BUFFER_NAME.to_string(),
          stage: PipelineStage::ComputeShader,
          usage: InputUsage::Sampled,
          is_history: false,
        },
      ],
      outputs: vec![
        Output::RenderTarget {
          name: SHARPEN_OUTPUT_NAME.to_string(),
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

pub(crate) fn build_pass<P: Platform>(device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>) -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  let sharpen_compute_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("sharpen.comp.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::ComputeShader, &bytes, Some("sharpen.comp.spv"))
  };

  let sampler = device.create_sampler(&SamplerInfo {
    mag_filter: Filter::Linear,
    min_filter: Filter::Linear,
    mip_filter: Filter::Linear,
    address_mode_u: AddressMode::Repeat,
    address_mode_v: AddressMode::Repeat,
    address_mode_w: AddressMode::Repeat,
    mip_bias: 0.0,
    max_anisotropy: 0.0,
    compare_op: None,
    min_lod: 0.0,
    max_lod: 1.0,
  });

  let sharpen_pipeline = device.create_compute_pipeline(&sharpen_compute_shader);
  (PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources, _frame_counter| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        command_buffer.set_pipeline(PipelineBinding::Compute(&sharpen_pipeline));
        command_buffer.bind_texture_view(BindingFrequency::PerDraw, 0, graph_resources.get_texture_srv(HISTORY_BUFFER_NAME, false).expect("Failed to get graph resource"), &sampler);
        command_buffer.bind_storage_texture(BindingFrequency::PerDraw, 1, graph_resources.get_texture_uav(SHARPEN_OUTPUT_NAME, false).expect("Failed to get graph resource"));
        command_buffer.finish_binding();

        let dimensions = graph_resources.texture_dimensions(HISTORY_BUFFER_NAME).unwrap();
        command_buffer.dispatch(dimensions.width, dimensions.height, 1);
      })
    ]
  ))
}