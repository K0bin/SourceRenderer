use std::sync::Arc;
use crate::renderer::Renderer;
use crossbeam_channel::{Sender, Receiver, TryRecvError};
use crate::renderer::command::RendererCommand;
use std::time::{SystemTime, Duration};
use crate::asset::AssetManager;
use sourcerenderer_core::Platform;
use sourcerenderer_core::graphics::{SwapchainError, RenderGraphInfo, RenderPassCallbacks, RenderGraphTemplateInfo, PassInfo, Backend, RenderGraph, Swapchain, Device, ExternalResource};
use std::collections::HashMap;
use crate::renderer::{DrawableType, View};
use sourcerenderer_core::platform::WindowState;

use crate::renderer::camera::LateLatchCamera;
use crate::renderer::passes;
use crate::renderer::drawable::{RDrawable, RDrawableType};
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;

pub(super) struct RendererInternal<P: Platform> {
  renderer: Arc<Renderer<P>>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  graph: <P::GraphicsBackend as Backend>::RenderGraph,
  asset_manager: Arc<AssetManager<P>>,
  lightmap: Arc<RendererTexture<P::GraphicsBackend>>,
  drawables: Arc<AtomicRefCell<Vec<RDrawable<P::GraphicsBackend>>>>,
  view: Arc<AtomicRefCell<View>>,
  sender: Sender<RendererCommand>,
  receiver: Receiver<RendererCommand>,
  last_tick: SystemTime,
  primary_camera: Arc<LateLatchCamera<P::GraphicsBackend>>,
  assets: RendererAssets<P>
}

impl<P: Platform> RendererInternal<P> {
  pub(super) fn new(
    renderer: &Arc<Renderer<P>>,
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    sender: Sender<RendererCommand>,
    receiver: Receiver<RendererCommand>,
    primary_camera: &Arc<LateLatchCamera<P::GraphicsBackend>>) -> Self {

    let mut assets = RendererAssets::new(device);
    let lightmap = assets.insert_placeholder_texture("lightmap");

    let drawables = Arc::new(AtomicRefCell::new(Vec::new()));
    let view = Arc::new(AtomicRefCell::new(View::default()));
    let graph = RendererInternal::<P>::build_graph(device, swapchain, &view, &drawables, &lightmap, &primary_camera);

    Self {
      renderer: renderer.clone(),
      device: device.clone(),
      graph,
      asset_manager: asset_manager.clone(),
      view,
      sender,
      receiver,
      last_tick: SystemTime::now(),
      primary_camera: primary_camera.clone(),
      assets,
      lightmap,
      drawables
    }
  }

  fn build_graph(
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    view: &Arc<AtomicRefCell<View>>,
    drawables: &Arc<AtomicRefCell<Vec<RDrawable<P::GraphicsBackend>>>>,
    lightmap: &Arc<RendererTexture<P::GraphicsBackend>>,
    primary_camera: &Arc<LateLatchCamera<P::GraphicsBackend>>)
    -> <P::GraphicsBackend as Backend>::RenderGraph {

    let passes: Vec<PassInfo> = vec![
      passes::late_latching::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::prepass::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::geometry::build_pass_template::<P::GraphicsBackend>()
    ];

    let external_resources = vec![
      passes::late_latching::external_resource_template()
    ];

    let graph_template = device.create_render_graph_template(&RenderGraphTemplateInfo {
      external_resources,
      passes,
      swapchain_sample_count: swapchain.sample_count(),
      swapchain_format: swapchain.format()
    });

    let mut callbacks: HashMap<String, RenderPassCallbacks<P::GraphicsBackend>> = HashMap::new();
    let (pre_pass_name, pre_pass_callback) = passes::desktop::prepass::build_pass::<P>(device, &graph_template, &view, &drawables);
    callbacks.insert(pre_pass_name, pre_pass_callback);

    let (geometry_pass_name, geometry_pass_callback) = passes::desktop::geometry::build_pass::<P>(device, &graph_template, &view, &drawables, &lightmap);
    callbacks.insert(geometry_pass_name, geometry_pass_callback);

    let (late_latch_pass_name, late_latch_pass_callback) = passes::late_latching::build_pass::<P>(device);
    callbacks.insert(late_latch_pass_name, late_latch_pass_callback);

    let mut external_resources = HashMap::<String, ExternalResource<P::GraphicsBackend>>::new();
    let (camera_resource_name, camera_resource) = passes::late_latching::external_resource(primary_camera);
    external_resources.insert(camera_resource_name, camera_resource);

    let graph = device.create_render_graph(&graph_template, &RenderGraphInfo {
      pass_callbacks: callbacks
    }, swapchain, Some(&external_resources));

    graph
  }

