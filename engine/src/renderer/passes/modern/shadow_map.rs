use std::marker::PhantomData;
use std::{path::Path, sync::Arc};

use legion::world;
use nalgebra::Point3;
use nalgebra_glm::proj;
use smallvec::SmallVec;
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
    shadow_map_res: u32,
    cascades: SmallVec<[ShadowMapCascade; 5]>,
    _marker: PhantomData<P>,
}

#[derive(Debug, Default)]
pub struct ShadowMapCascade {
    pub z_min: f32,
    pub z_max: f32,
    _padding: [u32; 2],
    pub view_proj: Matrix4
}

impl<P: Platform> ShadowMapPass<P> {
    pub const SHADOW_MAP_NAME: &'static str = "ShadowMap";
    pub const DRAW_BUFFER_NAME: &'static str = "ShadowMapDraws";
    pub const VISIBLE_BITFIELD: &'static str = "ShadowMapVisibility";
    pub fn new(
        _device: &Arc<<P::GraphicsBackend as Backend>::Device>,
        resources: &mut RendererResources<P::GraphicsBackend>,
        _init_cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let shadow_map_res = 4096;
        let cascades_count = 5;

        resources.create_texture(
            &Self::SHADOW_MAP_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2DArray,
                format: Format::D24,
                width: shadow_map_res,
                height: shadow_map_res,
                depth: 1,
                mip_levels: 1,
                array_length: cascades_count,
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

        let mut cascades = SmallVec::<[ShadowMapCascade; 5]>::with_capacity(cascades_count as usize);
        cascades.resize_with(cascades_count as usize, || ShadowMapCascade::default());

        Self {
            pipeline,
            draw_prep_pipeline: prep_pipeline,
            shadow_map_res,
            cascades,
            _marker: PhantomData,
        }
    }

    pub fn calculate_cascades(&mut self,
        scene: &RendererScene<P::GraphicsBackend>,
        view: &View) {
        for cascade in &mut self.cascades {
            *cascade = Default::default();
        }
        let light = scene.directional_lights().first();
        if light.is_none() {
            return;
        }
        let light: &RendererDirectionalLight<<P as Platform>::GraphicsBackend> = light.unwrap();

        let z_min = view.near_plane;
        let z_max = view.far_plane;

        let lambda = 0.15f32;
        let mut z_start = z_min;
        for cascade_index in 0..self.cascades.len() {
            let view_proj = view.proj_matrix * view.view_matrix;
            let inv_camera_view_proj = view_proj.try_inverse().unwrap();

            let i = cascade_index as u32 + 1u32;
            let m = self.cascades.len() as u32;
            let log_split = (z_min * (z_max / z_min)).powf(i as f32 / m as f32);
            let uniform_split = z_min + (z_max - z_min) * (i as f32 / m as f32);
            let z_end = log_split * lambda + (1.0f32 - lambda) * uniform_split;

            self.cascades[cascade_index] = Self::build_cascade(light, inv_camera_view_proj, z_start, z_end, z_min, z_max, self.shadow_map_res);
            z_start = z_end;
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
        _view: &View
    ) {
        cmd_buffer.begin_label("Shadow map");

        let light = scene.directional_lights().first();
        if light.is_none() {
            return;
        }

        let draw_buffer = resources.access_buffer(
            cmd_buffer,
            Self::DRAW_BUFFER_NAME,
            BarrierSync::INDIRECT,
            BarrierAccess::INDIRECT_READ,
            HistoryResourceEntry::Current,
        );

        let mut cascade_index = 0u32;
        for cascade in &self.cascades {
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
                    base_array_layer: cascade_index,
                    array_layer_length: 1,
                    format: None,
                },
                HistoryResourceEntry::Current,
            );

            cmd_buffer.flush_barriers();

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

            cmd_buffer.upload_dynamic_data_inline(&[cascade.view_proj], ShaderType::VertexShader);

            cmd_buffer.finish_binding();
            cmd_buffer.draw_indexed_indirect(&draw_buffer, 4, &draw_buffer, 0, DRAW_CAPACITY, 20);

            cmd_buffer.end_render_pass();

            cascade_index += 1;
        }
        cmd_buffer.end_label();
    }

