use std::{sync::Arc, path::Path, io::Read, cell::Ref};

use sourcerenderer_core::{graphics::{Backend, ShaderType, Device, TextureInfo, Format, Swapchain, SampleCount, TextureUsage, BarrierAccess, TextureLayout, TextureViewInfo, BarrierSync, CommandBuffer, PipelineBinding, WHOLE_BUFFER, BindingFrequency, Filter, AddressMode, SamplerInfo, TextureDimension}, Platform, platform::io::IO};

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, renderer_assets::RendererTexture, passes::{conservative::geometry::GeometryPass, ssao::SsaoPass}};

use super::{visibility_buffer::VisibilityBufferPass, rt_shadows::RTShadowPass};


pub struct ShadingPass<B: Backend> {
  sampler: Arc<B::Sampler>,
  pipeline: Arc<B::ComputePipeline>,
}

impl<B: Backend> ShadingPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, resources: &mut RendererResources<B>, _init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("shading.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      println!("shader");
      let shader = device.create_shader(ShaderType::ComputeShader, &bytes, Some("shading.comp.spv"));
      println!("shader done");
      shader
    };
    let pipeline = device.create_compute_pipeline(&shader, Some("Shading"));

    let sampler = device.create_sampler(&SamplerInfo {
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
      max_lod: None,
    });

    resources.create_texture(GeometryPass::<B>::GEOMETRY_PASS_TEXTURE_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA16Float,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
    }, false);

    Self  {
      sampler,
      pipeline,
    }
  }
  #[profiling::function]
  pub(super) fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    device: &B::Device,
    lightmap: &Arc<RendererTexture<B>>,
    zero_texture_view: &Arc<B::TextureSamplingView>,
    resources: &RendererResources<B>,
  ) {
    let (width, height) = {
      let info = resources.texture_info(GeometryPass::<B>::GEOMETRY_PASS_TEXTURE_NAME);
      (info.width, info.height)
    };

    cmd_buffer.begin_label("Shading Pass");

    let output = resources.access_storage_view(
      cmd_buffer,
      GeometryPass::<B>::GEOMETRY_PASS_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let ids = resources.access_storage_view(
      cmd_buffer,
      VisibilityBufferPass::<B>::PRIMITIVE_ID_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_READ,
      TextureLayout::Storage,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let barycentrics = resources.access_storage_view(
      cmd_buffer,
      VisibilityBufferPass::<B>::BARYCENTRICS_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_READ,
      TextureLayout::Storage,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let light_bitmask_buffer = resources.access_buffer(
      cmd_buffer,
      super::light_binning::LightBinningPass::<B>::LIGHT_BINNING_BUFFER_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_READ,
      HistoryResourceEntry::Current
    );

    let ssao = resources.access_sampling_view(
      cmd_buffer,
      SsaoPass::<B>::SSAO_TEXTURE_NAME,
      BarrierSync::FRAGMENT_SHADER | BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let rt_shadows: Ref<Arc<B::TextureSamplingView>>;
    let shadows = if device.supports_ray_tracing() {
      rt_shadows = resources.access_sampling_view(
        cmd_buffer,
        RTShadowPass::<B>::SHADOWS_TEXTURE_NAME,
        BarrierSync::FRAGMENT_SHADER,
        BarrierAccess::SAMPLING_READ,
        TextureLayout::Sampled,
        false,
        &TextureViewInfo::default(),
        HistoryResourceEntry::Current
      );
      &*rt_shadows
    } else {
      zero_texture_view
    };

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &ids);
    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 2, &barycentrics);
    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 3, &output);
    cmd_buffer.bind_sampler(BindingFrequency::VeryFrequent, 4, &self.sampler);
    cmd_buffer.bind_storage_buffer(BindingFrequency::VeryFrequent, 5, &light_bitmask_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 6, &lightmap.view, resources.linear_sampler());
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 7, shadows, resources.linear_sampler());
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 8, &ssao, resources.linear_sampler());

    cmd_buffer.flush_barriers();
    cmd_buffer.finish_binding();

    cmd_buffer.dispatch((width + 7) / 8 , (height + 7) / 8, 1);
    cmd_buffer.end_label();
  }
}
