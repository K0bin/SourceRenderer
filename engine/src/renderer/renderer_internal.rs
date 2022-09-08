use std::sync::{Arc, Mutex};
use crate::renderer::passes::web::WebRenderer;
use crate::renderer::render_path::{FrameInfo, SceneInfo, ZeroTextures};
use crate::renderer::{Renderer, RendererStaticDrawable};
use crate::transform::interpolation::deconstruct_transform;
use bitset_core::BitSet;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::trace;
use crate::renderer::command::RendererCommand;
use std::time::Duration;
use crate::asset::AssetManager;
use sourcerenderer_core::{Platform, Vec2UI, Vec3, Vec4, Matrix4, Console};
use sourcerenderer_core::graphics::{SwapchainError, Backend,Swapchain, Device};
use crate::renderer::View;
use sourcerenderer_core::platform::Event;
use smallvec::SmallVec;
use crate::renderer::drawable::DrawablePart;
use crate::renderer::renderer_assets::*;
use rayon::prelude::*;
use crate::math::{Frustum, BoundingBox};
use instant::Instant;

use super::PointLight;
use super::drawable::{make_camera_proj, make_camera_view};
use super::light::DirectionalLight;
use super::render_path::RenderPath;
use super::renderer_scene::RendererScene;

#[cfg(not(target_arch = "wasm32"))]
use super::passes::modern::ModernRenderer;
#[cfg(not(target_arch = "wasm32"))]
use super::passes::conservative::desktop_renderer::ConservativeRenderer;
use super::shader_manager::ShaderManager;

pub(super) struct RendererInternal<P: Platform> {
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
  render_path: Box<dyn RenderPath<P>>,
  asset_manager: Arc<AssetManager<P>>,
  scene: RendererScene<P::GraphicsBackend>,
  views: Vec<View>,
  sender: Sender<RendererCommand>,
  receiver: Receiver<RendererCommand>,
  window_event_receiver: Receiver<Event<P>>,
  last_frame: Instant,
  frame: u64,
  assets: RendererAssets<P>,
  console: Arc<Console>,
  shader_manager: ShaderManager<P>,
}

