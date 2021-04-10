use std::{cmp::min, sync::{Arc, Mutex}};
use crate::renderer::Renderer;
use bitvec::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use crate::renderer::command::RendererCommand;
use std::time::{SystemTime, Duration};
use crate::asset::AssetManager;
use sourcerenderer_core::Platform;
use sourcerenderer_core::graphics::{SwapchainError, RenderGraphInfo, RenderPassCallbacks, RenderGraphTemplateInfo, PassInfo, Backend, RenderGraph, Swapchain, Device, ExternalResource};
use std::collections::HashMap;
use crate::renderer::{DrawableType, View};
use sourcerenderer_core::platform::WindowState;
use smallvec::SmallVec;
use crate::renderer::camera::LateLatchCamera;
use crate::renderer::passes;
use crate::renderer::drawable::{RDrawable, RDrawableType, DrawablePart};
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use rayon::prelude::*;
use crate::math::Frustum;

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

    device.create_render_graph(&graph_template, &RenderGraphInfo {
      pass_callbacks: callbacks
    }, swapchain, Some(&external_resources))
  }

  fn receive_messages(&mut self) {
    let mut drawables = self.drawables.borrow_mut();
    let mut view = self.view.borrow_mut();

    let message_res = self.receiver.recv();
    if message_res.is_err() {
      panic!("Rendering channel closed");
    }
    let mut message_opt = message_res.ok();

    while message_opt.is_some() {
      let message = message_opt.take().unwrap();
      match message {
        RendererCommand::EndFrame => {
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
                  model,
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

      let message_res = self.receiver.recv();
      if message_res.is_err() {
        panic!("Rendering channel closed");
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
        self.renderer.stop();
        return;
      }
    };

    self.assets.receive_assets(&self.asset_manager);
    self.receive_messages();
    self.update_visibility();
    self.reorder();

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
    self.renderer.dec_queued_frames_counter();
  }

  fn update_visibility(&mut self) {
    let drawables_ref = self.drawables.borrow();
    let mut view_mut = self.view.borrow_mut();

    let mut existing_parts = std::mem::replace(&mut view_mut.drawable_parts, Vec::new());
    // take out vector, creating a new one doesn't allocate until we push an element to it.
    existing_parts.clear();
    let visible_parts = Mutex::new(existing_parts);

    let frustum = Frustum::new(self.primary_camera.z_near(), self.primary_camera.z_far(), self.primary_camera.fov(), self.primary_camera.aspect_ratio());
    let camera_matrix = self.primary_camera.view();
    const CHUNK_SIZE: usize = 64;
    drawables_ref.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_index, chunk)| {
      let mut chunk_visible_parts = SmallVec::<[DrawablePart; 64]>::new();
      for (index, drawable) in chunk.iter().enumerate() {
        let model_view_matrix = camera_matrix * drawable.transform;
        let bounding_box = match &drawable.drawable_type {
          RDrawableType::Static { model, .. } => &model.mesh.bounding_box,
          RDrawableType::Skinned => unimplemented!()
        };
        if let Some(bounding_box) = bounding_box {
          let is_visible = frustum.intersects(bounding_box, &model_view_matrix);
          if !is_visible {
            continue;
          }
          let drawable_index = chunk_index * CHUNK_SIZE + index;
          match &drawable.drawable_type {
            RDrawableType::Static { model, .. } => {
              for part_index in 0..model.mesh.parts.len() {
                if chunk_visible_parts.len() == chunk_visible_parts.capacity() {
                  let mut global_parts = visible_parts.lock().unwrap();
                  global_parts.extend_from_slice(&chunk_visible_parts[..]);
                  chunk_visible_parts.clear();
                }

                chunk_visible_parts.push(DrawablePart {
                  drawable_index,
                  part_index
                });
              }
            }
            RDrawableType::Skinned => unimplemented!()
          }
        }
      }

      let mut global_parts = visible_parts.lock().unwrap();
      global_parts.extend_from_slice(&chunk_visible_parts[..]);
      chunk_visible_parts.clear();
    });

    view_mut.drawable_parts = visible_parts.into_inner().unwrap();
  }

  fn reorder(&mut self) {
    let drawables = self.drawables.borrow();

    let mut view_mut = self.view.borrow_mut();
    view_mut.drawable_parts.sort_by(|a, b| {
      let drawable_a = &drawables[a.drawable_index];
      let drawable_b = &drawables[b.drawable_index];

      let material_a = match &drawable_a.drawable_type {
          RDrawableType::Static { model, .. } => &model.materials[a.part_index],
          RDrawableType::Skinned => unimplemented!()
      }.as_ref();
      let material_b = match &drawable_b.drawable_type {
          RDrawableType::Static { model, .. } => &model.materials[b.part_index],
          RDrawableType::Skinned => unimplemented!()
      }.as_ref();
      return material_a.cmp(material_b);
    });
  }
}

impl<P: Platform> Drop for RendererInternal<P> {
  fn drop(&mut self) {
    self.renderer.stop();
  }
}
