use sourcerenderer_core::{Vec2, Vec2I, Vec2UI};

use crate::{graphics::*, renderer::{asset::{GraphicsPipelineHandle, GraphicsPipelineInfo, RendererAssets, RendererAssetsReadOnly}, renderer_resources::RendererResources}};

pub struct BlitPass {
    pipeline_handle: GraphicsPipelineHandle
}

impl BlitPass {
    #[allow(unused)]
    pub fn new(
        _barriers: &mut RendererResources,
        assets: &RendererAssets,
        dst_format: Format
    ) -> Self {
        let pipeline = assets.request_graphics_pipeline(
            &GraphicsPipelineInfo {
                vs: "shaders/fullscreen_quad.vert.json",
                fs: Some("shaders/blit.frag.json"),
                vertex_layout: VertexLayoutInfo {
                    shader_inputs: &[],
                    input_assembler: &[],
                },
                rasterizer: RasterizerInfo::default(),
                depth_stencil: DepthStencilInfo {
                    depth_test_enabled: false,
                    depth_write_enabled: false,
                    ..Default::default()
                },
                blend: BlendInfo {
                    alpha_to_coverage_enabled: false,
                    logic_op_enabled: false,
                    logic_op: LogicOp::Noop,
                    attachments: &[AttachmentBlendInfo::default()],
                    constants: [1f32, 1f32, 1f32, 1f32],
                },
                primitive_type: PrimitiveType::Triangles,
                render_target_formats: &[dst_format],
                depth_stencil_format: Format::Unknown
            }
        );

        Self {
            pipeline_handle: pipeline
        }
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline_handle).is_some()
    }

    #[profiling::function]
    pub(super) fn execute(
        &mut self,
        _graphics_context: &GraphicsContext,
        cmd_buffer: &mut CommandBuffer,
        assets: &RendererAssetsReadOnly<'_>,
        src_view: &TextureView,
        dst_view: &TextureView,
        sampler: &Sampler,
        dst_resolution: Vec2UI
    ) {
        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[RenderTarget {
                view: dst_view,
                load_op: LoadOpColor::DontCare,
                store_op: StoreOp::Store
            }],
            depth_stencil: None,
            query_range: None,
        });

        let pipeline = assets.get_graphics_pipeline(self.pipeline_handle).unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));

        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0i32, 0i32),
            extent: dst_resolution,
        }]);
        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0f32, 0f32),
            extent: Vec2::new(
                dst_resolution.x as f32,
                dst_resolution.y as f32,
            ),
            min_depth: 0f32,
            max_depth: 1f32,
        }]);

        cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, src_view, sampler);
        cmd_buffer.finish_binding();
        cmd_buffer.draw(3, 1, 0, 0);

        cmd_buffer.end_render_pass();
    }
}