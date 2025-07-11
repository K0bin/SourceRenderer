use bevy_math::Affine3A;
use sourcerenderer_core::{
    Matrix4,
    Vec2,
    Vec2I,
    Vec2UI,
};

use crate::graphics::{
    CommandBuffer,
    *,
};
use crate::renderer::asset::{
    GraphicsPipelineHandle,
    GraphicsPipelineInfo,
    RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::passes::taa::scaled_halton_point;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};

#[allow(unused)]
#[derive(Clone, Copy)]
#[repr(C)]
struct PrepassCameraCB {
    view_projection: Matrix4,
    old_view_projection: Matrix4,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct PrepassModelCB {
    model: Affine3A,
    old_model: Affine3A,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FrameData {
    swapchain_transform: Matrix4,
    halton_point: Vec2,
}

pub struct Prepass {
    pipeline: GraphicsPipelineHandle,
}

impl Prepass {
    pub const DEPTH_TEXTURE_NAME: &'static str = "PrepassDepth";

    const DRAWABLE_LABELS: bool = false;

    #[allow(unused)]
    pub fn new(
        resources: &mut RendererResources,
        assets: &RendererAssets,
        resolution: Vec2UI,
    ) -> Self {
        let depth_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::D24S8,
            width: resolution.x,
            height: resolution.y,
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::DEPTH_STENCIL | TextureUsage::SAMPLED,
            supports_srgb: false,
        };
        resources.create_texture(Self::DEPTH_TEXTURE_NAME, &depth_info, true);

        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: &("shaders/prepass.vert.json"),
            fs: Some("shaders/prepass.frag.json"),
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
                attachments: &[
                    AttachmentBlendInfo::default(),
                    AttachmentBlendInfo::default(),
                ],
            },
            render_target_formats: &[Format::RG32Float, Format::RGBA32Float],
            depth_stencil_format: Format::D24S8,
        };
        let pipeline = assets.request_graphics_pipeline(&pipeline_info);

        Self { pipeline }
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    #[profiling::function]
    pub(super) fn execute<'a>(
        &mut self,
        context: &'a GraphicsContext,
        cmd_buffer: &mut CommandBuffer<'a>,
        pass_params: &RenderPassParameters<'_>,
        swapchain_transform: Matrix4,
        frame: u64,
        camera_buffer: &TransientBufferSlice,
        camera_history_buffer: &TransientBufferSlice,
    ) {
        let view = &pass_params.scene.scene.views()[pass_params.scene.active_view_index];

        cmd_buffer.begin_label("Depth prepass");
        let static_drawables = pass_params.scene.scene.static_drawables();

        let depth_buffer = pass_params.resources.access_view(
            cmd_buffer,
            Self::DEPTH_TEXTURE_NAME,
            BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
            BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
            TextureLayout::DepthStencilReadWrite,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let info = depth_buffer.texture().unwrap().info();
        let per_frame = FrameData {
            swapchain_transform,
            halton_point: scaled_halton_point(info.width, info.height, (frame % 8) as u32 + 1),
        };
        let transform_constant_buffer = cmd_buffer
            .upload_dynamic_data(&[per_frame], BufferUsage::CONSTANT)
            .unwrap();

        let assets = pass_params.assets;

        const CHUNK_SIZE: u32 = 128;
        let chunk_size = (view.drawable_parts.len() as u32 / 15).max(CHUNK_SIZE);
        let pipeline = pass_params
            .assets
            .get_graphics_pipeline(self.pipeline)
            .unwrap();

        let owned_cmd_buffer =
            std::mem::replace(cmd_buffer, context.get_command_buffer(QueueType::Graphics));
        *cmd_buffer = owned_cmd_buffer.split_render_pass_with_chunks(
            &RenderPassBeginInfo {
                render_targets: &[],
                depth_stencil: Some(&DepthStencilAttachment {
                    view: &*depth_buffer,
                    load_op: LoadOpDepthStencil::Clear(ClearDepthStencilValue::DEPTH_ONE),
                    store_op: StoreOp::Store,
                }),
                query_range: None,
            },
            &view.drawable_parts,
            chunk_size,
            |command_buffer, _chunk_index, _chunk_size, chunk| {
                command_buffer.set_pipeline(crate::graphics::PipelineBinding::Graphics(&pipeline));
                command_buffer.set_viewports(&[Viewport {
                    position: Vec2::new(0.0f32, 0.0f32),
                    extent: Vec2::new(info.width as f32, info.height as f32),
                    min_depth: 0.0f32,
                    max_depth: 1.0f32,
                }]);
                command_buffer.set_scissors(&[Scissor {
                    position: Vec2I::new(0, 0),
                    extent: Vec2UI::new(info.width, info.height),
                }]);
                command_buffer.bind_uniform_buffer(
                    BindingFrequency::Frequent,
                    2,
                    BufferRef::Transient(&transform_constant_buffer),
                    0,
                    WHOLE_BUFFER,
                );

                command_buffer.bind_uniform_buffer(
                    BindingFrequency::Frequent,
                    0,
                    BufferRef::Transient(camera_buffer),
                    0,
                    WHOLE_BUFFER,
                );
                command_buffer.bind_uniform_buffer(
                    BindingFrequency::Frequent,
                    1,
                    BufferRef::Transient(camera_history_buffer),
                    0,
                    WHOLE_BUFFER,
                );
                command_buffer.finish_binding();

                for part in chunk.iter() {
                    let drawable = &static_drawables[part.drawable_index];
                    if Self::DRAWABLE_LABELS {
                        command_buffer.begin_label(&format!("Drawable {}", part.drawable_index));
                    }

                    command_buffer.set_push_constant_data(
                        &[PrepassModelCB {
                            model: drawable.transform,
                            old_model: drawable.old_transform,
                        }],
                        ShaderType::VertexShader,
                    );

                    let model: Option<&crate::renderer::asset::RendererModel> =
                        assets.get_model(drawable.model);
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

                    command_buffer.set_vertex_buffer(
                        0,
                        BufferRef::Regular(mesh.vertices.buffer()),
                        mesh.vertices.offset() as u64,
                    );
                    if let Some(indices) = mesh.indices.as_ref() {
                        command_buffer.set_index_buffer(
                            BufferRef::Regular(indices.buffer()),
                            indices.offset() as u64,
                            IndexFormat::U32,
                        );
                    }

                    let range = &mesh.parts[part.part_index];

                    if mesh.indices.is_some() {
                        command_buffer.draw_indexed(range.count, 1, range.start, 0, 0);
                    } else {
                        command_buffer.draw(range.count, 1, range.start, 0);
                    }
                    if Self::DRAWABLE_LABELS {
                        command_buffer.end_label();
                    }
                }
            },
        );
        cmd_buffer.end_label();
    }
}
