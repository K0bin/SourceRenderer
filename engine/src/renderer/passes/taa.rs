use sourcerenderer_core::{Vec2, graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Format, PipelineBinding, SampleCount, TextureInfo, TextureViewInfo, TextureUsage, TextureLayout, BarrierAccess, BarrierSync, Texture, TextureDimension, BarrierTextureRange}, Vec2UI};
use sourcerenderer_core::Platform;
use std::{sync::Arc, cell::Ref};

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, shader_manager::{ComputePipelineHandle, ShaderManager}};

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

pub struct TAAPass {
  pipeline: ComputePipelineHandle,
}

impl TAAPass {
  pub const TAA_TEXTURE_NAME: &'static str = "TAAOuput";

  pub fn new<P: Platform>(
    resolution: Vec2UI,
    resources: &mut RendererResources<P::GraphicsBackend>,
    shader_manager: &mut ShaderManager<P>,
    visibility_buffer: bool
  ) -> Self {
    let pipeline = shader_manager.request_compute_pipeline(if !visibility_buffer { "shaders/taa.comp.spv" } else { "shaders/taa_vis_buf.comp.spv" });

    let texture_info = TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA8UNorm,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
      supports_srgb: false,
    };
    resources.create_texture(Self::TAA_TEXTURE_NAME, &texture_info, true);

    // TODO: Clear history texture

    Self {
      pipeline,
    }
  }

  pub fn execute<P: Platform>(
    &mut self,
    cmd_buf: &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer,
    resources: &RendererResources<P::GraphicsBackend>,
    shader_manager: &ShaderManager<P>,
    input_name: &str,
    depth_name: &str,
    motion_name: Option<&str>,
    visibility_buffer: bool
  ) {
    cmd_buf.begin_label("TAA pass");

    let output_texture = resources.access_texture(
      cmd_buf,
      input_name,
      &BarrierTextureRange::default(),
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      HistoryResourceEntry::Current
    );
    let output_srv = output_texture.primary_view();

    let taa_texture = resources.access_texture(
      cmd_buf,
      Self::TAA_TEXTURE_NAME,
      &BarrierTextureRange::default(),
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      HistoryResourceEntry::Current
    );
    let taa_uav = taa_texture.primary_view();

    let taa_history_texture = resources.access_texture(
      cmd_buf,
      Self::TAA_TEXTURE_NAME,
      &BarrierTextureRange::default(),
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      HistoryResourceEntry::Past
    );
    let taa_history_uav = taa_history_texture.primary_view();

    let mut motion_texture = Option::<Ref<Arc<<P::GraphicsBackend as GraphicsBackend>::Texture>>>::None;
    let mut id_texture = Option::<Ref<Arc<<P::GraphicsBackend as GraphicsBackend>::Texture>>>::None;
    let mut barycentrics_texture = Option::<Ref<Arc<<P::GraphicsBackend as GraphicsBackend>::Texture>>>::None;
    if !visibility_buffer {
      motion_texture = Some(resources.access_texture(
        cmd_buf,
        motion_name.unwrap(),
        &BarrierTextureRange::default(),
        BarrierSync::COMPUTE_SHADER,
        BarrierAccess::SAMPLING_READ,
        TextureLayout::Sampled,
        false,
        HistoryResourceEntry::Current
      ));
    } else {
      id_texture = Some(resources.access_texture(
        cmd_buf,
        super::modern::VisibilityBufferPass::PRIMITIVE_ID_TEXTURE_NAME,
        &BarrierTextureRange::default(),
        BarrierSync::COMPUTE_SHADER,
        BarrierAccess::STORAGE_READ,
        TextureLayout::Storage,
        false,
        HistoryResourceEntry::Current
      ));
      barycentrics_texture = Some(resources.access_texture(
        cmd_buf,
        super::modern::VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME,
        &BarrierTextureRange::default(),
        BarrierSync::COMPUTE_SHADER,
        BarrierAccess::STORAGE_READ,
        TextureLayout::Storage,
        false,
        HistoryResourceEntry::Current
      ));
    }

    let depth_srv = resources.access_sampling_view(
      cmd_buf,
      depth_name,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let pipeline = shader_manager.get_compute_pipeline(self.pipeline);
    cmd_buf.set_pipeline(PipelineBinding::Compute(&pipeline));
    cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, &*output_srv, resources.linear_sampler());
    cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 1, taa_history_texture.primary_view, resources.linear_sampler());
    cmd_buf.bind_storage_texture(BindingFrequency::VeryFrequent, 2, &*taa_uav);
    cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 3, &*depth_srv, resources.linear_sampler());
    if !visibility_buffer {
      cmd_buf.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 4, motion_texture.unwrap().primary_view(), resources.nearest_sampler());
    } else {
      cmd_buf.bind_storage_texture(BindingFrequency::VeryFrequent, 4, id_texture.unwrap().primary_view());
      cmd_buf.bind_storage_texture(BindingFrequency::VeryFrequent, 5, barycentrics_texture.unwrap().primary_view());
    }
    cmd_buf.finish_binding();

    let info = taa_uav.texture().info();
    cmd_buf.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
    cmd_buf.end_label();
  }
}
