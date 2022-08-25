use std::sync::Arc;

use nalgebra::Vector3;
use smallvec::SmallVec;
use sourcerenderer_core::{Matrix4, Platform, Vec2UI, graphics::{Backend, Barrier, CommandBuffer, Device, Queue, Swapchain, SwapchainError, TextureRenderTargetView, BarrierSync, BarrierAccess, TextureLayout, BarrierTextureRange, BindingFrequency, WHOLE_BUFFER, BufferUsage}, Vec2, Vec3};

use crate::{input::Input, renderer::{LateLatching, drawable::View, render_path::{RenderPath, SceneInfo, ZeroTextures, FrameInfo}, renderer_resources::{RendererResources, HistoryResourceEntry}, renderer_scene::RendererScene, passes::{blue_noise::BlueNoise, ssr::SsrPass, compositing::CompositingPass}}};
use crate::renderer::passes::fsr2::Fsr2Pass;
use crate::renderer::passes::modern::motion_vectors::MotionVectorPass;

use super::{clustering::ClusteringPass, light_binning::LightBinningPass, sharpen::SharpenPass, ssao::SsaoPass, taa::TAAPass, acceleration_structure_update::AccelerationStructureUpdatePass, rt_shadows::RTShadowPass, draw_prep::DrawPrepPass, hi_z::HierarchicalZPass, visibility_buffer::VisibilityBufferPass, shading_pass::ShadingPass};

pub struct ModernRenderer<B: Backend> {
  swapchain: Arc<B::Swapchain>,
  device: Arc<B::Device>,
  barriers: RendererResources<B>,
  clustering_pass: ClusteringPass<B>,
  light_binning_pass: LightBinningPass<B>,
  geometry_draw_prep: DrawPrepPass<B>,
  ssao: SsaoPass<B>,
  rt_passes: Option<RTPasses<B>>,
  blue_noise: BlueNoise<B>,
  hi_z_pass: HierarchicalZPass<B>,
  ssr_pass: SsrPass<B>,
  visibility_buffer: VisibilityBufferPass<B>,
  shading_pass: ShadingPass<B>,
  compositing_pass: CompositingPass<B>,
  motion_vector_pass: MotionVectorPass<B>,
  anti_aliasing: AntiAliasing<B>,
}

enum AntiAliasing<B: Backend> {
  TAA {
    taa: TAAPass<B>,
    sharpen: SharpenPass<B>
  },
  FSR2 {
    fsr: Fsr2Pass<B>
  }
}

pub struct RTPasses<B: Backend> {
  acceleration_structure_update: AccelerationStructureUpdatePass<B>,
  shadows: RTShadowPass<B>
}

impl<B: Backend> ModernRenderer<B> {
  const USE_FSR2: bool = true;

  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>) -> Self {
    let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();
    let resolution = Vec2UI::new(swapchain.width() / 4 * 3, swapchain.height() / 4 * 3);

    let mut barriers = RendererResources::<B>::new(device);

    let blue_noise = BlueNoise::new::<P>(device);

    let clustering = ClusteringPass::<B>::new::<P>(device, &mut barriers);
    let light_binning = LightBinningPass::<B>::new::<P>(device, &mut barriers);
    let ssao = SsaoPass::<B>::new::<P>(device, resolution, &mut barriers, true);
    let rt_passes = device.supports_ray_tracing().then(|| RTPasses {
      acceleration_structure_update: AccelerationStructureUpdatePass::<B>::new(device, &mut init_cmd_buffer),
      shadows: RTShadowPass::<B>::new::<P>(device, resolution, &mut barriers)
    });
    let visibility_buffer = VisibilityBufferPass::<B>::new::<P>(device, resolution, &mut barriers);
    let draw_prep = DrawPrepPass::<B>::new::<P>(device, &mut barriers);
    let hi_z_pass = HierarchicalZPass::<B>::new::<P>(device, &mut barriers, &mut init_cmd_buffer);
    let ssr_pass = SsrPass::<B>::new::<P>(device, resolution, &mut barriers, true);
    let shading_pass = ShadingPass::<B>::new::<P>(device, resolution, &mut barriers, &mut init_cmd_buffer);
    let compositing_pass = CompositingPass::<B>::new::<P>(device, resolution, &mut barriers);
    let motion_vector_pass = MotionVectorPass::<B>::new::<P>(device, &mut barriers, resolution);

    let anti_aliasing = if Self::USE_FSR2 {
      let fsr_pass = Fsr2Pass::<B>::new::<P>(device, &mut barriers, resolution, swapchain);
      AntiAliasing::FSR2 { fsr: fsr_pass }
    } else {
      let taa = TAAPass::<B>::new::<P>(device, swapchain, &mut barriers, true);
      let sharpen = SharpenPass::<B>::new::<P>(device, swapchain, &mut barriers);
      AntiAliasing::TAA { taa, sharpen }
    };

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
      geometry_draw_prep: draw_prep,
      ssao,
      rt_passes,
      blue_noise,
      hi_z_pass,
      ssr_pass,
      visibility_buffer,
      shading_pass,
      compositing_pass,
      motion_vector_pass,
      anti_aliasing,
    }
  }

  fn setup_frame(
    &self,
    cmd_buf: &mut B::CommandBuffer,
    gpu_scene_buffer: &Arc<B::Buffer>,
    camera_buffer: &Arc<B::Buffer>,
    camera_history_buffer: &Arc<B::Buffer>,
    vertex_buffer: &Arc<B::Buffer>,
    index_buffer: &Arc<B::Buffer>,
    scene: &RendererScene<B>,
    view: &View,
    _swapchain: &Arc<B::Swapchain>,
    rendering_resolution: &Vec2UI,
    frame: u64
  ) {
    cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 0, &gpu_scene_buffer, 0, WHOLE_BUFFER);
    cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 1, &camera_buffer, 0, WHOLE_BUFFER);
    cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 2, &camera_history_buffer, 0, WHOLE_BUFFER);
    cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 3, &vertex_buffer, 0, WHOLE_BUFFER);
    cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 4, &index_buffer, 0, WHOLE_BUFFER);
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
      swapchain_transform: Matrix4::identity(), // swapchain.transform(),
      halton_point: super::taa::scaled_halton_point(rendering_resolution.x, rendering_resolution.y, (frame % 8) as u32 + 1),
      rt_size: *rendering_resolution
    }], BufferUsage::CONSTANT);
    cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 5, &setup_buffer, 0, WHOLE_BUFFER);
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
    cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 6, &point_lights_buffer, 0, WHOLE_BUFFER);
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
    cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 7, &directional_lights_buffer, 0, WHOLE_BUFFER);
  }
}

