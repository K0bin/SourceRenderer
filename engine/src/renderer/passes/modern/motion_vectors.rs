use std::sync::Arc;

use sourcerenderer_core::Vec2UI;

use crate::asset::AssetManager;
use crate::renderer::passes::modern::VisibilityBufferPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::asset::{ComputePipelineHandle, RendererAssetsReadOnly};
use crate::graphics::*;

pub struct MotionVectorPass {
    pipeline: ComputePipelineHandle,
}

impl MotionVectorPass {
    pub const MOTION_TEXTURE_NAME: &'static str = "Motion";

    pub fn new(
        resources: &mut RendererResources,
        renderer_resolution: Vec2UI,
        asset_manager: &Arc<AssetManager>
    ) -> Self {
        let pipeline =
            asset_manager.request_compute_pipeline("shaders/motion_vectors_vis_buf.comp.json");

        resources.create_texture(
            Self::MOTION_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RG16Float,
                width: renderer_resolution.x,
                height: renderer_resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
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
        cmd_buffer: &mut CommandBufferRecorder,
        pass_params: &RenderPassParameters<'_>
    ) {
        let pipeline = pass_params.assets.get_compute_pipeline(self.pipeline).unwrap();

        cmd_buffer.begin_label("Motion Vectors");

        let output_srv = pass_params.resources.access_view(
            cmd_buffer,
            Self::MOTION_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let (width, height) = {
            let info = output_srv.texture().unwrap().info();
            (info.width, info.height)
        };

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

        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &output_srv);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &ids);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 2, &barycentrics);
        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((width + 7) / 8, (height + 7) / 8, 1);
        cmd_buffer.end_label();
    }
}
