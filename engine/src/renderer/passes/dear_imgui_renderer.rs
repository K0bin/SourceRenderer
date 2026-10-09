use crate::graphics::*;
use crate::renderer::asset::{
    GraphicsPipelineHandle, PathPipelineShaderStage, RendererAssets, RendererAssetsReadOnly,
};
use crate::renderer::renderer_resources::RendererResources;
use bytemuck::{Pod, Zeroable};
use dear_imgui_rs;
use sourcerenderer_core::gpu::{Texture as _, TexturePlane};
use sourcerenderer_core::{Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec3UI};
use std::collections::HashMap;
use std::sync::Arc;
use crate::asset::{AssetHandle, AssetType};

pub struct DearImguiRenderer {
    next_id: u64,
    textures:
        HashMap<dear_imgui_rs::SnapshotTextureId, (dear_imgui_rs::TextureId, Arc<TextureView>)>,
    pipeline: GraphicsPipelineHandle,
}

impl DearImguiRenderer {
    const IMGUI_IDENTIFY_MASK: u64 = 1 << 63;

    pub fn new(
        _device: &Device,
        _resources: &mut RendererResources,
        assets: &RendererAssets,
        rt_format: Format,
    ) -> Self {
        let vs_path = crate::renderer::shader_path!("dear_imgui.vert");
        let fs_path = crate::renderer::shader_path!("dear_imgui.frag");
        let pipeline =
            assets.request_graphics_pipeline(&crate::renderer::asset::GraphicsPipelineInfo {
                vs: PathPipelineShaderStage::empty_spec_consts(&vs_path),
                fs: Some(PathPipelineShaderStage::empty_spec_consts(&fs_path)),
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
            next_id: 1u64,
            pipeline,
            textures: HashMap::new(),
        }
    }

    pub(in crate::renderer) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        device: &Device,
        command_buffer: &mut CommandBuffer,
        renderer_assets: &RendererAssets,
        resources: &RendererResources,
        snapshot: dear_imgui_rs::FrameSnapshot,
        backbuffer_view: &Arc<TextureView>,
        backbuffer_handle: &BackendTexture,
    ) {
        command_buffer.begin_label("DearImgui");

        command_buffer.begin_label("Pre-Copy Barriers");
        for texture_request in snapshot.texture_requests() {
            match texture_request.operation() {
                dear_imgui_rs::TextureOp::Create {
                    format,
                    width,
                    height,
                    row_pitch,
                    pixels: _,
                } => {
                    assert_eq!(*row_pitch, imgui_tight_pitch(*format, *width as u64) as usize);
                    if self.textures.contains_key(&texture_request.texture()) {
                        log::warn!(
                            "EPOCH: {:?}: DearImgui texture creation request for a texture that already exists: {:?}",
                            snapshot.epoch(), texture_request.texture()
                        );
                    }

                    let id = self.next_id;
                    self.next_id += 1;

                    let texture = device
                        .create_texture(
                            &TextureInfo {
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
                            Some(&format!("DearImgui texture {}", id)),
                        )
                        .unwrap();

                    let view = device.create_texture_view(
                        &texture,
                        &TextureViewInfo {
                            base_mip_level: 0,
                            mip_level_length: 1,
                            base_array_layer: 0,
                            array_layer_length: 1,
                            plane: TexturePlane::Primary,
                            format: None,
                        },
                        Some(&format!("DearImgui view {}", id)),
                    );

                    command_buffer.barrier(&[Barrier::TextureBarrier {
                        old_sync: BarrierSync::FRAGMENT_SHADER | BarrierSync::COPY,
                        new_sync: BarrierSync::COPY,
                        old_layout: TextureLayout::Undefined,
                        new_layout: TextureLayout::CopyDst,
                        old_access: BarrierAccess::empty(),
                        new_access: BarrierAccess::COPY_WRITE,
                        texture: texture.as_ref(),
                        range: Default::default(),
                        queue_ownership: None,
                    }]);

                    let imgui_id = dear_imgui_rs::TextureId::new(id | Self::IMGUI_IDENTIFY_MASK);
                    self.textures
                        .insert(texture_request.texture(), (imgui_id, view));
                }
                dear_imgui_rs::TextureOp::Update { .. } => {
                    if let Some((_, texture_view)) =
                        self.textures.get(&texture_request.texture())
                    {
                        let texture = texture_view.texture().unwrap();
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
                _ => {}
            }
        }

        command_buffer.flush_barriers();
        command_buffer.end_label();

        command_buffer.begin_label("Copies");
        let mut feedback = Vec::<dear_imgui_rs::TextureFeedback>::new();
        for texture_request in snapshot.texture_requests() {
            match texture_request.operation() {
                dear_imgui_rs::TextureOp::Create {
                    format,
                    width,
                    height,
                    row_pitch,
                    pixels,
                } => {
                    let (imgui_id, view) = self.textures.get(&texture_request.texture()).unwrap();
                    let texture = view.texture().unwrap();

                    // Do the copy manually here to avoid problems if it's immediately used or updated.
                    // Device::init_texture uses a separate command buffer.
                    let data_buffer = command_buffer
                        .upload_dynamic_data(device, pixels, BufferUsage::COPY_SRC)
                        .unwrap();

                    assert_eq!(imgui_tight_pitch(*format, *width as u64), *row_pitch as u64);
                    assert_eq!(*row_pitch * (*height as usize), pixels.len());

                    command_buffer.copy_buffer_to_texture(
                        BufferRef::Transient(&data_buffer),
                        texture.as_ref(),
                        &BufferTextureCopyRegion {
                            buffer_offset: 0,
                            buffer_row_pitch: *row_pitch as u64,
                            buffer_slice_pitch: (*row_pitch as u64) * (*height as u64),
                            texture_subresource: TextureSubresource {
                                array_layer: 0,
                                mip_level: 0,
                            },
                            texture_offset: Vec3UI::new(
                                0, 0, 0,
                            ),
                            texture_extent: Vec3UI::new(
                                *width,
                                *height,
                                1,
                            ),
                        },
                    );

                    command_buffer.barrier(&[Barrier::TextureBarrier {
                        old_sync: BarrierSync::COPY,
                        new_sync: BarrierSync::FRAGMENT_SHADER,
                        old_layout: TextureLayout::CopyDst,
                        new_layout: TextureLayout::Sampled,
                        old_access: BarrierAccess::COPY_WRITE,
                        new_access: BarrierAccess::SAMPLING_READ,
                        texture: texture.as_ref(),
                        range: Default::default(),
                        queue_ownership: None,
                    }]);

                    match texture_request.uploaded(*imgui_id) {
                        Ok(f) => {
                            feedback.push(f);
                        }
                        Err(e) => {
                            log::error!("DearImgui error: {:?}", e);
                        }
                    }
                }
                dear_imgui_rs::TextureOp::Update {
                    format,
                    width,
                    height,
                    rects,
                } => {
                    let (imgui_id, texture_view) = self.textures.get(&texture_request.texture()).unwrap();
                    let texture = texture_view.texture().unwrap();
                    assert_eq!(*width, texture.info().width);
                    assert_eq!(*height, texture.info().height);
                    for rect in rects {
                        let data_buffer = command_buffer
                            .upload_dynamic_data(device, &rect.data, BufferUsage::COPY_SRC)
                            .unwrap();

                        assert_eq!(imgui_tight_pitch(*format, rect.rect.w as u64), rect.row_pitch as u64);
                        assert_eq!(rect.row_pitch * (rect.rect.h as usize), rect.data.len());

                        command_buffer.copy_buffer_to_texture(
                            BufferRef::Transient(&data_buffer),
                            texture,
                            &BufferTextureCopyRegion {
                                buffer_offset: 0,
                                buffer_row_pitch: rect.row_pitch as u64,
                                buffer_slice_pitch: (rect.row_pitch as u64) * (rect.rect.h as u64),
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

                        // Dunno if they overlap...
                        command_buffer.barrier(&[Barrier::TextureBarrier {
                            old_sync: BarrierSync::COPY,
                            new_sync: BarrierSync::COPY,
                            old_layout: TextureLayout::CopyDst,
                            new_layout: TextureLayout::CopyDst,
                            old_access: BarrierAccess::COPY_WRITE,
                            new_access: BarrierAccess::COPY_WRITE,
                            texture,
                            range: Default::default(),
                            queue_ownership: None,
                        }]);
                        command_buffer.flush_barriers();
                    }

                    match texture_request.uploaded(*imgui_id) {
                        Ok(f) => {
                            feedback.push(f);
                        }
                        Err(e) => {
                            log::error!("DearImgui error: {:?}", e);
                        }
                    }
                }
                dear_imgui_rs::TextureOp::Destroy => {
                    self.textures.remove(&texture_request.texture());
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

        command_buffer.flush_barriers();
        command_buffer.end_label();

        command_buffer.begin_label("Post-Copy Barriers");
        for texture_request in snapshot.texture_requests() {
            match texture_request.operation() {
                dear_imgui_rs::TextureOp::Update { .. } => {
                    let (_, texture_view) =
                        self.textures.get(&texture_request.texture()).unwrap();
                    let texture = texture_view.texture().unwrap();
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
                _ => {}
            }
        }
        command_buffer.end_label();

        command_buffer.flush_barriers();

        command_buffer.begin_label("Drawing");
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
            draw.display_pos[0] - draw.display_size[0] / 2.0f32,
            draw.display_pos[1] - draw.display_size[1] / 2.0f32,
            0.0f32,
        ));

        command_buffer.set_push_constant_data(&[transform]);

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
                .upload_dynamic_data(device, pod_data, BufferUsage::VERTEX)
                .unwrap();

            let idx_buffer = command_buffer
                .upload_dynamic_data(device, &draw_list.idx[..], BufferUsage::INDEX)
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
                    dear_imgui_rs::DrawCmdSnapshot::Elements {
                        count,
                        clip_rect,
                        texture,
                        vtx_offset,
                        idx_offset,
                    } => {
                        command_buffer.set_scissors(&[Scissor {
                            position: Vec2I::new(clip_rect[0] as i32, clip_rect[1] as i32),
                            extent: Vec2UI::new(
                                (clip_rect[2].ceil() as u32)
                                    .min(backbuffer_handle.info().width - (clip_rect[0] as u32)),
                                (clip_rect[3].ceil() as u32)
                                    .min(backbuffer_handle.info().height - (clip_rect[1] as u32)),
                            ),
                        }]);

                        let view = match texture {
                            dear_imgui_rs::TextureBinding::Legacy(id) => {
                                if (id.id() & Self::IMGUI_IDENTIFY_MASK) != 0 {
                                    log::warn!("Got a managed texture in the legacy path.");
                                    self.textures
                                        .iter()
                                        .find(|(_, (stored_id, _))| id == stored_id)
                                        .map(|(_, (_, texture))| texture)
                                        .unwrap()
                                } else {
                                    let asset_handle = AssetHandle::new(id.id(), AssetType::Texture);
                                    let texture = assets_readonly.get_texture_opt(asset_handle.into());
                                    texture.map(|t| &t.view).unwrap_or(&assets_readonly.get_placeholder_texture_black().view)
                                }
                            }
                            dear_imgui_rs::TextureBinding::Managed(id) => {
                                &(self.textures.get(id).unwrap().1)
                            }
                        };
                        command_buffer.bind_sampling_view(BindingFrequency::VeryFrequent, 0, view);

                        command_buffer.finish_binding();

                        command_buffer.draw_indexed(
                            *count as u32,
                            1,
                            *idx_offset as u32,
                            *vtx_offset as i32,
                            0,
                        );
                    }
                    dear_imgui_rs::DrawCmdSnapshot::ResetRenderState => {}
                    dear_imgui_rs::DrawCmdSnapshot::SetSamplerLinear => {
                        command_buffer.bind_sampler(
                            BindingFrequency::VeryFrequent,
                            1,
                            resources.linear_sampler(),
                        );
                    }
                    dear_imgui_rs::DrawCmdSnapshot::SetSamplerNearest => {
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
        command_buffer.end_label();
        command_buffer.end_label();

        snapshot.commit(feedback).unwrap();
    }
}

fn imgui_format_to_format(format: dear_imgui_rs::TextureFormat) -> gpu::Format {
    match format {
        dear_imgui_rs::TextureFormat::RGBA32 => gpu::Format::RGBA8UNorm,
        dear_imgui_rs::TextureFormat::Alpha8 => todo!("Alpha8 not implemented"),
    }
}

fn imgui_tight_pitch(format: dear_imgui_rs::TextureFormat, width: u64) -> u64 {
    let element_size = match format {
        dear_imgui_rs::TextureFormat::RGBA32 => 4,
        dear_imgui_rs::TextureFormat::Alpha8 => 1,
    };
    width * element_size
}

// Copy of dear_imgui_rs::DrawVert with bytemucks Zeroable and Pod
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Zeroable, Pod)]
struct ImguiDrawVert {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub col: u32,
}
