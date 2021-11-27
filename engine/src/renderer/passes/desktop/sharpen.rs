use sourcerenderer_core::{graphics::{AddressMode, Backend as GraphicsBackend, Barrier, BindingFrequency, CommandBuffer, Device, Filter, Format, PipelineBinding, SamplerInfo, ShaderType, Swapchain, Texture, TextureInfo, TextureShaderResourceView, TextureUnorderedAccessView, TextureUnorderedAccessViewInfo, TextureUsage}};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

pub struct SharpenPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>,
  sampler: Arc<B::Sampler>,
  sharpen_uav: Arc<B::TextureUnorderedAccessView>
}

impl<B: GraphicsBackend> SharpenPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let sharpen_compute_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("sharpen.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("sharpen.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&sharpen_compute_shader);

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
      max_lod: 1.0,
    });

    let texture = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: sourcerenderer_core::graphics::SampleCount::Samples1,
      usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE | TextureUsage::COPY_SRC,
    }, Some("SharpenOutput"));
    let uav = device.create_unordered_access_view(&texture, &TextureUnorderedAccessViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });

    init_cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::UNINITIALIZED,
        new_primary_usage: TextureUsage::COPY_SRC,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: &texture,
      }
    ]);

    Self {
      pipeline,
      sampler,
      sharpen_uav: uav
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, input_image: &Arc<B::TextureShaderResourceView>) {
    cmd_buffer.begin_label("Sharpening pass");
    cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_usages: TextureUsage::COMPUTE_SHADER_SAMPLED,
        texture: input_image.texture(),
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::COPY_SRC,
        new_primary_usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: self.sharpen_uav.texture()
      },
    ]);

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.bind_texture_view(BindingFrequency::PerDraw, 0, input_image, &self.sampler);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 1, &self.sharpen_uav);
    cmd_buffer.finish_binding();

    let info = self.sharpen_uav.texture().get_info();
    cmd_buffer.dispatch((info.width + 15) / 16, (info.height + 15) / 16, 1);
    cmd_buffer.end_label();
  }

  pub fn sharpened_texture(&self) -> &Arc<B::Texture> {
    self.sharpen_uav.texture()
  }
}

