use std::cell::Ref;
use std::sync::Arc;

use sourcerenderer_core::graphics::{
    AddressMode,
    Backend,
    BarrierAccess,
    BarrierSync,
    BindingFrequency,
    CommandBuffer,
    Device,
    Filter,
    Format,
    PipelineBinding,
    SampleCount,
    SamplerInfo,
    TextureDimension,
    TextureInfo,
    TextureLayout,
    TextureUsage,
    TextureViewInfo,
    WHOLE_BUFFER, ShaderType, CompareFunc,
};
use sourcerenderer_core::{
    Platform,
    Vec2UI, Matrix4,
};

use super::rt_shadows::RTShadowPass;
use super::shadow_map::ShadowMapPass;
use super::visibility_buffer::VisibilityBufferPass;
use crate::renderer::passes::ssao::SsaoPass;
use crate::renderer::renderer_assets::RendererTexture;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    ComputePipelineHandle,
    ShaderManager,
};

pub struct ShadingPass<P: Platform> {
    sampler: Arc<<P::GraphicsBackend as Backend>::Sampler>,
    shadow_sampler: Arc<<P::GraphicsBackend as Backend>::Sampler>,
    pipeline: ComputePipelineHandle,
}

impl<P: Platform> ShadingPass<P> {
    pub const SHADING_TEXTURE_NAME: &'static str = "Shading";

    pub fn new(
        device: &Arc<<P::GraphicsBackend as Backend>::Device>,
        resolution: Vec2UI,
        resources: &mut RendererResources<P::GraphicsBackend>,
        shader_manager: &mut ShaderManager<P>,
        _init_cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
    ) -> Self {
        let pipeline = shader_manager.request_compute_pipeline("shaders/shading.comp.spv");

        let sampler = device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mip_bias: 0.0,
            max_anisotropy: 0.0,
            compare_op: None,
            min_lod: 0.0,
            max_lod: None,
        });

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

        let shadow_sampler = device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mip_bias: 0.0f32,
            max_anisotropy: 0.0f32,
            compare_op: Some(CompareFunc::Less),
            min_lod: 0f32,
            max_lod: None,
        });

        Self { sampler, shadow_sampler, pipeline }
    }

    #[profiling::function]
    pub(super) fn execute(
        &mut self,
        cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        device: &<P::GraphicsBackend as Backend>::Device,
        lightmap: &RendererTexture<P::GraphicsBackend>,
        zero_texture_view: &Arc<<P::GraphicsBackend as Backend>::TextureView>,
        resources: &RendererResources<P::GraphicsBackend>,
        shader_manager: &ShaderManager<P>
    ) {
        let (width, height) = {
            let info = resources.texture_info(Self::SHADING_TEXTURE_NAME);
            (info.width, info.height)
        };

        cmd_buffer.begin_label("Shading Pass");

        let output = resources.access_view(
            cmd_buffer,
            Self::SHADING_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let ids = resources.access_view(
            cmd_buffer,
            VisibilityBufferPass::PRIMITIVE_ID_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            TextureLayout::Storage,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let barycentrics = resources.access_view(
            cmd_buffer,
            VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            TextureLayout::Storage,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let light_bitmask_buffer = resources.access_buffer(
            cmd_buffer,
            super::light_binning::LightBinningPass::LIGHT_BINNING_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );

        let ssao = resources.access_view(
            cmd_buffer,
            SsaoPass::<P>::SSAO_TEXTURE_NAME,
            BarrierSync::FRAGMENT_SHADER | BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let rt_shadows: Ref<Arc<<P::GraphicsBackend as Backend>::TextureView>>;
        let shadows = if device.supports_ray_tracing() {
            rt_shadows = resources.access_view(
                cmd_buffer,
                RTShadowPass::SHADOWS_TEXTURE_NAME,
                BarrierSync::FRAGMENT_SHADER,
                BarrierAccess::SAMPLING_READ,
                TextureLayout::Sampled,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            );
            &*rt_shadows
        } else {
            zero_texture_view
        };

        let cascade_count = {
            let shadow_map_info = resources.texture_info(ShadowMapPass::<P>::SHADOW_MAP_NAME);
            shadow_map_info.array_length
        };

        let shadow_map = resources.access_view(
            cmd_buffer,
            ShadowMapPass::<P>::SHADOW_MAP_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_array_layer: 0,
                array_layer_length: cascade_count,
                mip_level_length: 1,
                base_mip_level: 0,
                format: None
            },
            HistoryResourceEntry::Current,
        );

        let pipeline = shader_manager.get_compute_pipeline(self.pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &ids);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 2, &barycentrics);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 3, &output);
        cmd_buffer.bind_sampler(BindingFrequency::VeryFrequent, 4, &self.sampler);
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            5,
            &light_bitmask_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            6,
            &lightmap.view,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            7,
            shadows,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            8,
            &ssao,
            resources.linear_sampler(),
        );

        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            9,
            &shadow_map,
            &self.shadow_sampler
        );

        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();

        cmd_buffer.dispatch((width + 7) / 8, (height + 7) / 8, 1);
        cmd_buffer.end_label();
    }
}
