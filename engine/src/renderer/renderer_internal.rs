use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use bevy_log::trace;
use bitset_core::BitSet;
use crossbeam_channel::{
    Receiver,
    TryRecvError,
};
use instant::Instant;
use rayon::prelude::*;
use smallvec::SmallVec;
use sourcerenderer_core::platform::Event;
use sourcerenderer_core::{
    Console,
    Matrix4,
    Platform,
    Vec2UI,
    Vec3,
    Vec4,
};

use super::drawable::{
    make_camera_proj,
    make_camera_view,
};
use super::light::DirectionalLight;
use super::passes::modern::ModernRenderer;
//#[cfg(not(target_arch = "wasm32"))]
use super::passes::conservative::desktop_renderer::ConservativeRenderer;
use super::passes::path_tracing::PathTracingRenderer;
use super::passes::web::WebRenderer;
//#[cfg(not(target_arch = "wasm32"))]
//use super::passes::modern::ModernRenderer;
use super::render_path::RenderPath;
use super::renderer_scene::RendererScene;
use super::shader_manager::ShaderManager;
use super::PointLight;
use crate::asset::AssetManager;
use crate::graphics::*;
use crate::math::{
    BoundingBox,
    Frustum,
};
use crate::renderer::command::RendererCommand;
use crate::renderer::drawable::DrawablePart;
//use crate::renderer::passes::web::WebRenderer;
use crate::renderer::render_path::{
    FrameInfo,
    SceneInfo,
    ZeroTextures,
};
use crate::renderer::renderer_assets::*;
use crate::renderer::{
    Renderer,
    RendererStaticDrawable,
    View,
};

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
enum ReceiveMessagesResult {
    FrameCompleted,
    Quit,
    WaitForMessages
}

pub(super) struct RendererInternal<P: Platform> {
    device: Arc<Device<P::GPUBackend>>,
    swapchain: Option<Arc<Swapchain<P::GPUBackend>>>,
    render_path: Box<dyn RenderPath<P>>,
    asset_manager: Arc<AssetManager<P>>,
    scene: RendererScene<P::GPUBackend>,
    views: Vec<View>,
    receiver: Receiver<RendererCommand<P::GPUBackend>>,
    window_event_receiver: Receiver<Event<P>>,
    last_frame: Instant,
    frame: u64,
    assets: RendererAssets<P>,
    console: Arc<Console>,
    shader_manager: ShaderManager<P>,
    context: GraphicsContext<P::GPUBackend>,
}

impl<P: Platform> RendererInternal<P> {
    pub(super) fn new(
        device: &Arc<Device<P::GPUBackend>>,
        swapchain: Swapchain<P::GPUBackend>,
        asset_manager: &Arc<AssetManager<P>>,
        window_event_receiver: Receiver<Event<P>>,
        receiver: Receiver<RendererCommand<P::GPUBackend>>,
        console: &Arc<Console>,
    ) -> Self {
        let assets = RendererAssets::new(device);

        let mut shader_manager = ShaderManager::<P>::new(device, asset_manager);

        let scene = RendererScene::new();
        let view = View::default();
        let views = vec![view];

        let mut context = device.create_context();

        #[cfg(target_arch = "wasm32")]
        let path = Box::new(WebRenderer::new(device, swapchain, &mut shader_manager));

        #[cfg(not(target_arch = "wasm32"))]
        let path: Box<dyn RenderPath<P>> = if false {
            Box::new(WebRenderer::new(
                device,
                &swapchain,
                &mut context,
                &mut shader_manager,
            ))
        } else {
            Box::new(PathTracingRenderer::new(device, &swapchain, &mut context, &mut shader_manager))

            /*if device.supports_indirect()
                && device.supports_bindless()
                && device.supports_barycentrics()
                && device.supports_min_max_filter()
            {
                Box::new(ModernRenderer::new(device, &swapchain, &mut context, &mut shader_manager))
            } else {
                Box::new(ConservativeRenderer::new(
                    device,
                    &swapchain,
                    &mut context,
                    &mut shader_manager,
                ))
            }*/
        };

        Self {
            device: device.clone(),
            render_path: path,
            context,
            swapchain: Some(Arc::new(swapchain)),
            scene,
            asset_manager: asset_manager.clone(),
            views,
            receiver,
            window_event_receiver,
            last_frame: Instant::now(),
            assets,
            frame: 0,
            console: console.clone(),
            shader_manager,
        }
    }

