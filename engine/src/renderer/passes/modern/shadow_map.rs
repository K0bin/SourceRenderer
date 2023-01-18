use std::marker::PhantomData;
use std::{path::Path, sync::Arc};

use nalgebra::Point3;
use sourcerenderer_core::{Matrix4, Vec3, Vec4};
use sourcerenderer_core::graphics::{
    BindingFrequency, BufferInfo, BufferUsage, CommandBuffer, MemoryUsage, PipelineBinding,
    RenderPassBeginInfo,  WHOLE_BUFFER,
};
use sourcerenderer_core::{
    graphics::{
        AttachmentInfo, Backend, BarrierAccess, BarrierSync, BlendInfo, Buffer, CompareFunc,
        CullMode, DepthStencilAttachmentRef, DepthStencilInfo, FillMode, Format, FrontFace,
        IndexFormat, InputAssemblerElement, InputRate, LoadOp, LogicOp,
        PrimitiveType, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView,
        RenderPassInfo, RenderpassRecordingMode, SampleCount, Scissor, ShaderInputElement,
        ShaderType, StencilInfo, StoreOp, SubpassInfo, Texture, TextureDimension, TextureInfo,
        TextureLayout, TextureUsage, TextureView, TextureViewInfo, VertexLayoutInfo, Viewport,
    },
    Platform, Vec2, Vec2I, Vec2UI,
};

use crate::renderer::drawable::View;
use crate::renderer::light::{DirectionalLight, RendererDirectionalLight};
use crate::renderer::passes::modern::gpu_scene::{DRAWABLE_CAPACITY, DRAW_CAPACITY, PART_CAPACITY};
use crate::renderer::renderer_scene::RendererScene;
use crate::renderer::shader_manager::{
    ComputePipelineHandle, GraphicsPipelineHandle, GraphicsPipelineInfo, ShaderManager,
};
use crate::renderer::{
    renderer_resources::{HistoryResourceEntry, RendererResources},
    Vertex,
};

/*
TODO:
- implement multiple cascades
- filter shadows
- research shadow map ray marching (UE5)
- cache shadows of static objects and copy every frame
- point light shadows, spot light shadows
- multiple lights
*/

pub struct ShadowMapPass<P: Platform> {
    pipeline: GraphicsPipelineHandle,
    draw_prep_pipeline: ComputePipelineHandle,
    _marker: PhantomData<P>,
}

