use std::sync::Arc;

use sourcerenderer_core::{Matrix4, Platform, Vec2UI, atomic_refcell::AtomicRefCell, graphics::{Backend, Barrier, CommandBuffer, Device, Queue, Swapchain, SwapchainError, TextureRenderTargetView, TextureUsage, BarrierSync, BarrierAccess, TextureLayout, BufferInfo, BufferUsage, MemoryUsage, Buffer}};

use crate::{input::Input, renderer::{LateLatching, drawable::View, render_path::RenderPath, renderer_assets::RendererTexture, renderer_scene::RendererScene}};

use super::{clustering::ClusteringPass, geometry::GeometryPass, light_binning::LightBinningPass, prepass::Prepass, sharpen::SharpenPass, ssao::SsaoPass, taa::TAAPass, occlusion::OcclusionPass, rt::RayTracingPass};

pub struct DesktopRenderer<B: Backend> {
  swapchain: Arc<B::Swapchain>,
  device: Arc<B::Device>,
  clustering_pass: ClusteringPass<B>,
  light_binning_pass: LightBinningPass<B>,
  prepass: Prepass<B>,
  geometry: GeometryPass<B>,
  taa: TAAPass<B>,
  sharpen: SharpenPass<B>,
  ssao: SsaoPass<B>,
  occlusion: OcclusionPass<B>,
  rt: RayTracingPass<B>
}

impl<B: Backend> DesktopRenderer<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>) -> Self {
    let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();

    let clustering = ClusteringPass::<B>::new::<P>(device);
    let light_binning = LightBinningPass::<B>::new::<P>(device);
    let prepass = Prepass::<B>::new::<P>(device, swapchain, &mut init_cmd_buffer);
    let geometry = GeometryPass::<B>::new::<P>(device, swapchain, &mut init_cmd_buffer);
    let taa = TAAPass::<B>::new::<P>(device, swapchain, &mut init_cmd_buffer);
    let sharpen = SharpenPass::<B>::new::<P>(device, swapchain, &mut init_cmd_buffer);
    let ssao = SsaoPass::<B>::new::<P>(device, Vec2UI::new(swapchain.width(), swapchain.height()), &mut init_cmd_buffer);
    let occlusion = OcclusionPass::<B>::new::<P>(device);
    let rt = RayTracingPass::<B>::new(device, &mut init_cmd_buffer);
    device.flush_transfers();

    let c_graphics_queue = device.graphics_queue().clone();
    c_graphics_queue.submit(init_cmd_buffer.finish(), None, &[], &[], true);
    rayon::spawn(move || c_graphics_queue.process_submissions());

    device.wait_for_idle();

    Self {
      swapchain: swapchain.clone(),
      device: device.clone(),
      clustering_pass: clustering,
      light_binning_pass: light_binning,
      prepass,
      geometry,
      taa,
      sharpen,
      ssao,
      occlusion,
      rt
    }
  }
}

impl<B: Backend> RenderPath<B> for DesktopRenderer<B> {
  fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>) {
    self.occlusion.write_occlusion_query_results(frame, bitset);
  }

  fn on_swapchain_changed(&mut self, swapchain: &std::sync::Arc<B::Swapchain>) {
    // TODO: resize render targets
    self.swapchain = swapchain.clone();
  }

  fn render(&mut self,
    scene: &Arc<AtomicRefCell<RendererScene<B>>>,
    view: &Arc<AtomicRefCell<View>>,
    zero_texture_view: &Arc<B::TextureShaderResourceView>,
    lightmap: &Arc<RendererTexture<B>>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input,
    frame: u64) -> Result<(), SwapchainError> {
    let graphics_queue = self.device.graphics_queue();
    let mut cmd_buf = graphics_queue.create_command_buffer();

    println!("pre frame");

    let view_ref = view.borrow();
    let scene_ref = scene.borrow();
    self.device.wait_for_idle();

    let late_latching_buffer = late_latching.unwrap().buffer();
    let late_latching_history_buffer = late_latching.unwrap().history_buffer().unwrap();
    self.rt.update(&mut cmd_buf, &scene_ref, &late_latching_buffer);
    self.occlusion.execute(&self.device, &mut cmd_buf, frame, self.prepass.depth_dsv_history(), &late_latching_buffer, &scene_ref, &view_ref);
    self.clustering_pass.execute(&mut cmd_buf, Vec2UI::new(self.swapchain.width(), self.swapchain.height()), view, &late_latching_buffer);
    self.light_binning_pass.execute(&mut cmd_buf, &scene_ref, self.clustering_pass.clusters_buffer(), &late_latching_buffer);
    self.prepass.execute(&mut cmd_buf, &self.device, &scene_ref, &view_ref, Matrix4::identity(), frame, &late_latching_buffer, &late_latching_history_buffer);
    self.ssao.execute(&mut cmd_buf, self.prepass.normals_srv(), self.prepass.depth_srv(), &late_latching_buffer, self.prepass.motion_srv());
    self.geometry.execute(&mut cmd_buf, &self.device, &scene_ref, &view_ref, zero_texture_view, lightmap, Matrix4::identity(), frame, self.prepass.depth_dsv(), self.light_binning_pass.light_bitmask_buffer(), &late_latching_buffer, self.ssao.ssao_srv(), self.clustering_pass.clusters_buffer());
    self.taa.execute(&mut cmd_buf, self.geometry.output_srv(), self.prepass.motion_srv());
    self.sharpen.execute(&mut cmd_buf, self.taa.taa_srv());

    self.taa.swap_history_resources();
    self.ssao.swap_history_resources();
    self.prepass.swap_history_resources();

    cmd_buf.barrier(&[
        Barrier::TextureBarrier {
          old_layout: TextureLayout::Storage,
          new_layout: TextureLayout::CopySrc,
          old_sync: BarrierSync::COMPUTE_SHADER,
          new_sync: BarrierSync::COPY,
          old_access: BarrierAccess::STORAGE_WRITE,
          new_access: BarrierAccess::COPY_READ,
          texture: self.sharpen.sharpened_texture(),
        },
      ]
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
        }
    ]);
    cmd_buf.flush_barriers();
    cmd_buf.blit(self.sharpen.sharpened_texture(), 0, 0, back_buffer.texture(), 0, 0);
    cmd_buf.barrier(&[
        Barrier::TextureBarrier {
          old_sync: BarrierSync::COPY,
          new_sync: BarrierSync::empty(),
          old_access: BarrierAccess::COPY_WRITE,
          new_access: BarrierAccess::empty(),
          old_layout: TextureLayout::CopyDst,
          new_layout: TextureLayout::Present,
          texture: back_buffer.texture(),
        }
    ]);

    if let Some(late_latching) = late_latching {
      let input_state = input.poll();
      late_latching.before_submit(&input_state, &view_ref);
    }
    self.device.wait_for_idle();
    println!("submitting frame");
    graphics_queue.submit(cmd_buf.finish(), None, &[&prepare_sem], &[&cmd_buf_sem], true);
    graphics_queue.present(&self.swapchain, &[&cmd_buf_sem], true);

    let c_graphics_queue = graphics_queue.clone();
    rayon::spawn(move || c_graphics_queue.process_submissions());

    if let Some(late_latching) = late_latching {
      late_latching.after_submit(&self.device);
    }
    self.device.wait_for_idle();

    Ok(())
  }
}
