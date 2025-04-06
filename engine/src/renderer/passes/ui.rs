use std::sync::Arc;

use sourcerenderer_core::Vec2;

use crate::{asset::AssetManager, renderer::{asset::{GraphicsPipelineHandle, RendererAssetsReadOnly}, render_path::RenderPassParameters, renderer_resources::HistoryResourceEntry}, ui::UIDrawData};
use crate::graphics::*;
use crate::renderer::asset::GraphicsPipelineInfo;

pub struct UIPass {
    pipeline: GraphicsPipelineHandle,
}

impl UIPass {
    #[allow(unused)]
    pub fn new(device: &Arc<Device>, asset_manager: &Arc<AssetManager>) -> Self {
        let pipeline = asset_manager.request_graphics_pipeline(&GraphicsPipelineInfo {
            vs: "shaders/dear_imgui.vert.json",
            fs: Some("shaders/dear_imgui.frag.json"),
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
            render_target_formats: &[Format::RGBA8UNorm],
            depth_stencil_format: Format::Unknown
        });

        Self {
            pipeline,
        }
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        command_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters<'_>,
        output_texture_name: &str,
        draw: &UIDrawData
    ) {
        let rtv = pass_params.resources.access_view(
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

        let pipeline = pass_params.assets.get_graphics_pipeline(self.pipeline).unwrap();
        command_buffer.set_pipeline(PipelineBinding::Graphics(pipeline));

        if draw.viewport.extent.x <= 0f32 || draw.viewport.extent.y <= 0f32 {
            return;
        }

        #[repr(C)]
        #[derive(Debug, Clone)]
        struct ImguiPushConstants {
            scale: Vec2,
            translate: Vec2
        }
        command_buffer.set_push_constant_data(&[ImguiPushConstants {
            scale: draw.scale,
            translate: draw.translate,
        }], ShaderType::VertexShader);

        command_buffer.set_viewports(&[draw.viewport.clone()]);

        command_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[
                RenderTarget {
                    view: &rtv,
                    load_op: LoadOpColor::Load,
                    store_op: StoreOp::Store
                }
            ],
            depth_stencil: None,
            query_range: None,
        });

        for list in &draw.draw_lists {
            command_buffer.set_index_buffer(BufferRef::Regular(&list.index_buffer), 0, IndexFormat::U32); //if std::mem::size_of::<imgui::DrawIdx>() == 2 { IndexFormat::U16 } else { IndexFormat::U32 });
            command_buffer.set_vertex_buffer(0, BufferRef::Regular(&list.vertex_buffer), 0);

            for draw in &list.draws {
                command_buffer.set_scissors(&[
                    draw.scissor.clone()
                ]);

                if let Some(texture) = &draw.texture {
                    command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, texture, pass_params.resources.linear_sampler());
                } else {
                    command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, &pass_params.assets.get_placeholder_texture_white().view, pass_params.resources.linear_sampler());
                }

                command_buffer.finish_binding();
                command_buffer.draw_indexed(draw.index_count, 1, draw.first_index, draw.vertex_offset as i32, 1);
            }
        }
        command_buffer.end_render_pass();
    }
}
