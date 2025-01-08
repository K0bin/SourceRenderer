use bevy_tasks::ParallelSlice;
use bitset_core::BitSet;
use log::trace;
use smallvec::SmallVec;
use sourcerenderer_core::{Matrix4, Platform, Vec3};

use crate::{asset::AssetManager, math::{BoundingBox, Frustum}, renderer::DrawablePart};

use super::{renderer_scene::RendererScene};

#[profiling::function]
pub(crate) fn update_visibility<P: Platform>(scene: &mut RendererScene<P::GPUBackend>, asset_manager: &AssetManager<P>) {
    let (views, static_meshes, _, _) = scene.view_update_info();

    for (index, view_mut) in views.iter_mut().enumerate() {
        let mut old_visible = std::mem::take(&mut view_mut.visible_drawables_bitset);

        /*if index == active_view_index {
            self.render_path
                .write_occlusion_culling_results(self.frame, &mut old_visible);
        } else {
            old_visible.fill(!0u32);
        }*/
        old_visible.fill(!0u32);

        let mut visible_drawables_bitset =
            std::mem::take(&mut view_mut.old_visible_drawables_bitset);
        let mut visible_parts = std::mem::take(&mut view_mut.drawable_parts);
        // take out vector, creating a new one doesn't allocate until we push an element to it.
        visible_drawables_bitset.clear();
        visible_parts.clear();
        let drawable_u32_count = (static_meshes.len() + 31) / 32;
        if visible_drawables_bitset.len() < drawable_u32_count {
            visible_drawables_bitset.resize(drawable_u32_count, 0);
        }

        let frustum = Frustum::new(
            view_mut.near_plane,
            view_mut.far_plane,
            view_mut.camera_fov,
            view_mut.aspect_ratio,
        );
        let camera_matrix = view_mut.view_matrix;
        let camera_position = view_mut.camera_position;

        let task_pool = bevy_tasks::ComputeTaskPool::get();
        const CHUNK_SIZE: usize = 64;
        let assets = asset_manager.read_renderer_assets();
        static_meshes
            .par_chunk_map(task_pool, CHUNK_SIZE, |chunk_index, chunk| {
                let mut chunk_visible_parts = SmallVec::<[DrawablePart; CHUNK_SIZE]>::new();
                let mut visible_drawables = [0u32; CHUNK_SIZE / 32];
                debug_assert_eq!(CHUNK_SIZE % 32, 0);
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
                        let bb_transform = Matrix4::from_translation(bb_translation)
                            * Matrix4::from_scale(bb_scale);
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
                        println!("Previous frame faile");
                        continue;
                    }

                    for part_index in 0..mesh.parts.len() {
                        chunk_visible_parts.push(DrawablePart {
                            drawable_index,
                            part_index,
                        });
                    }
                }

                (chunk_visible_parts, visible_drawables)
            })
            .iter()
            .enumerate()
            .for_each(|(chunk_index, (chunk_visible_parts, visible_drawables))| {
                let global_drawable_bit_offset = chunk_index * visible_drawables.len();
                let global_drawable_bit_end = ((chunk_index + 1) * visible_drawables.len())
                    .min(visible_drawables_bitset.len() - 1);
                let slice_len = global_drawable_bit_end - global_drawable_bit_offset + 1;
                visible_drawables_bitset
                    [global_drawable_bit_offset..global_drawable_bit_end]
                    .copy_from_slice(&visible_drawables[..(slice_len - 1)]);

                visible_parts.extend_from_slice(&chunk_visible_parts[..]);
            });

        view_mut.drawable_parts = visible_parts;
        view_mut.visible_drawables_bitset = visible_drawables_bitset;
        view_mut.old_visible_drawables_bitset = old_visible;
    }
}