use std::sync::Arc;

use nalgebra::Vector3;
use smallvec::SmallVec;
use sourcerenderer_core::{Matrix4, Platform, Vec2UI, graphics::{Backend, Barrier, CommandBuffer, Device, Queue, Swapchain, SwapchainError, BarrierSync, BarrierAccess, TextureLayout, BarrierTextureRange, BindingFrequency, WHOLE_BUFFER, BufferUsage, MemoryUsage, BufferInfo}, Vec2, Vec3};

use crate::{input::Input, renderer::{LateLatching, drawable::View, render_path::{RenderPath, SceneInfo, ZeroTextures, FrameInfo}, renderer_resources::{RendererResources, HistoryResourceEntry}, renderer_scene::RendererScene, passes::blue_noise::BlueNoise, shader_manager::ShaderManager, renderer_assets::RendererAssets}};

use super::{clustering::ClusteringPass, geometry::GeometryPass, light_binning::LightBinningPass, prepass::Prepass, sharpen::SharpenPass, ssao::SsaoPass, taa::TAAPass, occlusion::OcclusionPass, acceleration_structure_update::AccelerationStructureUpdatePass, rt_shadows::RTShadowPass};

pub struct ConservativeRenderer<P: Platform> {
  swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  barriers: RendererResources<P::GraphicsBackend>,
  clustering_pass: ClusteringPass,
  light_binning_pass: LightBinningPass,
  prepass: Prepass,
  geometry: GeometryPass<P>,
  taa: TAAPass,
  sharpen: SharpenPass,
  ssao: SsaoPass<P>,
  occlusion: OcclusionPass<P>,
  rt_passes: Option<RTPasses<P>>,
  blue_noise: BlueNoise<P::GraphicsBackend>
}

pub struct RTPasses<P: Platform> {
  acceleration_structure_update: AccelerationStructureUpdatePass<P>,
  shadows: RTShadowPass
}

pub struct FrameBindings<B: Backend> {
  gpu_scene_buffer: Arc<B::Buffer>,
  camera_buffer: Arc<B::Buffer>,
  camera_history_buffer: Arc<B::Buffer>,
  vertex_buffer: Arc<B::Buffer>,
  index_buffer: Arc<B::Buffer>,
  directional_lights: Arc<B::Buffer>,
  point_lights: Arc<B::Buffer>,
  setup_buffer: Arc<B::Buffer>,
}

impl<P: Platform> ConservativeRenderer<P> {
  pub fn new(device: &Arc<<P::GraphicsBackend as Backend>::Device>, swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>, shader_manager: &mut ShaderManager<P>) -> Self {
    let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();
    let resolution = Vec2UI::new(swapchain.width(), swapchain.height());

    let mut barriers = RendererResources::<P::GraphicsBackend>::new(device);

    let blue_noise = BlueNoise::new::<P>(device);

    let clustering = ClusteringPass::new::<P>(&mut barriers, shader_manager);
    let light_binning = LightBinningPass::new::<P>(&mut barriers, shader_manager);
    let prepass = Prepass::new::<P>(&mut barriers, shader_manager, resolution);
    let geometry = GeometryPass::<P>::new(device, resolution, &mut barriers, shader_manager);
    let taa = TAAPass::new::<P>(resolution, &mut barriers, shader_manager, false);
    let sharpen = SharpenPass::new::<P>(resolution, &mut barriers, shader_manager);
    let ssao = SsaoPass::<P>::new(device, resolution, &mut barriers, shader_manager, false);
    let occlusion = OcclusionPass::<P>::new(device, shader_manager);
    let rt_passes = device.supports_ray_tracing().then(|| RTPasses {
      acceleration_structure_update: AccelerationStructureUpdatePass::<P>::new(device, &mut init_cmd_buffer),
      shadows: RTShadowPass::new::<P>(resolution, &mut barriers, shader_manager)
    });
    init_cmd_buffer.flush_barriers();
    device.flush_transfers();

    let c_graphics_queue = device.graphics_queue().clone();
    c_graphics_queue.submit(init_cmd_buffer.finish(), None, &[], &[], true);
    rayon::spawn(move || c_graphics_queue.process_submissions());

    Self {
      swapchain: swapchain.clone(),
      device: device.clone(),
      barriers,
      clustering_pass: clustering,
      light_binning_pass: light_binning,
      prepass,
      geometry,
      taa,
      sharpen,
      ssao,
      occlusion,
      rt_passes,
      blue_noise,
    }
  }

