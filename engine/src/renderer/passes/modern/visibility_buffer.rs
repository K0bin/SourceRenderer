use sourcerenderer_core::{
    Vec2,
    Vec2I,
    Vec2UI,
};

use super::draw_prep::DrawPrepPass;
use super::gpu_scene::DRAW_CAPACITY;
use crate::graphics::*;
use crate::renderer::asset::{
    GraphicsPipelineHandle,
    GraphicsPipelineInfo,
    RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};

pub struct VisibilityBufferPass {
    pipeline: GraphicsPipelineHandle,
}

impl VisibilityBufferPass {
    pub const BARYCENTRICS_TEXTURE_NAME: &'static str = "barycentrics";
    pub const PRIMITIVE_ID_TEXTURE_NAME: &'static str = "primitive";
    pub const DEPTH_TEXTURE_NAME: &'static str = "depth";

    pub fn new(
        resolution: Vec2UI,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        let barycentrics_texture_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::RG16UNorm,
            width: resolution.x,
            height: resolution.y,
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::SAMPLED
                | TextureUsage::RENDER_TARGET
                | TextureUsage::COPY_SRC
                | TextureUsage::STORAGE,
            supports_srgb: false,
        };
        resources.create_texture(
            Self::BARYCENTRICS_TEXTURE_NAME,
            &barycentrics_texture_info,
            false,
        );

        let primitive_id_texture_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::R32UInt,
            width: resolution.x,
            height: resolution.y,
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::SAMPLED
                | TextureUsage::RENDER_TARGET
                | TextureUsage::COPY_SRC
                | TextureUsage::STORAGE,
            supports_srgb: false,
        };
        resources.create_texture(
            Self::PRIMITIVE_ID_TEXTURE_NAME,
            &primitive_id_texture_info,
            false,
        );

        let depth_texture_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::D24S8,
            width: resolution.x,
            height: resolution.y,
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::SAMPLED | TextureUsage::DEPTH_STENCIL,
            supports_srgb: false,
        };
        resources.create_texture(Self::DEPTH_TEXTURE_NAME, &depth_texture_info, true);

        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: "shaders/visibility_buffer.vert.json",
            fs: Some("shaders/visibility_buffer.frag.json"),
            primitive_type: PrimitiveType::Triangles,
            vertex_layout: VertexLayoutInfo {
                input_assembler: &[InputAssemblerElement {
                    binding: 0,
                    stride: 64,
                    input_rate: InputRate::PerVertex,
                }],
                shader_inputs: &[ShaderInputElement {
                    input_assembler_binding: 0,
                    location_vk_mtl: 0,
                    semantic_name_d3d: String::from(""),
                    semantic_index_d3d: 0,
                    offset: 0,
                    format: Format::RGB32Float,
                }],
            },
            rasterizer: RasterizerInfo {
                fill_mode: FillMode::Fill,
                cull_mode: CullMode::Back,
                front_face: FrontFace::Clockwise,
                sample_count: SampleCount::Samples1,
            },
            depth_stencil: DepthStencilInfo {
                depth_test_enabled: true,
                depth_write_enabled: true,
                depth_func: CompareFunc::LessEqual,
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
                attachments: &[
                    AttachmentBlendInfo::default(),
                    AttachmentBlendInfo::default(),
                ],
            },
            render_target_formats: &[
                primitive_id_texture_info.format,
                barycentrics_texture_info.format,
            ],
            depth_stencil_format: depth_texture_info.format,
        };
        let pipeline = assets.request_graphics_pipeline(&pipeline_info);

        Self { pipeline }
    }

    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    #[profiling::function]
    pub(super) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        params: &RenderPassParameters<'_>,
    ) {
        cmd_buffer.begin_label("Visibility Buffer pass");
        let draw_buffer = params.resources.access_buffer(
            cmd_buffer,
            DrawPrepPass::INDIRECT_DRAW_BUFFER,
            BarrierSync::INDIRECT,
            BarrierAccess::INDIRECT_READ,
            HistoryResourceEntry::Current,
        );

        let barycentrics_rtv = params.resources.access_view(
            cmd_buffer,
            Self::BARYCENTRICS_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let primitive_id_rtv = params.resources.access_view(
            cmd_buffer,
            Self::PRIMITIVE_ID_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let dsv = params.resources.access_view(
            cmd_buffer,
            Self::DEPTH_TEXTURE_NAME,
            BarrierSync::LATE_DEPTH | BarrierSync::EARLY_DEPTH,
            BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
            TextureLayout::DepthStencilReadWrite,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[
                RenderTarget {
                    view: &primitive_id_rtv,
                    load_op: LoadOpColor::Clear(ClearColor::BLACK),
                    store_op: StoreOp::Store,
                },
                RenderTarget {
                    view: &barycentrics_rtv,
                    load_op: LoadOpColor::Clear(ClearColor::BLACK),
                    store_op: StoreOp::Store,
                },
            ],
            depth_stencil: Some(&DepthStencilAttachment {
                view: &dsv,
                load_op: LoadOpDepthStencil::Clear(ClearDepthStencilValue::DEPTH_ONE),
                store_op: StoreOp::Store,
            }),
            query_range: None,
        });

        let rtv_info = barycentrics_rtv.texture().unwrap().info();
        let pipeline = params.assets.get_graphics_pipeline(self.pipeline).unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0.0f32, 0.0f32),
            extent: Vec2::new(rtv_info.width as f32, rtv_info.height as f32),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);
        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0, 0),
            extent: Vec2UI::new(rtv_info.width, rtv_info.height),
        }]);

        cmd_buffer.set_vertex_buffer(0, params.scene.vertex_buffer, 0);
        cmd_buffer.set_index_buffer(params.scene.index_buffer, 0, IndexFormat::U32);

        cmd_buffer.finish_binding();
        cmd_buffer.draw_indexed_indirect_count(
            BufferRef::Regular(&draw_buffer),
            4,
            BufferRef::Regular(&draw_buffer),
            0,
            DRAW_CAPACITY,
            20,
        );

        cmd_buffer.end_render_pass();
        cmd_buffer.end_label();
    }
}
