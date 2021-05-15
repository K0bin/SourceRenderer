use sourcerenderer_core::{Matrix4, Vec2, graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, InputUsage, Output, PassInfo, PassInput, PassType, PipelineBinding, PipelineStage, RenderPassCallbacks, RenderPassTextureExtent, ShaderType, TextureUnorderedAccessView}};
use sourcerenderer_core::Platform;
use std::{sync::Arc, usize};
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;
use crate::renderer::passes::desktop::{geometry::OUTPUT_IMAGE, prepass::OUTPUT_MOTION};
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
        PassInput {
          name: OUTPUT_MOTION.to_string(),
          stage: PipelineStage::ComputeShader,
          usage: InputUsage::Sampled,
          is_history: false
        }
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
      Arc::new(move |command_buffer_a, graph_resources, _frame_counter| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        command_buffer.set_pipeline(PipelineBinding::Compute(&copy_camera_pipeline));
        command_buffer.bind_texture_view(BindingFrequency::PerDraw, 0, graph_resources.get_texture_srv(OUTPUT_IMAGE, false).expect("Failed to get graph resource"));
        command_buffer.bind_texture_view(BindingFrequency::PerDraw, 1, graph_resources.get_texture_srv(HISTORY_BUFFER_NAME, true).expect("Failed to get graph resource"));
        command_buffer.bind_storage_texture(BindingFrequency::PerDraw, 2, graph_resources.get_texture_uav(HISTORY_BUFFER_NAME, false).expect("Failed to get graph resource"));
        command_buffer.bind_texture_view(BindingFrequency::PerDraw, 3, graph_resources.get_texture_srv(OUTPUT_MOTION, false).expect("Failed to get graph resource"));
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
      Arc::new(move |command_buffer_a, graph_resources, _frame_counter| {
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

pub(crate) fn scaled_halton_point(width: u32, height: u32, index: u32) -> Vec2 {
  let width_frac = 1.0f32 / width as f32;
  let height_frac = 1.0f32 / height as f32;
  let mut halton_point = halton_point(index);
  halton_point.x *= width_frac;
  halton_point.y *= height_frac;
  halton_point
}

pub(crate) fn halton_point(index: u32) -> Vec2 {
  Vec2::new(
    halton_sequence(index, 2) * 2f32 - 1f32, halton_sequence(index, 3) * 2f32 - 1f32
  )
}

pub(crate) fn halton_sequence(mut index: u32, base: u32) -> f32 {
  let mut f = 1.0f32;
  let mut r = 0.0f32;

  while index > 0 {
    f = f / (base as f32);
    r += f * (index as f32 % (base as f32));
    index = (index as f32 / (base as f32)).floor() as u32;
  }

  return r;
}