impl<P: Platform> RendererInternal<P> {
  pub(super) fn new(
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    sender: Sender<RendererCommand>,
    window_event_receiver: Receiver<Event<P>>,
    receiver: Receiver<RendererCommand>,
    console: &Arc<Console>) -> Self {

    let assets = RendererAssets::new(device);

    let mut shader_manager = ShaderManager::<P>::new(device, asset_manager);

    let scene = RendererScene::new();
    let view = View::default();
    let views = vec![view];

    #[cfg(target_arch = "wasm32")]
    let path = Box::new(WebRenderer::new::<P>(device, swapchain));

    #[cfg(not(target_arch = "wasm32"))]
    let path: Box<dyn RenderPath<P>> = if cfg!(target_family = "wasm") {
      Box::new(WebRenderer::new(device, swapchain, &mut shader_manager))
    } else {
      if device.supports_indirect() && device.supports_bindless() && device.supports_barycentrics() {
        Box::new(ModernRenderer::new(device, swapchain, &mut shader_manager))
      } else {
        Box::new(ConservativeRenderer::new(device, swapchain, &mut shader_manager))
      }
    };

    Self {
      device: device.clone(),
      render_path: path,
      swapchain: swapchain.clone(),
      scene,
      asset_manager: asset_manager.clone(),
      views,
      sender,
      receiver,
      window_event_receiver,
      last_frame: Instant::now(),
      assets,
      frame: 0,
      console: console.clone(),
      shader_manager
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
    let message_res = self.receiver.recv();

    // recv blocks, so do the preparation after receiving the first event
    self.receive_window_events();
    self.assets.receive_assets(&self.asset_manager, &mut self.shader_manager);

    let main_view = &mut self.views[0];

    if let Result::Err(err) = &message_res {
      panic!("Rendering channel closed {:?}", err);
    }
    let mut message_opt = message_res.ok();

    while message_opt.is_some() {
      let message = message_opt.take().unwrap();
      match message {
        RendererCommand::EndFrame => {
          break;
        }

        RendererCommand::UpdateCameraTransform { camera_transform_mat, fov } => {
          main_view.camera_transform = camera_transform_mat;
          main_view.camera_fov = fov;
          main_view.old_camera_matrix = main_view.proj_matrix * main_view.view_matrix;
          let (position, rotation, _) = deconstruct_transform(&camera_transform_mat);
          main_view.camera_position = position;
          main_view.camera_rotation = rotation;
          main_view.view_matrix = make_camera_view(position, rotation);
          main_view.proj_matrix = make_camera_proj(main_view.camera_fov, main_view.aspect_ratio, main_view.near_plane, main_view.far_plane)
        }

        RendererCommand::UpdateTransform { entity, transform_mat } => {
          self.scene.update_transform(&entity, transform_mat);
        }

        RendererCommand::RegisterStatic {
          model_path, entity, transform, receive_shadows, cast_shadows, can_move
         } => {
          let model = self.assets.get_or_create_model_handle(&model_path);
          self.scene.add_static_drawable(entity, RendererStaticDrawable {
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
          self.scene.remove_static_drawable(&entity);
        }

        RendererCommand::RegisterPointLight {
          entity,
          transform,
          intensity
        } => {
          self.scene.add_point_light(entity, PointLight {
            position: (transform * Vec4::new(0f32, 0f32, 0f32, 1f32)).xyz(),
            intensity,
          });
        },
        RendererCommand::UnregisterPointLight(entity) => {
          self.scene.remove_point_light(&entity);
        },

        RendererCommand::RegisterDirectionalLight {
          entity,
          transform,
          intensity
        } => {
          let (_, rotation, _) = deconstruct_transform(&transform);
          let base_dir = Vec3::new(0f32, 1f32, 0f32);
          let dir = rotation.transform_vector(&base_dir);
          self.scene.add_directional_light(entity, DirectionalLight { direction: dir, intensity});
        },
        RendererCommand::UnregisterDirectionalLight(entity) => {
          self.scene.remove_directional_light(&entity);
        },
        RendererCommand::SetLightmap(path) => {
          let handle = self.assets.get_or_create_texture_handle(&path);
          self.scene.set_lightmap(Some(handle));
        },
      }

      let message_res = self.receiver.recv();
      if message_res.is_err() {
        panic!("Rendering channel closed");
      }
      message_opt = message_res.ok();
    }
  }

  #[profiling::function]
  pub(super) fn render(&mut self, renderer: &Renderer<P>) {
    self.receive_messages();

    let delta = Instant::now().duration_since(self.last_frame);
    self.last_frame = Instant::now();

    self.update_visibility();
    self.reorder();

    let render_result = {
      let frame_info = FrameInfo {
        frame: self.frame,
        delta: delta
      };

      let zero_textures = ZeroTextures {
        zero_texture_view: &self.assets.placeholder_texture().view,
        zero_texture_view_black: &self.assets.placeholder_black().view,
      };

      let lightmap: &RendererTexture<P::GraphicsBackend> = if let Some(lightmap_handle) = self.scene.lightmap() {
        self.assets.get_texture(lightmap_handle)
      } else {
        self.assets.placeholder_texture()
      }; // Doing .map() here is considered a immutable borrow of self
      let scene_info = SceneInfo {
        scene: &self.scene,
        views: &self.views,
        active_view_index: 0,
        vertex_buffer: self.assets.vertex_buffer(),
        index_buffer: self.assets.index_buffer(),
        lightmap: Some(lightmap)
      };

      self.render_path.render(&scene_info, &zero_textures, renderer.late_latching(), renderer.input(), &frame_info, &self.shader_manager, &self.assets)
    };

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

        {
          let frame_info = FrameInfo {
            frame: self.frame,
            delta: delta
          };

          let zero_textures = ZeroTextures {
            zero_texture_view: &self.assets.placeholder_texture().view,
            zero_texture_view_black: &self.assets.placeholder_black().view,
          };

          let lightmap: &RendererTexture<P::GraphicsBackend> = if let Some(lightmap_handle) = self.scene.lightmap() {
            self.assets.get_texture(lightmap_handle)
          } else {
            self.assets.placeholder_texture()
          }; // Doing .map() here is considered a immutable borrow of self
          let scene_info = SceneInfo {
            scene: &self.scene,
            views: &self.views,
            active_view_index: 0,
            vertex_buffer: self.assets.vertex_buffer(),
            index_buffer: self.assets.index_buffer(),
            lightmap: Some(lightmap)
          };

          self.render_path.render(&scene_info, &zero_textures, renderer.late_latching(), renderer.input(), &frame_info, &self.shader_manager, &self.assets).expect("Rendering still fails after recreating swapchain.");
        }
        self.swapchain = new_swapchain;
      }
    }
    self.frame += 1;
    renderer.dec_queued_frames_counter();
    profiling::finish_frame!();

    std::thread::sleep(Duration::new(0, 16_000_000));
  }

  #[profiling::function]
  fn update_visibility(&mut self) {
    let static_meshes = self.scene.static_drawables();

    let view_mut = &mut self.views[0];

    let mut old_visible = std::mem::take(&mut view_mut.visible_drawables_bitset);
    self.render_path.write_occlusion_culling_results(self.frame, &mut old_visible);

    let mut existing_drawable_bitset = std::mem::take(&mut view_mut.old_visible_drawables_bitset);
    let mut existing_parts = std::mem::take(&mut view_mut.drawable_parts);
    // take out vector, creating a new one doesn't allocate until we push an element to it.
    existing_drawable_bitset.clear();
    existing_parts.clear();
    let drawable_u32_count = (static_meshes.len() + 31) / 32;
    if existing_drawable_bitset.len() < drawable_u32_count {
      existing_drawable_bitset.resize(drawable_u32_count, 0);
    }
    let visible_drawables_bitset = Mutex::new(existing_drawable_bitset);
    let visible_parts = Mutex::new(existing_parts);

    let frustum = Frustum::new(view_mut.near_plane, view_mut.far_plane, view_mut.camera_fov, view_mut.aspect_ratio);
    let camera_matrix = view_mut.view_matrix;
    let camera_position = view_mut.camera_position;
    let assets = &self.assets;

    const CHUNK_SIZE: usize = 64;
    static_meshes.par_chunks(CHUNK_SIZE).enumerate().for_each(|(chunk_index, chunk)| {
      let mut chunk_visible_parts = SmallVec::<[DrawablePart; CHUNK_SIZE]>::new();
      let mut visible_drawables = [0u32; CHUNK_SIZE / 32];
      visible_drawables.bit_init(false);
      for (index, static_mesh) in chunk.iter().enumerate() {
        let model_view_matrix = camera_matrix * static_mesh.transform;
        let model = assets.get_model(static_mesh.model);
        if model.is_none() {
          continue;
        }
        let mesh = assets.get_mesh(model.unwrap().mesh_handle());
        if mesh.is_none() {
          continue;
        }
        let mesh = mesh.unwrap();
        let bounding_box = &mesh.bounding_box;
        let is_visible = if let Some(bounding_box) = bounding_box {
          frustum.intersects(bounding_box, &model_view_matrix)
        } else {
          true
        };
        if !is_visible {
          continue;
        }

        visible_drawables.bit_set(index);
        let drawable_index = chunk_index * CHUNK_SIZE + index;

        // Enlarge bounding box to check if camera is inside it.
        // To avoid objects disappearing because of the near plane and/or backface culling.
        // https://stackoverflow.com/questions/21037241/how-to-determine-a-point-is-inside-or-outside-a-cube
        let camera_in_bb = if let Some(bb) = bounding_box.as_ref() {
          let mut bb_scale = bb.max - bb.min;
          let bb_translation = bb.min + bb_scale / 2.0f32;
          bb_scale *= 1.2f32; // make bounding box 20% bigger, we used 10% for the occlusion query geo.
          bb_scale.x = bb_scale.x.max(0.4f32);
          bb_scale.y = bb_scale.y.max(0.4f32);
          bb_scale.z = bb_scale.z.max(0.4f32);
          let bb_transform = Matrix4::new_translation(&bb_translation)
            * Matrix4::new_nonuniform_scaling(&bb_scale);
          let transformed_bb = BoundingBox::new(Vec3::new(-0.5f32, -0.5f32, -0.5f32), Vec3::new(0.5f32, 0.5f32, 0.5f32))
            .transform(&(static_mesh.transform * bb_transform))
            .enlarge(&Vec3::new(view_mut.near_plane, view_mut.near_plane, view_mut.near_plane)); // Enlarge by the near plane to make check simpler.

          transformed_bb.contains(&camera_position)
        } else {
          false
        };

        if old_visible.len() * 32 > drawable_index && !old_visible.bit_test(drawable_index) && !camera_in_bb {
          // Mesh was not visible in the previous frame.
          continue;
        }

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

      debug_assert_eq!(CHUNK_SIZE % 32, 0);
      let mut global_drawables_bitset = visible_drawables_bitset.lock().unwrap();
      let global_drawables_bitset_mut: &mut Vec<u32> = global_drawables_bitset.as_mut();
      let global_drawable_bit_offset = chunk_index * visible_drawables.len();
      let global_drawable_bit_end = ((chunk_index + 1) * visible_drawables.len()).min(global_drawables_bitset_mut.len() - 1);
      let slice_len = global_drawable_bit_end - global_drawable_bit_offset + 1;
      global_drawables_bitset_mut[global_drawable_bit_offset .. global_drawable_bit_end].copy_from_slice(&visible_drawables[.. (slice_len - 1)]);

      let mut global_parts = visible_parts.lock().unwrap();
      global_parts.extend_from_slice(&chunk_visible_parts[..]);
      chunk_visible_parts.clear();
    });

    view_mut.drawable_parts = visible_parts.into_inner().unwrap();
    view_mut.visible_drawables_bitset = visible_drawables_bitset.into_inner().unwrap();
    view_mut.old_visible_drawables_bitset = old_visible;
  }

  #[profiling::function]
  fn reorder(&mut self) {
    let static_meshes = self.scene.static_drawables();

    let view_mut = &mut self.views[0];
    let assets = &self.assets;
    view_mut.drawable_parts.par_sort_unstable_by(|a, b| {
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
        let model_a = assets.get_model(static_mesh_a.model);
        let model_b = assets.get_model(static_mesh_b.model);
        if model_a.is_none() || model_b.is_none() {
          // doesn't matter, we'll skip the draws anyway
          return std::cmp::Ordering::Equal;
        }
        let materials_a = model_a.unwrap().material_handles();
        let materials_b = model_b.unwrap().material_handles();
        let material_a = &materials_a[a.part_index];
        let material_b = &materials_b[b.part_index];
        material_a.cmp(material_b)
      }
    });
  }
}
