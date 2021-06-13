use std::sync::{Arc, Mutex};
use crate::renderer::{Renderer, RendererStaticDrawable};
use crossbeam_channel::{Receiver, Sender};
use crate::renderer::command::RendererCommand;
use std::time::{SystemTime, Duration};
use crate::asset::AssetManager;
use sourcerenderer_core::Platform;
use sourcerenderer_core::graphics::{SwapchainError, RenderGraphInfo, RenderPassCallbacks, RenderGraphTemplateInfo, PassInfo, Backend, RenderGraph, Swapchain, Device, ExternalResource};
use std::collections::HashMap;
use crate::renderer::View;
use sourcerenderer_core::platform::WindowState;
use smallvec::SmallVec;
use crate::renderer::camera::LateLatchCamera;
use crate::renderer::passes;
use crate::renderer::drawable::DrawablePart;
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use rayon::prelude::*;
use crate::math::Frustum;

use super::renderer_scene::RendererScene;

pub(super) struct RendererInternal<P: Platform> {
  renderer: Arc<Renderer<P>>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  graph: <P::GraphicsBackend as Backend>::RenderGraph,
  asset_manager: Arc<AssetManager<P>>,
  lightmap: Arc<RendererTexture<P::GraphicsBackend>>,
  scene: Arc<AtomicRefCell<RendererScene<P::GraphicsBackend>>>,
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

    let scene = Arc::new(AtomicRefCell::new(RendererScene::new()));
    let view = Arc::new(AtomicRefCell::new(View::default()));
    let graph = RendererInternal::<P>::build_graph(device, swapchain, &view, &scene, &lightmap, &primary_camera);