  fn receive_messages(&mut self) {
    let mut drawables = self.drawables.borrow_mut();
    let mut view = self.view.borrow_mut();

    let message_res = self.receiver.try_recv();
    if let Some(err) = message_res.as_ref().err() {
      if let TryRecvError::Disconnected = err {
        panic!("Rendering channel closed");
      }
    }
    let mut message_opt = message_res.ok();

    while message_opt.is_some() {
      let message = std::mem::replace(&mut message_opt, None).unwrap();
      match message {
        RendererCommand::EndFrame => {
          self.renderer.dec_queued_frames_counter();
          self.last_tick = SystemTime::now();
          break;
        }

        RendererCommand::UpdateCameraTransform { camera_transform_mat, fov } => {
          view.camera_transform = camera_transform_mat;
          view.camera_fov = fov;

          view.old_camera_matrix = view.camera_matrix;
          let position = camera_transform_mat.column(3).xyz();
          self.primary_camera.update_position(position);
          view.camera_matrix = self.primary_camera.get_camera();
        }

        RendererCommand::UpdateTransform { entity, transform_mat } => {
          let element = drawables.iter_mut()
            .find(|r| r.entity == entity);
          // TODO optimize

          if let Some(element) = element {
            element.old_transform = element.transform;
            element.transform = transform_mat;
          }
        }

        RendererCommand::Register(drawable) => {
          drawables.push(RDrawable {
            drawable_type: match &drawable.drawable_type {
              DrawableType::Static {
                model_path, receive_shadows, cast_shadows, can_move
              } => {
                let model = self.assets.get_model(model_path);
                RDrawableType::Static {
                  model: model,
                  receive_shadows: *receive_shadows,
                  cast_shadows: *cast_shadows,
                  can_move: *can_move
                }
              }
              _ => unimplemented!()
            },
            entity: drawable.entity,
            transform: drawable.transform,
            old_transform: drawable.transform
          });
        }

        RendererCommand::UnregisterStatic(entity) => {
          let index = drawables.iter()
            .position(|r| r.entity == entity);

          if let Some(index) = index {
            drawables.remove(index);
          }
        }
      }

      let message_res = self.receiver.try_recv();
      if let Some(err) = message_res.as_ref().err() {
        if let TryRecvError::Disconnected = err {
          panic!("Rendering channel closed");
        }
      }
      message_opt = message_res.ok();
    }
  }

  pub(super) fn render(&mut self) {
    let state = {
      let state_guard = self.renderer.window_state().lock().unwrap();
      state_guard.clone()
    };

    let (swapchain_width, swapchain_height) = match state {
      WindowState::Minimized => {
        std::thread::sleep(Duration::new(1, 0));
        return;
      },
      WindowState::FullScreen {
        width, height
      } => {
        (width, height)
      },
      WindowState::Visible {
        width, height, focussed: _focussed
      } => {
        (width, height)
      },
      WindowState::Exited => {
        return;
      }
    };

    self.assets.receive_assets(&self.asset_manager);
    self.receive_messages();

    let result = self.graph.render();
    if result.is_err() {
      self.device.wait_for_idle();

      let new_swapchain = if result.err().unwrap() == SwapchainError::SurfaceLost {
        // No point in trying to recreate with the old surface
        let renderer_surface = self.renderer.surface();
        if &*renderer_surface != self.graph.swapchain().surface() {
          println!("Recreating swapchain on a different surface");
          let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate_on_surface(&self.graph.swapchain(), &*renderer_surface, swapchain_width, swapchain_height);
          if new_swapchain_result.is_err() {
            println!("Swapchain recreation failed: {:?}", new_swapchain_result.err().unwrap());
            return;
          }
          new_swapchain_result.unwrap()
        } else {
          return;
        }
      } else {
        println!("Recreating swapchain");
        let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate(&self.graph.swapchain(), swapchain_width, swapchain_height);
        if new_swapchain_result.is_err() {
          println!("Swapchain recreation failed: {:?}", new_swapchain_result.err().unwrap());
          return;
        }
        new_swapchain_result.unwrap()
      };

      if new_swapchain.format() != self.graph.swapchain().format() || new_swapchain.sample_count() != self.graph.swapchain().sample_count() {
        panic!("Swapchain format or sample count changed. Can not recreate render graph.");
      }
      let new_graph = <P::GraphicsBackend as Backend>::RenderGraph::recreate(&self.graph, &new_swapchain);
      self.graph = new_graph;
      let _ = self.graph.render();
    }
  }
}
