use std::sync::Arc;

use gltf::texture::{
    MagFilter,
    MinFilter,
};
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{
    AddressMode,
    AttachmentBlendInfo,
    AttachmentInfo,
    Backend,
    Barrier,
    BarrierAccess,
    BarrierSync,
    BarrierTextureRange,
    BindingFrequency,
    BlendInfo,
    CommandBuffer,
    CompareFunc,
    CullMode,
    DepthStencilAttachmentRef,
    DepthStencilInfo,
    Device,
    FillMode,
    Filter,
    Format,
    FrontFace,
    IndexFormat,
    InputAssemblerElement,
    InputRate,
    LoadOp,
    LogicOp,
    OutputAttachmentRef,
    PipelineBinding,
    PrimitiveType,
    RasterizerInfo,
    RenderPassAttachment,
    RenderPassAttachmentView,
    RenderPassBeginInfo,
    RenderPassInfo,
    RenderpassRecordingMode,
    SampleCount,
    SamplerInfo,
    Scissor,
    ShaderInputElement,
    ShaderType,
    StencilInfo,
    StoreOp,
    SubpassInfo,
    Swapchain,
    Texture,
    TextureDimension,
    TextureInfo,
    TextureLayout,
    TextureUsage,
    TextureView,
    TextureViewInfo,
    VertexLayoutInfo,
    Viewport,
    WHOLE_BUFFER,
};
use sourcerenderer_core::{
    Platform,
    Vec2,
    Vec2I,
    Vec2UI,
};

use crate::renderer::drawable::View;
use crate::renderer::renderer_assets::{
    RendererAssets,
    RendererMaterial,
    RendererMaterialValue,
};
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::renderer_scene::RendererScene;
use crate::renderer::shader_manager::{
    GraphicsPipelineHandle,
    GraphicsPipelineInfo,
    ShaderManager,
};

pub struct GeometryPass<P: Platform> {
    pipeline: GraphicsPipelineHandle,
    sampler: Arc<<P::GraphicsBackend as Backend>::Sampler>,
}

impl<P: Platform> GeometryPass<P> {
    pub const DEPTH_TEXTURE_NAME: &'static str = "Depth";