    Self {
      renderer: renderer.clone(),
      device: device.clone(),
      graph,
      scene,
      asset_manager: asset_manager.clone(),
      view,
      sender,
      receiver,
      last_tick: SystemTime::now(),
      primary_camera: primary_camera.clone(),
      assets,
      lightmap
    }
  }

  fn build_graph(
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    view: &Arc<AtomicRefCell<View>>,
    scene: &Arc<AtomicRefCell<RendererScene<P::GraphicsBackend>>>,
    lightmap: &Arc<RendererTexture<P::GraphicsBackend>>,
    primary_camera: &Arc<LateLatchCamera<P::GraphicsBackend>>)
    -> <P::GraphicsBackend as Backend>::RenderGraph {

    let passes: Vec<PassInfo> = vec![
      passes::late_latching::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::prepass::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::geometry::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::taa::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::sharpen::build_pass_template::<P::GraphicsBackend>(),
      passes::desktop::blit::build_blit_pass_template::<P::GraphicsBackend>(),
      passes::desktop::clustering::build_pass_template::<P::GraphicsBackend>(),
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
    let (pre_pass_name, pre_pass_callback) = passes::desktop::prepass::build_pass::<P>(device, &graph_template, &view, scene);
    callbacks.insert(pre_pass_name, pre_pass_callback);

    let (clustering_pass_name, clustering_pass_callback) = passes::desktop::clustering::build_pass::<P>(device, &view);
    callbacks.insert(clustering_pass_name, clustering_pass_callback);

    let (geometry_pass_name, geometry_pass_callback) = passes::desktop::geometry::build_pass::<P>(device, &graph_template, &view, scene, &lightmap);
    callbacks.insert(geometry_pass_name, geometry_pass_callback);

    let (late_latch_pass_name, late_latch_pass_callback) = passes::late_latching::build_pass::<P>(device);
    callbacks.insert(late_latch_pass_name, late_latch_pass_callback);

    let (taa_pass_name, taa_pass_callback) = passes::desktop::taa::build_pass::<P>(device);
    callbacks.insert(taa_pass_name, taa_pass_callback);

    let (sharpen_pass_name, sharpen_pass_callback) = passes::desktop::sharpen::build_pass::<P>(device);
    callbacks.insert(sharpen_pass_name, sharpen_pass_callback);

    let (blit_pass_name, blit_pass_callback) = passes::desktop::blit::build_blit_pass::<P>();
    callbacks.insert(blit_pass_name, blit_pass_callback);

    let mut external_resources = HashMap::<String, ExternalResource<P::GraphicsBackend>>::new();
    let (camera_resource_name, camera_resource) = passes::late_latching::external_resource(primary_camera);
    external_resources.insert(camera_resource_name, camera_resource);

    device.create_render_graph(&graph_template, &RenderGraphInfo {
      pass_callbacks: callbacks
    }, swapchain, Some(&external_resources))
  }

  fn receive_messages(&mut self) {
    let mut scene = self.scene.borrow_mut();
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

          view.old_camera_matrix = view.proj_matrix * view.view_matrix;
          let position = camera_transform_mat.column(3).xyz();
          self.primary_camera.update_position(position);
          view.view_matrix = self.primary_camera.view();
          view.proj_matrix = self.primary_camera.proj();
        }

        RendererCommand::UpdateTransform { entity, transform_mat } => {
          scene.update_transform(&entity, transform_mat);
        }

        RendererCommand::RegisterStatic(drawable) => {
          let model = self.assets.get_model(&drawable.model_path);
          scene.add_static_drawable(drawable.entity, RendererStaticDrawable::<P::GraphicsBackend> {
            entity: drawable.entity,
            transform: drawable.transform,
            old_transform: drawable.transform,
            model,
            receive_shadows: drawable.receive_shadows,
            cast_shadows: drawable.cast_shadows,
            can_move: drawable.can_move
          });
        }

        RendererCommand::UnregisterStatic(entity) => {
          scene.remove_static_drawable(&entity);
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
    let scene = self.scene.borrow();
    let static_meshes = scene.static_drawables();

    let mut view_mut = self.view.borrow_mut();

    let mut existing_parts = std::mem::replace(&mut view_mut.drawable_parts, Vec::new());
    // take out vector, creating a new one doesn't allocate until we push an element to it.
    existing_parts.clear();
    let visible_parts = Mutex::new(existing_parts);

    let frustum = Frustum::new(self.primary_camera.z_near(), self.primary_camera.z_far(), self.primary_camera.fov(), self.primary_camera.aspect_ratio());
    let camera_matrix = self.primary_camera.view();
    const CHUNK_SIZE: usize = 64;
    static_meshes.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_index, chunk)| {
      let mut chunk_visible_parts = SmallVec::<[DrawablePart; 64]>::new();
      for (index, static_mesh) in chunk.iter().enumerate() {
        let model_view_matrix = camera_matrix * static_mesh.transform;
        let model = &static_mesh.model;
        let bounding_box = &model.mesh.bounding_box;
        if let Some(bounding_box) = bounding_box {
          let is_visible = frustum.intersects(bounding_box, &model_view_matrix);
          if !is_visible {
            continue;
          }
          let drawable_index = chunk_index * CHUNK_SIZE + index;
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
      }

      let mut global_parts = visible_parts.lock().unwrap();
      global_parts.extend_from_slice(&chunk_visible_parts[..]);
      chunk_visible_parts.clear();
    });

    view_mut.drawable_parts = visible_parts.into_inner().unwrap();
  }

  fn reorder(&mut self) {
    let scene = self.scene.borrow();
    let static_meshes = scene.static_drawables();

    let mut view_mut = self.view.borrow_mut();
    view_mut.drawable_parts.sort_by(|a, b| {
      // if the drawable index is greater than the amount of static meshes, it is a skinned mesh
      let b_is_skinned = a.drawable_index > static_meshes.len();
      let a_is_skinned = a.drawable_index > static_meshes.len();
      return if b_is_skinned && a_is_skinned {
        unimplemented!()
      } else if b_is_skinned {
        std::cmp::Ordering::Less
      } else if a_is_skinned {
        std::cmp::Ordering::Greater
      } else {
        let static_mesh_a = &static_meshes[a.drawable_index];
        let static_mesh_b = &static_meshes[b.drawable_index];
        let material_a = &static_mesh_a.model.materials[a.part_index];
        let material_b = &static_mesh_b.model.materials[a.part_index];
        material_a.cmp(material_b)
      }
    });
  }
}

impl<P: Platform> Drop for RendererInternal<P> {
  fn drop(&mut self) {
    self.renderer.stop();
  }
}
