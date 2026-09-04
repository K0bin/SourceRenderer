use crate::asset::{
    AssetData, AssetHandle, AssetLoadPriority, AssetType, TextureData, TextureHandle,
};
use crate::graphics::*;
use crate::renderer::asset::{
    GraphicsPipelineHandle, PathPipelineShaderStage, RendererAssets, RendererAssetsReadOnly,
};
use crate::renderer::renderer_resources::RendererResources;
use bytemuck::{Pod, Zeroable, box_bytes_of};
use dear_imgui_rs::{
    DrawCmdSnapshot, FrameSnapshot, SnapshotTextureId, TextureBinding, TextureFeedback, TextureId,
    TextureOp,
};
use smallvec::{SmallVec, smallvec};
use sourcerenderer_core::gpu::Texture as _;
use sourcerenderer_core::{Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec3UI};
use std::collections::HashMap;
use std::sync::Arc;

pub struct DearImguiRenderer {
    textures: HashMap<SnapshotTextureId, TextureHandle>,
    pipeline: GraphicsPipelineHandle,
}

impl DearImguiRenderer {
    pub fn new(
        _device: &Device,
        _resources: &mut RendererResources,
        assets: &RendererAssets,
        rt_format: Format,
    ) -> Self {
        let pipeline =
            assets.request_graphics_pipeline(&crate::renderer::asset::GraphicsPipelineInfo {
                vs: PathPipelineShaderStage::empty_spec_consts("shaders/dear_imgui.vert.json"),
                fs: Some(PathPipelineShaderStage::empty_spec_consts(
                    "shaders/dear_imgui.frag.json",
                )),
                vertex_layout: VertexLayoutInfo {
                    shader_inputs: &[
                        ShaderInputElement {
                            input_assembler_binding: 0,
                            location_vk_mtl: 0,
                            semantic_name_d3d: "".to_string(),
                            semantic_index_d3d: 0,
                            offset: 0,
                            format: Format::RG32Float,
                        },
                        ShaderInputElement {
                            input_assembler_binding: 0,
                            location_vk_mtl: 1,
                            semantic_name_d3d: "".to_string(),
                            semantic_index_d3d: 0,
                            offset: 8,
                            format: Format::RG32Float,
                        },
                        ShaderInputElement {
                            input_assembler_binding: 0,
                            location_vk_mtl: 2,
                            semantic_name_d3d: "".to_string(),
                            semantic_index_d3d: 0,
                            offset: 16,
                            format: Format::RGBA8UNorm,
                        },
                    ],
                    input_assembler: &[InputAssemblerElement {
                        binding: 0,
                        input_rate: InputRate::PerVertex,
                        stride: std::mem::size_of::<ImguiDrawVert>(),
                    }],
                },
                rasterizer: RasterizerInfo {
                    cull_mode: CullMode::None,
                    front_face: FrontFace::Clockwise,
                    fill_mode: FillMode::Fill,
                    sample_count: SampleCount::Samples1,
                },
                depth_stencil: DepthStencilInfo {
                    depth_test_enabled: false,
                    depth_write_enabled: false,
                    stencil_enable: false,
                    ..Default::default()
                },
                blend: BlendInfo {
                    attachments: &[AttachmentBlendInfo {
                        blend_enabled: true,
                        dst_alpha_blend_factor: BlendFactor::One,
                        src_alpha_blend_factor: BlendFactor::Zero,
                        dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                        src_color_blend_factor: BlendFactor::SrcAlpha,
                        color_blend_op: BlendOp::Add,
                        alpha_blend_op: BlendOp::Add,
                        write_mask: ColorComponents::all(),
                    }],
                    ..Default::default()
                },
                primitive_type: PrimitiveType::Triangles,
                render_target_formats: &[rt_format],
                depth_stencil_format: Format::Unknown,
            });

        Self {
            pipeline,
            textures: HashMap::new(),
        }
    }

