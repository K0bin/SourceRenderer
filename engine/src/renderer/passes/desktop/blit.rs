use std::sync::Arc;

use sourcerenderer_core::{Platform, graphics::{BACK_BUFFER_ATTACHMENT_NAME, Backend as GraphicsBackend, InputUsage, Output, PassInfo, PassInput, PassType, PipelineStage, RenderPassCallbacks, TextureShaderResourceView, TextureUnorderedAccessView, CommandBuffer}};

use crate::renderer::passes::desktop::sharpen::SHARPEN_OUTPUT_NAME;

const BLIT_PASS_NAME: &str = "blit";

pub(crate) fn build_blit_pass_template<B: GraphicsBackend>() -> PassInfo {
  sourcerenderer_core::graphics::PassInfo {
    name: BLIT_PASS_NAME.to_string(),
    pass_type: PassType::Copy {
      inputs: vec![
        PassInput {
          name: SHARPEN_OUTPUT_NAME.to_string(),
          stage: PipelineStage::ComputeShader,
          usage: InputUsage::Copy,
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

pub(crate) fn build_blit_pass<P: Platform>() -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  (BLIT_PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources, _frame_counter| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        let frame_view = graph_resources.get_texture_srv(SHARPEN_OUTPUT_NAME, false).expect("Failed to get graph resource");
        let frame_texture = frame_view.texture();
        let backbuffer_view = graph_resources.get_texture_uav(BACK_BUFFER_ATTACHMENT_NAME, false).expect("Failed to get graph resource");
        let backbuffer_texture = backbuffer_view.texture();
        command_buffer.blit(frame_texture, 0, 0, backbuffer_texture, 0, 0);
      })
    ]
  ))
}