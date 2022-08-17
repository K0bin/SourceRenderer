use sourcerenderer_core::graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, PipelineBinding, ShaderType, Swapchain, Texture, TextureInfo, TextureStorageView, TextureViewInfo, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, BufferUsage, WHOLE_BUFFER, TextureDimension};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::passes::conservative::geometry::GeometryPass;
use crate::renderer::{renderer_resources::{HistoryResourceEntry, RendererResources}};

use super::ssr::SsrPass;

const USE_CAS: bool = true;

pub struct CompositingPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>
}

impl<B: GraphicsBackend> CompositingPass<B> {
  pub const COMPOSITION_TEXTURE_NAME: &'static str = "Composition";

  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, resources: &mut RendererResources<B>) -> Self {
    let sharpen_compute_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("compositing.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("compositing.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&sharpen_compute_shader, Some("Compositing"));

    resources.create_texture(Self::COMPOSITION_TEXTURE_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA8UNorm,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: sourcerenderer_core::graphics::SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
    }, false);

    Self {
      pipeline
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, resources: &RendererResources<B>) {
    let input_image = resources.access_sampling_view(
      cmd_buffer,
      GeometryPass::<B>::GEOMETRY_PASS_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let ssr = resources.access_sampling_view(
      cmd_buffer,
      SsrPass::<B>::SSR_TEXTURE_NAME,
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

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));

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
