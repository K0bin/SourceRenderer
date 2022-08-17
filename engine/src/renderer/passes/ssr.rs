use std::{io::Read, path::Path, sync::Arc, cell::Ref};

use sourcerenderer_core::{Platform, Vec2UI, graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, PipelineBinding, SampleCount, ShaderType, Texture, TextureInfo, TextureViewInfo, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, TextureStorageView, WHOLE_BUFFER, TextureDimension}, platform::io::IO};

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}};

use super::{prepass::Prepass, conservative::geometry::GeometryPass, modern::VisibilityBufferPass};

pub struct SsrPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>,
}

impl<B: GraphicsBackend> SsrPass<B> {
  pub const SSR_TEXTURE_NAME: &'static str = "SSR";

  pub fn new<P: Platform>(device: &Arc<B::Device>, resolution: Vec2UI, resources: &mut RendererResources<B>, _visibility_buffer: bool) -> Self {
    resources.create_texture(Self::SSR_TEXTURE_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA16Float,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
    }, false);

    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("ssr.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("ssr.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&shader, Some("SSR"));

    Self {
      pipeline,
    }
  }

  pub fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    camera: &Arc<B::Buffer>,
    resources: &RendererResources<B>,
    visibility_buffer: bool
  ){
    // TODO: merge back into the original image
    // TODO: specularity map

    let ssr_uav = resources.access_storage_view(
      cmd_buffer,
      Self::SSR_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let depth_srv = resources.access_sampling_view(
      cmd_buffer,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let color_srv = resources.access_sampling_view(
      cmd_buffer,
      GeometryPass::<B>::GEOMETRY_PASS_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let mut ids = Option::<Ref<Arc<B::TextureStorageView>>>::None;
    let mut barycentrics = Option::<Ref<Arc<B::TextureStorageView>>>::None;

    if visibility_buffer {
      ids = Some(resources.access_storage_view(
        cmd_buffer,
        VisibilityBufferPass::<B>::PRIMITIVE_ID_TEXTURE_NAME,
        BarrierSync::COMPUTE_SHADER,
        BarrierAccess::STORAGE_READ,
        TextureLayout::Storage,
        false,
        &TextureViewInfo::default(),
        HistoryResourceEntry::Current
      ));

      barycentrics = Some(resources.access_storage_view(
        cmd_buffer,
        VisibilityBufferPass::<B>::BARYCENTRICS_TEXTURE_NAME,
        BarrierSync::COMPUTE_SHADER,
        BarrierAccess::STORAGE_READ,
        TextureLayout::Storage,
        false,
        &TextureViewInfo::default(),
        HistoryResourceEntry::Current
      ));
    }

    cmd_buffer.begin_label("SSR pass");
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.flush_barriers();
    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &ssr_uav);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 1, &*color_srv, resources.linear_sampler());
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 2, &*depth_srv, resources.linear_sampler());
    if visibility_buffer {
      cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 3, ids.as_ref().unwrap());
      cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 4, barycentrics.as_ref().unwrap());
    }
    cmd_buffer.finish_binding();
    let ssr_info = ssr_uav.texture().info();
    cmd_buffer.dispatch((ssr_info.width + 7) / 8, (ssr_info.height + 7) / 8, ssr_info.depth);
    cmd_buffer.end_label();
  }
}
