use std::{sync::Arc, path::Path, io::Read};

use sourcerenderer_core::{graphics::{Backend, Device, TextureInfo, Format, SampleCount, TextureUsage, TextureUnorderedAccessViewInfo, ShaderType, RayTracingPipelineInfo, CommandBuffer, BindingFrequency, PipelineBinding, TextureUnorderedAccessView, Texture, Barrier, BarrierSync, TextureLayout, BarrierAccess, TextureShaderResourceViewInfo, AddressMode, Filter, SamplerInfo, BufferUsage}, Vec2UI, Platform, platform::io::IO};

pub struct RTShadowPass<B: Backend> {
  texture_view: Arc<B::TextureUnorderedAccessView>,
  srv: Arc<B::TextureShaderResourceView>,
  pipeline: Arc<B::RayTracingPipeline>,
  sampler: Arc<B::Sampler>,
}

impl<B: Backend> RTShadowPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, resolution: Vec2UI, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let texture = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
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

    let srv = device.create_shader_resource_view(&texture, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });

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
      max_lod: 0.0f32,
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

    init_cmd_buffer.barrier(&[Barrier::TextureBarrier {
      old_sync: BarrierSync::empty(),
      new_sync: BarrierSync::FRAGMENT_SHADER,
      old_layout: TextureLayout::Undefined,
      new_layout: TextureLayout::Sampled,
      old_access: BarrierAccess::empty(),
      new_access: BarrierAccess::SHADER_READ,
      texture: view.texture(),
    }]);

    Self {
      texture_view: view,
      pipeline,
      srv,
      sampler
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, frame: u64, acceleration_structure: &Arc<B::AccelerationStructure>, camera_buffer: &Arc<B::Buffer>, depth: &Arc<B::TextureShaderResourceView>) {
    cmd_buffer.set_pipeline(PipelineBinding::RayTracing(&self.pipeline));
    cmd_buffer.bind_acceleration_structure(BindingFrequency::PerFrame, 0, acceleration_structure);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerFrame, 1, &self.texture_view);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 2, camera_buffer);
    cmd_buffer.bind_texture_view(BindingFrequency::PerFrame, 5, depth, &self.sampler);
    let info = self.texture_view.texture().get_info();
    cmd_buffer.barrier(&[Barrier::TextureBarrier {
      old_sync: BarrierSync::FRAGMENT_SHADER | BarrierSync::ACCELERATION_STRUCTURE_BUILD | BarrierSync::COMPUTE_SHADER,
      new_sync: BarrierSync::RAY_TRACING,
      old_layout: TextureLayout::Sampled,
      new_layout: TextureLayout::Storage,
      old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE | BarrierAccess::SHADER_WRITE,
      new_access: BarrierAccess::SHADER_WRITE | BarrierAccess::ACCELERATION_STRUCTURE_READ,
      texture: self.texture_view.texture(),
    }]);

    #[derive(Clone)]
    struct FrameData {
      frame: u32,
      directional_light_count: u32,
    }
    let frame_data = cmd_buffer.upload_dynamic_data(&[FrameData {
      frame: frame as u32,
      directional_light_count: 0
    }], BufferUsage::CONSTANT);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 3, &frame_data);

    cmd_buffer.flush_barriers();
    cmd_buffer.finish_binding();
    cmd_buffer.trace_ray(info.width, info.height, 1);
    cmd_buffer.barrier(&[Barrier::TextureBarrier {
      old_sync: BarrierSync::RAY_TRACING | BarrierSync::COMPUTE_SHADER,
      new_sync: BarrierSync::FRAGMENT_SHADER,
      old_layout: TextureLayout::Storage,
      new_layout: TextureLayout::Sampled,
      old_access: BarrierAccess::SHADER_WRITE,
      new_access: BarrierAccess::SHADER_READ,
      texture: self.texture_view.texture(),
    }]);
  }

  pub fn shadows_srv(&self) -> &Arc<B::TextureShaderResourceView> {
    &self.srv
  }
}
