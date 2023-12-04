use std::sync::Arc;

use nalgebra::Vector3;
use sourcerenderer_core::graphics::{
    Backend as GraphicsBackend,
    Barrier,
    BarrierAccess,
    BarrierSync,
    BindingFrequency,
    BufferInfo,
    BufferUsage,
    CommandBuffer,
    MemoryUsage,
    PipelineBinding,
    WHOLE_BUFFER,
};
use sourcerenderer_core::{
    Platform,
    Vec3,
};

use super::clustering::ClusteringPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    ComputePipelineHandle,
    ShaderManager,
};
use crate::renderer::RendererScene;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SetupInfo {
    cluster_count: u32,
    point_light_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CullingPointLight {
    position: Vec3,
    radius: f32,
}

const LIGHT_CUTOFF: f32 = 0.05f32;

pub struct LightBinningPass {
    light_binning_pipeline: ComputePipelineHandle,
}

impl LightBinningPass {
    pub const LIGHT_BINNING_BUFFER_NAME: &'static str = "binned_lights";

    pub fn new<P: Platform>(
        barriers: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let pipeline = shader_manager.request_compute_pipeline("shaders/light_binning.comp.spv");

        barriers.create_buffer(
            Self::LIGHT_BINNING_BUFFER_NAME,
            &BufferInfo {
                size: std::mem::size_of::<u32>() * 16 * 9 * 24,
                usage: BufferUsage::STORAGE | BufferUsage::CONSTANT,
            },
            MemoryUsage::VRAM,
            false,
        );

        Self {
            light_binning_pipeline: pipeline,
        }
    }

    pub fn execute<P: Platform>(
        &mut self,
        cmd_buffer: &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer,
        pass_params: &RenderPassParameters<'_, P>,
        camera_buffer: &Arc<<P::GraphicsBackend as GraphicsBackend>::Buffer>
    ) {
        cmd_buffer.begin_label("Light binning");
        let cluster_count = Vector3::<u32>::new(16, 9, 24);
        let setup_info = SetupInfo {
            point_light_count: pass_params.scene.scene.point_lights().len() as u32,
            cluster_count: cluster_count.x * cluster_count.y * cluster_count.z,
        };
        let point_lights: Vec<CullingPointLight> = pass_params.scene.scene
            .point_lights()
            .iter()
            .map(|l| CullingPointLight {
                position: l.position,
                radius: (l.intensity / LIGHT_CUTOFF).sqrt(),
            })
            .collect();

        let light_info_buffer = cmd_buffer.upload_dynamic_data(&[setup_info], BufferUsage::STORAGE);
        let point_lights_buffer =
            cmd_buffer.upload_dynamic_data(&point_lights[..], BufferUsage::STORAGE);

        cmd_buffer.barrier(&[Barrier::BufferBarrier {
            old_sync: BarrierSync::COMPUTE_SHADER,
            new_sync: BarrierSync::COMPUTE_SHADER
                | BarrierSync::VERTEX_SHADER
                | BarrierSync::FRAGMENT_SHADER,
            old_access: BarrierAccess::STORAGE_WRITE,
            new_access: BarrierAccess::CONSTANT_READ | BarrierAccess::STORAGE_READ,
            buffer: camera_buffer,
        }]);

        let light_bitmask_buffer = pass_params.resources.access_buffer(
            cmd_buffer,
            Self::LIGHT_BINNING_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ | BarrierAccess::STORAGE_WRITE,
            HistoryResourceEntry::Current,
        );
        let clusters_buffer = pass_params.resources.access_buffer(
            cmd_buffer,
            ClusteringPass::CLUSTERS_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );

        let pipeline = pass_params.shader_manager.get_compute_pipeline(self.light_binning_pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            0,
            camera_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            1,
            &*clusters_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            2,
            &light_info_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            3,
            &point_lights_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            4,
            &*light_bitmask_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch(
            (cluster_count.x * cluster_count.y * cluster_count.z + 63) / 64,
            1,
            1,
        );
        cmd_buffer.end_label();
    }
}
