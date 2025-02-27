use std::sync::Arc;

use sourcerenderer_core::{
    Platform,
    Vec2UI,
};

use crate::asset::AssetManager;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::asset::*;
use crate::renderer::asset::ComputePipelineHandle;
use crate::graphics::*;

pub struct PathTracerPass<P: Platform> {
    pipeline: ComputePipelineHandle,
    sampler: Sampler<P::GPUBackend>
}

impl<P: Platform> PathTracerPass<P> {
    pub const PATH_TRACING_TARGET: &'static str = "PathTracingTarget";

    pub fn new(
        device: &Device<P::GPUBackend>,
        resolution: Vec2UI,
        resources: &mut RendererResources<P::GPUBackend>,
        asset_manager: &Arc<AssetManager<P>>,
        _init_cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
    ) -> Self {
        resources.create_texture(
            Self::PATH_TRACING_TARGET,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA32Float,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            true,
        );

        let pipeline = asset_manager.request_compute_pipeline("shaders/path_tracer.comp.json");

        let sampler = device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mip_bias: 0.0,
            max_anisotropy: 1f32,
            compare_op: None,
            min_lod: 0.0,
            max_lod: None,
        });

        Self {
            pipeline,
            sampler
        }
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_, P>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
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
        let texture_uav_history = pass_params.resources.access_view(
            cmd_buffer,
            Self::PATH_TRACING_TARGET,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            TextureLayout::Storage,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Past,
        );

        let pipeline = pass_params.assets.get_compute_pipeline(self.pipeline).unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.bind_acceleration_structure(
            BindingFrequency::Frequent,
            0,
            acceleration_structure,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::Frequent, 1, &*texture_uav);
        cmd_buffer.bind_storage_texture(BindingFrequency::Frequent, 4, &*&texture_uav_history);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            2,
            blue_noise,
            blue_noise_sampler,
        );
        cmd_buffer.bind_sampler(BindingFrequency::VeryFrequent, 3, &self.sampler);
        let info = texture_uav.texture().unwrap().info();

        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
    }
}
