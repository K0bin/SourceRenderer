use std::{sync::Arc, path::Path, io::Read};

use sourcerenderer_core::{graphics::{Backend, Device, TextureInfo, Format, SampleCount, TextureUsage, TextureUnorderedAccessViewInfo, ShaderType, RayTracingPipelineInfo}, Vec2UI, Platform, platform::io::IO};

pub struct RTShadowPass<B: Backend> {
  texture_view: Arc<B::TextureUnorderedAccessView>,
  pipeline: Arc<B::RayTracingPipeline>,
}

impl<B: Backend> RTShadowPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, resolution: Vec2UI, init_cmd_buffer: &B::CommandBuffer) -> Self {
    let texture = device.create_texture(&TextureInfo {
      format: Format::R16,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
    }, Some("RTShadows"));

    let view = device.create_unordered_access_view(&texture, &TextureUnorderedAccessViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });

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
      closest_hit_shader: &closest_hit_shader,
      miss_shader: &miss_shader,
    });

    Self {
      texture_view: view,
      pipeline
    }
  }
}