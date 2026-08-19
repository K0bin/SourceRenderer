use std::sync::Arc;

use crate::asset::{AssetLoadPriority, AssetType, TextureHandle};
use crate::graphics::*;
use crate::renderer::asset::{
    GraphicsPipelineHandle, GraphicsPipelineInfo, PathPipelineShaderStage, RendererAssets,
    RendererAssetsReadOnly, RendererMaterial, RendererMaterialValue,
};
use crate::renderer::drawable::View;
use crate::renderer::passes::volume::ibl::ImageBasedLightingPreparation;
use crate::renderer::passes::volume::GeometryPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use crate::renderer::renderer_scene::RendererScene;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::TexturePlane;
use sourcerenderer_core::{HalfVec3, Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec4};

pub struct BackgroundPass {
    pipeline: GraphicsPipelineHandle,
}

impl BackgroundPass {
    pub(crate) fn new(
        device: &Arc<crate::graphics::Device>,
        assets: &RendererAssets,
        _init_cmd_buffer: &mut crate::graphics::CommandBuffer,
        resources: &mut RendererResources,
    ) -> Self {
        let shader_file_extension = "json";

        let vs_path = format!("shaders/fullscreen_quad.vert.{}", shader_file_extension);
        let fs_path = format!("shaders/background.frag.{}", shader_file_extension);
        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: PathPipelineShaderStage::empty_spec_consts(&vs_path),
            fs: Some(PathPipelineShaderStage::empty_spec_consts(&fs_path)),
            primitive_type: PrimitiveType::Triangles,
            vertex_layout: VertexLayoutInfo {
                input_assembler: &[],
                shader_inputs: &[],
            },
            rasterizer: RasterizerInfo {
                fill_mode: FillMode::Fill,
                cull_mode: CullMode::None,
                front_face: FrontFace::Clockwise,
                sample_count: SampleCount::Samples1,
            },
            depth_stencil: DepthStencilInfo {
                depth_test_enabled: false,
                depth_write_enabled: false,
                depth_func: CompareFunc::Always,
                stencil_enable: false,
                stencil_read_mask: 0u8,
                stencil_write_mask: 0u8,
                stencil_front: StencilInfo::default(),
                stencil_back: StencilInfo::default(),
            },
            blend: BlendInfo {
                alpha_to_coverage_enabled: false,
                logic_op_enabled: false,
                logic_op: LogicOp::And,
                constants: [0f32, 0f32, 0f32, 0f32],
                attachments: &[AttachmentBlendInfo {
                    blend_enabled: false,
                    src_color_blend_factor: BlendFactor::SrcAlpha,
                    dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                    color_blend_op: BlendOp::Add,
                    src_alpha_blend_factor: BlendFactor::Zero,
                    dst_alpha_blend_factor: BlendFactor::One,
                    alpha_blend_op: BlendOp::Add,
                    write_mask: ColorComponents::all(),
                }],
            },
            render_target_formats: &[Format::RGBA16UNorm],
            depth_stencil_format: Format::Unknown,
        };
        let pipeline = assets.request_graphics_pipeline(&pipeline_info);

        Self { pipeline }
    }

    #[inline(always)]
    pub(crate) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    pub(crate) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        scene: &RendererScene,
        view: &View,
        camera_buffer: &TransientBufferSlice,
        params: &RenderPassParameters,
        assets: &RendererAssetsReadOnly<'_>,
    ) {
        let resources = &params.resources;

        if !resources.has_resource(
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
        ) {
            return;
        }

        let color_view = resources.access_view(
            cmd_buffer,
            GeometryPass::COLOR_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let rt_info = color_view.texture().unwrap().info();

        let env_map_specular = resources.access_view(
            cmd_buffer,
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                mip_level_length: 1u32,
                format: None,
                plane: TexturePlane::Primary,
            },
            HistoryResourceEntry::Current,
        );

        cmd_buffer.flush_barriers();

        cmd_buffer.begin_label("Background");

        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[RenderTarget {
                view: &color_view,
                load_op: LoadOpColor::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil: None,
            query_range: None,
        });

        let pipeline: &Arc<GraphicsPipeline> = assets
            .get_graphics_pipeline(self.pipeline)
            .expect("Pipeline is not compiled yet");
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0.0f32, 0.0f32),
            extent: Vec2::new(rt_info.width as f32, rt_info.height as f32),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);
        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0, 0),
            extent: Vec2UI::new(rt_info.width, rt_info.height),
        }]);

        //let camera_buffer = cmd_buffer.upload_dynamic_data(&[view.proj_matrix * view.view_matrix], BufferUsage::CONSTANT);
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::Frame,
            0,
            BufferRef::Transient(camera_buffer),
            0,
            WHOLE_BUFFER,
        );

        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            0u32,
            &env_map_specular,
            resources.linear_sampler(),
        );
        cmd_buffer.finish_binding();
        cmd_buffer.draw(3u32, 1u32, 0u32, 0u32);

        cmd_buffer.end_render_pass();

        cmd_buffer.end_label();
    }
}
