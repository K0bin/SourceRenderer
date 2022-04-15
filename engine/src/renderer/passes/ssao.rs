use std::{io::Read, path::Path, sync::Arc};

use sourcerenderer_core::{Platform, Vec2UI, Vec4, graphics::{AddressMode, Backend as GraphicsBackend, BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, Device, Filter, Format, MemoryUsage, PipelineBinding, SampleCount, SamplerInfo, ShaderType, Texture, TextureInfo, TextureSamplingViewInfo, TextureStorageViewInfo, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, TextureStorageView, WHOLE_BUFFER}, platform::io::IO, atomic_refcell::AtomicRef};

use rand::random;

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}};

use super::prepass::Prepass;

pub struct SsaoPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>,
  kernel: Arc<B::Buffer>,
  nearest_sampler: Arc<B::Sampler>,
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
    let pipeline = device.create_compute_pipeline(&shader, Some("SSAO"));

    // TODO: Clear history texture

    let kernel = Self::create_hemisphere(device, 64);

    let blur_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("ssao_blur.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("ssao_blur.comp.spv"))
    };
    let blur_pipeline = device.create_compute_pipeline(&blur_shader, Some("SSAOBlur"));

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
        -random::<f32>(),
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
    device.init_buffer(&temp_buffer, &buffer, 0, 0, WHOLE_BUFFER);
    buffer
  }

  pub fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    camera: &Arc<B::Buffer>,
    blue_noise_view: &Arc<B::TextureSamplingView>,
    blue_noise_sampler: &Arc<B::Sampler>,
    resources: &RendererResources<B>
  ){
    let ssao_uav = resources.access_uav(
      cmd_buffer,
      Self::SSAO_INTERNAL_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureStorageViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let depth_srv = resources.access_srv(
      cmd_buffer,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureSamplingViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let motion_srv = resources.access_srv(
      cmd_buffer,
      Prepass::<B>::MOTION_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureSamplingViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.begin_label("SSAO pass");
    cmd_buffer.flush_barriers();
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, &self.kernel, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 1, blue_noise_view, blue_noise_sampler);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 2, &*depth_srv, &self.linear_sampler);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 3, camera, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 4, &*ssao_uav);
    cmd_buffer.finish_binding();
    let ssao_info = ssao_uav.texture().info();
    cmd_buffer.dispatch((ssao_info.width + 7) / 8, (ssao_info.height + 7) / 8, ssao_info.depth);

    std::mem::drop(ssao_uav);
    let ssao_srv = resources.access_srv(
      cmd_buffer,
      Self::SSAO_INTERNAL_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureSamplingViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let blurred_uav = resources.access_uav(
      cmd_buffer,
      Self::SSAO_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      false,
      &TextureStorageViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let blurred_srv_b = resources.access_srv(
      cmd_buffer,
      Self::SSAO_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureSamplingViewInfo::default(),
      HistoryResourceEntry::Past
    );

    cmd_buffer.flush_barriers();
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.blur_pipeline));
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 0, &*blurred_uav);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 1, &*ssao_srv, &self.blur_sampler);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 2, &*blurred_srv_b, &self.blur_sampler);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 3, &*motion_srv, &self.nearest_sampler);
    cmd_buffer.finish_binding();
    let blur_info = blurred_uav.texture().info();
    cmd_buffer.dispatch((blur_info.width + 7) / 8, (blur_info.height + 7) / 8, blur_info.depth);
    cmd_buffer.end_label();
  }
}
