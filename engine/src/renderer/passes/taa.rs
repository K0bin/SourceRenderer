use sourcerenderer_core::{Vec2, graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, PipelineBinding, SampleCount, ShaderType, Swapchain, TextureInfo, TextureViewInfo, TextureUsage, TextureLayout, BarrierAccess, BarrierSync, TextureStorageView, Texture}};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::renderer_resources::{RendererResources, HistoryResourceEntry};

use super::prepass::Prepass;

pub(crate) fn scaled_halton_point(width: u32, height: u32, index: u32) -> Vec2 {
  let width_frac = 1.0f32 / (width as f32 * 0.5f32);
  let height_frac = 1.0f32 / (height as f32 * 0.5f32);
  let mut halton_point = halton_point(index);
  halton_point.x *= width_frac;
  halton_point.y *= height_frac;
  halton_point
}

pub(crate) fn halton_point(index: u32) -> Vec2 {
  Vec2::new(
    halton_sequence(index, 2) - 0.5f32, halton_sequence(index, 3) - 0.5f32
  )
}

pub(crate) fn halton_sequence(mut index: u32, base: u32) -> f32 {
  let mut f = 1.0f32;
  let mut r = 0.0f32;

  while index > 0 {
    f /= base as f32;
    r += f * (index as f32 % (base as f32));
    index = (index as f32 / (base as f32)).floor() as u32;
  }

  r
}

pub struct TAAPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>,
}

impl<B: GraphicsBackend> TAAPass<B> {
  pub const TAA_TEXTURE_NAME: &'static str = "TAAOuput";

  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, resources: &mut RendererResources<B>) -> Self {
    let taa_compute_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("taa.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("taa.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&taa_compute_shader, Some("TAA"));

    let texture_info = TextureInfo {
      format: Format::RGBA8,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
    };
    resources.create_texture(Self::TAA_TEXTURE_NAME, &texture_info, true);

    // TODO: Clear history texture

    Self {
      pipeline,
    }
  }

  pub fn execute(
    &mut self,
    cmd_buf: &mut B::CommandBuffer,
    input_name: &str,
    resources: &RendererResources<B>
  ) {
    cmd_buf.begin_label("TAA pass");

    let output_srv = resources.access_sampling_view(
      cmd_buf,
      input_name,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let taa_uav = resources.access_storage_view(
      cmd_buf,
      Self::TAA_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let taa_history_srv = resources.access_sampling_view(
      cmd_buf,
      Self::TAA_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Past
    );

    let motion_srv = resources.access_sampling_view(
      cmd_buf,
      Prepass::<B>::MOTION_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let depth_srv = resources.access_sampling_view(
      cmd_buf,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buf.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 0, &*output_srv, resources.linear_sampler());
    cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 1, &*taa_history_srv, resources.linear_sampler());
    cmd_buf.bind_storage_texture(BindingFrequency::PerDraw, 2, &*taa_uav);
    cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 3, &*motion_srv, resources.nearest_sampler());
    cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 4, &*depth_srv, resources.linear_sampler());
    cmd_buf.finish_binding();

    let info = taa_uav.texture().info();
    cmd_buf.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
    cmd_buf.end_label();
  }
}
