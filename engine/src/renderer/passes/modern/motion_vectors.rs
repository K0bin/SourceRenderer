use std::sync::Arc;
use std::path::Path;
use std::io::Read;

use sourcerenderer_core::graphics::{Backend, BarrierAccess, BarrierSync, BindingFrequency,
                                    CommandBuffer, Format, PipelineBinding, TextureInfo,
                                    TextureLayout, TextureStorageView, TextureUsage,
                                    TextureViewInfo, Texture, Device, TextureDimension,
                                    ShaderType, SampleCount};
use sourcerenderer_core::{Platform, Vec2UI, platform::io::IO};
use crate::renderer::passes::modern::VisibilityBufferPass;
use crate::renderer::passes::prepass::Prepass;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};

pub struct MotionVectorPass<B: Backend> {
  pipeline: Arc<B::ComputePipeline>
}

impl<B: Backend> MotionVectorPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, resources: &mut RendererResources<B>, renderer_resolution: Vec2UI) -> Self {
    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("motion_vectors_vis_buf.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("motion_vectors_vis_buf.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&shader, Some("Motion Vectors"));

    resources.create_texture(
        Prepass::<B>::MOTION_TEXTURE_NAME,
        &TextureInfo {
          dimension: TextureDimension::Dim2D,
          format: Format::RG16Float,
          width: renderer_resolution.x,
          height: renderer_resolution.y,
          depth: 1,
          mip_levels: 1,
          array_length: 1,
          samples: SampleCount::Samples1,
          usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
          supports_srgb: false,
        },
        false
    );
    Self {
      pipeline
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, resources: &RendererResources<B>) {
    cmd_buffer.begin_label("Motion Vectors");

    let output_srv = resources.access_storage_view(
      cmd_buffer,
      Prepass::<B>::MOTION_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let (width, height) = {
      let info = output_srv.texture().info();
      (info.width, info.height)
    };

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

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &output_srv);
    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &ids);
    cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 2, &barycentrics);
    cmd_buffer.flush_barriers();
    cmd_buffer.finish_binding();
    cmd_buffer.dispatch((width + 7) / 8, (height + 7) / 8, 1);
    cmd_buffer.end_label();
  }
}
