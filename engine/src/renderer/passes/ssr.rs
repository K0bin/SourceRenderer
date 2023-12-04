use std::cell::Ref;
use std::sync::Arc;

use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::graphics::{
    Backend as GraphicsBackend,
    BarrierAccess,
    BarrierSync,
    BindingFrequency,
    CommandBuffer,
    Format,
    PipelineBinding,
    SampleCount,
    Texture,
    TextureDimension,
    TextureInfo,
    TextureLayout,
    TextureUsage,
    TextureView,
    TextureViewInfo,
};
use sourcerenderer_core::{
    Platform,
    Vec2UI,
};

use crate::renderer::passes::modern::VisibilityBufferPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    ComputePipelineHandle,
    ShaderManager,
};

pub struct SsrPass {
    pipeline: ComputePipelineHandle,
}

impl SsrPass {
    pub const SSR_TEXTURE_NAME: &'static str = "SSR";

    pub fn new<P: Platform>(
        resolution: Vec2UI,
        resources: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
        _visibility_buffer: bool,
    ) -> Self {
        resources.create_texture(
            Self::SSR_TEXTURE_NAME,
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
                supports_srgb: false,
            },
            false,
        );

        let pipeline = shader_manager.request_compute_pipeline("shaders/ssr.comp.spv");

        Self { pipeline }
    }

    pub fn execute<P: Platform>(
        &mut self,
        cmd_buffer: &mut <P::GPUBackend as GPUBackend>::CommandBuffer,
        params: &RenderPassParameters<'_, P>,
        input_name: &str,
        depth_name: &str,
        visibility_buffer: bool,
    ) {
        // TODO: merge back into the original image
        // TODO: specularity map

        let ssr_uav = params.resources.access_view(
            cmd_buffer,
            Self::SSR_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let depth_srv = params.resources.access_view(
            cmd_buffer,
            depth_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let color_srv = params.resources.access_view(
            cmd_buffer,
            input_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let mut ids =
            Option::<Ref<Arc<<P::GraphicsBackend as GraphicsBackend>::TextureView>>>::None;
        let mut barycentrics =
            Option::<Ref<Arc<<P::GraphicsBackend as GraphicsBackend>::TextureView>>>::None;

        if visibility_buffer {
            ids = Some(params.resources.access_view(
                cmd_buffer,
                VisibilityBufferPass::PRIMITIVE_ID_TEXTURE_NAME,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_READ,
                TextureLayout::Storage,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            ));

            barycentrics = Some(params.resources.access_view(
                cmd_buffer,
                VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_READ,
                TextureLayout::Storage,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            ));
        }

        let pipeline = params.shader_manager.get_compute_pipeline(self.pipeline);
        cmd_buffer.begin_label("SSR pass");
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.flush_barriers();
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &ssr_uav);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1,
            &*color_srv,
            params.resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            2,
            &*depth_srv,
            params.resources.linear_sampler(),
        );
        if visibility_buffer {
            cmd_buffer.bind_storage_texture(
                BindingFrequency::VeryFrequent,
                3,
                ids.as_ref().unwrap(),
            );
            cmd_buffer.bind_storage_texture(
                BindingFrequency::VeryFrequent,
                4,
                barycentrics.as_ref().unwrap(),
            );
        }
        cmd_buffer.finish_binding();
        let ssr_info = ssr_uav.texture().info();
        cmd_buffer.dispatch(
            (ssr_info.width + 7) / 8,
            (ssr_info.height + 7) / 8,
            ssr_info.depth,
        );
        cmd_buffer.end_label();
    }
}
