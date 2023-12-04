use sourcerenderer_core::graphics::{
    Backend as GraphicsBackend,
    BarrierAccess,
    BarrierSync,
    BindingFrequency,
    BufferUsage,
    CommandBuffer,
    Format,
    PipelineBinding,
    Texture,
    TextureDimension,
    TextureInfo,
    TextureLayout,
    TextureUsage,
    TextureView,
    TextureViewInfo,
    WHOLE_BUFFER,
};
use sourcerenderer_core::{
    Platform,
    Vec2UI,
};

use super::taa::TAAPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    ComputePipelineHandle,
    ShaderManager,
};

const USE_CAS: bool = true;

pub struct SharpenPass {
    pipeline: ComputePipelineHandle,
}

impl SharpenPass {
    pub const SHAPENED_TEXTURE_NAME: &'static str = "Sharpened";

    pub fn new<P: Platform>(
        resolution: Vec2UI,
        resources: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let pipeline = shader_manager.request_compute_pipeline(if !USE_CAS {
            "shaders/sharpen.comp.spv"
        } else {
            "shaders/cas.comp.spv"
        });

        resources.create_texture(
            Self::SHAPENED_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: sourcerenderer_core::graphics::SampleCount::Samples1,
                usage: TextureUsage::STORAGE | TextureUsage::COPY_SRC,
                supports_srgb: false,
            },
            false,
        );

        Self { pipeline }
    }

    pub fn execute<P: Platform>(
        &mut self,
        cmd_buffer: &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer,
        pass_params: &RenderPassParameters<'_, P>
    ) {
        let input_image_uav = pass_params.resources.access_view(
            cmd_buffer,
            TAAPass::TAA_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            TextureLayout::Storage,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let sharpen_uav = pass_params.resources.access_view(
            cmd_buffer,
            Self::SHAPENED_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        cmd_buffer.begin_label("Sharpening pass");

        let pipeline = pass_params.shader_manager.get_compute_pipeline(self.pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        let sharpen_setup_ubo = cmd_buffer.upload_dynamic_data(&[0.3f32], BufferUsage::CONSTANT);
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            2,
            &sharpen_setup_ubo,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &*input_image_uav);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &*sharpen_uav);
        cmd_buffer.finish_binding();

        let info = sharpen_uav.texture().info();
        cmd_buffer.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
        cmd_buffer.end_label();
    }
}
