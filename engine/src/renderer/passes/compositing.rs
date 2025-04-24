use sourcerenderer_core::Vec2UI;

use super::ssr::SsrPass;
use crate::graphics::*;
use crate::renderer::asset::{
    ComputePipelineHandle,
    RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};

pub struct CompositingPass {
    pipeline: ComputePipelineHandle,
}

impl CompositingPass {
    pub const COMPOSITION_TEXTURE_NAME: &'static str = "Composition";

    #[allow(unused)]
    pub fn new(
        resolution: Vec2UI,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        let pipeline = assets.request_compute_pipeline("shaders/compositing.comp.json");

        resources.create_texture(
            Self::COMPOSITION_TEXTURE_NAME,
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

        Self { pipeline }
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        params: &RenderPassParameters<'_>,
        input_name: &str,
    ) {
        let input_image = params.resources.access_view(
            cmd_buffer,
            input_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let ssr = params.resources.access_view(
            cmd_buffer,
            SsrPass::SSR_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let output: std::cell::Ref<'_, std::sync::Arc<TextureView>> = params.resources.access_view(
            cmd_buffer,
            Self::COMPOSITION_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        cmd_buffer.begin_label("Compositing pass");

        let pipeline = params.assets.get_compute_pipeline(self.pipeline).unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));

        #[repr(C)]
        #[derive(Debug, Clone)]
        struct Setup {
            gamma: f32,
            exposure: f32,
        }
        let setup_ubo = cmd_buffer
            .upload_dynamic_data(
                &[Setup {
                    gamma: 2.2f32,
                    exposure: 0.01f32,
                }],
                BufferUsage::CONSTANT,
            )
            .unwrap();

        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &output);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1,
            &input_image,
            params.resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            2,
            &ssr,
            params.resources.linear_sampler(),
        );
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            3,
            BufferRef::Transient(&setup_ubo),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.finish_binding();

        let info = output.texture().unwrap().info();
        cmd_buffer.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
        cmd_buffer.end_label();
    }
}
