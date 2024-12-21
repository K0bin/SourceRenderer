use std::{sync::Arc, io::Read};

use sourcerenderer_core::{gpu::PackedShader, platform::IO, Platform, Vec2};

use crate::{renderer::{renderer_resources::HistoryResourceEntry, render_path::RenderPassParameters}, ui::UIDrawData};
use crate::graphics::*;

pub struct UIPass<P: Platform> {
    device: Arc<Device<P::GPUBackend>>,
    pipeline: Arc<GraphicsPipeline<P::GPUBackend>>,
}

impl<P: Platform> UIPass<P> {
    pub fn new(device: &Arc<Device<P::GPUBackend>>) -> Self {
        let vs = {
            let mut file = <P::IO as IO>::open_asset("shaders/dear_imgui.vert.json").unwrap();
            let mut bytes: Vec<u8> = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            let shader: PackedShader = serde_json::from_slice(&bytes).unwrap();
            device.create_shader(shader, Some("DearImguiVS"))
        };

        let ps = {
            let mut file = <P::IO as IO>::open_asset("shaders/dear_imgui.frag.json").unwrap();
            let mut bytes: Vec<u8> = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            let shader: PackedShader = serde_json::from_slice(&bytes).unwrap();
            device.create_shader(shader, Some("DearImguiPS"))
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
            render_target_formats: &[Format::RGBA8UNorm],
            depth_stencil_format: Format::Unknown
        }, Some("DearImgui"));

        Self {
            device: device.clone(),
            pipeline,
        }
    }

    pub fn execute(
        &mut self,
        command_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>,
        output_texture_name: &str,
        draw: &UIDrawData<P::GPUBackend>
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
                    store_op: StoreOp::<P::GPUBackend>::Store
                }
            ],
            depth_stencil: None
        }, RenderpassRecordingMode::Commands);

        for list in &draw.draw_lists {
            command_buffer.set_index_buffer(BufferRef::Regular(&list.index_buffer), 0, if std::mem::size_of::<imgui::DrawIdx>() == 2 { IndexFormat::U16 } else { IndexFormat::U32 });
            command_buffer.set_vertex_buffer(0, BufferRef::Regular(&list.vertex_buffer), 0);

            for draw in &list.draws {
                command_buffer.set_scissors(&[
                    draw.scissor.clone()
                ]);

                if let Some(texture) = &draw.texture {
                    command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, texture, pass_params.resources.linear_sampler());
                } else {
                    command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, pass_params.zero_textures.zero_texture_view, pass_params.resources.linear_sampler());
                }

                command_buffer.finish_binding();
                command_buffer.draw_indexed(1, 0, draw.index_count, draw.first_index, draw.vertex_offset as i32);
            }
        }
        command_buffer.end_render_pass();
    }
}
