use std::{io::Read, path::Path, sync::Arc};

use sourcerenderer_core::{Platform, Vec2UI, Vec4, graphics::{AddressMode, Backend as GraphicsBackend, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, Filter, Format, MemoryUsage, PipelineBinding, SampleCount, SamplerInfo, ShaderType, Texture, TextureInfo, TextureShaderResourceViewInfo, TextureUnorderedAccessViewInfo, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, TextureUnorderedAccessView}, platform::io::IO, atomic_refcell::AtomicRef};

use rand::random;

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, drawable::View};

use super::prepass::Prepass;

pub struct SsaoPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>,
  kernel: Arc<B::Buffer>,
  noise: Arc<B::TextureShaderResourceView>,
  nearest_sampler: Arc<B::Sampler>,
  noise_sampler: Arc<B::Sampler>,
  blur_pipeline: Arc<B::ComputePipeline>,
  blur_sampler: Arc<B::Sampler>,
  linear_sampler: Arc<B::Sampler>
}

fn lerp(a: f32, b: f32, f: f32) -> f32 {
  a + f * (b - a)
}

impl<B: GraphicsBackend> SsaoPass<B> {
  const SSAO_INTERNAL_TEXTURE_NAME: &'static str = "SSAO";
  pub const SSAO_TEXTURE_NAME: &'static str = "SSAOBlurred";

  pub fn new<P: Platform>(device: &Arc<B::Device>, resolution: Vec2UI, resources: &mut RendererResources<B>) -> Self {
    resources.create_texture(Self::SSAO_INTERNAL_TEXTURE_NAME, &TextureInfo {
      format: Format::R16Float,
      width: resolution.x / 2,
      height: resolution.y / 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
    }, false);

    resources.create_texture(Self::SSAO_TEXTURE_NAME, &TextureInfo {
      format: Format::R16Float,
      width: resolution.x / 2,
      height: resolution.y / 2,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
    }, true);

    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("ssao.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("ssao.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&shader);

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
      max_lod: None,
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
      max_lod: None,
    });
    let linear = device.create_sampler(&SamplerInfo {
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
      max_lod: None,
    });

    Self {
      pipeline,
      kernel,
      noise,
      noise_sampler,
      nearest_sampler,
      blur_pipeline,
      blur_sampler,
      linear_sampler: linear
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
      usage: BufferUsage::COPY_DST | BufferUsage::CONSTANT,
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
      usage: TextureUsage::COPY_DST | TextureUsage::SAMPLED,
    }, Some("SSAONoise"));
    let buffer = device.upload_data(&ssao_noise[..], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    device.init_texture(&texture, &buffer, 0, 0);
    device.create_shader_resource_view(&texture, &TextureShaderResourceViewInfo::default(), Some("SSAONoiseView"))
  }

  pub fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    camera: &Arc<B::Buffer>,
    view_ref: &AtomicRef<View>,
    resources: &RendererResources<B>
  ){
    let ssao_uav = resources.access_uav(
      cmd_buffer,
      Self::SSAO_INTERNAL_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureUnorderedAccessViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let depth_srv = resources.access_srv(
      cmd_buffer,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureShaderResourceViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let normals_srv = resources.access_srv(
      cmd_buffer,
      Prepass::<B>::NORMALS_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureShaderResourceViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let motion_srv = resources.access_srv(
      cmd_buffer,
      Prepass::<B>::MOTION_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureShaderResourceViewInfo::default(),
      HistoryResourceEntry::Current
    );

    #[repr(C)]
    #[derive(Clone)]
    struct SSAOSetup {
      z_near: f32,
      z_far: f32
    }

    cmd_buffer.begin_label("SSAO pass");
    cmd_buffer.flush_barriers();
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, &self.kernel);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 1, &self.noise, &self.noise_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 2, &*depth_srv, &self.linear_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 3, &*normals_srv, &self.linear_sampler);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 4, camera);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 5, &*ssao_uav);
    let setup_ubo = cmd_buffer.upload_dynamic_data(&[SSAOSetup {
      z_near: view_ref.near_plane,
      z_far: view_ref.far_plane,
    }], BufferUsage::CONSTANT);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 6, &setup_ubo);
    cmd_buffer.finish_binding();
    let ssao_info = ssao_uav.texture().get_info();
    cmd_buffer.dispatch((ssao_info.width + 7) / 8, (ssao_info.height + 7) / 8, ssao_info.depth);

    std::mem::drop(ssao_uav);
    let ssao_srv = resources.access_srv(
      cmd_buffer,
      Self::SSAO_INTERNAL_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureShaderResourceViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let blurred_uav = resources.access_uav(
      cmd_buffer,
      Self::SSAO_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      false,
      &TextureUnorderedAccessViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let blurred_srv_b = resources.access_srv(
      cmd_buffer,
      Self::SSAO_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureShaderResourceViewInfo::default(),
      HistoryResourceEntry::Past
    );

    cmd_buffer.flush_barriers();
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.blur_pipeline));
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 0, &*blurred_uav);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 1, &*ssao_srv, &self.blur_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 2, &*blurred_srv_b, &self.blur_sampler);
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 3, &*motion_srv, &self.nearest_sampler);
    cmd_buffer.finish_binding();
    let blur_info = blurred_uav.texture().get_info();
    cmd_buffer.dispatch((blur_info.width + 7) / 8, (blur_info.height + 7) / 8, blur_info.depth);
    cmd_buffer.end_label();
  }
}