impl<P: Platform> ShadowMapPass<P> {
    pub const SHADOW_MAP_NAME: &'static str = "ShadowMap";
    pub const DRAW_BUFFER_NAME: &'static str = "ShadowMapDraws";
    pub const VISIBLE_BITFIELD: &'static str = "ShadowMapVisibility";
    pub fn new(
        device: &Arc<<P::GraphicsBackend as Backend>::Device>,
        resources: &mut RendererResources<P::GraphicsBackend>,
        init_cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        resources.create_texture(
            &Self::SHADOW_MAP_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::D24,
                width: 4096,
                height: 4096,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::DEPTH_STENCIL | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            false,
        );

        resources.create_buffer(
            &Self::DRAW_BUFFER_NAME,
            &BufferInfo {
                size: 4 + 20 * PART_CAPACITY as usize,
                usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
            },
            MemoryUsage::VRAM,
            false,
        );

        resources.create_buffer(
            &Self::VISIBLE_BITFIELD,
            &BufferInfo {
                size: ((DRAWABLE_CAPACITY as usize + 31) / 32) * 4,
                usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
            },
            MemoryUsage::VRAM,
            false,
        );

        let vs_path = Path::new("shaders").join(Path::new("shadow_map_bindless.vert.spv"));
        let pipeline = shader_manager.request_graphics_pipeline(
            &GraphicsPipelineInfo {
                vs: vs_path.to_str().unwrap(),
                fs: None,
                vertex_layout: VertexLayoutInfo {
                    shader_inputs: &[ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 0,
                        semantic_name_d3d: "pos".to_string(),
                        semantic_index_d3d: 0,
                        offset: 0,
                        format: Format::RGB32Float,
                    }],
                    input_assembler: &[InputAssemblerElement {
                        binding: 0,
                        input_rate: InputRate::PerVertex,
                        stride: std::mem::size_of::<Vertex>(),
                    }],
                },
                rasterizer: RasterizerInfo {
                    fill_mode: FillMode::Fill,
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
                    sample_count: SampleCount::Samples1,
                },
                depth_stencil: DepthStencilInfo {
                    depth_test_enabled: true,
                    depth_write_enabled: true,
                    depth_func: CompareFunc::Less,
                    stencil_enable: false,
                    stencil_read_mask: 0,
                    stencil_write_mask: 0,
                    stencil_front: StencilInfo::default(),
                    stencil_back: StencilInfo::default(),
                },
                blend: BlendInfo {
                    alpha_to_coverage_enabled: false,
                    logic_op_enabled: false,
                    logic_op: LogicOp::And,
                    attachments: &[],
                    constants: [0f32; 4],
                },
                primitive_type: PrimitiveType::Triangles,
            },
            &RenderPassInfo {
                attachments: &[AttachmentInfo {
                    format: Format::D24,
                    samples: SampleCount::Samples1,
                }],
                subpasses: &[SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[],
                    depth_stencil_attachment: Some(DepthStencilAttachmentRef {
                        index: 0,
                        read_only: false,
                    }),
                }],
            },
            0,
        );

        let prep_pipeline = shader_manager.request_compute_pipeline("shaders/draw_prep.comp.spv");

        Self {
            pipeline,
            draw_prep_pipeline: prep_pipeline,
            _marker: PhantomData,
        }
    }

    pub fn prepare(
        &mut self,
        cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        resources: &RendererResources<P::GraphicsBackend>,
        shader_manager: &ShaderManager<P>,
        scene: &RendererScene<P::GraphicsBackend>,
    ) {
        cmd_buffer.begin_label("Shadow map culling");

        let draws_buffer = resources.access_buffer(
            cmd_buffer,
            Self::DRAW_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            HistoryResourceEntry::Current,
        );

        {
            let visibility_buffer = resources.access_buffer(
                cmd_buffer,
                Self::VISIBLE_BITFIELD,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_WRITE,
                HistoryResourceEntry::Current,
            );

            cmd_buffer.flush_barriers();
            cmd_buffer.clear_storage_buffer(
                &visibility_buffer,
                0,
                visibility_buffer.info().size / 4,
                !0,
            );
        }

        let visibility_buffer = resources.access_buffer(
            cmd_buffer,
            Self::VISIBLE_BITFIELD,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );

        let pipeline = shader_manager.get_compute_pipeline(self.draw_prep_pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            0,
            &visibility_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            1,
            &draws_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((scene.static_drawables().len() as u32 + 63) / 64, 1, 1);
        cmd_buffer.end_label();
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        resources: &RendererResources<P::GraphicsBackend>,
        shader_manager: &ShaderManager<P>,
        vertex_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
        index_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
        scene: &RendererScene<P::GraphicsBackend>,
        view: &View
    ) {
        let light = scene.directional_lights().first();
        if light.is_none() {
            return;
        }
        let light = light.unwrap();

        const Z_MULT: f32 = 100.0f32;
        let view_proj = view.proj_matrix * view.view_matrix;
        let inv_camera_view = view_proj.try_inverse().unwrap();
        let light_mv = Self::build_directional_light_view_proj(light, inv_camera_view, Z_MULT);

        cmd_buffer.begin_label("Shadow map");
        let shadow_map = resources.access_view(
            cmd_buffer,
            Self::SHADOW_MAP_NAME,
            BarrierSync::EARLY_DEPTH,
            BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
            TextureLayout::DepthStencilReadWrite,
            true,
            &TextureViewInfo {
                base_mip_level: 0,
                mip_level_length: 1,
                base_array_layer: 0,
                array_layer_length: 1,
                format: None,
            },
            HistoryResourceEntry::Current,
        );

        let draw_buffer = resources.access_buffer(
            cmd_buffer,
            Self::DRAW_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::INDIRECT_READ,
            HistoryResourceEntry::Current,
        );

        cmd_buffer.begin_render_pass(
            &RenderPassBeginInfo {
                attachments: &[RenderPassAttachment {
                    view: RenderPassAttachmentView::DepthStencil(&shadow_map),
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                }],
                subpasses: &[SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[],
                    depth_stencil_attachment: Some(DepthStencilAttachmentRef {
                        index: 0,
                        read_only: false,
                    }),
                }],
            },
            RenderpassRecordingMode::Commands,
        );

        let dsv_info = shadow_map.texture().info();
        let pipeline = shader_manager.get_graphics_pipeline(self.pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0.0f32, 0.0f32),
            extent: Vec2::new(dsv_info.width as f32, dsv_info.height as f32),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);
        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0, 0),
            extent: Vec2UI::new(9999, 9999),
        }]);

        cmd_buffer.set_vertex_buffer(vertex_buffer, 0);
        cmd_buffer.set_index_buffer(index_buffer, 0, IndexFormat::U32);

        cmd_buffer.upload_dynamic_data_inline(&[light_mv], ShaderType::VertexShader);

        cmd_buffer.finish_binding();
        cmd_buffer.draw_indexed_indirect(&draw_buffer, 4, &draw_buffer, 0, DRAW_CAPACITY, 20);

        cmd_buffer.end_render_pass();
        cmd_buffer.end_label();
    }

    pub fn build_directional_light_view_proj(light: &RendererDirectionalLight<P::GraphicsBackend>, inv_camera_view: Matrix4, z_mult: f32) -> Matrix4 {
        let mut world_space_frustum_corners = [Vec4::new(0f32, 0f32, 0f32, 0f32); 8];
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    let mut world_space_frustum_corner = inv_camera_view * Vec4::new(
                        2.0f32 * (x as f32) - 1.0f32,
                        2.0f32 * (y as f32) - 1.0f32,
                        z as f32,
                        1.0f32
                    );
                    world_space_frustum_corner /= world_space_frustum_corner.w;
                    world_space_frustum_corners[x * 4 + y * 2 + z] = world_space_frustum_corner;
                }
            }
        }
        let mut center = Vec3::new(0f32, 0f32, 0f32);
        for corner in &world_space_frustum_corners {
            center += corner.xyz();
        }
        center /= world_space_frustum_corners.len() as f32;

        let mut light_view = Matrix4::look_at_lh(&Point3::from(center - light.direction), &Point3::from(center), &Vec3::new(0f32, 1f32, 0f32));

        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for corner in &world_space_frustum_corners {
            let light_space_frustum_corner = light_view * corner;
            min.x = min.x.min(light_space_frustum_corner.x);
            max.x = max.x.max(light_space_frustum_corner.x);
            min.y = min.y.min(light_space_frustum_corner.y);
            max.y = max.y.max(light_space_frustum_corner.y);
            min.z = min.z.min(light_space_frustum_corner.z);
            max.z = max.z.max(light_space_frustum_corner.z);
        }

        let cascade_extent = max - min;

        light_view = Matrix4::look_at_lh(&Point3::from(center - light.direction * -min.z), &Point3::from(center), &Vec3::new(0f32, 1f32, 0f32));

        let light_proj = nalgebra_glm::ortho_lh_zo(min.x, max.x, min.y, max.y, 0.0f32, cascade_extent.z);
        light_proj * light_view
    }
}
