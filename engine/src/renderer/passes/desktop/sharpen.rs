use half::f16;
use sourcerenderer_core::{graphics::{Backend as GraphicsBackend, BindingFrequency, CommandBuffer, Device, Format, PipelineBinding, ShaderType, Swapchain, Texture, TextureInfo, TextureUnorderedAccessView, TextureUnorderedAccessViewInfo, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, BufferUsage}, Vec4, Vec2UI, Vec2};
use sourcerenderer_core::Platform;
use std::sync::Arc;
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;

use crate::renderer::{renderer_resources::{HistoryResourceEntry, RendererResources}};

use super::{geometry::GeometryPass, taa::TAAPass};

const USE_CAS: bool = true;

pub struct SharpenPass<B: GraphicsBackend> {
  pipeline: Arc<B::ComputePipeline>
}

impl<B: GraphicsBackend> SharpenPass<B> {
  pub const SHAPENED_TEXTURE_NAME: &'static str = "Sharpened";

  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, resources: &mut RendererResources<B>) -> Self {
    let sharpen_compute_shader = if !USE_CAS {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("sharpen.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("sharpen.comp.spv"))
    } else {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("cas.comp.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::ComputeShader, &bytes, Some("cas.comp.spv"))
    };
    let pipeline = device.create_compute_pipeline(&sharpen_compute_shader);

    resources.create_texture(Self::SHAPENED_TEXTURE_NAME, &TextureInfo {
      format: Format::RGBA8,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: sourcerenderer_core::graphics::SampleCount::Samples1,
      usage: TextureUsage::STORAGE | TextureUsage::COPY_SRC,
    }, false);

    Self {
      pipeline
    }
  }

  pub fn execute(&mut self, cmd_buffer: &mut B::CommandBuffer, resources: &RendererResources<B>) {
    let input_image_uav = resources.access_uav(
      cmd_buffer,
      TAAPass::<B>::TAA_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_READ,
      TextureLayout::Storage,
      false,
      &TextureUnorderedAccessViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let sharpen_uav = resources.access_uav(
      cmd_buffer,
      Self::SHAPENED_TEXTURE_NAME,
      BarrierSync::COMPUTE_SHADER,
      BarrierAccess::STORAGE_WRITE,
      TextureLayout::Storage,
      true,
      &TextureUnorderedAccessViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.begin_label("Sharpening pass");

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    if USE_CAS {
      let input_size = Vec2UI::new(
        input_image_uav.texture().get_info().width,
        input_image_uav.texture().get_info().height,
      );
      let output_size = Vec2UI::new(
        sharpen_uav.texture().get_info().width,
        sharpen_uav.texture().get_info().height,
      );
      let setup_data = cas_setup(1f32, input_size, output_size);
      let cas_setup_ubo = cmd_buffer.upload_dynamic_data(&[setup_data], BufferUsage::CONSTANT);
      cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 2, &cas_setup_ubo);
    }
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 0, &*input_image_uav);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 1, &*sharpen_uav);
    cmd_buffer.finish_binding();

    let info = sharpen_uav.texture().get_info();
    cmd_buffer.dispatch((info.width + 15) / 16, (info.height + 15) / 16, 1);
    cmd_buffer.end_label();
  }
}


/*
 #define AU1_AH2_AF2 packHalf2x16

A_STATIC void CasSetup(
 outAU4 const0,
 outAU4 const1,
 AF1 sharpness, // 0 := default (lower ringing), 1 := maximum (higest ringing)
 AF1 inputSizeInPixelsX,
 AF1 inputSizeInPixelsY,
 AF1 outputSizeInPixelsX,
 AF1 outputSizeInPixelsY){
  // Scaling terms.
  const0[0]=AU1_AF1(inputSizeInPixelsX*ARcpF1(outputSizeInPixelsX));
  const0[1]=AU1_AF1(inputSizeInPixelsY*ARcpF1(outputSizeInPixelsY));
  const0[2]=AU1_AF1(AF1_(0.5)*inputSizeInPixelsX*ARcpF1(outputSizeInPixelsX)-AF1_(0.5));
  const0[3]=AU1_AF1(AF1_(0.5)*inputSizeInPixelsY*ARcpF1(outputSizeInPixelsY)-AF1_(0.5));
  // Sharpness value.
  AF1 sharp=-ARcpF1(ALerpF1(8.0,5.0,ASatF1(sharpness)));
  varAF2(hSharp)=initAF2(sharp,0.0);
  const1[0]=AU1_AF1(sharp);
  const1[1]=AU1_AH2_AF2(hSharp);
  const1[2]=AU1_AF1(AF1_(8.0)*inputSizeInPixelsX*ARcpF1(outputSizeInPixelsX));
  const1[3]=0;}
*/

fn lerp(a: f32, b: f32, frac: f32) -> f32 {
  a * (1.0 - frac) + b * frac
}

fn cas_setup(sharpness: f32, input_size_px: Vec2UI, output_size_px: Vec2UI) -> (Vec4, Vec4) {
  let input_size_f = Vec2::new(input_size_px.x as f32, input_size_px.y as f32);
  let output_size_f = Vec2::new(output_size_px.x as f32, output_size_px.y as f32);
  let const0 = Vec4::new(
    input_size_f.x / output_size_f.x,
    input_size_f.y / output_size_f.y,
    0.5f32 * input_size_f.x / output_size_f.x - 0.5f32,
    0.5f32 * input_size_f.y / output_size_f.y - 0.5f32
  );
  let sharp = 1.0f32 / lerp(8.0, 5.0, sharpness);
  let h_sharp = Vec2::new(sharp, 0f32);
  let const1 = Vec4::new(
    sharp,
    unsafe { std::mem::transmute((f16::from_f32(h_sharp.x).to_bits() as u32) | ((f16::from_f32(h_sharp.y).to_bits() as u32) << 16)) },
    8.0f32 * input_size_f.x / output_size_f.x,
    0f32
  );
  (const0, const1)
}


