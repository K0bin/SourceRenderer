use std::sync::Arc;

use sourcerenderer_core::{Matrix4, Platform, Vec2UI, atomic_refcell::AtomicRefCell, graphics::{Backend, Barrier, CommandBuffer, Device, Queue, Swapchain, SwapchainError, TextureRenderTargetView, BarrierSync, BarrierAccess, TextureLayout, BarrierTextureRange}};

use crate::{input::Input, renderer::{LateLatching, drawable::View, render_path::RenderPath, renderer_resources::{RendererResources, HistoryResourceEntry}, renderer_assets::RendererTexture, renderer_scene::RendererScene, passes::blue_noise::BlueNoise}};

use super::{clustering::ClusteringPass, geometry::GeometryPass, light_binning::LightBinningPass, prepass::Prepass, sharpen::SharpenPass, ssao::SsaoPass, taa::TAAPass, occlusion::OcclusionPass, acceleration_structure_update::AccelerationStructureUpdatePass, rt_shadows::RTShadowPass};

pub struct ConservativeRenderer<B: Backend> {
  swapchain: Arc<B::Swapchain>,
  device: Arc<B::Device>,
  barriers: RendererResources<B>,
  clustering_pass: ClusteringPass<B>,
  light_binning_pass: LightBinningPass<B>,
  prepass: Prepass<B>,
  geometry: GeometryPass<B>,
  taa: TAAPass<B>,
  sharpen: SharpenPass<B>,
  ssao: SsaoPass<B>,
  occlusion: OcclusionPass<B>,
  rt_passes: Option<RTPasses<B>>,
  blue_noise: BlueNoise<B>
}

pub struct RTPasses<B: Backend> {
  acceleration_structure_update: AccelerationStructureUpdatePass<B>,
  shadows: RTShadowPass<B>
}

impl<B: Backend> ConservativeRenderer<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>) -> Self {
    let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();
    let resolution = Vec2UI::new(swapchain.width(), swapchain.height());

    let mut barriers = RendererResources::<B>::new(device);

    let blue_noise = BlueNoise::new::<P>(device);

    let clustering = ClusteringPass::<B>::new::<P>(device, &mut barriers);
    let light_binning = LightBinningPass::<B>::new::<P>(device, &mut barriers);
    let prepass = Prepass::<B>::new::<P>(device, swapchain, &mut barriers);
    let geometry = GeometryPass::<B>::new::<P>(device, swapchain, &mut barriers);
    let taa = TAAPass::<B>::new::<P>(device, swapchain, &mut barriers, false);
    let sharpen = SharpenPass::<B>::new::<P>(device, swapchain, &mut barriers);
    let ssao = SsaoPass::<B>::new::<P>(device, resolution, &mut barriers, false);
    let occlusion = OcclusionPass::<B>::new::<P>(device);
    let rt_passes = device.supports_ray_tracing().then(|| RTPasses {
      acceleration_structure_update: AccelerationStructureUpdatePass::<B>::new(device, &mut init_cmd_buffer),
      shadows: RTShadowPass::<B>::new::<P>(device, resolution, &mut barriers)
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
}

impl<B: Backend> RenderPath<B> for ConservativeRenderer<B> {
  fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>) {
    self.occlusion.write_occlusion_query_results(frame, bitset);
  }

  fn on_swapchain_changed(&mut self, swapchain: &std::sync::Arc<B::Swapchain>) {
    // TODO: resize render targets
    self.swapchain = swapchain.clone();
  }

  #[profiling::function]
  fn render(
    &mut self,
    scene: &Arc<AtomicRefCell<RendererScene<B>>>,
    view: &Arc<AtomicRefCell<View>>,
    zero_texture_view: &Arc<B::TextureSamplingView>,
    zero_texture_view_black: &Arc<B::TextureSamplingView>,
    lightmap: &Arc<RendererTexture<B>>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input,
    frame: u64,
    _vertex_buffer: &Arc<B::Buffer>,
    _index_buffer: &Arc<B::Buffer>
  ) -> Result<(), SwapchainError> {
    let graphics_queue = self.device.graphics_queue();
    let mut cmd_buf = graphics_queue.create_command_buffer();

    let view_ref = view.borrow();
    let scene_ref = scene.borrow();

    let late_latching_buffer = late_latching.unwrap().buffer();
    let late_latching_history_buffer = late_latching.unwrap().history_buffer().unwrap();
    if let Some(rt_passes) = self.rt_passes.as_mut() {
      rt_passes.acceleration_structure_update.execute(&mut cmd_buf, &scene_ref, &late_latching_buffer);
    }
    self.occlusion.execute(&self.device, &mut cmd_buf, frame, &self.barriers, &late_latching_buffer, &scene_ref, &view_ref);
    self.clustering_pass.execute(&mut cmd_buf, Vec2UI::new(self.swapchain.width(), self.swapchain.height()), &view_ref, &late_latching_buffer, &mut self.barriers);
    self.light_binning_pass.execute(&mut cmd_buf, &scene_ref, &late_latching_buffer, &mut self.barriers);
    self.prepass.execute(&mut cmd_buf, &self.device, &scene_ref, &view_ref, Matrix4::identity(), frame, &late_latching_buffer, &late_latching_history_buffer, &self.barriers);
    self.ssao.execute(&mut cmd_buf, &late_latching_buffer, self.blue_noise.frame(frame), self.blue_noise.sampler(), &self.barriers, false);
    if let Some(rt_passes) = self.rt_passes.as_mut() {
      rt_passes.shadows.execute(&mut cmd_buf, rt_passes.acceleration_structure_update.acceleration_structure(),  &self.barriers, &self.blue_noise.frame(frame), &self.blue_noise.sampler());
    }
    self.geometry.execute(&mut cmd_buf, &self.device, &scene_ref, &view_ref, zero_texture_view, zero_texture_view_black, lightmap, Matrix4::identity(), frame, &self.barriers, &late_latching_buffer);
    self.taa.execute(&mut cmd_buf, GeometryPass::<B>::GEOMETRY_PASS_TEXTURE_NAME, &self.barriers, false);
    self.sharpen.execute(&mut cmd_buf, &self.barriers);

    let sharpened_texture = self.barriers.access_texture(
      &mut cmd_buf,
      SharpenPass::<B>::SHAPENED_TEXTURE_NAME,
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
      late_latching.before_submit(&input_state, &view_ref);
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
