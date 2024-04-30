use std::sync::Arc;

use sourcerenderer_core::Platform;

use crate::{graphics::*, renderer::{render_path::RenderPassParameters, renderer_resources::RendererResources, shader_manager::{GraphicsPipelineHandle, GraphicsPipelineInfo, ShaderManager}}};

pub struct BlitPass {
    pipeline_handle: GraphicsPipelineHandle
}

impl BlitPass {
    pub fn new<P: Platform>(
        barriers: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
        dst_format: Format
    ) -> Self {
        let pipeline = shader_manager.request_graphics_pipeline(
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
            },
            &RenderPassInfo {
                attachments: &[
                    AttachmentInfo {
                        format: dst_format,
                        samples: SampleCount::Samples1,
                    },
                ],
                subpasses: &[SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[
                        OutputAttachmentRef {
                            index: 0,
                            resolve_attachment_index: None,
                        },
                    ],
                    depth_stencil_attachment: None,
                }],
            },
            0,
        );

        Self {
            pipeline_handle: pipeline
        }
    }

    #[profiling::function]
    pub(super) fn execute<P: Platform>(
        &mut self,
        _graphics_context: &GraphicsContext<P::GPUBackend>,
        cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        shader_manager: &ShaderManager<P>,
        src_view: &TextureView<P::GPUBackend>,
        dst_view: &TextureView<P::GPUBackend>,
        sampler: &Sampler<P::GPUBackend>
    ) {
        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            attachments: &[
                RenderPassAttachment {
                    view: RenderPassAttachmentView::RenderTarget(dst_view), load_op: LoadOp::DontCare, store_op: StoreOp::Store
                }
            ], subpasses: &[
                SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[
                        OutputAttachmentRef {
                            index: 0,
                            resolve_attachment_index: None
                        }
                    ], depth_stencil_attachment: None
                }
            ]

        }, RenderpassRecordingMode::Commands);

        let pipeline = shader_manager.get_graphics_pipeline(self.pipeline_handle);
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));

        cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, src_view, sampler);
        cmd_buffer.finish_binding();
        cmd_buffer.draw(4, 0);

        cmd_buffer.end_render_pass();
    }
}