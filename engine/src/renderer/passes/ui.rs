use std::{sync::Arc, io::Read};

use sourcerenderer_core::{Vec2, Platform, graphics::{Backend, CommandBuffer, BufferUsage, Device, ShaderType, GraphicsPipelineInfo, VertexLayoutInfo, ShaderInputElement, Format, InputAssemblerElement, InputRate, RasterizerInfo, FillMode, CullMode, FrontFace, SampleCount, DepthStencilInfo, CompareFunc, BlendInfo, AttachmentBlendInfo, BlendFactor, BlendOp, ColorComponents, PrimitiveType, RenderPassInfo, AttachmentInfo, SubpassInfo, AttachmentRef, RenderPassPipelineStage, OutputAttachmentRef, PipelineBinding, RenderpassRecordingMode, RenderPassBeginInfo, Scissor, Viewport, IndexFormat, TextureInfo, TextureDimension, TextureUsage, MemoryUsage, BindingFrequency, BarrierSync, BarrierAccess, TextureLayout, TextureViewInfo, RenderPassAttachment, RenderPassAttachmentView, LoadOp, StoreOp}, platform::IO, Vec2I, Vec2UI};

use crate::{renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, passes::compositing::CompositingPass}, ui::{UICmdList, UIDrawData}};

pub struct UIPass<P: Platform> {
    device: Arc<<P::GraphicsBackend as Backend>::Device>,
    pipeline: Arc<<P::GraphicsBackend as Backend>::GraphicsPipeline>,
}

impl<P: Platform> UIPass<P> {
    pub fn new(device: &Arc<<P::GraphicsBackend as Backend>::Device>) -> Self {
        let vs = {
            let mut file = <P::IO as IO>::open_asset("shaders/dear_imgui.vert.spv").unwrap();
            let mut bytes: Vec<u8> = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            device.create_shader(ShaderType::VertexShader, &bytes, Some("DearImguiVS"))
        };

        let ps = {
            let mut file = <P::IO as IO>::open_asset("shaders/dear_imgui.frag.spv").unwrap();
            let mut bytes: Vec<u8> = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            device.create_shader(ShaderType::FragmentShader, &bytes, Some("DearImguiPS"))
        };

        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineInfo {
            vs: &vs,
            fs: Some(&ps),
            vertex_layout: VertexLayoutInfo {
                shader_inputs: &[
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 0,
                        semantic_name_d3d: "aPos".to_string(),
                        semantic_index_d3d: 0,
                        offset: 0,
                        format: Format::RG32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 1,
                        semantic_name_d3d: "aUV".to_string(),
                        semantic_index_d3d: 0,
                        offset: 8,
                        format: Format::RG32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 2,
                        semantic_name_d3d: "aColor".to_string(),
                        semantic_index_d3d: 0,
                        offset: 16,
                        format: Format::RGBA8UNorm,
                    },
                ],
                input_assembler: &[
                    InputAssemblerElement {
                        binding: 0,
                        input_rate: InputRate::PerVertex,
                        stride: 20,
                    },
                ],
            },
            rasterizer: RasterizerInfo {
                fill_mode: FillMode::Fill,
                cull_mode: CullMode::None,
                front_face: FrontFace::CounterClockwise,
                sample_count: SampleCount::Samples1,
            },
            depth_stencil: DepthStencilInfo::default(),
            blend: BlendInfo {
                attachments: &[
                    AttachmentBlendInfo {
                        blend_enabled: true,
                        src_color_blend_factor: BlendFactor::SrcAlpha,
                        dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                        color_blend_op: BlendOp::Add,
                        src_alpha_blend_factor: BlendFactor::One,
                        dst_alpha_blend_factor: BlendFactor::OneMinusSrcAlpha,
                        alpha_blend_op: BlendOp::Add,
                        write_mask: ColorComponents::all(),
                    }
                ],
                ..Default::default()
            },
            primitive_type: PrimitiveType::Triangles,
        }, &RenderPassInfo {
            attachments: &[
                AttachmentInfo {
                    format: Format::RGBA8UNorm,
                    samples: SampleCount::Samples1,
                }
            ],
            subpasses: &[
                SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[
                        OutputAttachmentRef { index: 0, resolve_attachment_index: None }
                    ],
                    depth_stencil_attachment: None,
                }
            ]
        }, 0, Some("DearImgui"));

        Self {
            device: device.clone(),
            pipeline,
        }
    }

    pub fn execute(
        &mut self,
        command_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        renderer_resources: &RendererResources<P::GraphicsBackend>,
        zero_view: &Arc<<P::GraphicsBackend as Backend>::TextureView>,
        output_texture_name: &str,
        draw: &UIDrawData<P::GraphicsBackend>
    ) {
        let rtv = renderer_resources.access_view(
            command_buffer,
            output_texture_name,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current
        );

        command_buffer.flush_barriers();

        command_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));

        if draw.viewport.extent.x <= 0f32 || draw.viewport.extent.y <= 0f32 {
            return;
        }

        #[repr(C)]
        #[derive(Debug, Clone)]
        struct ImguiPushConstants {
            scale: Vec2,
            translate: Vec2
        }
        command_buffer.upload_dynamic_data_inline(&[ImguiPushConstants {
            scale: draw.scale,
            translate: draw.translate,
        }], ShaderType::VertexShader);

        command_buffer.set_viewports(&[draw.viewport.clone()]);

        command_buffer.begin_render_pass(&RenderPassBeginInfo {
            attachments: &[
                RenderPassAttachment {
                    view: RenderPassAttachmentView::RenderTarget(&rtv),
                    load_op: LoadOp::Load,
                    store_op: StoreOp::Store,
                }
            ],
            subpasses: &[
                SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[
                        OutputAttachmentRef { index: 0, resolve_attachment_index: None }
                    ],
                    depth_stencil_attachment: None,
                }
            ],
        }, RenderpassRecordingMode::Commands);

        for list in &draw.draw_lists {
            command_buffer.set_index_buffer(&list.index_buffer, 0, if std::mem::size_of::<imgui::DrawIdx>() == 2 { IndexFormat::U16 } else { IndexFormat::U32 });
            command_buffer.set_vertex_buffer(&list.vertex_buffer, 0);

            for draw in &list.draws {
                command_buffer.set_scissors(&[
                    draw.scissor.clone()
                ]);

                if let Some(texture) = &draw.texture {
                    command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, texture, renderer_resources.linear_sampler());
                } else {
                    command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, zero_view, renderer_resources.linear_sampler());
                }

                command_buffer.finish_binding();
                command_buffer.draw_indexed(1, 0, draw.index_count, draw.first_index, draw.vertex_offset as i32);
            }
        }
        command_buffer.end_render_pass();
    }
}