    pub(super) fn new(
        device: &Arc<<P::GraphicsBackend as Backend>::Device>,
        swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
        _init_cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        resources: &mut RendererResources<P::GraphicsBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let sampler = device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::ClampToEdge,
            mip_bias: 0.0f32,
            max_anisotropy: 0.0f32,
            compare_op: None,
            min_lod: 0.0f32,
            max_lod: None,
        });

        resources.create_texture(
            Self::DEPTH_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::D32,
                width: swapchain.width(),
                height: swapchain.height(),
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::DEPTH_STENCIL,
                supports_srgb: false,
            },
            false,
        );

        let shader_file_extension = if cfg!(target_family = "wasm") {
            "glsl"
        } else {
            "spv"
        };

        let fs_name = format!("shaders/web_geometry.web.frag.{}", shader_file_extension);
        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: &format!("shaders/web_geometry.web.vert.{}", shader_file_extension),
            fs: Some(&fs_name),
            primitive_type: PrimitiveType::Triangles,
            vertex_layout: VertexLayoutInfo {
                input_assembler: &[InputAssemblerElement {
                    binding: 0,
                    stride: 64,
                    input_rate: InputRate::PerVertex,
                }],
                shader_inputs: &[
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 0,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 0,
                        format: Format::RGB32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 1,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 16,
                        format: Format::RGB32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 2,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 32,
                        format: Format::RG32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 3,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 40,
                        format: Format::RG32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 4,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 48,
                        format: Format::R32Float,
                    },
                ],
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
                depth_func: CompareFunc::Less,
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
                attachments: &[AttachmentBlendInfo::default()],
            },
        };
        let pipeline = shader_manager.request_graphics_pipeline(
            &pipeline_info,
            &RenderPassInfo {
                attachments: &[
                    AttachmentInfo {
                        format: swapchain.format(),
                        samples: swapchain.sample_count(),
                    },
                    AttachmentInfo {
                        format: Format::D32,
                        samples: SampleCount::Samples1,
                    },
                ],
                subpasses: &[SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[OutputAttachmentRef {
                        index: 0,
                        resolve_attachment_index: None,
                    }],
                    depth_stencil_attachment: Some(DepthStencilAttachmentRef {
                        index: 1,
                        read_only: false,
                    }),
                }],
            },
            0,
        );

        Self { pipeline, sampler }
    }

    pub(super) fn execute(
        &mut self,
        cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        scene: &RendererScene<P::GraphicsBackend>,
        view: &View,
        camera_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
        resources: &RendererResources<P::GraphicsBackend>,
        backbuffer: &Arc<<P::GraphicsBackend as Backend>::TextureView>,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>,
    ) {
        cmd_buffer.barrier(&[Barrier::TextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::RENDER_TARGET,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::RenderTarget,
            texture: backbuffer.texture(),
            range: BarrierTextureRange::default(),
        }]);

        let dsv = resources.access_view(
            cmd_buffer,
            Self::DEPTH_TEXTURE_NAME,
            BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
            BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
            TextureLayout::DepthStencilReadWrite,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        cmd_buffer.flush_barriers();
        cmd_buffer.begin_render_pass(
            &RenderPassBeginInfo {
                attachments: &[
                    RenderPassAttachment {
                        view: RenderPassAttachmentView::RenderTarget(&backbuffer),
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::Store,
                    },
                    RenderPassAttachment {
                        view: RenderPassAttachmentView::DepthStencil(&dsv),
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::Store,
                    },
                ],
                subpasses: &[SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[OutputAttachmentRef {
                        index: 0,
                        resolve_attachment_index: None,
                    }],
                    depth_stencil_attachment: Some(DepthStencilAttachmentRef {
                        index: 1,
                        read_only: false,
                    }),
                }],
            },
            RenderpassRecordingMode::Commands,
        );

        let rtv_info = backbuffer.texture().info();

        let pipeline = shader_manager.get_graphics_pipeline(self.pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0.0f32, 0.0f32),
            extent: Vec2::new(rtv_info.width as f32, rtv_info.height as f32),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);
        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0, 0),
            extent: Vec2UI::new(9999, 9999),
        }]);

        //let camera_buffer = cmd_buffer.upload_dynamic_data(&[view.proj_matrix * view.view_matrix], BufferUsage::CONSTANT);
        cmd_buffer.bind_uniform_buffer(BindingFrequency::Frame, 0, camera_buffer, 0, WHOLE_BUFFER);

        let drawables = scene.static_drawables();
        let parts = &view.drawable_parts;
        for part in parts {
            let drawable = &drawables[part.drawable_index];
            cmd_buffer.upload_dynamic_data_inline(&[drawable.transform], ShaderType::VertexShader);
            let model = assets.get_model(drawable.model);
            if model.is_none() {
                log::info!("Skipping draw because of missing model");
                continue;
            }
            let model = model.unwrap();
            let mesh = assets.get_mesh(model.mesh_handle());
            if mesh.is_none() {
                log::info!("Skipping draw because of missing mesh");
                continue;
            }
            let mesh = mesh.unwrap();
            let materials: SmallVec<[&RendererMaterial; 4]> = model
                .material_handles()
                .iter()
                .map(|handle| assets.get_material(*handle))
                .collect();
            let range = &mesh.parts[part.part_index];
            let material = &materials[part.part_index];
            let albedo_value = material.get("albedo").unwrap();
            match albedo_value {
                RendererMaterialValue::Texture(handle) => {
                    let texture = assets.get_texture(*handle);
                    let albedo_view = &texture.view;
                    cmd_buffer.bind_sampling_view_and_sampler(
                        BindingFrequency::Frequent,
                        0,
                        albedo_view,
                        &self.sampler,
                    );
                }
                _ => unimplemented!(),
            }
            cmd_buffer.finish_binding();

            cmd_buffer.set_vertex_buffer(mesh.vertices.buffer(), mesh.vertices.offset() as usize);
            if let Some(indices) = mesh.indices.as_ref() {
                cmd_buffer.set_index_buffer(
                    indices.buffer(),
                    indices.offset() as usize,
                    IndexFormat::U32,
                );
                cmd_buffer.draw_indexed(1, 0, range.count, range.start, 0);
            } else {
                cmd_buffer.draw(range.count, range.start);
            }
        }
        cmd_buffer.end_render_pass();

        cmd_buffer.barrier(&[Barrier::TextureBarrier {
            old_sync: BarrierSync::RENDER_TARGET,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::RENDER_TARGET_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::RenderTarget,
            new_layout: TextureLayout::Present,
            texture: backbuffer.texture(),
            range: BarrierTextureRange::default(),
        }]);
    }
}
