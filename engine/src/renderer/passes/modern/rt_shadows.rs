use std::{sync::Arc, path::Path, io::Read};

use sourcerenderer_core::{graphics::{Backend, Device, TextureInfo, Format, SampleCount, TextureUsage, TextureViewInfo, ShaderType, RayTracingPipelineInfo, CommandBuffer, BindingFrequency, PipelineBinding, TextureStorageView, Texture, BarrierSync, TextureLayout, BarrierAccess, TextureDimension}, Vec2UI, Platform, platform::io::IO};

use crate::renderer::{passes::prepass::Prepass, renderer_resources::{HistoryResourceEntry, RendererResources}};

pub struct RTShadowPass<B: Backend> {
  pipeline: Arc<B::RayTracingPipeline>,
}

impl<B: Backend> RTShadowPass<B> {
  pub const SHADOWS_TEXTURE_NAME: &'static str = "RTShadow";

  pub fn new<P: Platform>(device: &Arc<B::Device>, resolution: Vec2UI, resources: &mut RendererResources<B>) -> Self {
    resources.create_texture(Self::SHADOWS_TEXTURE_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA8UNorm,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
      supports_srgb: false,
    }, false);

    let ray_gen_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("shadows.rgen.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::RayGen, &bytes, Some("shadows.rgen.spv"))
    };

    let closest_hit_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("shadows.rchit.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::RayClosestHit, &bytes, Some("shadows.rchit.spv"))
    };

    let miss_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("shadows.rmiss.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::RayMiss, &bytes, Some("shadows.rmiss.spv"))
    };

    let pipeline = device.create_raytracing_pipeline(&RayTracingPipelineInfo::<B> {
      ray_gen_shader: &ray_gen_shader,
      closest_hit_shaders: &[&closest_hit_shader],
      miss_shaders: &[&miss_shader],
    });

    Self {
      pipeline,
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, acceleration_structure: &Arc<B::AccelerationStructure>, resources: &RendererResources<B>, blue_noise: &Arc<B::TextureSamplingView>, blue_noise_sampler: &Arc<B::Sampler>) {
    let texture_uav = resources.access_storage_view(
      cmd_buffer,
      Self::SHADOWS_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER | BarrierSync::RAY_TRACING,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let depth = resources.access_sampling_view(
      cmd_buffer,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::RAY_TRACING | BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.set_pipeline(PipelineBinding::RayTracing(&self.pipeline));
    cmd_buffer.bind_acceleration_structure(BindingFrequency::Frequent, 0, acceleration_structure);
    cmd_buffer.bind_storage_texture(BindingFrequency::Frequent, 1, &*texture_uav);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 2, &*depth, resources.linear_sampler());
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 3, blue_noise, blue_noise_sampler);
    let info = texture_uav.texture().info();

    cmd_buffer.flush_barriers();
    cmd_buffer.finish_binding();
    cmd_buffer.trace_ray(info.width, info.height, 1);
  }
}
