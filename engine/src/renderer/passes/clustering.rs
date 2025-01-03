use std::sync::Arc;

use sourcerenderer_core::{
    Platform, Vec2UI, Vec3UI, Vec4
};

use crate::asset::AssetManager;
use crate::renderer::asset::ComputePipelineHandle;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::graphics::*;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ShaderScreenToView {
    tile_size: Vec2UI,
    rt_dimensions: Vec2UI,
    z_near: f32,
    z_far: f32,
}

pub struct ClusteringPass {
    pipeline: ComputePipelineHandle,
}

impl ClusteringPass {
    pub const CLUSTERS_BUFFER_NAME: &'static str = "clusters";

    pub fn new<P: Platform>(
        barriers: &mut RendererResources<P::GPUBackend>,
        asset_manager: &mut Arc<AssetManager<P>>,
    ) -> Self {
        let pipeline = asset_manager.request_compute_pipeline("shaders/clustering.comp.json");

        barriers.create_buffer(
            Self::CLUSTERS_BUFFER_NAME,
            &BufferInfo {
                size: (std::mem::size_of::<Vec4>() * 2 * 16 * 9 * 24) as u64,
                usage: BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            false,
        );

        Self { pipeline }
    }

    pub fn execute<P: Platform>(
        &mut self,
        command_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>,
        rt_size: Vec2UI,
        camera_buffer: &TransientBufferSlice<P::GPUBackend>
    ) {
        command_buffer.begin_label("Clustering pass");

        let view = &(*pass_params.scene.scene).views()[pass_params.scene.active_view_index];

        let cluster_count = Vec3UI::new(16, 9, 24);
        let screen_to_view = ShaderScreenToView {
            tile_size: Vec2UI::new(
                ((rt_size.x as f32) / cluster_count.x as f32).ceil() as u32,
                ((rt_size.y as f32) / cluster_count.y as f32).ceil() as u32,
            ),
            rt_dimensions: rt_size,
            z_near: view.near_plane,
            z_far: view.far_plane,
        };

        let screen_to_view_cbuffer =
            command_buffer.upload_dynamic_data(&[screen_to_view], BufferUsage::STORAGE).unwrap();
        let clusters_buffer = pass_params.resources.access_buffer(
            command_buffer,
            Self::CLUSTERS_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            HistoryResourceEntry::Current,
        );
        debug_assert!(
            clusters_buffer.info().size as u32
                >= cluster_count.x
                    * cluster_count.y
                    * cluster_count.z
                    * 2
                    * std::mem::size_of::<Vec4>() as u32
        );
        debug_assert_eq!(cluster_count.x % 8, 0);
        debug_assert_eq!(cluster_count.y % 1, 0);
        debug_assert_eq!(cluster_count.z % 8, 0); // Ensure the cluster count fits with the work group size
        let pipeline = pass_params.assets.get_compute_pipeline(self.pipeline).unwrap();
        command_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        command_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            0,
            BufferRef::Regular(&*clusters_buffer),
            0,
            WHOLE_BUFFER,
        );
        command_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            1,
            BufferRef::Transient(&screen_to_view_cbuffer),
            0,
            WHOLE_BUFFER,
        );
        command_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            2,
            BufferRef::Transient(camera_buffer),
            0,
            WHOLE_BUFFER,
        );
        command_buffer.finish_binding();
        command_buffer.dispatch(
            (cluster_count.x + 7) / 8,
            cluster_count.y,
            (cluster_count.z + 7) / 8,
        );

        command_buffer.end_label();
    }

    pub fn cluster_count(&self) -> Vec3UI {
        Vec3UI::new(16, 9, 24)
    }
}
