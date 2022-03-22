use sourcerenderer_core::{Vec2, graphics::{AddressMode, Backend as GraphicsBackend, Barrier, BindingFrequency, CommandBuffer, Device, Filter, Format, PipelineBinding, SampleCount, SamplerInfo, ShaderType, Swapchain, Texture, TextureInfo, TextureShaderResourceView, TextureShaderResourceViewInfo, TextureUnorderedAccessViewInfo, TextureUsage, TextureLayout, BarrierAccess, BarrierSync}};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

pub(crate) fn scaled_halton_point(width: u32, height: u32, index: u32) -> Vec2 {
  let width_frac = 1.0f32 / width as f32;
  let height_frac = 1.0f32 / height as f32;
  let mut halton_point = halton_point(index);
  halton_point.x *= width_frac;
  halton_point.y *= height_frac;
  halton_point
}

pub(crate) fn halton_point(index: u32) -> Vec2 {
  Vec2::new(
    halton_sequence(index, 2) * 2f32 - 1f32, halton_sequence(index, 3) * 2f32 - 1f32
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
  taa_texture: Arc<B::Texture>,
  taa_texture_b: Arc<B::Texture>,
  taa_srv: Arc<B::TextureShaderResourceView>,
  taa_srv_b: Arc<B::TextureShaderResourceView>,
  taa_uav: Arc<B::TextureUnorderedAccessView>,
  taa_uav_b: Arc<B::TextureUnorderedAccessView>,
  pipeline: Arc<B::ComputePipeline>,
  nearest_sampler: Arc<B::Sampler>,
  linear_sampler: Arc<B::Sampler>
}

impl<B: GraphicsBackend> TAAPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let taa_compute_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("taa.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("taa.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&taa_compute_shader);

    let linear_sampler = device.create_sampler(&SamplerInfo {
      mag_filter: Filter::Linear,
      min_filter: Filter::Linear,
      mip_filter: Filter::Linear,
      address_mode_u: AddressMode::Repeat,
      address_mode_v: AddressMode::Repeat,
      address_mode_w: AddressMode::Repeat,
      mip_bias: 0.0,
      max_anisotropy: 0.0,
      compare_op: None,
      min_lod: 0.0,
      max_lod: 1.0,
    });

    let nearest_sampler = device.create_sampler(&SamplerInfo {
      mag_filter: Filter::Linear,
      min_filter: Filter::Linear,
      mip_filter: Filter::Linear,
      address_mode_u: AddressMode::Repeat,
      address_mode_v: AddressMode::Repeat,
      address_mode_w: AddressMode::Repeat,
      mip_bias: 0.0,
      max_anisotropy: 0.0,
      compare_op: None,
      min_lod: 0.0,
      max_lod: 1.0,
    });

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
    let taa_texture = device.create_texture(&texture_info, Some("TAAOutput"));
    let taa_texture_b = device.create_texture(&texture_info, Some("TAAOutput_b"));

    let srv_info = TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    };
    let taa_srv = device.create_shader_resource_view(&taa_texture, &srv_info);
    let taa_srv_b = device.create_shader_resource_view(&taa_texture_b, &srv_info);

    let uav_info = TextureUnorderedAccessViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    };
    let taa_uav = device.create_unordered_access_view(&taa_texture, &uav_info);
    let taa_uav_b = device.create_unordered_access_view(&taa_texture_b, &uav_info);

    init_cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_sync: BarrierSync::empty(),
        new_sync: BarrierSync::COMPUTE_SHADER,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::SHADER_RESOURCE_READ,
        old_layout: TextureLayout::Undefined,
        new_layout: TextureLayout::Sampled,
        texture: &taa_texture,
      },
      Barrier::TextureBarrier {
        old_sync: BarrierSync::empty(),
        new_sync: BarrierSync::COMPUTE_SHADER,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::SHADER_RESOURCE_READ,
        old_layout: TextureLayout::Undefined,
        new_layout: TextureLayout::Storage,
        texture: &taa_texture_b,
      }
    ]);

    // TODO: Clear history texture

    Self {
      pipeline,
      taa_texture,
      taa_texture_b,
      taa_srv,
      taa_srv_b,
      taa_uav,
      taa_uav_b,
      linear_sampler,
      nearest_sampler
    }
  }

  pub fn execute(
    &mut self,
    cmd_buf: &mut B::CommandBuffer,
    output_srv: &Arc<B::TextureShaderResourceView>,
    motion_srv: &Arc<B::TextureShaderResourceView>
  ) {
    cmd_buf.begin_label("TAA pass");
    cmd_buf.barrier(&[
      Barrier::TextureBarrier {
        old_sync: BarrierSync::RENDER_TARGET,
        new_sync: BarrierSync::COMPUTE_SHADER,
        old_access: BarrierAccess::RENDER_TARGET_WRITE,
        new_access: BarrierAccess::SHADER_RESOURCE_READ,
        old_layout: TextureLayout::RenderTarget,
        new_layout: TextureLayout::Sampled,
        texture: output_srv.texture(),
      },
      Barrier::TextureBarrier {
        old_sync: BarrierSync::COMPUTE_SHADER,
        new_sync: BarrierSync::COMPUTE_SHADER,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::STORAGE_WRITE,
        old_layout: TextureLayout::Sampled,
        new_layout: TextureLayout::Storage,
        texture: self.taa_srv.texture(),
      },
      Barrier::TextureBarrier {
        old_sync: BarrierSync::COMPUTE_SHADER,
        new_sync: BarrierSync::COMPUTE_SHADER,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::SHADER_RESOURCE_READ,
        old_layout: TextureLayout::Storage,
        new_layout: TextureLayout::Sampled,
        texture: self.taa_srv_b.texture(),
      }
    ]);

    cmd_buf.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buf.bind_texture_view(BindingFrequency::PerDraw, 0, output_srv, &self.linear_sampler);
    cmd_buf.bind_texture_view(BindingFrequency::PerDraw, 1, &self.taa_srv_b, &self.linear_sampler);
    cmd_buf.bind_storage_texture(BindingFrequency::PerDraw, 2, &self.taa_uav);
    cmd_buf.bind_texture_view(BindingFrequency::PerDraw, 3, motion_srv, &self.nearest_sampler);
    cmd_buf.finish_binding();

    let info = self.taa_texture.get_info();
    cmd_buf.dispatch((info.width + 15) / 16, (info.height + 15) / 16, 1);
    cmd_buf.end_label();
  }

  pub fn taa_srv(&self) -> &Arc<B::TextureShaderResourceView> {
    &self.taa_srv
  }

  pub fn taa_uav(&self) -> &Arc<B::TextureUnorderedAccessView> {
    &self.taa_uav
  }

  pub fn swap_history_resources(&mut self) {
    std::mem::swap(&mut self.taa_texture, &mut self.taa_texture_b);
    std::mem::swap(&mut self.taa_srv, &mut self.taa_srv_b);
    std::mem::swap(&mut self.taa_uav, &mut self.taa_uav_b);
  }
}
