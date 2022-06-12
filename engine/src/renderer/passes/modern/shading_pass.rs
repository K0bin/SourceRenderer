use std::{sync::Arc, path::Path, io::Read};

use nalgebra::Vector3;
use nalgebra_glm::Vec3;
use smallvec::SmallVec;
use sourcerenderer_core::{graphics::{Backend, ShaderType, Device, TextureInfo, Format, Swapchain, SampleCount, TextureUsage, BarrierAccess, TextureLayout, TextureViewInfo, BarrierSync, CommandBuffer, PipelineBinding, WHOLE_BUFFER, BindingFrequency, BufferUsage, Filter, AddressMode, SamplerInfo}, Platform, platform::io::IO, Matrix4};

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, renderer_assets::RendererTexture, drawable::View, renderer_scene::RendererScene, PointLight, light::DirectionalLight};

use super::visibility_buffer::VisibilityBufferPass;


pub struct ShadingPass<B: Backend> {
  sampler: Arc<B::Sampler>,
  pipeline: Arc<B::ComputePipeline>,
}

impl<B: Backend> ShadingPass<B> {
  pub const SHADING_PASS_TEXTURE_NAME: &'static str = "shading";

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

    resources.create_texture(Self::SHADING_PASS_TEXTURE_NAME, &TextureInfo {
      format: Format::RGBA32Float,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::STORAGE,
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
    device: &Arc<B::Device>,
    scene: &RendererScene<B>,
    view: &View,
    gpu_scene: &Arc<B::Buffer>,
    zero_texture_view: &Arc<B::TextureSamplingView>,
    _zero_texture_view_black: &Arc<B::TextureSamplingView>,
    lightmap: &Arc<RendererTexture<B>>,
    swapchain_transform: Matrix4,
    frame: u64,
    resources: &RendererResources<B>,
    camera_buffer: &Arc<B::Buffer>,
    vertex_buffer: &Arc<B::Buffer>,
    index_buffer: &Arc<B::Buffer>,
  ) {
    let (width, height) = {
      let info = resources.texture_info(Self::SHADING_PASS_TEXTURE_NAME);
      (info.width, info.height)
    };

    let output = resources.access_storage_view(
      cmd_buffer,
      Self::SHADING_PASS_TEXTURE_NAME,
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

    let mut point_lights = SmallVec::<[PointLight; 16]>::new();
    for point_light in scene.point_lights() {
      point_lights.push(PointLight {
        position: point_light.position,
        intensity: point_light.intensity
      });
    }
    let mut directional_lights = SmallVec::<[DirectionalLight; 16]>::new();
    for directional_light in scene.directional_lights() {
      directional_lights.push(DirectionalLight {
        direction: directional_light.direction,
        intensity: directional_light.intensity
      });
    }
    let point_light_buffer = cmd_buffer.upload_dynamic_data(&point_lights[..], BufferUsage::STORAGE);
    let directional_light_buffer = cmd_buffer.upload_dynamic_data(&directional_lights[..], BufferUsage::STORAGE);

    #[repr(C)]
    #[derive(Clone)]
    struct Setup {
      cluster_z_bias: f32,
      cluster_z_scale: f32,
      point_light_count: u32,
      directional_light_count: u32,
      cluster_count: Vector3<u32>,
    }
    let cluster_count = nalgebra::Vector3::<u32>::new(16, 9, 24);
    let near = view.near_plane;
    let far = view.far_plane;
    let cluster_z_scale = (cluster_count.z as f32) / (far / near).log2();
    let cluster_z_bias = -(cluster_count.z as f32) * (near).log2() / (far / near).log2();
    let setup_buffer = cmd_buffer.upload_dynamic_data(&[Setup {
      cluster_z_bias,
      cluster_z_scale,
      point_light_count: point_lights.len() as u32,
      directional_light_count: directional_lights.len() as u32,
      cluster_count
    }], BufferUsage::CONSTANT);

    cmd_buffer.set_pipeline(PipelineBinding::Compute(&self.pipeline));
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 0, vertex_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 1, index_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 2, &ids);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 3, &barycentrics);
    cmd_buffer.bind_storage_texture(BindingFrequency::PerDraw, 4, &output);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 5, camera_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 6, gpu_scene, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampler(BindingFrequency::PerDraw, 7, &self.sampler);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 8, &light_bitmask_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 9, &point_light_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerDraw, 10, &directional_light_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 11, &setup_buffer, 0, WHOLE_BUFFER);

    cmd_buffer.flush_barriers();
    cmd_buffer.finish_binding();

    cmd_buffer.dispatch((width + 7) / 8 , (height + 7) / 8, 1);
  }
}
