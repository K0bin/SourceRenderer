use std::{sync::Arc, path::Path, io::Read};

use sourcerenderer_core::{graphics::{Backend, TextureUsage, Format, Device, ShaderType, CommandBuffer, PipelineBinding, BarrierSync, BarrierAccess, TextureLayout, TextureViewInfo, BindingFrequency}, Platform, platform::io::IO};

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, passes::prepass::Prepass};

pub struct HierarchicalZPass<B: Backend> {
  pipeline: Arc<B::ComputePipeline>,
}

impl<B: Backend> HierarchicalZPass<B> {
  pub const HI_Z_BUFFER_NAME: &'static str = "Hierarchical Z Buffer";
  pub fn new<P: Platform>(device: &Arc<B::Device>, resources: &mut RendererResources<B>) -> Self {
    let mut texture_info = resources.texture_info(Prepass::<B>::DEPTH_TEXTURE_NAME).clone();
    let size = texture_info.width.max(texture_info.height) as f32;
    texture_info.mip_levels = (size.log(2f32).ceil() as u32).max(1);
    texture_info.usage = TextureUsage::STORAGE | TextureUsage::SAMPLED;
    texture_info.format = Format::R32Float;

    resources.create_texture(Self::HI_Z_BUFFER_NAME, &texture_info, false);

    let shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("hi_z_gen.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("hi_z_gen.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&shader, Some("Hi-Z Gen"));

    Self {
      pipeline
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, resources: &RendererResources<B>) {
    let (width, height, mips) = {
      let info = resources.texture_info(Self::HI_Z_BUFFER_NAME);
      (info.width, info.height, info.mip_levels)
    };

    cmd_buffer.begin_label("Hierarchical Z");
    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));

    for mip in 0..mips {
      let mip_width = width >> mip;
      let mip_height = height >> mip;

      let src_texture = if mip == 0 {
        resources.access_sampling_view(
          cmd_buffer,
          Prepass::<B>::DEPTH_TEXTURE_NAME,
          BarrierSync::COMPUTE_SHADER,
          BarrierAccess::SAMPLING_READ,
          TextureLayout::Sampled,
          false,
          &TextureViewInfo::default(),
          HistoryResourceEntry::Current
        )
      } else {
        resources.access_sampling_view(
          cmd_buffer,
          Self::HI_Z_BUFFER_NAME,
          BarrierSync::COMPUTE_SHADER,
          BarrierAccess::SAMPLING_READ,
          TextureLayout::Sampled,
          false,
          &TextureViewInfo {
            base_array_layer: 0,
            array_layer_length: 1,
            base_mip_level: mip - 1,
            mip_level_length: 1
          },
          HistoryResourceEntry::Current
        )
      }.clone();
      let dst_texture = resources.access_storage_view(
        cmd_buffer,
        Self::HI_Z_BUFFER_NAME,
        BarrierSync::COMPUTE_SHADER,
        BarrierAccess::STORAGE_WRITE,
        TextureLayout::General,
        true,
        &TextureViewInfo {
          base_mip_level: mip,
          mip_level_length: 1,
          base_array_layer: 0,
          array_layer_length: 1,
        }, HistoryResourceEntry::Current
      );

      #[derive(Clone)]
      #[repr(C)]
      struct PushConstantData {
        base_width: u32,
        base_height: u32,
        mip_level: u32
      }

      cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerDraw, 0, &src_texture, resources.nearest_sampler());
      cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 1, &dst_texture);
      cmd_buffer.upload_dynamic_data_inline(&[PushConstantData {
        base_width: width,
        base_height: height,
        mip_level: mip,
    }], ShaderType::ComputeShader);
      cmd_buffer.flush_barriers();
      cmd_buffer.finish_binding();
      cmd_buffer.dispatch((mip_width + 7) / 8, (mip_height + 7) / 8, 1);
    }
    cmd_buffer.end_label();
  }
}
