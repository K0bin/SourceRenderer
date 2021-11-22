use std::sync::{Arc, Mutex, Weak};
use crate::renderer::passes::web::WebRenderer;
use crate::renderer::{Renderer, RendererStaticDrawable};
use crate::transform::interpolation::deconstruct_transform;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::trace;
use crate::renderer::command::RendererCommand;
use std::time::Duration;
use crate::asset::AssetManager;
use sourcerenderer_core::{Platform, Vec2UI, Vec3, Vec4};
use sourcerenderer_core::graphics::{SwapchainError, Backend,Swapchain, Device};
use crate::renderer::View;
use sourcerenderer_core::platform::Event;
use smallvec::SmallVec;
use crate::renderer::drawable::DrawablePart;
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use rayon::prelude::*;
use crate::math::Frustum;
use instant::Instant;

use super::PointLight;
use super::drawable::{make_camera_proj, make_camera_view};
use super::light::DirectionalLight;
use super::passes::desktop::desktop_renderer::DesktopRenderer;
use super::render_path::RenderPath;
use super::renderer_scene::RendererScene;

pub(super) struct RendererInternal<P: Platform> {
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
  render_path: Box<dyn RenderPath<P::GraphicsBackend>>,
  asset_manager: Arc<AssetManager<P>>,
  lightmap: Arc<RendererTexture<P::GraphicsBackend>>,
  scene: Arc<AtomicRefCell<RendererScene<P::GraphicsBackend>>>,
  view: Arc<AtomicRefCell<View>>,
  sender: Sender<RendererCommand>,
  receiver: Receiver<RendererCommand>,
  window_event_receiver: Receiver<Event<P>>,
  last_tick: Instant,
  assets: RendererAssets<P>
}

impl<P: Platform> RendererInternal<P> {
  pub(super) fn new(
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    sender: Sender<RendererCommand>,
    window_event_receiver: Receiver<Event<P>>,
    receiver: Receiver<RendererCommand>) -> Self {

    let mut assets = RendererAssets::new(device);
    let lightmap = assets.insert_placeholder_texture("lightmap");

    let scene = Arc::new(AtomicRefCell::new(RendererScene::new()));
    let view = Arc::new(AtomicRefCell::new(View::default()));

    let path: Box<dyn RenderPath<P::GraphicsBackend>> = //if cfg!(target_family = "wasm") {
      Box::new(WebRenderer::new::<P>(device, swapchain))
    /*} else {
      Box::new(DesktopRenderer::new::<P>(device, swapchain))
    }*/;

    Self {
      device: device.clone(),
      render_path: path,
      swapchain: swapchain.clone(),
      scene,
      asset_manager: asset_manager.clone(),
      view,
      sender,
      receiver,
      window_event_receiver,
      last_tick: Instant::now(),
      assets,
      lightmap
    }
  }

  fn receive_window_events(&mut self) -> bool {
    let mut window_message_res = self.window_event_receiver.try_recv();

    let mut new_surface = Option::<Arc<<P::GraphicsBackend as Backend>::Surface>>::None;
    let mut new_size = Option::<Vec2UI>::None;

    while window_message_res.is_ok() {
      match window_message_res.unwrap() {
        Event::WindowMinimized => {
          std::thread::sleep(Duration::new(1, 0));
        }
        Event::WindowRestored(size) => {
          new_size = Some(size);
        }
        Event::WindowSizeChanged(size) => {
          new_size = Some(size);
        }
        Event::SurfaceChanged(surface) => {
          new_surface = Some(surface);
        }
        _ => unreachable!()
      }
      window_message_res = self.window_event_receiver.try_recv();
    }
    if let Result::Err(err) = &window_message_res {
      if let TryRecvError::Disconnected = err {
        panic!("Rendering window event channel closed {:?}", err);
      }
    }

    if new_surface.is_some() || new_size.is_some() {
      // We need to recreate the swapchain
      let size = new_size.unwrap_or_else(|| Vec2UI::new(self.swapchain.width(), self.swapchain.height()));
      let surface = new_surface.unwrap_or_else(|| self.swapchain.surface().clone());

      self.device.wait_for_idle();
      let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate_on_surface(&self.swapchain, &surface, size.x, size.y);
      if let Result::Err(error) = new_swapchain_result {
        trace!("Swapchain recreation failed: {:?}", error);
      } else {
        self.swapchain = new_swapchain_result.unwrap();
      }
      self.render_path.on_swapchain_changed(&self.swapchain);
      true
    } else {
      false
    }
  }

