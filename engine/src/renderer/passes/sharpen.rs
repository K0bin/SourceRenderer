use sourcerenderer_core::graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, PipelineBinding, ShaderType, Swapchain, Texture, TextureInfo, TextureStorageView, TextureViewInfo, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, BufferUsage, WHOLE_BUFFER};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::{renderer_resources::{HistoryResourceEntry, RendererResources}};

use super::taa::TAAPass;

const USE_CAS: bool = true;

pub struct SharpenPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>
}

impl<B: GraphicsBackend> SharpenPass<B> {
  pub const SHAPENED_TEXTURE_NAME: &'static str = "Sharpened";

  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, resources: &mut RendererResources<B>) -> Self {
    let sharpen_compute_shader = if !USE_CAS {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("sharpen.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("sharpen.comp.spv"))
    } else {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("cas.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("cas.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&sharpen_compute_shader, Some("Sharpen"));

    resources.create_texture(Self::SHAPENED_TEXTURE_NAME, &TextureInfo {
      format: Format::RGBA8,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: sourcerenderer_core::graphics::SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::COPY_SRC,
    }, false);

    Self {
      pipeline
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, resources: &RendererResources<B>) {
    let input_image_uav = resources.access_uav(
      cmd_buffer,
      TAAPass::<B>::TAA_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_READ,
      TextureLayout::Storage,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let sharpen_uav = resources.access_uav(
      cmd_buffer,
      Self::SHAPENED_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.begin_label("Sharpening pass");

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    let sharpen_setup_ubo = cmd_buffer.upload_dynamic_data(&[0.3f32], BufferUsage::CONSTANT);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 2, &sharpen_setup_ubo, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 0, &*input_image_uav);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 1, &*sharpen_uav);
    cmd_buffer.finish_binding();

    let info = sharpen_uav.texture().info();
    cmd_buffer.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
    cmd_buffer.end_label();
  }
}
