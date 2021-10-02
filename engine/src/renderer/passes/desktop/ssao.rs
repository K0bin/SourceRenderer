use std::{io::Read, path::Path, sync::Arc};

use sourcerenderer_core::{Platform, Vec2UI, Vec4, graphics::{AddressMode, Backend as GraphicsBackend, Barrier, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, Filter, Format, MemoryUsage, PipelineBinding, SampleCount, SamplerInfo, ShaderType, Texture, TextureInfo, TextureShaderResourceView, TextureShaderResourceViewInfo, TextureUnorderedAccessViewInfo, TextureUsage}, platform::io::IO};

use rand::random;

pub struct SsaoPass<B: GraphicsBackend> {
  ssao_texture: Arc<B::Texture>,
  ssao_uav: Arc<B::TextureUnorderedAccessView>,
  ssao_srv: Arc<B::TextureShaderResourceView>,
  pipeline: Arc<B::ComputePipeline>,
  kernel: Arc<B::Buffer>,
  noise: Arc<B::TextureShaderResourceView>,
  nearest_sampler: Arc<B::Sampler>,
  noise_sampler: Arc<B::Sampler>,
  blur_pipeline: Arc<B::ComputePipeline>,
  blurred_texture: Arc<B::Texture>,
  blurred_uav: Arc<B::TextureUnorderedAccessView>,
  blurred_srv: Arc<B::TextureShaderResourceView>,
  blurred_texture_b: Arc<B::Texture>,
  blurred_uav_b: Arc<B::TextureUnorderedAccessView>,
  blurred_srv_b: Arc<B::TextureShaderResourceView>,
  blur_sampler: Arc<B::Sampler>
}

fn lerp(a: f32, b: f32, f: f32) -> f32 {
  a + f * (b - a)
}