  fn create_frame_bindings(
    &self,
    cmd_buf: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
    gpu_scene_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    camera_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    camera_history_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    vertex_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    index_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    scene: &RendererScene<P::GraphicsBackend>,
    view: &View,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    rendering_resolution: &Vec2UI,
    frame: u64
  ) -> FrameBindings<P::GraphicsBackend> {
    let cluster_count = self.clustering_pass.cluster_count();
    let cluster_z_scale = (cluster_count.z as f32) / (view.far_plane / view.near_plane).log2();
    let cluster_z_bias = -(cluster_count.z as f32) * (view.near_plane).log2() / (view.far_plane / view.near_plane).log2();
    #[repr(C)]
    #[derive(Debug, Clone)]
    struct SetupBuffer {
      point_light_count: u32,
      directional_light_count: u32,
      cluster_z_bias: f32,
      cluster_z_scale: f32,
      cluster_count: Vector3<u32>,
      _padding: u32,
      swapchain_transform: Matrix4,
      halton_point: Vec2,
      rt_size: Vec2UI,
    }
    let setup_buffer = cmd_buf.upload_dynamic_data(&[SetupBuffer {
      point_light_count: scene.point_lights().len() as u32,
      directional_light_count: scene.directional_lights().len() as u32,
      cluster_z_bias,
      cluster_z_scale,
      cluster_count,
      _padding: 0,
      swapchain_transform: swapchain.transform(),
      halton_point: super::taa::scaled_halton_point(rendering_resolution.x, rendering_resolution.y, (frame % 8) as u32 + 1),
      rt_size: *rendering_resolution
    }], BufferUsage::CONSTANT);
    #[repr(C)]
    #[derive(Debug, Clone)]
    struct PointLight {
      position: Vec3,
      intensity: f32
    }
    let point_lights: SmallVec<[PointLight; 16]> = scene.point_lights().iter().map(|l| PointLight {
      position: l.position,
      intensity: l.intensity
    }).collect();
    let point_lights_buffer = cmd_buf.upload_dynamic_data(&point_lights, BufferUsage::CONSTANT);
    #[repr(C)]
    #[derive(Debug, Clone)]
    struct DirectionalLight {
      direction: Vec3,
      intensity: f32
    }
    let directional_lights: SmallVec<[DirectionalLight; 16]> = scene.directional_lights().iter().map(|l| DirectionalLight {
      direction: l.direction,
      intensity: l.intensity
    }).collect();
    let directional_lights_buffer = cmd_buf.upload_dynamic_data(&directional_lights, BufferUsage::CONSTANT);

    FrameBindings {
      gpu_scene_buffer: gpu_scene_buffer.clone(),
      camera_buffer: camera_buffer.clone(),
      camera_history_buffer: camera_history_buffer.clone(),
      vertex_buffer: vertex_buffer.clone(),
      index_buffer: index_buffer.clone(),
      directional_lights: directional_lights_buffer,
      point_lights: point_lights_buffer,
      setup_buffer,
    }
  }
}

impl<P: Platform> RenderPath<P> for ConservativeRenderer<P> {
  fn is_gpu_driven(&self) -> bool {
    false
  }

  fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>) {
    self.occlusion.write_occlusion_query_results(frame, bitset);
  }

  fn on_swapchain_changed(&mut self, swapchain: &std::sync::Arc<<P::GraphicsBackend as Backend>::Swapchain>) {
    // TODO: resize render targets
    self.swapchain = swapchain.clone();
  }

  #[profiling::function]
  fn render(
    &mut self,
    scene: &SceneInfo<P::GraphicsBackend>,
    zero_textures: &ZeroTextures<P::GraphicsBackend>,
    late_latching: Option<&dyn LateLatching<P::GraphicsBackend>>,
    input: &Input,
    frame_info: &FrameInfo,
    shader_manager: &ShaderManager<P>,
    assets: &RendererAssets<P>
  ) -> Result<(), SwapchainError> {
    let graphics_queue = self.device.graphics_queue();
    let mut cmd_buf = graphics_queue.create_command_buffer();

    let late_latching_buffer = late_latching.unwrap().buffer();
    let late_latching_history_buffer = late_latching.unwrap().history_buffer().unwrap();
    if let Some(rt_passes) = self.rt_passes.as_mut() {
      rt_passes.acceleration_structure_update.execute(&mut cmd_buf, scene.scene, assets);
    }

    let primary_view = &scene.views[scene.active_view_index];

    let empty_buffer = cmd_buf.create_temporary_buffer(&BufferInfo {
      size: 16,
      usage: BufferUsage::STORAGE
    }, MemoryUsage::VRAM);

    let frame_bindings = self.create_frame_bindings(
      &mut cmd_buf,
      &empty_buffer,
      &late_latching_buffer,
      &late_latching_history_buffer,
      &empty_buffer,
      &empty_buffer,
      scene.scene,
      primary_view,
      &self.swapchain,
      &Vec2UI::new(self.swapchain.width(), self.swapchain.height()),
      frame_info.frame
    );
    setup_frame::<P::GraphicsBackend>(&mut cmd_buf, &frame_bindings);

    self.occlusion.execute(&mut cmd_buf, &self.barriers, shader_manager, &self.device, frame_info.frame, &late_latching_buffer, scene, Prepass::DEPTH_TEXTURE_NAME, assets);
    self.clustering_pass.execute::<P>(&mut cmd_buf, Vec2UI::new(self.swapchain.width(), self.swapchain.height()), primary_view, &late_latching_buffer, &mut self.barriers, shader_manager);
    self.light_binning_pass.execute(&mut cmd_buf, scene.scene, &late_latching_buffer, &mut self.barriers, shader_manager);
    self.prepass.execute(&mut cmd_buf, &self.device, scene.scene, primary_view, self.swapchain.transform(), frame_info.frame, &late_latching_buffer, &late_latching_history_buffer, &self.barriers, shader_manager, assets);
    self.ssao.execute(&mut cmd_buf, &self.barriers, Prepass::DEPTH_TEXTURE_NAME, Some(Prepass::MOTION_TEXTURE_NAME), &late_latching_buffer, self.blue_noise.frame(frame_info.frame), self.blue_noise.sampler(), shader_manager, false);
    if let Some(rt_passes) = self.rt_passes.as_mut() {
      rt_passes.shadows.execute(&mut cmd_buf, &self.barriers, shader_manager, Prepass::DEPTH_TEXTURE_NAME, rt_passes.acceleration_structure_update.acceleration_structure(), &self.blue_noise.frame(frame_info.frame), &self.blue_noise.sampler());
    }
    self.geometry.execute(&mut cmd_buf, &self.barriers, shader_manager, &self.device, Prepass::DEPTH_TEXTURE_NAME, scene, &frame_bindings, zero_textures, scene.lightmap.unwrap(), assets);
    self.taa.execute(&mut cmd_buf, &self.barriers, shader_manager, GeometryPass::<P>::GEOMETRY_PASS_TEXTURE_NAME, Prepass::DEPTH_TEXTURE_NAME, Some(Prepass::MOTION_TEXTURE_NAME), false);
    self.sharpen.execute(&mut cmd_buf, &self.barriers, shader_manager);

    let sharpened_texture = self.barriers.access_texture(
      &mut cmd_buf,
      SharpenPass::SHAPENED_TEXTURE_NAME,
      &BarrierTextureRange::default(),
      BarrierSync::COPY,
      BarrierAccess::COPY_READ,
      TextureLayout::CopySrc,
      false,
      HistoryResourceEntry::Current
    );

    let prepare_sem = self.device.create_semaphore();
    let cmd_buf_sem = self.device.create_semaphore();
    let back_buffer_res = self.swapchain.prepare_back_buffer(&prepare_sem);
    if back_buffer_res.is_none() {
      return Err(SwapchainError::Other);
    }

    let back_buffer = back_buffer_res.unwrap();

    cmd_buf.barrier(&[
        Barrier::TextureBarrier {
          old_sync: BarrierSync::empty(),
          new_sync: BarrierSync::COPY,
          old_access: BarrierAccess::empty(),
          new_access: BarrierAccess::COPY_WRITE,
          old_layout: TextureLayout::Undefined,
          new_layout: TextureLayout::CopyDst,
          texture: back_buffer.texture(),
          range: BarrierTextureRange::default(),
        }
    ]);
    cmd_buf.flush_barriers();
    cmd_buf.blit(&*sharpened_texture, 0, 0, back_buffer.texture(), 0, 0);
    cmd_buf.barrier(&[
        Barrier::TextureBarrier {
          old_sync: BarrierSync::COPY,
          new_sync: BarrierSync::empty(),
          old_access: BarrierAccess::COPY_WRITE,
          new_access: BarrierAccess::empty(),
          old_layout: TextureLayout::CopyDst,
          new_layout: TextureLayout::Present,
          texture: back_buffer.texture(),
          range: BarrierTextureRange::default(),
        }
    ]);
    std::mem::drop(sharpened_texture);

    self.barriers.swap_history_resources();

    if let Some(late_latching) = late_latching {
      let input_state = input.poll();
      late_latching.before_submit(&input_state, primary_view);
    }
    graphics_queue.submit(cmd_buf.finish(), None, &[&prepare_sem], &[&cmd_buf_sem], true);
    graphics_queue.present(&self.swapchain, &[&cmd_buf_sem], true);

    let c_graphics_queue = graphics_queue.clone();
    rayon::spawn(move || c_graphics_queue.process_submissions());

    if let Some(late_latching) = late_latching {
      late_latching.after_submit(&self.device);
    }

    Ok(())
  }
}

pub fn setup_frame<B: Backend>(
  cmd_buf: &mut B::CommandBuffer,
  frame_bindings: &FrameBindings<B>
) {
  cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 0, &frame_bindings.gpu_scene_buffer, 0, WHOLE_BUFFER);
  cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 1, &frame_bindings.camera_buffer, 0, WHOLE_BUFFER);
  cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 2, &frame_bindings.camera_history_buffer, 0, WHOLE_BUFFER);
  cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 3, &frame_bindings.vertex_buffer, 0, WHOLE_BUFFER);
  cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 4, &frame_bindings.index_buffer, 0, WHOLE_BUFFER);
  cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 5, &frame_bindings.setup_buffer, 0, WHOLE_BUFFER);
  cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 6, &frame_bindings.point_lights, 0, WHOLE_BUFFER);
  cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 7, &frame_bindings.directional_lights, 0, WHOLE_BUFFER);
}