impl<B: Backend> RenderPath<B> for ModernRenderer<B> {
  fn write_occlusion_culling_results(&self, _frame: u64, bitset: &mut Vec<u32>) {
    bitset.fill(!0u32);
  }

  fn on_swapchain_changed(&mut self, swapchain: &std::sync::Arc<B::Swapchain>) {
    // TODO: resize render targets
    self.swapchain = swapchain.clone();
  }

  #[profiling::function]
  fn render(
    &mut self,
    scene: &SceneInfo<B>,
    zero_textures: &ZeroTextures<B>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input,
    frame_info: &FrameInfo,
  ) -> Result<(), SwapchainError> {
    let graphics_queue = self.device.graphics_queue();
    let mut cmd_buf = graphics_queue.create_command_buffer();

    let main_view = &scene.views[scene.active_view_index];

    let camera_buffer = late_latching.unwrap().buffer();
    let camera_history_buffer = late_latching.unwrap().history_buffer().unwrap();

    let gpu_scene_buffer = super::gpu_scene::upload(&mut cmd_buf, scene.scene, 0 /* TODO */);

    self.setup_frame(
      &mut cmd_buf,
      &gpu_scene_buffer,
      &camera_buffer,
      &camera_history_buffer,
      scene.vertex_buffer,
      scene.index_buffer,
      scene.scene,
      main_view,
      &self.swapchain,
      &Vec2UI::new(self.swapchain.width(), self.swapchain.height()),
      frame_info.frame
    );

    let resolution = {
      let info = self.barriers.texture_info(VisibilityBufferPass::<B>::BARYCENTRICS_TEXTURE_NAME);
      Vec2UI::new(info.width, info.height)
    };

    if let Some(rt_passes) = self.rt_passes.as_mut() {
      rt_passes.acceleration_structure_update.execute(&mut cmd_buf, scene.scene, &camera_buffer);
    }
    self.hi_z_pass.execute(&mut cmd_buf, &self.barriers);
    self.geometry_draw_prep.execute(&mut cmd_buf, &self.barriers, scene.scene, main_view);
    self.visibility_buffer.execute(&mut cmd_buf, &self.barriers, scene.vertex_buffer, scene.index_buffer);
    self.motion_vector_pass.execute(&mut cmd_buf, &self.barriers);
    self.clustering_pass.execute(&mut cmd_buf, resolution, main_view, &camera_buffer, &mut self.barriers);
    self.light_binning_pass.execute(&mut cmd_buf, scene.scene, &camera_buffer, &mut self.barriers);
    self.ssao.execute(&mut cmd_buf, &camera_buffer, self.blue_noise.frame(frame_info.frame), self.blue_noise.sampler(), &self.barriers, true);
    if let Some(rt_passes) = self.rt_passes.as_mut() {
      rt_passes.shadows.execute(&mut cmd_buf, rt_passes.acceleration_structure_update.acceleration_structure(),  &self.barriers, &self.blue_noise.frame(frame_info.frame), &self.blue_noise.sampler());
    }
    self.shading_pass.execute(&mut cmd_buf,  &self.device, scene.lightmap.unwrap(), zero_textures.zero_texture_view, &self.barriers);
    self.ssr_pass.execute(&mut cmd_buf, &camera_buffer, &self.barriers, true);
    self.compositing_pass.execute(&mut cmd_buf, &self.barriers);

    match &mut self.anti_aliasing {
      AntiAliasing::FSR2 { fsr } => {
        fsr.execute(&mut cmd_buf, &self.barriers, &main_view, frame_info.delta, frame_info.frame);
      }
      AntiAliasing::TAA { taa, sharpen } => {
        taa.execute(&mut cmd_buf, CompositingPass::<B>::COMPOSITION_TEXTURE_NAME, &self.barriers, true);
        sharpen.execute(&mut cmd_buf, &self.barriers);
      }
    }

    let output_texture = self.barriers.access_texture(
      &mut cmd_buf,
      TAAPass::<B>::TAA_TEXTURE_NAME,
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
    cmd_buf.blit(&*output_texture, 0, 0, back_buffer.texture(), 0, 0);
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
    std::mem::drop(output_texture);

    self.barriers.swap_history_resources();

    if let Some(late_latching) = late_latching {
      let input_state = input.poll();
      late_latching.before_submit(&input_state, main_view);
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
