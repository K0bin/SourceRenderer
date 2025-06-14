use std::cell::Ref;
use std::sync::Arc;

use sourcerenderer_core::Vec2UI;

use super::rt_shadows::RTShadowPass;
use super::shadow_map::ShadowMapPass;
use super::visibility_buffer::VisibilityBufferPass;
use crate::graphics::*;
use crate::renderer::asset::{
    ComputePipelineHandle,
    RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::passes::ssao::SsaoPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};

pub struct ShadingPass {
    sampler: Arc<crate::graphics::Sampler>,
    shadow_sampler: Arc<crate::graphics::Sampler>,
    pipeline: ComputePipelineHandle,
}

impl ShadingPass {
    pub const SHADING_TEXTURE_NAME: &'static str = "Shading";

    pub fn new(
        device: &Arc<crate::graphics::Device>,
        resolution: Vec2UI,
        resources: &mut RendererResources,
        assets: &RendererAssets,
        _init_cmd_buffer: &mut CommandBuffer,
    ) -> Self {
        let pipeline = assets.request_compute_pipeline("shaders/shading.comp.json");

        let sampler = Arc::new(device.create_sampler(&SamplerInfo {
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
        }));

        resources.create_texture(
            Self::SHADING_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA16Float,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                supports_srgb: true,
            },
            false,
        );

        let shadow_sampler = Arc::new(device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mip_bias: 0.0f32,
            max_anisotropy: 1f32,
            compare_op: Some(CompareFunc::Less),
            min_lod: 0f32,
            max_lod: None,
        }));

        Self {
            sampler,
            shadow_sampler,
            pipeline,
        }
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    #[profiling::function]
    pub(super) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters<'_>,
    ) {
        let (width, height) = {
            let info = pass_params
                .resources
                .texture_info(Self::SHADING_TEXTURE_NAME);
            (info.width, info.height)
        };

        cmd_buffer.begin_label("Shading Pass");

        let output = pass_params.resources.access_view(
            cmd_buffer,
            Self::SHADING_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let ids = pass_params.resources.access_view(
            cmd_buffer,
            VisibilityBufferPass::PRIMITIVE_ID_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            TextureLayout::Storage,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let barycentrics = pass_params.resources.access_view(
            cmd_buffer,
            VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            TextureLayout::Storage,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let light_bitmask_buffer = pass_params.resources.access_buffer(
            cmd_buffer,
            super::light_binning::LightBinningPass::LIGHT_BINNING_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );

        let ssao = pass_params.resources.access_view(
            cmd_buffer,
            SsaoPass::SSAO_TEXTURE_NAME,
            BarrierSync::FRAGMENT_SHADER | BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let rt_shadows: Ref<Arc<TextureView>>;
        let shadows = if pass_params.device.supports_ray_tracing_pipeline() {
            rt_shadows = pass_params.resources.access_view(
                cmd_buffer,
                RTShadowPass::SHADOWS_TEXTURE_NAME,
                BarrierSync::FRAGMENT_SHADER,
                BarrierAccess::SAMPLING_READ,
                TextureLayout::Sampled,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            );
            Some(&*rt_shadows)
        } else {
            None
        };

        let cascade_count = {
            let shadow_map_info = pass_params
                .resources
                .texture_info(ShadowMapPass::SHADOW_MAP_NAME);
            shadow_map_info.array_length
        };

        let shadow_map = pass_params.resources.access_view(
            cmd_buffer,
            ShadowMapPass::SHADOW_MAP_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_array_layer: 0,
                array_layer_length: cascade_count,
                mip_level_length: 1,
                base_mip_level: 0,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &ids);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 2, &barycentrics);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 3, &output);
        cmd_buffer.bind_sampler(BindingFrequency::VeryFrequent, 4, &self.sampler);
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            5,
            BufferRef::Regular(&light_bitmask_buffer),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            6,
            &pass_params.scene.lightmap.unwrap().view,
            pass_params.resources.linear_sampler(),
        );
        if let Some(shadows) = shadows {
            cmd_buffer.bind_sampling_view_and_sampler(
                BindingFrequency::VeryFrequent,
                7,
                shadows,
                pass_params.resources.linear_sampler(),
            );
        }
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            8,
            &ssao,
            pass_params.resources.linear_sampler(),
        );

        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            9,
            &shadow_map,
            &self.shadow_sampler,
        );

        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();

        cmd_buffer.dispatch((width + 7) / 8, (height + 7) / 8, 1);
        cmd_buffer.end_label();
    }
}