  fn receive_messages(&mut self) {
    let mut scene = self.scene.borrow_mut();
    let mut view = self.view.borrow_mut();

    let message_res = self.receiver.recv();
    if let Result::Err(err) = &message_res {
      panic!("Rendering channel closed {:?}", err);
    }
    let mut message_opt = message_res.ok();

    while message_opt.is_some() {
      let message = message_opt.take().unwrap();
      match message {
        RendererCommand::EndFrame => {
          self.last_tick = Instant::now();
          break;
        }

        RendererCommand::UpdateCameraTransform { camera_transform_mat, fov } => {
          view.camera_transform = camera_transform_mat;
          view.camera_fov = fov;
          view.old_camera_matrix = view.proj_matrix * view.view_matrix;
          let (position, rotation, _) = deconstruct_transform(&camera_transform_mat);
          view.view_matrix = make_camera_view(position, rotation);
          view.proj_matrix = make_camera_proj(view.camera_fov, view.aspect_ratio, view.near_plane, view.far_plane)
        }

        RendererCommand::UpdateTransform { entity, transform_mat } => {
          scene.update_transform(&entity, transform_mat);
        }

        RendererCommand::RegisterStatic {
          model_path, entity, transform, receive_shadows, cast_shadows, can_move
         } => {
          let model = self.assets.get_model(&model_path);
          scene.add_static_drawable(entity, RendererStaticDrawable::<P::GraphicsBackend> {
            entity,
            transform,
            old_transform: transform,
            model,
            receive_shadows,
            cast_shadows,
            can_move
          });
        }
        RendererCommand::UnregisterStatic(entity) => {
          scene.remove_static_drawable(&entity);
        }

        RendererCommand::RegisterPointLight {
          entity,
          transform,
          intensity
        } => {
          scene.add_point_light(entity, PointLight {
            position: (transform * Vec4::new(0f32, 0f32, 0f32, 1f32)).xyz(),
            intensity,
          });
        },
        RendererCommand::UnregisterPointLight(entity) => {
          scene.remove_point_light(&entity);
        },

        RendererCommand::RegisterDirectionalLight {
          entity,
          transform,
          intensity
        } => {
          let (_, rotation, _) = deconstruct_transform(&transform);
          let base_dir = Vec3::new(0f32, -1f32, 0f32);
          let dir = rotation.transform_vector(&base_dir);
          scene.add_directional_light(entity, DirectionalLight { direction: dir, intensity: intensity});
        },
        RendererCommand::UnregisterDirectionalLight(entity) => {
          scene.remove_directional_light(&entity);
        },
      }

      let message_res = self.receiver.recv();
      if message_res.is_err() {
        panic!("Rendering channel closed");
      }
      message_opt = message_res.ok();
    }
  }

  pub(super) fn render(&mut self, renderer: &Renderer<P>) {
    self.receive_window_events();
    self.assets.receive_assets(&self.asset_manager);
    self.receive_messages();
    self.update_visibility();
    self.reorder();

    self.lightmap = self.assets.get_texture("lightmap");

    let render_result = self.render_path.render(&self.scene, &self.view, self.assets.zero_view(), &self.lightmap, renderer.late_latching(), renderer.input());
    if let Err(swapchain_error) = render_result {
      self.device.wait_for_idle();

      // Recheck window events
      if !self.receive_window_events() {
        let new_swapchain = if swapchain_error == SwapchainError::SurfaceLost {
          // No point in trying to recreate with the old surface
          let renderer_surface = renderer.surface();
          if &*renderer_surface != self.swapchain.surface() {
            trace!("Recreating swapchain on a different surface");
            let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate_on_surface(&self.swapchain, &*renderer_surface, self.swapchain.width(), self.swapchain.height());
            if new_swapchain_result.is_err() {
              trace!("Swapchain recreation failed: {:?}", new_swapchain_result.err().unwrap());
              return;
            }
            new_swapchain_result.unwrap()
          } else {
            return;
          }
        } else {
          trace!("Recreating swapchain");
          let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate(&self.swapchain, self.swapchain.width(), self.swapchain.height());
          if new_swapchain_result.is_err() {
            trace!("Swapchain recreation failed: {:?}", new_swapchain_result.err().unwrap());
            return;
          }
          new_swapchain_result.unwrap()
        };
        self.render_path.on_swapchain_changed(&new_swapchain);
        self.render_path.render(&self.scene, &self.view, self.assets.zero_view(), &self.lightmap, renderer.late_latching(), renderer.input()).expect("Rendering still fails after recreating swapchain.");
        self.swapchain = new_swapchain;
      }
    }
    renderer.dec_queued_frames_counter();
  }

  fn update_visibility(&mut self) {
    let scene = self.scene.borrow();
    let static_meshes = scene.static_drawables();

    let mut view_mut = self.view.borrow_mut();

    let mut existing_parts = std::mem::replace(&mut view_mut.drawable_parts, Vec::new());
    // take out vector, creating a new one doesn't allocate until we push an element to it.
    existing_parts.clear();
    let visible_parts = Mutex::new(existing_parts);

    let frustum = Frustum::new(view_mut.near_plane, view_mut.far_plane, view_mut.camera_fov, view_mut.aspect_ratio);
    let camera_matrix = view_mut.view_matrix;
    const CHUNK_SIZE: usize = 64;
    static_meshes.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_index, chunk)| {
      let mut chunk_visible_parts = SmallVec::<[DrawablePart; 64]>::new();
      for (index, static_mesh) in chunk.iter().enumerate() {
        let model_view_matrix = camera_matrix * static_mesh.transform;
        let model = &static_mesh.model;
        let mesh = model.mesh();
        let bounding_box = &mesh.bounding_box;
        let is_visible = if let Some(bounding_box) = bounding_box {
          frustum.intersects(bounding_box, &model_view_matrix)
        } else {
          true
        };
        if !is_visible {
          continue;
        }
        let drawable_index = chunk_index * CHUNK_SIZE + index;
        for part_index in 0..mesh.parts.len() {
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
        let materials_a = static_mesh_a.model.materials();
        let materials_b = static_mesh_b.model.materials();
        let material_a = &materials_a[a.part_index];
        let material_b = &materials_b[b.part_index];
        material_a.cmp(material_b)
      }
    });
  }
}