    fn receive_window_events(&mut self) -> bool {
        let mut window_message_res = self.window_event_receiver.try_recv();

        let mut new_surface = Option::<<P::GPUBackend as GPUBackend>::Surface>::None;
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
                _ => unreachable!(),
            }
            window_message_res = self.window_event_receiver.try_recv();
        }
        if let Result::Err(err) = &window_message_res {
            if let TryRecvError::Disconnected = err {
                panic!("Rendering window event channel closed {:?}", err);
            }
        }

        if let Some(surface) = new_surface {
            // We need to recreate the swapchain
            let old_swapchain_arc = self.swapchain.take().unwrap();
            let old_swapchain = Arc::try_unwrap(old_swapchain_arc).ok().expect("Swapchain was still in use");

            let size = new_size
                .unwrap_or_else(|| Vec2UI::new(old_swapchain.width(), old_swapchain.height()));

            self.device.wait_for_idle();

            let new_swapchain_result =
                Swapchain::<P::GPUBackend>::recreate_on_surface(
                    old_swapchain,
                    surface,
                    size.x,
                    size.y,
                );
            if let Result::Err(error) = new_swapchain_result {
                trace!("Swapchain recreation failed: {:?}", error);
            } else {
                self.swapchain = Some(Arc::new(new_swapchain_result.unwrap()));
            }
            self.render_path.on_swapchain_changed(self.swapchain.as_ref().unwrap());
            true
        } else if let Some(size) = new_size {
            // We need to recreate the swapchain
            let old_swapchain_arc = self.swapchain.take().unwrap();
            let old_swapchain = Arc::try_unwrap(old_swapchain_arc).ok().expect("Swapchain was still in use");

            self.device.wait_for_idle();

            let new_swapchain_result =
                Swapchain::<P::GPUBackend>::recreate(
                    old_swapchain,
                    size.x,
                    size.y,
                );
            if let Result::Err(error) = new_swapchain_result {
                trace!("Swapchain recreation failed: {:?}", error);
            } else {
                self.swapchain = Some(Arc::new(new_swapchain_result.unwrap()));
            }
            self.render_path.on_swapchain_changed(self.swapchain.as_ref().unwrap());
            true
       } else {
            false
        }
    }


    fn receive_messages(&mut self) -> ReceiveMessagesResult {
        // TODO: merge channels to get rid of this mess.
        while self.shader_manager.has_remaining_mandatory_compilations() {
            self.assets
                .receive_assets(&self.asset_manager, &mut self.shader_manager);
        }

        let mut message_opt = if self.assets.is_empty()
            || self.asset_manager.has_open_renderer_assets()
            || !self.window_event_receiver.is_empty()
            || self.scene.static_drawables().is_empty()
        {
            let message_res = self.receiver.try_recv();
            if let Err(err) = &message_res {
                if err.is_disconnected() {
                    panic!("Rendering channel closed {:?}", err);
                }
            }
            message_res.ok()
        } else if cfg!(target_arch = "wasm32") {
            let message_res = self.receiver.recv();
            if let Err(err) = &message_res {
                panic!("Rendering channel closed {:?}", err);
            }
            message_res.ok()
        } else {
            let message_res = self.receiver.recv_timeout(Duration::from_millis(16));
            if let Err(err) = &message_res {
                if err.is_disconnected() {
                    panic!("Rendering channel closed {:?}", err);
                }
            }
            message_res.ok()
        };

        // recv blocks, so do the preparation after receiving the first event
        self.receive_window_events();
        self.assets
            .receive_assets(&self.asset_manager, &mut self.shader_manager);

        if message_opt.is_none() {
            return ReceiveMessagesResult::WaitForMessages;
        }

        let main_view = &mut self.views[0];

        while message_opt.is_some() {
            let message = message_opt.take().unwrap();
            match message {
                RendererCommand::<P::GPUBackend>::EndFrame => {
                    return ReceiveMessagesResult::FrameCompleted;
                }

                RendererCommand::<P::GPUBackend>::Quit => {
                    return ReceiveMessagesResult::Quit;
                }

                RendererCommand::<P::GPUBackend>::UpdateCameraTransform {
                    camera_transform_mat,
                    fov,
                } => {
                    main_view.camera_transform = camera_transform_mat;
                    main_view.camera_fov = fov;
                    main_view.old_camera_matrix = main_view.proj_matrix * main_view.view_matrix;
                    let (position, rotation, _) = deconstruct_transform(&camera_transform_mat);
                    main_view.camera_position = position;
                    main_view.camera_rotation = rotation;
                    main_view.view_matrix = make_camera_view(position, rotation);
                    main_view.proj_matrix = make_camera_proj(
                        main_view.camera_fov,
                        main_view.aspect_ratio,
                        main_view.near_plane,
                        main_view.far_plane,
                    )
                }

                RendererCommand::<P::GPUBackend>::UpdateTransform {
                    entity,
                    transform_mat,
                } => {
                    self.scene.update_transform(&entity, transform_mat);
                }

                RendererCommand::<P::GPUBackend>::RegisterStatic {
                    model_path,
                    entity,
                    transform,
                    receive_shadows,
                    cast_shadows,
                    can_move,
                } => {
                    let model = self.assets.get_or_create_model_handle(&model_path);
                    self.scene.add_static_drawable(
                        entity,
                        RendererStaticDrawable {
                            entity,
                            transform,
                            old_transform: transform,
                            model,
                            receive_shadows,
                            cast_shadows,
                            can_move,
                        },
                    );
                }
                RendererCommand::<P::GPUBackend>::UnregisterStatic(entity) => {
                    self.scene.remove_static_drawable(&entity);
                }

                RendererCommand::<P::GPUBackend>::RegisterPointLight {
                    entity,
                    transform,
                    intensity,
                } => {
                    self.scene.add_point_light(
                        entity,
                        PointLight {
                            position: (transform * Vec4::new(0f32, 0f32, 0f32, 1f32)).xyz(),
                            intensity,
                        },
                    );
                }
                RendererCommand::<P::GPUBackend>::UnregisterPointLight(entity) => {
                    self.scene.remove_point_light(&entity);
                }

                RendererCommand::<P::GPUBackend>::RegisterDirectionalLight {
                    entity,
                    transform,
                    intensity,
                } => {
                    let (_, rotation, _) = deconstruct_transform(&transform);
                    let base_dir = Vec3::new(0f32, 0f32, 1f32);
                    let dir = rotation.transform_vector(&base_dir);
                    self.scene.add_directional_light(
                        entity,
                        DirectionalLight {
                            direction: dir,
                            intensity,
                        },
                    );
                }
                RendererCommand::<P::GPUBackend>::UnregisterDirectionalLight(entity) => {
                    self.scene.remove_directional_light(&entity);
                }
                RendererCommand::<P::GPUBackend>::SetLightmap(path) => {
                    let handle = self.assets.get_or_create_texture_handle(&path);
                    self.scene.set_lightmap(Some(handle));
                }
                RendererCommand::RenderUI(data) => { self.render_path.set_ui_data(data); },
            }

            let message_res = self.receiver.try_recv();
            if let Err(err) = &message_res {
                if err.is_disconnected() {
                    panic!("Rendering channel closed {:?}", err);
                }
            }
            message_opt = message_res.ok();
        }
        ReceiveMessagesResult::WaitForMessages
    }

    #[profiling::function]
    pub(super) fn render(&mut self) {
        let mut message_receiving_result = ReceiveMessagesResult::WaitForMessages;
        while message_receiving_result == ReceiveMessagesResult::WaitForMessages {
            message_receiving_result = self.receive_messages();
        }

        if message_receiving_result == ReceiveMessagesResult::Quit {
            return;
        }

        let delta = Instant::now().duration_since(self.last_frame);
        self.last_frame = Instant::now();

        let swapchain = self.swapchain.as_ref().unwrap();
        self.views[0].aspect_ratio =
            (swapchain.width() as f32) / (swapchain.height() as f32);

        self.update_visibility();
        self.reorder();

        self.context.begin_frame();

        let render_result = {
            let frame_info = FrameInfo {
                frame: self.frame,
                delta: delta,
            };

            let zero_textures = ZeroTextures {
                zero_texture_view: &self.assets.placeholder_texture().view,
                zero_texture_view_black: &self.assets.placeholder_black().view,
            };

            let lightmap: &RendererTexture<P::GPUBackend> =
                if let Some(lightmap_handle) = self.scene.lightmap() {
                    self.assets.get_texture(lightmap_handle)
                } else {
                    self.assets.placeholder_texture()
                }; // Doing .map() here is considered a immutable borrow of self
            let scene_info = SceneInfo {
                scene: &self.scene,
                views: &self.views,
                active_view_index: 0,
                vertex_buffer: BufferRef::Regular(self.assets.vertex_buffer()),
                index_buffer: BufferRef::Regular(self.assets.index_buffer()),
                lightmap: Some(lightmap),
            };

            self.render_path.render(
                &mut self.context,
                self.swapchain.as_ref().expect("No swapchain"),
                &scene_info,
                &zero_textures,
                &frame_info,
                &self.shader_manager,
                &self.assets,
            )
        };

        if let Err(swapchain_error) = render_result {
            self.device.wait_for_idle();

            // Recheck window events
            if !self.receive_window_events() {
                let new_swapchain = if swapchain_error == SwapchainError::SurfaceLost {
                    unimplemented!()
                } else {
                    trace!("Recreating swapchain");
                    let old_swapchain_arc = self.swapchain.take().unwrap();
                    let old_swapchain = Arc::try_unwrap(old_swapchain_arc).ok().expect("Old swapchain is still being used.");
                    let size = Vec2UI::new(old_swapchain.width(), old_swapchain.height());
                    let new_swapchain_result = Swapchain::recreate(
                        old_swapchain,
                        size.x,
                        size.y
                    );
                    if new_swapchain_result.is_err() {
                        trace!(
                            "Swapchain recreation failed: {:?}",
                            new_swapchain_result.err().unwrap()
                        );
                        return;
                    }
                    Arc::new(new_swapchain_result.unwrap())
                };
                self.render_path.on_swapchain_changed(&new_swapchain);

                {
                    let frame_info = FrameInfo {
                        frame: self.frame,
                        delta: delta,
                    };

                    let zero_textures = ZeroTextures {
                        zero_texture_view: &self.assets.placeholder_texture().view,
                        zero_texture_view_black: &self.assets.placeholder_black().view,
                    };

                    let lightmap: &RendererTexture<P::GPUBackend> =
                        if let Some(lightmap_handle) = self.scene.lightmap() {
                            self.assets.get_texture(lightmap_handle)
                        } else {
                            self.assets.placeholder_texture()
                        }; // Doing .map() here is considered a immutable borrow of self
                    let scene_info = SceneInfo {
                        scene: &self.scene,
                        views: &self.views,
                        active_view_index: 0,
                        vertex_buffer: BufferRef::Regular(self.assets.vertex_buffer()),
                        index_buffer: BufferRef::Regular(self.assets.index_buffer()),
                        lightmap: Some(lightmap),
                    };

                    self.render_path
                        .render(
                            &mut self.context,
                            &new_swapchain,
                            &scene_info,
                            &zero_textures,
                            &frame_info,
                            &self.shader_manager,
                            &self.assets,
                        )
                        .expect("Rendering still fails after recreating swapchain.");
                }
                self.swapchain = Some(new_swapchain);
            }
        }
        self.frame += 1;
        profiling::finish_frame!();
    }

    #[profiling::function]
    fn update_visibility(&mut self) {
        if self.render_path.is_gpu_driven() {
            return;
        }

        let static_meshes = self.scene.static_drawables();

        let active_view_index = 0;

        for (index, view_mut) in self.views.iter_mut().enumerate() {
            let mut old_visible = std::mem::take(&mut view_mut.visible_drawables_bitset);

            if index == active_view_index {
                self.render_path
                    .write_occlusion_culling_results(self.frame, &mut old_visible);
            } else {
                old_visible.fill(!0u32);
            }

            let mut existing_drawable_bitset =
                std::mem::take(&mut view_mut.old_visible_drawables_bitset);
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

            let frustum = Frustum::new(
                view_mut.near_plane,
                view_mut.far_plane,
                view_mut.camera_fov,
                view_mut.aspect_ratio,
            );
            let camera_matrix = view_mut.view_matrix;
            let camera_position = view_mut.camera_position;
            let assets = &self.assets;

            const CHUNK_SIZE: usize = 64;
            static_meshes
                .par_chunks(CHUNK_SIZE)
                .enumerate()
                .for_each(|(chunk_index, chunk)| {
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
                            let transformed_bb = BoundingBox::new(
                                Vec3::new(-0.5f32, -0.5f32, -0.5f32),
                                Vec3::new(0.5f32, 0.5f32, 0.5f32),
                            )
                            .transform(&(static_mesh.transform * bb_transform))
                            .enlarge(&Vec3::new(
                                view_mut.near_plane,
                                view_mut.near_plane,
                                view_mut.near_plane,
                            )); // Enlarge by the near plane to make check simpler.

                            transformed_bb.contains(&camera_position)
                        } else {
                            false
                        };

                        if old_visible.len() * 32 > drawable_index
                            && !old_visible.bit_test(drawable_index)
                            && !camera_in_bb
                        {
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
                                part_index,
                            });
                        }
                    }

                    debug_assert_eq!(CHUNK_SIZE % 32, 0);
                    let mut global_drawables_bitset = visible_drawables_bitset.lock().unwrap();
                    let global_drawables_bitset_mut: &mut Vec<u32> =
                        global_drawables_bitset.as_mut();
                    let global_drawable_bit_offset = chunk_index * visible_drawables.len();
                    let global_drawable_bit_end = ((chunk_index + 1) * visible_drawables.len())
                        .min(global_drawables_bitset_mut.len() - 1);
                    let slice_len = global_drawable_bit_end - global_drawable_bit_offset + 1;
                    global_drawables_bitset_mut
                        [global_drawable_bit_offset..global_drawable_bit_end]
                        .copy_from_slice(&visible_drawables[..(slice_len - 1)]);

                    let mut global_parts = visible_parts.lock().unwrap();
                    global_parts.extend_from_slice(&chunk_visible_parts[..]);
                    chunk_visible_parts.clear();
                });

            view_mut.drawable_parts = visible_parts.into_inner().unwrap();
            view_mut.visible_drawables_bitset = visible_drawables_bitset.into_inner().unwrap();
            view_mut.old_visible_drawables_bitset = old_visible;
        }
    }

    #[profiling::function]
    fn reorder(&mut self) {
        if self.render_path.is_gpu_driven() {
            return;
        }

        let static_meshes = self.scene.static_drawables();

        let active_view_index = 0;
        let view_mut = &mut self.views[active_view_index];
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
            };
        });
    }
}
