use std::sync::Arc;

use sourcerenderer_core::{
    Platform,
    Vec2UI,
};

use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    ComputePipelineHandle, RayTracingPipelineHandle, RayTracingPipelineInfo, ShaderManager
};
use crate::graphics::*;

pub struct PathTracerPass {
    pipeline: ComputePipelineHandle,
}

impl PathTracerPass {
    pub const PATH_TRACING_TARGET: &'static str = "PathTracingTarget";

    pub fn new<P: Platform>(
        resolution: Vec2UI,
        resources: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        resources.create_texture(
            Self::PATH_TRACING_TARGET,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            false,
        );

        let pipeline = shader_manager.request_compute_pipeline("shaders/path_tracer.comp.json");

        Self { pipeline }
    }

    pub fn execute<P: Platform>(
        &mut self,
        cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>,
        acceleration_structure: &Arc<AccelerationStructure<P::GPUBackend>>,
        blue_noise: &Arc<TextureView<P::GPUBackend>>,
        blue_noise_sampler: &Arc<Sampler<P::GPUBackend>>,
    ) {
        let texture_uav = pass_params.resources.access_view(
            cmd_buffer,
            Self::PATH_TRACING_TARGET,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let pipeline = pass_params.shader_manager.get_compute_pipeline(self.pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.bind_acceleration_structure(
            BindingFrequency::Frequent,
            0,
            acceleration_structure,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::Frequent, 1, &*texture_uav);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            2,
            blue_noise,
            blue_noise_sampler,
        );
        cmd_buffer.bind_sampler(BindingFrequency::VeryFrequent, 3, pass_params.resources.linear_sampler());
        let info = texture_uav.texture().unwrap().info();

        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
    }
}