    pub fn build_cascade(light: &RendererDirectionalLight<P::GraphicsBackend>, inv_camera_view_proj: Matrix4, cascade_z_start: f32, cascade_z_end: f32, z_min: f32, z_max: f32, shadow_map_res: u32) -> ShadowMapCascade {
        // https://www.junkship.net/News/2020/11/22/shadow-of-a-doubt-part-2
        // https://github.com/BabylonJS/Babylon.js/blob/master/packages/dev/core/src/Lights/Shadows/cascadedShadowGenerator.ts
        // https://alextardif.com/shadowmapping.html
        // https://therealmjp.github.io/posts/shadow-maps/
        // https://learn.microsoft.com/en-us/windows/win32/dxtecharts/common-techniques-to-improve-shadow-depth-maps
        // https://github.com/TheRealMJP/Shadows/blob/master/Shadows/SetupShadows.hlsl

        let mut world_space_frustum_corners = [Vec4::new(0f32, 0f32, 0f32, 0f32); 8];
        for x in 0..2 {
            for y in 0..2 {
                    for z in 0..2 {
                    let mut world_space_frustum_corner = inv_camera_view_proj * Vec4::new(
                        2.0f32 * (x as f32) - 1.0f32,
                        2.0f32 * (y as f32) - 1.0f32,
                        z as f32,
                        1.0f32
                    );
                    world_space_frustum_corner /= world_space_frustum_corner.w;
                    world_space_frustum_corners[z * 4 + x * 2 + y] = world_space_frustum_corner;
                }
            }
        }

        let z_range = z_max - z_min;
        let start_depth = (cascade_z_start - z_min) / z_range;
        let end_depth = (cascade_z_end - z_min) / z_range;
        for i in 0..4 {
            let corner_ray = world_space_frustum_corners[i + 4] - world_space_frustum_corners[i];
            let near_corner_ray = corner_ray * start_depth;
            let far_corner_ray = corner_ray * end_depth;
            world_space_frustum_corners[i + 4] = world_space_frustum_corners[i] + far_corner_ray;
            world_space_frustum_corners[i] = world_space_frustum_corners[i] + near_corner_ray;
        }

        let mut center = Vec3::new(0f32, 0f32, 0f32);
        for corner in &world_space_frustum_corners {
            center += corner.xyz();
        }
        center /= world_space_frustum_corners.len() as f32;

        let mut radius = 0.0f32;
        for corner in &world_space_frustum_corners {
            radius = radius.max((corner.xyz() - center).magnitude());
        }

        let mut min = Vec3::new(-radius, -radius, -radius);
        let mut max = Vec3::new(radius, radius, radius);

        let mut light_view = Matrix4::look_at_lh(&Point3::from(center - light.direction), &Point3::from(center), &Vec3::new(0f32, 1f32, 0f32));

        // Snap center to texel
        let texels_per_unit = (shadow_map_res as f32) / (radius * 2.0f32);
        let snapping_view = Matrix4::new_scaling(texels_per_unit) * light_view.clone();
        let snapping_view_inv = snapping_view.try_inverse().unwrap();
        let mut view_space_center = snapping_view.transform_vector(&center);
        view_space_center.x = view_space_center.x.floor();
        view_space_center.y = view_space_center.y.floor();
        center = snapping_view_inv.transform_vector(&view_space_center);
        light_view = Matrix4::look_at_lh(&Point3::from(center - light.direction), &Point3::from(center), &Vec3::new(0f32, 1f32, 0f32));

        // Snap left, right, top. bottom to texel
        let world_units_per_texel = (radius * 2f32) / (shadow_map_res as f32);
        min.x = (min.x / world_units_per_texel).floor() * world_units_per_texel;
        min.y = (min.y / world_units_per_texel).floor() * world_units_per_texel;
        max.x = (max.x / world_units_per_texel).ceil() * world_units_per_texel;
        max.y = (max.y / world_units_per_texel).ceil() * world_units_per_texel;

        let light_proj = nalgebra_glm::ortho_lh_zo(min.x, max.x, min.y, max.y, 0.01f32, max.z - min.z);
        let light_mat = light_proj * light_view;

        ShadowMapCascade {
            _padding: Default::default(),
            view_proj: light_mat,
            z_min: cascade_z_start,
            z_max: cascade_z_end
        }
    }

    pub fn resolution(&self) -> u32 {
        self.shadow_map_res
    }

    pub fn cascades(&self) -> &[ShadowMapCascade] {
        return &self.cascades;
    }
}
