use std::sync::Arc;

use sourcerenderer_core::Vec2UI;

use crate::graphics::*;
use crate::renderer::asset::{
    RayTracingPipelineHandle,
    RayTracingPipelineInfo,
    RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};

pub struct RTShadowPass {
    pipeline: RayTracingPipelineHandle,
}

impl RTShadowPass {
    pub const SHADOWS_TEXTURE_NAME: &'static str = "RTShadow";

    pub fn new(
        resolution: Vec2UI,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        resources.create_texture(
            Self::SHADOWS_TEXTURE_NAME,
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

        let pipeline = assets.request_ray_tracing_pipeline(&RayTracingPipelineInfo {
            ray_gen_shader: "shaders/shadows.rgen.json",
            closest_hit_shaders: &["shaders/shadows.rchit.json"],
            any_hit_shaders: &[],
            miss_shaders: &["shaders/shadows.rmiss.json"],
        });

        Self { pipeline }
    }

    #[inline(always)]
    pub(crate) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_ray_tracing_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters<'_>,
        depth_name: &str,
        acceleration_structure: &Arc<AccelerationStructure>,
        blue_noise: &Arc<TextureView>,
        blue_noise_sampler: &Arc<Sampler>,
    ) {
        let texture_uav = pass_params.resources.access_view(
            cmd_buffer,
            Self::SHADOWS_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER | BarrierSync::RAY_TRACING_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let depth = pass_params.resources.access_view(
            cmd_buffer,
            depth_name,
            BarrierSync::RAY_TRACING_SHADER | BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let pipeline = pass_params
            .assets
            .get_ray_tracing_pipeline(self.pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::RayTracing(&pipeline));
        cmd_buffer.bind_acceleration_structure(
            BindingFrequency::Frequent,
            0,
            acceleration_structure,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::Frequent, 1, &*texture_uav);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            2,
            &*depth,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            3,
            blue_noise,
            blue_noise_sampler,
        );
        let info = texture_uav.texture().unwrap().info();

        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();
        cmd_buffer.trace_ray(info.width, info.height, 1);
    }
}