impl<B: GraphicsBackend> SsaoPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, resolution: Vec2UI, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let ssao_texture = device.create_texture(&TextureInfo {
      format: Format::R16Float,
      width: resolution.x / 2,
      height: resolution.y / 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE | TextureUsage::COMPUTE_SHADER_SAMPLED,
    }, Some("SSAO"));
    let blurred_texture = device.create_texture(&TextureInfo {
      format: Format::R16Float,
      width: resolution.x / 2,
      height: resolution.y / 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE | TextureUsage::FRAGMENT_SHADER_SAMPLED,
    }, Some("SSAOBlurred"));
    let blurred_texture_b = device.create_texture(&TextureInfo {
      format: Format::R16Float,
      width: resolution.x / 2,
      height: resolution.y / 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE | TextureUsage::FRAGMENT_SHADER_SAMPLED,
    }, Some("SSAOBlurred_b"));

    let uav_info = TextureUnorderedAccessViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    };
    let ssao_uav = device.create_unordered_access_view(&ssao_texture, &uav_info);
    let blurred_uav = device.create_unordered_access_view(&blurred_texture, &uav_info);
    let blurred_uav_b = device.create_unordered_access_view(&blurred_texture_b, &uav_info);
    let srv_info = TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    };
    let ssao_srv = device.create_shader_resource_view(&ssao_texture, &srv_info);
    let blurred_srv = device.create_shader_resource_view(&blurred_texture, &srv_info);
    let blurred_srv_b = device.create_shader_resource_view(&blurred_texture_b, &srv_info);

    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("ssao.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("ssao.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&shader);

    init_cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::UNINITIALIZED,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: &ssao_texture,
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::UNINITIALIZED,
        new_primary_usage: TextureUsage::FRAGMENT_SHADER_SAMPLED,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: &blurred_texture,
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::UNINITIALIZED,
        new_primary_usage: TextureUsage::FRAGMENT_SHADER_SAMPLED,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: &blurred_texture_b,
      }
    ]);

    // TODO: Clear history texture

    let kernel = Self::create_hemisphere(device, 64);
    let noise = Self::create_noise(device, 4);

    let blur_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("ssao_blur.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("ssao_blur.comp.spv"))
    };  
    let blur_pipeline = device.create_compute_pipeline(&blur_shader);

    let noise_sampler = device.create_sampler(&SamplerInfo {
      mag_filter: Filter::Nearest,
      min_filter: Filter::Nearest,
      mip_filter: Filter::Nearest,
      address_mode_u: AddressMode::Repeat,
      address_mode_v: AddressMode::Repeat,
      address_mode_w: AddressMode::ClampToEdge,
      mip_bias: 0.0f32,
      max_anisotropy: 0.0f32,
      compare_op: None,
      min_lod: 0.0f32,
      max_lod: 1.0f32,
    });
    let nearest_sampler = device.create_sampler(&SamplerInfo {
      mag_filter: Filter::Nearest,
      min_filter: Filter::Nearest,
      mip_filter: Filter::Nearest,
      address_mode_u: AddressMode::ClampToEdge,
      address_mode_v: AddressMode::ClampToEdge,
      address_mode_w: AddressMode::ClampToEdge,
      mip_bias: 0.0f32,
      max_anisotropy: 0.0f32,
      compare_op: None,
      min_lod: 0.0f32,
      max_lod: 1.0f32,
    });
    let blur_sampler = device.create_sampler(&SamplerInfo {
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
      max_lod: 1.0f32,
    });

    Self {
      ssao_texture,
      ssao_uav,
      pipeline,
      kernel,
      noise,
      noise_sampler,
      nearest_sampler,
      blurred_texture,
      blurred_uav,
      blurred_srv_b,
      blurred_texture_b,
      blurred_uav_b,
      blur_pipeline,
      ssao_srv,
      blur_sampler,
      blurred_srv
    }
  }

  fn create_hemisphere(device: &Arc<B::Device>, samples: u32) -> Arc<B::Buffer> {
    let mut ssao_kernel = Vec::<Vec4>::with_capacity(samples as usize);
    const BIAS: f32 = 0.15f32;
    for i in 0..samples {
      let mut sample = Vec4::new(
        (random::<f32>() - BIAS) * 2.0f32 - (1.0f32 - BIAS),
        (random::<f32>() - BIAS) * 2.0f32 - (1.0f32 - BIAS),
        random::<f32>(),
        0.0f32
      );
      sample.normalize_mut();
      sample *= random::<f32>();
      let mut scale = (i as f32) / (samples as f32);
      scale = lerp(0.1f32, 1.0f32, scale * scale);
      sample *= scale;
      ssao_kernel.push(sample);
    }

    let buffer = device.create_buffer(&BufferInfo {
      size: std::mem::size_of_val(&ssao_kernel[..]),
      usage: BufferUsage::COPY_DST | BufferUsage::COMPUTE_SHADER_CONSTANT,
    }, MemoryUsage::GpuOnly, Some("SSAOKernel"));

    let temp_buffer = device.upload_data(&ssao_kernel[..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    device.init_buffer(&temp_buffer, &buffer);
    buffer
  }

  fn create_noise(device: &Arc<B::Device>, size: u32) -> Arc<B::TextureShaderResourceView> {
    let mut ssao_noise = Vec::<Vec4>::new();
    for _ in 0.. size * size {
      let noise = Vec4::new(
        random::<f32>() * 2.0f32 - 1.0f32,
        random::<f32>()* 2.0f32 - 1.0f32,
        0.0f32,
        0.0f32
      );
      ssao_noise.push(noise);
    }

    let texture = device.create_texture(&TextureInfo {
      format: Format::RGBA32Float,
      width: size,
      height: size,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::COPY_DST | TextureUsage::COMPUTE_SHADER_SAMPLED,
    }, Some("SSAONoise"));
    let buffer = device.upload_data(&ssao_noise[..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    device.init_texture(&texture, &buffer, 0, 0);
    let srv = device.create_shader_resource_view(&texture, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });

    srv
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, normals: &Arc<B::TextureShaderResourceView>, depth: &Arc<B::TextureShaderResourceView>, camera: &Arc<B::Buffer>, motion_srv: &Arc<B::TextureShaderResourceView>) {
    cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: &self.ssao_texture,
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::DEPTH_WRITE,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::DEPTH_WRITE,
        new_usages: TextureUsage::DEPTH_READ | TextureUsage::DEPTH_WRITE | TextureUsage::COMPUTE_SHADER_SAMPLED,
        texture: depth.texture(),
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::RENDER_TARGET,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::RENDER_TARGET,
        new_usages: TextureUsage::COMPUTE_SHADER_SAMPLED,
        texture: normals.texture(),
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::RENDER_TARGET,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::RENDER_TARGET,
        new_usages: TextureUsage::COMPUTE_SHADER_SAMPLED,
        texture: motion_srv.texture(),
      },
    ]);
    cmd_buffer.flush_barriers();
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, &self.kernel);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 1, &self.noise, &self.noise_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 2, depth, &self.nearest_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 3, normals, &self.nearest_sampler);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 4, camera);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 5, &self.ssao_uav);
    cmd_buffer.finish_binding();
    let ssao_info = self.ssao_srv.texture().get_info();
    cmd_buffer.dispatch((ssao_info.width + 15) / 16, (ssao_info.height + 15) / 16, ssao_info.depth);
    cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_usages: TextureUsage::COMPUTE_SHADER_SAMPLED,
        texture: &self.ssao_texture,
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::FRAGMENT_SHADER_SAMPLED,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: &self.blurred_texture,
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::FRAGMENT_SHADER_SAMPLED,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: &self.blurred_texture_b,
      },
    ]);
    cmd_buffer.flush_barriers();
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.blur_pipeline));
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 0, &self.blurred_uav);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 1, &self.ssao_srv, &self.blur_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 2, &self.blurred_srv_b, &self.blur_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 3, motion_srv, &self.nearest_sampler);
    cmd_buffer.finish_binding();
    let blur_info = self.blurred_texture.get_info();
    cmd_buffer.dispatch((blur_info.width + 15) / 16, (blur_info.height + 15) / 16, blur_info.depth);
  }

  pub fn swap_history_resources(&mut self) {
    std::mem::swap(&mut self.blurred_texture, &mut self.blurred_texture_b);
    std::mem::swap(&mut self.blurred_srv, &mut self.blurred_srv_b);
    std::mem::swap(&mut self.blurred_uav, &mut self.blurred_uav_b);
  }

  pub fn ssao_texture(&self) -> &Arc<B::Texture> {
    &self.blurred_texture
  }

  pub fn ssao_srv(&self) -> &Arc<B::TextureShaderResourceView> {
    &self.blurred_srv
  }
}