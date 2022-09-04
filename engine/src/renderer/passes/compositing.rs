use sourcerenderer_core::graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, PipelineBinding, Texture, TextureInfo, TextureStorageView, TextureViewInfo, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, BufferUsage, WHOLE_BUFFER, TextureDimension};
use sourcerenderer_core::{Platform, Vec2UI};

use crate::renderer::shader_manager::{PipelineHandle, ShaderManager};
use crate::renderer::{renderer_resources::{HistoryResourceEntry, RendererResources}};

use super::ssr::SsrPass;

const USE_CAS: bool = true;

pub struct CompositingPass {
  pipeline: PipelineHandle
}

impl CompositingPass {
  pub const COMPOSITION_TEXTURE_NAME: &'static str = "Composition";

  pub fn new<P: Platform>(resolution: Vec2UI, resources: &mut RendererResources<P::GraphicsBackend>, shader_manager: &mut ShaderManager<P>) -> Self {
    let pipeline = shader_manager.request_compute_pipeline("shaders/compositing.comp.spv");

    resources.create_texture(Self::COMPOSITION_TEXTURE_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA8UNorm,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: sourcerenderer_core::graphics::SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
      supports_srgb: false,
    }, false);

    Self {
      pipeline
    }
  }

  pub fn execute<P: Platform>(
    &mut self,
    cmd_buffer: &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer,
    resources: &RendererResources<P::GraphicsBackend>,
    input_name: &str,
    shader_manager: &ShaderManager<P>
  ) {
    let input_image = resources.access_sampling_view(
      cmd_buffer,
      input_name,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let ssr = resources.access_sampling_view(
      cmd_buffer,
      SsrPass::SSR_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let output = resources.access_storage_view(
      cmd_buffer,
      Self::COMPOSITION_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.begin_label("Compositing pass");

    let pipeline = shader_manager.get_compute_pipeline(self.pipeline);
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));

    #[repr(C)]
    #[derive(Debug, Clone)]
    struct Setup {
      gamma: f32,
      exposure: f32,
    }
    let setup_ubo = cmd_buffer.upload_dynamic_data(&[Setup {
      gamma: 2.2f32,
      exposure: 0.01f32
    }], BufferUsage::CONSTANT);

    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &output);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 1, &input_image, resources.linear_sampler());
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 2, &ssr, resources.linear_sampler());
    cmd_buffer.bind_uniform_buffer(BindingFrequency::VeryFrequent, 3, &setup_ubo, 0, WHOLE_BUFFER);
    cmd_buffer.finish_binding();

    let info = output.texture().info();
    cmd_buffer.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
    cmd_buffer.end_label();
  }
}
