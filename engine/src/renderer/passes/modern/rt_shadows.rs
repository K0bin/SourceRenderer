use std::{sync::Arc, path::Path, io::Read};

use sourcerenderer_core::{graphics::{Backend, Device, TextureInfo, Format, SampleCount, TextureUsage, TextureStorageViewInfo, ShaderType, RayTracingPipelineInfo, CommandBuffer, BindingFrequency, PipelineBinding, TextureStorageView, Texture, BarrierSync, TextureLayout, BarrierAccess, TextureSamplingViewInfo, AddressMode, Filter, SamplerInfo, BufferUsage, WHOLE_BUFFER}, Vec2UI, Platform, platform::io::IO};

use crate::renderer::{passes::prepass::Prepass, renderer_resources::{HistoryResourceEntry, RendererResources}};

pub struct RTShadowPass<B: Backend> {
  pipeline: Arc<B::RayTracingPipeline>,
  sampler: Arc<B::Sampler>
}

impl<B: Backend> RTShadowPass<B> {
  pub const SHADOWS_TEXTURE_NAME: &'static str = "RTShadow";

  pub fn new<P: Platform>(device: &Arc<B::Device>, resolution: Vec2UI, resources: &mut RendererResources<B>) -> Self {
    resources.create_texture(Self::SHADOWS_TEXTURE_NAME, &TextureInfo {
      format: Format::RGBA8,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
    }, false);

    let sampler = device.create_sampler(&SamplerInfo {
      mag_filter: Filter::Linear,
      min_filter: Filter::Linear,
      mip_filter: Filter::Linear,
      address_mode_u: AddressMode::ClampToEdge,
      address_mode_v: AddressMode::ClampToEdge,
      address_mode_w: AddressMode::ClampToEdge,
      mip_bias: 0.0f32,
      max_anisotropy: 0.0f32,
      compare_op: None,
      min_lod: 0.0f32,
      max_lod: None,
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
      closest_hit_shaders: &[&closest_hit_shader],
      miss_shaders: &[&miss_shader],
    });

    Self {
      pipeline,
      sampler
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, frame: u64, acceleration_structure: &Arc<B::AccelerationStructure>, camera_buffer: &Arc<B::Buffer>, resources: &RendererResources<B>, blue_noise: &Arc<B::TextureSamplingView>, blue_noise_sampler: &Arc<B::Sampler>) {
    let texture_uav = resources.access_uav(
      cmd_buffer,
      Self::SHADOWS_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER | BarrierSync::RAY_TRACING,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureStorageViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let depth = resources.access_srv(
      cmd_buffer,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::RAY_TRACING | BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureSamplingViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.set_pipeline(PipelineBinding::RayTracing(&self.pipeline));
    cmd_buffer.bind_acceleration_structure(BindingFrequency::PerFrame, 0, acceleration_structure);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerFrame, 1, &*texture_uav);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 2, camera_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerFrame, 5, &*depth, &self.sampler);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerFrame, 6, blue_noise, blue_noise_sampler);
    let info = texture_uav.texture().info();

    #[derive(Clone)]
    struct FrameData {
      frame: u32,
      directional_light_count: u32,
    }
    let frame_data = cmd_buffer.upload_dynamic_data(&[FrameData {
      frame: frame as u32,
      directional_light_count: 0
    }], BufferUsage::CONSTANT);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 3, &frame_data, 0, WHOLE_BUFFER);

    cmd_buffer.flush_barriers();
    cmd_buffer.finish_binding();
    cmd_buffer.trace_ray(info.width, info.height, 1);
  }
}
