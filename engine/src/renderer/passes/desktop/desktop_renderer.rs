use std::sync::Arc;

use sourcerenderer_core::{Matrix4, Platform, Vec2UI, atomic_refcell::AtomicRefCell, graphics::{Backend, Barrier, CommandBuffer, Device, Queue, Swapchain, SwapchainError, TextureRenderTargetView, TextureUsage}};

use crate::{input::Input, renderer::{LateLatchCamera, LateLatching, Renderer, drawable::View, render_path::RenderPath, renderer_assets::RendererTexture, renderer_scene::RendererScene}};

use super::{clustering::ClusteringPass, geometry::GeometryPass, light_binning::LightBinningPass, prepass::Prepass, sharpen::SharpenPass, ssao::SsaoPass, taa::TAAPass};

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
  frame: u64
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

    device.graphics_queue().submit(init_cmd_buffer.finish(), None, &[], &[]);

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
      frame: 0
    }
  }
}

impl<B: Backend> RenderPath<B> for DesktopRenderer<B> {
  fn on_swapchain_changed(&mut self, _swapchain: &std::sync::Arc<B::Swapchain>) {
    todo!()
  }

  fn render(&mut self,
    scene: &Arc<AtomicRefCell<RendererScene<B>>>,
    view: &Arc<AtomicRefCell<View>>,
    lightmap: &Arc<RendererTexture<B>>,
    late_latching: Option<&dyn LateLatching<B>>,
    input: &Input) -> Result<(), SwapchainError> {
    let graphics_queue = self.device.graphics_queue();
    let mut cmd_buf = graphics_queue.create_command_buffer();

    let view_ref = view.borrow();
    let scene_ref = scene.borrow();
    let late_latching_buffer = late_latching.unwrap().buffer();
    let late_latching_history_buffer = late_latching.unwrap().history_buffer().unwrap();
    self.clustering_pass.execute(&mut cmd_buf, Vec2UI::new(self.swapchain.width(), self.swapchain.height()), 0.1f32, 10f32,&late_latching_buffer);
    self.light_binning_pass.execute(&mut cmd_buf, &scene_ref, self.clustering_pass.clusters_buffer(), &late_latching_buffer);
    self.prepass.execute(&mut cmd_buf, &self.device, &scene_ref, &view_ref, Matrix4::identity(), self.frame, &late_latching_buffer, &late_latching_history_buffer);
    self.ssao.execute(&mut cmd_buf, self.prepass.normals_srv(), self.prepass.depth_srv(), &late_latching_buffer, self.prepass.motion_srv());
    self.geometry.execute(&mut cmd_buf, &self.device, &scene_ref, &view_ref, lightmap, Matrix4::identity(), self.frame, self.prepass.depth_dsv(), self.light_binning_pass.light_bitmask_buffer(), &late_latching_buffer, self.ssao.ssao_srv());
    self.taa.execute(&mut cmd_buf, self.geometry.output_srv(), self.prepass.motion_srv());
    self.sharpen.execute(&mut cmd_buf, self.taa.taa_srv());

    self.taa.swap_history_resources();
    self.ssao.swap_history_resources();

    cmd_buf.barrier(&[
        Barrier::TextureBarrier {
          old_primary_usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
          new_primary_usage: TextureUsage::COPY_SRC,
          old_usages: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
          new_usages: TextureUsage::COPY_SRC,
          texture: self.sharpen.sharpened_texture(),
        }
      ]
    );

    let prepare_sem = self.device.create_semaphore();
    let cmd_buf_sem = self.device.create_semaphore();
    self.frame += 1;
    let back_buffer_res = self.swapchain.prepare_back_buffer(&prepare_sem);
    if back_buffer_res.is_none() {
      return Err(SwapchainError::Other);
    }

    let back_buffer = back_buffer_res.unwrap();

    cmd_buf.barrier(
      &[
        Barrier::TextureBarrier {
          old_primary_usage: TextureUsage::UNINITIALIZED,
          new_primary_usage: TextureUsage::COPY_DST,
          old_usages: TextureUsage::empty(),
          new_usages: TextureUsage::empty(),
          texture: back_buffer.texture(),
        }
      ]
    );
    cmd_buf.flush_barriers();
    cmd_buf.blit(self.sharpen.sharpened_texture(), 0, 0, back_buffer.texture(), 0, 0);
    cmd_buf.barrier(
      &[
        Barrier::TextureBarrier {
          old_primary_usage: TextureUsage::COPY_DST,
          new_primary_usage: TextureUsage::PRESENT,
          old_usages: TextureUsage::empty(),
          new_usages: TextureUsage::empty(),
          texture: back_buffer.texture(),
        }
      ]
    );

    if let Some(late_latching) = late_latching {
      let input_state = input.poll();
      late_latching.before_submit(&input_state, &view_ref);
    }
    graphics_queue.submit(cmd_buf.finish(), None, &[&prepare_sem], &[&cmd_buf_sem]);
    graphics_queue.present(&self.swapchain, &[&cmd_buf_sem]);

    if let Some(late_latching) = late_latching {
      late_latching.after_submit(&self.device);
    }

    return Ok(());
  }
}