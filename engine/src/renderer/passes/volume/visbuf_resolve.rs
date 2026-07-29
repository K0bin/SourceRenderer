use crate::asset::TextureHandle;
use crate::graphics::{
    BackendTexture, Barrier, BufferRef, BufferSlice, CommandBuffer, Device, GraphicsPipeline,
    MemoryUsage, PipelineBinding, RenderPassBeginInfo, RenderTarget, StoreOp, TextureView,
};
use crate::renderer::asset::{
    ComputePipelineHandle, GraphicsPipelineHandle, GraphicsPipelineInfo, RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::passes::volume::ibl::ImageBasedLightingPreparation;
use crate::renderer::passes::volume::GeometryPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use sourcerenderer_core::gpu::{
    AttachmentBlendInfo, BarrierAccess, BarrierSync, BarrierTextureRange, BindingFrequency,
    BlendFactor, BlendInfo, BlendOp, BufferInfo, BufferUsage, ColorComponents, CompareFunc,
    CullMode, DepthStencilInfo, FillMode, Format, FrontFace, LoadOpColor, LogicOp, PrimitiveType,
    QueueSharingMode, RasterizerInfo, SampleCount, Scissor, ShaderInputElement, ShaderType,
    Texture, TextureDimension, TextureInfo, TextureLayout, TextureUsage, TextureViewInfo,
    VertexLayoutInfo, Viewport, WHOLE_BUFFER,
};
use sourcerenderer_core::{Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec4};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone)]
struct VisibilityBufferResolvePushData {
    model_matrix: Matrix4,
    inv_mode_matrix: Matrix4,
    threshold: f32,
    lod: u32,
    roughness: f32,
    metalness: f32,
    f0: Vec3,
}

pub struct VisibilityBufferResolvePass {
    pipeline: ComputePipelineHandle,
    color_view_name: &'static str,
}

impl VisibilityBufferResolvePass {
    pub fn new(
        device: &Arc<Device>,
        assets: &RendererAssets,
        resources: &mut RendererResources,
        swapchain: &crate::graphics::Swapchain,
        color_rt_name: &'static str,
    ) -> Self {
        let pipeline = assets.request_compute_pipeline("shaders/volume_visbuf_resolve.comp.json");

        /*resources.create_texture(
            GeometryPass::COLOR_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: swapchain.width(),
                height: swapchain.height(),
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET,
                supports_srgb: false,
            },
            false,
        );*/

        Self {
            pipeline,
            color_view_name: color_rt_name,
        }
    }

    #[inline(always)]
    pub(crate) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        params: &RenderPassParameters,
        visbuf_name: &str,
        depth_name: &str,
        index_buffer_name: &str,
        density_map_handle: TextureHandle,
        transfer_function_handle: TextureHandle,
        assets: &RendererAssetsReadOnly<'_>,
        model_matrix: Matrix4,
        threshold: f32,
        lod: u32,
    ) {
        let resources = &params.resources;

        if !resources.has_resource(
            ImageBasedLightingPreparation::FILTERED_DIFFUSE_ENVIRONMENT_MAP_TEXTURE_NAME,
        ) || !resources.has_resource(
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
        ) {
            return;
        }

        let color_tex_info = resources.texture_info(self.color_view_name);
        let color_tex_extent = Vec2UI::new(color_tex_info.width, color_tex_info.height);
        std::mem::drop(color_tex_info);

        let visbuf_view = resources.access_view(
            cmd_buffer,
            visbuf_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            TextureLayout::Storage,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        let depth_view = resources.access_view(
            cmd_buffer,
            depth_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        let color_texture = resources.access_view(
            cmd_buffer,
            self.color_view_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        let index_buffer = resources.access_buffer(
            cmd_buffer,
            index_buffer_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );

        let integration_lut = resources.access_view(
            cmd_buffer,
            ImageBasedLightingPreparation::PREINTEGRATION_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let env_map_diffuse = resources.access_view(
            cmd_buffer,
            ImageBasedLightingPreparation::FILTERED_DIFFUSE_ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let env_specular_info = resources.texture_info(
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
        );
        let env_specular_mips = env_specular_info.mip_levels;
        std::mem::drop(env_specular_info);
        let env_map_specular = resources.access_view(
            cmd_buffer,
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                mip_level_length: env_specular_mips,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        cmd_buffer.begin_label("Visibility buffer resolve");

        cmd_buffer.flush_barriers();

        let pipeline = params.assets.get_compute_pipeline(self.pipeline).unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(pipeline));

        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0u32, &visbuf_view);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1u32,
            &depth_view,
            resources.linear_sampler(),
        );

        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 2u32, &color_texture);
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            3u32,
            BufferRef::Regular(&*index_buffer),
            0,
            WHOLE_BUFFER,
        );

        let density_map = params.assets.get_texture(density_map_handle);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            4u32,
            &density_map.view,
            resources.linear_sampler(),
        );

        let transfer_function = assets.get_texture(transfer_function_handle);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            5u32,
            &transfer_function.view,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            6u32,
            &env_map_diffuse,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            7u32,
            &env_map_specular,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            8u32,
            &integration_lut,
            resources.linear_sampler(),
        );

        cmd_buffer.set_push_constant_data(
            &[VisibilityBufferResolvePushData {
                model_matrix,
                inv_mode_matrix: Matrix4::inverse(&model_matrix),
                threshold,
                lod,
                roughness: 0.6f32,
                metalness: 0.3f32,
                //roughness: 0.1f32,
                //metalness: 0.9f32,
                f0: Vec3::new(0.04f32, 0.04f32, 0.04f32),
            }],
            ShaderType::ComputeShader,
        );

        cmd_buffer.finish_binding();

        cmd_buffer.dispatch(
            (color_tex_extent.x + 7) / 8,
            (color_tex_extent.y + 7) / 8,
            1,
        );

        cmd_buffer.end_label();
    }
}