    pub(in crate::renderer) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        command_buffer: &mut CommandBuffer,
        renderer_assets: &RendererAssets,
        resources: &RendererResources,
        snapshot: FrameSnapshot,
        backbuffer_view: &Arc<TextureView>,
        backbuffer_handle: &BackendTexture,
    ) {
        let mut feedback = Vec::<TextureFeedback>::new();

        {
            let assets_readonly = renderer_assets.read();
            let mut layout_transfer_textures = SmallVec::<[&Texture; 4]>::new();
            for texture_request in snapshot.texture_requests() {
                match texture_request.operation() {
                    TextureOp::Update { .. } => {
                        let handle = *self.textures.get(&texture_request.texture()).unwrap();
                        let renderer_texture = assets_readonly.get_texture(handle);
                        let texture = renderer_texture.texture();
                        layout_transfer_textures.push(texture);
                    }
                    _ => {}
                }
            }

            for texture in layout_transfer_textures {
                command_buffer.barrier(&[Barrier::TextureBarrier {
                    old_sync: BarrierSync::FRAGMENT_SHADER | BarrierSync::COPY,
                    new_sync: BarrierSync::COPY,
                    old_layout: TextureLayout::Sampled,
                    new_layout: TextureLayout::CopyDst,
                    old_access: BarrierAccess::empty(),
                    new_access: BarrierAccess::COPY_WRITE,
                    texture,
                    range: Default::default(),
                    queue_ownership: None,
                }]);
            }
        }

        for texture_request in snapshot.texture_requests() {
            match texture_request.operation() {
                TextureOp::Create {
                    format,
                    width,
                    height,
                    row_pitch,
                    pixels,
                } => {
                    assert_eq!(*row_pitch, imgui_tight_pitch(*format, *width) as usize);
                    let data = pixels.clone();
                    let data_box = box_bytes_of(data.into_boxed_slice());

                    let handle = renderer_assets.integrate(
                        AssetData::Texture(TextureData {
                            info: TextureInfo {
                                dimension: TextureDimension::Dim2D,
                                width: *width,
                                height: *height,
                                depth: 1,
                                mip_levels: 1,
                                array_length: 1,
                                samples: SampleCount::Samples1,
                                usage: TextureUsage::INITIAL_COPY
                                    | TextureUsage::COPY_DST
                                    | TextureUsage::SAMPLED,
                                format: imgui_format_to_format(*format),
                                supports_srgb: false,
                            },
                            data: smallvec![data_box],
                        }),
                        AssetLoadPriority::Normal,
                    );
                    let texture_id = TextureId::new(handle.index());

                    self.textures
                        .insert(texture_request.texture(), handle.into());
                    match texture_request.uploaded(texture_id) {
                        Ok(f) => {
                            feedback.push(f);
                        }
                        Err(e) => {
                            log::error!("DearImgui error: {:?}", e);
                        }
                    }
                }
                TextureOp::Update {
                    format,
                    width: _,
                    height: _,
                    rects,
                } => {
                    let assets_readonly = renderer_assets.read();
                    let handle = *self.textures.get(&texture_request.texture()).unwrap();
                    let renderer_texture = assets_readonly.get_texture(handle);
                    let texture = renderer_texture.texture();
                    for rect in rects {
                        let data_buffer = command_buffer
                            .upload_dynamic_data(&rect.data, BufferUsage::COPY_SRC)
                            .unwrap();

                        command_buffer.copy_buffer_to_texture(
                            BufferRef::Transient(&data_buffer),
                            texture,
                            &BufferTextureCopyRegion {
                                buffer_offset: 0,
                                buffer_row_pitch: rect.row_pitch as u64,
                                buffer_slice_pitch: (imgui_tight_pitch(*format, rect.rect.w as u32)
                                    as u64)
                                    * (rect.rect.h as u64),
                                texture_subresource: TextureSubresource {
                                    array_layer: 0,
                                    mip_level: 0,
                                },
                                texture_offset: Vec3UI::new(
                                    rect.rect.x as u32,
                                    rect.rect.y as u32,
                                    0,
                                ),
                                texture_extent: Vec3UI::new(
                                    rect.rect.w as u32,
                                    rect.rect.h as u32,
                                    1,
                                ),
                            },
                        );
                    }

                    let asset_handle: AssetHandle = handle.into();
                    let texture_id = TextureId::new(asset_handle.index());
                    match texture_request.uploaded(texture_id) {
                        Ok(f) => {
                            feedback.push(f);
                        }
                        Err(e) => {
                            log::error!("DearImgui error: {:?}", e);
                        }
                    }
                }
                TextureOp::Destroy => {
                    if let Some(handle) = self.textures.remove(&texture_request.texture()) {
                        let asset_handle: AssetHandle = handle.into();
                        renderer_assets.remove_asset(asset_handle);
                    }
                    match texture_request.destroyed() {
                        Ok(f) => {
                            feedback.push(f);
                        }
                        Err(e) => {
                            log::error!("DearImgui error: {:?}", e);
                        }
                    }
                }
            }
        }

        {
            let assets_readonly = renderer_assets.read();
            let mut layout_transfer_textures = SmallVec::<[&Texture; 4]>::new();
            for texture_request in snapshot.texture_requests() {
                match texture_request.operation() {
                    TextureOp::Update { .. } => {
                        let handle = *self.textures.get(&texture_request.texture()).unwrap();
                        let renderer_texture = assets_readonly.get_texture(handle);
                        let texture = renderer_texture.texture();
                        layout_transfer_textures.push(texture);
                    }
                    _ => {}
                }
            }

            for texture in layout_transfer_textures {
                command_buffer.barrier(&[Barrier::TextureBarrier {
                    old_sync: BarrierSync::COPY,
                    new_sync: BarrierSync::FRAGMENT_SHADER,
                    old_layout: TextureLayout::CopyDst,
                    new_layout: TextureLayout::Sampled,
                    old_access: BarrierAccess::COPY_WRITE,
                    new_access: BarrierAccess::SAMPLING_READ,
                    texture,
                    range: Default::default(),
                    queue_ownership: None,
                }]);
            }
        }

        command_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[RenderTarget {
                view: backbuffer_view,
                load_op: LoadOpColor::Load,
                store_op: StoreOp::Store,
            }],
            depth_stencil: None,
            resume_suspend: RenderPassResumeSuspend::empty(),
            query_range: None,
        });

        let assets_readonly = renderer_assets.read();
        let pipeline = assets_readonly
            .get_graphics_pipeline(self.pipeline)
            .unwrap();
        command_buffer.set_pipeline(PipelineBinding::Graphics(pipeline));

        let draw = snapshot.draw_data();

        // Transform 0 - window size to -1 - 1
        // and flip y.
        let transform = Matrix4::from_scale(Vec3::new(
            2.0f32 / draw.display_size[0],
            -2.0f32 / draw.display_size[1],
            1.0f32,
        )) * Matrix4::from_translation(Vec3::new(
            draw.display_pos[0],
            draw.display_pos[1],
            0.0f32,
        ));

        command_buffer.set_push_constant_data(&[transform], ShaderType::VertexShader);

        for draw_list in &draw.draw_lists {
            // Same type, just make bytemuck happy.
            assert_eq!(
                std::mem::size_of::<ImguiDrawVert>(),
                std::mem::size_of::<dear_imgui_rs::DrawVert>()
            );
            assert_eq!(
                std::mem::align_of::<ImguiDrawVert>(),
                std::mem::align_of::<dear_imgui_rs::DrawVert>()
            );
            let pod_data: &[ImguiDrawVert] = unsafe {
                std::slice::from_raw_parts(
                    draw_list.vtx.as_ptr() as *const ImguiDrawVert,
                    draw_list.vtx.len(),
                )
            };

            let vtx_buffer = command_buffer
                .upload_dynamic_data(pod_data, BufferUsage::VERTEX)
                .unwrap();

            let idx_buffer = command_buffer
                .upload_dynamic_data(&draw_list.idx[..], BufferUsage::INDEX)
                .unwrap();

            command_buffer.set_vertex_buffer(0, BufferRef::Transient(&vtx_buffer), 0);
            command_buffer.set_index_buffer(BufferRef::Transient(&idx_buffer), 0, IndexFormat::U16);

            command_buffer.set_viewports(&[Viewport {
                position: Vec2::new(0f32, 0f32),
                extent: Vec2::new(
                    backbuffer_handle.info().width as f32,
                    backbuffer_handle.info().height as f32,
                ),
                min_depth: 0.0f32,
                max_depth: 1.0f32,
            }]);
            command_buffer.bind_sampler(
                BindingFrequency::VeryFrequent,
                1,
                resources.linear_sampler(),
            );
            for cmd in &draw_list.commands {
                match cmd {
                    DrawCmdSnapshot::Elements {
                        count,
                        clip_rect,
                        texture,
                        vtx_offset,
                        idx_offset,
                    } => {
                        command_buffer.set_scissors(&[Scissor {
                            position: Vec2I::new(clip_rect[0] as i32, clip_rect[1] as i32),
                            extent: Vec2UI::new(
                                ((clip_rect[2] + 0.5f32) as u32)
                                    .min(backbuffer_handle.info().width - (clip_rect[0] as u32)),
                                ((clip_rect[3] + 0.5f32) as u32)
                                    .min(backbuffer_handle.info().height - (clip_rect[1] as u32)),
                            ),
                        }]);
                        let texture_handle = match texture {
                            TextureBinding::Legacy(id) => {
                                AssetHandle::new(id.id(), AssetType::Texture).into()
                            }
                            TextureBinding::Managed(id) => *self.textures.get(id).unwrap(),
                        };
                        let renderer_texture = assets_readonly.get_texture(texture_handle);
                        let view = &renderer_texture.view;
                        command_buffer.bind_sampling_view(BindingFrequency::VeryFrequent, 0, view);

                        command_buffer.finish_binding();

                        command_buffer.draw_indexed(
                            *count as u32,
                            1,
                            *idx_offset as u32, // TODO is this in elements or bytes?
                            *vtx_offset as i32,
                            0,
                        );
                    }
                    DrawCmdSnapshot::ResetRenderState => {}
                    DrawCmdSnapshot::SetSamplerLinear => {
                        command_buffer.bind_sampler(
                            BindingFrequency::VeryFrequent,
                            1,
                            resources.linear_sampler(),
                        );
                    }
                    DrawCmdSnapshot::SetSamplerNearest => {
                        command_buffer.bind_sampler(
                            BindingFrequency::VeryFrequent,
                            1,
                            resources.nearest_sampler(),
                        );
                    }
                }
            }
        }

        command_buffer.end_render_pass();

        snapshot.commit(feedback).unwrap();
    }
}

fn imgui_format_to_format(format: dear_imgui_rs::TextureFormat) -> gpu::Format {
    match format {
        dear_imgui_rs::TextureFormat::RGBA32 => gpu::Format::RGBA8UNorm,
        dear_imgui_rs::TextureFormat::Alpha8 => todo!("Alpha8 not implemented"),
    }
}

fn imgui_tight_pitch(format: dear_imgui_rs::TextureFormat, width: u32) -> u32 {
    let element_size = match format {
        dear_imgui_rs::TextureFormat::RGBA32 => 4,
        dear_imgui_rs::TextureFormat::Alpha8 => 1,
    };
    width * element_size
}

// Copy of dear_imgui_rs::DrawVert with bytemucks Zeroable and Pod
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Zeroable, Pod)]
pub struct ImguiDrawVert {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub col: u32,
}
