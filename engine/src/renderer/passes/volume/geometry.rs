use crate::graphics::*;
use crate::renderer::asset::{GraphicsPipelineHandle, GraphicsPipelineInfo, MeshGraphicsPipelineInfo, MeshGraphicsPipelineHandle, PathPipelineShaderStage, RendererAssets, RendererAssetsReadOnly, RendererMaterial, PipelinePermutations, PipelineHandle, GraphicsPipelinePermutations, MeshGraphicsPipelinePermutations};
use crate::renderer::drawable::{RendererVolumeDrawable, VolumeDrawableTransparencyMode};
use crate::renderer::passes::volume::ibl::ImageBasedLightingTextures;
use crate::renderer::passes::volume::marching_cubes::{
    MarchingCubesIndirectCall, MarchingCubesPass,
};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{BufferAccessor, HistoryResourceEntry, RendererResources};
use bytemuck::{Pod, Zeroable};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{SpecConstValue, StencilOp, TexturePlane};
use sourcerenderer_core::{Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec3UI};
use std::cell::Ref;
use std::default::Default;
use std::sync::Arc;
use crate::asset::MaterialHandle;
use crate::renderer::VolumeRendererOptions;

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
struct PushConstantData {
    model_matrix: Matrix4,
    lod_extents: Vec3UI,
    threshold: f32,
    lod: u32,
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
    material_data: MaterialData,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
struct MaterialData {
    inv_model_matrix: Matrix4,
    f0: Vec3,
    roughness: f32,
    metalness: f32,
    lod: u32,
    threshold: f32,
    width: f32,
    height: f32,
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum GeometryPassPipelineType {
    Opaque, NonOverlapping, Transparent, TransparentPrepass
}

struct GeometryPassPipelines<T : PipelineHandle, const PIPELINE_SHADER_STAGES: usize> {
    opaque: PipelinePermutations<T, PIPELINE_SHADER_STAGES>,
    non_overlapping: PipelinePermutations<T, PIPELINE_SHADER_STAGES>,
    transparent: PipelinePermutations<T, PIPELINE_SHADER_STAGES>,
    transparent_prepass: PipelinePermutations<T, PIPELINE_SHADER_STAGES>,
}

impl<T: PipelineHandle, const PIPELINE_SHADER_STAGES: usize> GeometryPassPipelines<T, PIPELINE_SHADER_STAGES> {
    fn is_ready(&self, assets: &RendererAssetsReadOnly) -> bool {
        self.opaque.all_ready(assets)
        && self.non_overlapping.all_ready(assets)
        && self.transparent.all_ready(assets)
        && self.transparent_prepass.all_ready(assets)
    }
}

impl GeometryPassPipelines<GraphicsPipelineHandle, 2> {
    fn get<'a>(&self, renderer_assets: &'a RendererAssetsReadOnly<'a>, pipeline_type: GeometryPassPipelineType, raymarch_normals: bool) -> Option<&'a Arc<GraphicsPipeline>> {
        let mut fs_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        fs_spec_consts[0] = Some(SpecConstValue::Bool(raymarch_normals));

        match pipeline_type {
            GeometryPassPipelineType::Opaque => self.opaque.get_pipeline(renderer_assets, &[None; 8], &fs_spec_consts),
            GeometryPassPipelineType::NonOverlapping => self.non_overlapping.get_pipeline(renderer_assets, &[None; 8], &fs_spec_consts),
            GeometryPassPipelineType::Transparent => self.transparent.get_pipeline(renderer_assets, &[None; 8], &fs_spec_consts),
            GeometryPassPipelineType::TransparentPrepass => self.transparent_prepass.get_pipeline(renderer_assets, &[None; 8], &[None; 8]),
        }
    }
}

impl GeometryPassPipelines<MeshGraphicsPipelineHandle, 3> {
    fn get<'a>(&self, renderer_assets: &'a RendererAssetsReadOnly<'a>, pipeline_type: GeometryPassPipelineType,
               raymarch_normals: bool, render_as_cubes: bool, culled: bool) -> Option<&'a Arc<MeshGraphicsPipeline>> {
        let mut ts_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        ts_spec_consts[0] = Some(SpecConstValue::Bool(render_as_cubes));
        ts_spec_consts[1] = Some(SpecConstValue::Bool(culled));
        let mut fs_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        fs_spec_consts[0] = Some(SpecConstValue::Bool(raymarch_normals));

        match pipeline_type {
            GeometryPassPipelineType::Opaque => self.opaque.get_pipeline(renderer_assets, &ts_spec_consts, &[None; 8], &fs_spec_consts),
            GeometryPassPipelineType::NonOverlapping => self.non_overlapping.get_pipeline(renderer_assets, &ts_spec_consts, &[None; 8], &fs_spec_consts),
            GeometryPassPipelineType::Transparent => self.transparent.get_pipeline(renderer_assets, &ts_spec_consts, &[None; 8], &fs_spec_consts),
            GeometryPassPipelineType::TransparentPrepass => self.transparent_prepass.get_pipeline(renderer_assets, &ts_spec_consts, &[None; 8], &[None; 8]),
        }
    }
}

pub struct GeometryPass {
    pipelines: GeometryPassPipelines<GraphicsPipelineHandle, 2>,
    mesh_pipelines: Option<GeometryPassPipelines<MeshGraphicsPipelineHandle, 3>>,
}

impl GeometryPass {
    pub const COLOR_TEXTURE_NAME: &'static str = "GeometryColor";
    pub const DEPTH_TEXTURE_NAME: &'static str = "Depth";
    pub const SSS_INTENSITY_TEXTURE_NAME: &'static str = "SSSIntensity";

    pub(crate) fn new(
        device: &Arc<crate::graphics::Device>,
        assets: &RendererAssets,
        resources: &mut RendererResources,
        resolution: Vec2UI,
    ) -> Self {
        Self::create_textures(resources, resolution);

        let vs_path = crate::renderer::shader_path!("volume_geometry.vert");
        let fs_path = crate::renderer::shader_path!("volume_geometry.frag");
        let ms_path = crate::renderer::shader_path!("marching_cubes.mesh");
        let ts_path = crate::renderer::shader_path!("marching_cubes.task");

        let ts_stage_path_opt = Some(PathPipelineShaderStage::empty_spec_consts(ts_path));
        let ms_stage_path = PathPipelineShaderStage::empty_spec_consts(ms_path);

        let mut ts_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        ts_spec_consts[0] = Some(SpecConstValue::Bool(false));
        ts_spec_consts[1] = Some(SpecConstValue::Bool(false));

        let mut ts_culled_cube_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        ts_culled_cube_spec_consts[0] = Some(SpecConstValue::Bool(true));
        ts_culled_cube_spec_consts[1] = Some(SpecConstValue::Bool(true));

        let mut ts_culled_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        ts_culled_spec_consts[0] = Some(SpecConstValue::Bool(false));
        ts_culled_spec_consts[1] = Some(SpecConstValue::Bool(true));

        let mut ts_cube_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        ts_cube_spec_consts[0] = Some(SpecConstValue::Bool(true));
        ts_cube_spec_consts[1] = Some(SpecConstValue::Bool(false));

        let mut fs_spec_consts: [Option::<SpecConstValue>; 8] = [None; 8];
        fs_spec_consts[0] = Some(SpecConstValue::Bool(false));

        let mut fs_spec_consts_raymarched: [Option::<SpecConstValue>; 8] = [None; 8];
        fs_spec_consts_raymarched[0] = Some(SpecConstValue::Bool(true));

        let fs_permutations = [
            fs_spec_consts,
            fs_spec_consts_raymarched,
        ];

        let ts_permutations = [
            ts_spec_consts,
            ts_cube_spec_consts,
            ts_culled_cube_spec_consts,
            ts_culled_spec_consts,
        ];

        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: PathPipelineShaderStage::empty_spec_consts(&vs_path),
            fs: Some(PathPipelineShaderStage::empty_spec_consts(&fs_path)),
            primitive_type: PrimitiveType::Triangles,
            //primitive_type: PrimitiveType::Points, // Uncomment to debug culling
            vertex_layout: VertexLayoutInfo {
                input_assembler: &[],
                shader_inputs: &[],
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
                stencil_enable: true,
                stencil_read_mask: !0u8,
                stencil_write_mask: !0u8,
                stencil_front: StencilInfo {
                    pass_op: StencilOp::Replace,
                    fail_op: StencilOp::Keep,
                    func: CompareFunc::Always,
                    depth_fail_op: StencilOp::Keep,
                },
                stencil_back: StencilInfo::default(),
            },
            blend: BlendInfo {
                alpha_to_coverage_enabled: false,
                logic_op_enabled: false,
                logic_op: LogicOp::And,
                constants: [0f32, 0f32, 0f32, 0f32],
                attachments: &[
                    AttachmentBlendInfo {
                        blend_enabled: false,
                        src_color_blend_factor: BlendFactor::SrcAlpha,
                        dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                        color_blend_op: BlendOp::Add,
                        src_alpha_blend_factor: BlendFactor::Zero,
                        dst_alpha_blend_factor: BlendFactor::One,
                        alpha_blend_op: BlendOp::Add,
                        write_mask: ColorComponents::all(),
                    },
                    AttachmentBlendInfo {
                        blend_enabled: false,
                        src_color_blend_factor: BlendFactor::One,
                        dst_color_blend_factor: BlendFactor::Zero,
                        color_blend_op: BlendOp::Add,
                        src_alpha_blend_factor: BlendFactor::One,
                        dst_alpha_blend_factor: BlendFactor::Zero,
                        alpha_blend_op: BlendOp::Add,
                        write_mask: ColorComponents::all(),
                    },
                ],
            },
            render_target_formats: &[Format::RGBA16UNorm, Format::R8UNorm],
            depth_stencil_format: Format::D32S8, // I'd prefer D24S8 but AMD & Apple don't support that.
        };
        let pipeline = GraphicsPipelinePermutations::request(
            assets, &pipeline_info, &[], &fs_permutations);

        let mut pipeline_non_overlapping_info: GraphicsPipelineInfo =
            pipeline_info.clone();
        pipeline_non_overlapping_info
            .depth_stencil
            .stencil_front = StencilInfo {
            pass_op: StencilOp::Keep,
            fail_op: StencilOp::Keep,
            func: CompareFunc::Greater,
            depth_fail_op: StencilOp::Keep,
        };
        let pipeline_non_overlapping = GraphicsPipelinePermutations::request(
            assets, &pipeline_non_overlapping_info, &[], &fs_permutations);

        let mut pipeline_transparency_prepass_info: GraphicsPipelineInfo = pipeline_info.clone();
        pipeline_transparency_prepass_info.fs = None;
        let blend_attachments_prepass = [
            AttachmentBlendInfo {
                blend_enabled: false,
                write_mask: ColorComponents::empty(),
                ..Default::default()
            },
            AttachmentBlendInfo {
                blend_enabled: false,
                write_mask: ColorComponents::empty(),
                ..Default::default()
            },
        ];
        pipeline_transparency_prepass_info.blend = BlendInfo {
            alpha_to_coverage_enabled: false,
            logic_op_enabled: false,
            logic_op: LogicOp::And,
            constants: [0f32, 0f32, 0f32, 0f32],
            attachments: &blend_attachments_prepass,
        };
        pipeline_transparency_prepass_info
            .depth_stencil
            .stencil_front = StencilInfo {
            pass_op: StencilOp::Keep,
            fail_op: StencilOp::Keep,
            func: CompareFunc::LessEqual,
            depth_fail_op: StencilOp::Keep,
        };
        let pipeline_transparent_prepass = GraphicsPipelinePermutations::request(
            assets, &pipeline_transparency_prepass_info, &[], &[]);

        let mut pipeline_transparency_info: GraphicsPipelineInfo = pipeline_info.clone();
        pipeline_transparency_info.depth_stencil.depth_func = CompareFunc::Equal;
        pipeline_transparency_info.depth_stencil.depth_write_enabled = false;
        pipeline_transparency_info.depth_stencil.stencil_front = StencilInfo {
            pass_op: StencilOp::Keep,
            fail_op: StencilOp::Keep,
            func: CompareFunc::LessEqual,
            depth_fail_op: StencilOp::Keep,
        };
        let blend_attachments = [
            AttachmentBlendInfo {
                blend_enabled: true,
                src_color_blend_factor: BlendFactor::SrcAlpha,
                dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                color_blend_op: BlendOp::Add,
                src_alpha_blend_factor: BlendFactor::One,
                dst_alpha_blend_factor: BlendFactor::Zero,
                alpha_blend_op: BlendOp::Add,
                write_mask: ColorComponents::all(),
            },
            AttachmentBlendInfo {
                blend_enabled: true,
                src_color_blend_factor: BlendFactor::One,
                dst_color_blend_factor: BlendFactor::Zero,
                color_blend_op: BlendOp::Add,
                src_alpha_blend_factor: BlendFactor::One,
                dst_alpha_blend_factor: BlendFactor::Zero,
                alpha_blend_op: BlendOp::Add,
                write_mask: ColorComponents::all(),
            },
        ];
        pipeline_transparency_info.blend = BlendInfo {
            alpha_to_coverage_enabled: false,
            logic_op_enabled: false,
            logic_op: LogicOp::And,
            constants: [0f32, 0f32, 0f32, 0f32],
            attachments: &blend_attachments,
        };
        let pipeline_transparent = GraphicsPipelinePermutations::request(
            assets, &pipeline_transparency_info, &[], &fs_permutations);

        let mesh_pipelines = if device.supports_mesh_shader() {
            let pipeline_info_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_mesh = MeshGraphicsPipelinePermutations::request(
                assets, &pipeline_info_mesh, &ts_permutations, &[], &fs_permutations);

            let pipeline_non_overlapping_info_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_non_overlapping_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_non_overlapping_mesh = MeshGraphicsPipelinePermutations::request(
                assets, &pipeline_non_overlapping_info_mesh, &ts_permutations, &[], &fs_permutations);

            let pipeline_transparency_prepass_info_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_transparency_prepass_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_transparent_prepass_mesh = MeshGraphicsPipelinePermutations::request(
                assets, &pipeline_transparency_prepass_info_mesh, &ts_permutations, &[], &[]);

            let pipeline_info_transparency_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_transparency_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_transparent_mesh = MeshGraphicsPipelinePermutations::request(
                assets, &pipeline_info_transparency_mesh, &ts_permutations, &[], &fs_permutations);

            Some(GeometryPassPipelines {
                opaque: pipeline_mesh,
                non_overlapping: pipeline_non_overlapping_mesh,
                transparent: pipeline_transparent_mesh,
                transparent_prepass: pipeline_transparent_prepass_mesh,
            })
        } else {
            log::warn!("Mesh shaders are not supported. Ignoring mesh shader option.");
            None
        };

        Self {
            pipelines: GeometryPassPipelines {
                opaque: pipeline,
                non_overlapping: pipeline_non_overlapping,
                transparent: pipeline_transparent,
                transparent_prepass: pipeline_transparent_prepass,
            },
            mesh_pipelines
        }
    }

    pub fn create_textures(resources: &mut RendererResources, resolution: Vec2UI) {
        resources.create_texture(
            Self::COLOR_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA16UNorm,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET | TextureUsage::STORAGE,
                supports_srgb: false,
            },
            false,
        );

        resources.create_texture(
            Self::DEPTH_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::D32S8,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::DEPTH_STENCIL,
                supports_srgb: false,
            },
            false,
        );

        resources.create_texture(
            Self::SSS_INTENSITY_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::R8UNorm,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET | TextureUsage::STORAGE,
                supports_srgb: false,
            },
            false,
        );
    }

    #[inline(always)]
    pub(crate) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        self.pipelines.is_ready(assets)
        && self.mesh_pipelines.as_ref().map_or(true, |p| p.is_ready(assets))
    }

    pub(crate) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        params: &RenderPassParameters,
        options: &VolumeRendererOptions,
        marching_cubes_map: &super::marching_cubes::OffsetsMap,
        tris_table: &Arc<BufferSlice>,
        ibl_textures: &ImageBasedLightingTextures,
    ) {
        cmd_buffer.clear_all_bindings(BindingFrequency::Frequent);
        cmd_buffer.clear_all_bindings(BindingFrequency::VeryFrequent);

        let use_mesh_shader = params.device.supports_mesh_shader() && options.use_mesh_shader;

        let resources = &params.resources;

        if !resources.has_resource(ibl_textures.filtered_diffuse_environment_map_texture_name)
            || !resources.has_resource(ibl_textures.filtered_specular_environment_map_teture_name)
        {
            return;
        }

        let color_tex_info = resources.texture_info(Self::COLOR_TEXTURE_NAME);
        let color_tex_extent = Vec2UI::new(color_tex_info.width, color_tex_info.height);

        let color_view = resources.access_view(
            cmd_buffer,
            Self::COLOR_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let sss_view = resources.access_view(
            cmd_buffer,
            Self::SSS_INTENSITY_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

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

        let marchingcubes_indirect = resources.access_buffer(
            cmd_buffer,
            MarchingCubesPass::ATOMICS_BUFFER_NAME,
            BarrierSync::INDIRECT,
            BarrierAccess::INDIRECT_READ,
            HistoryResourceEntry::Current,
        );

        let integration_lut = resources.access_view(
            cmd_buffer,
            ibl_textures.preintegration_nap_texture_name,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let env_map_diffuse = resources.access_view(
            cmd_buffer,
            ibl_textures.filtered_diffuse_environment_map_texture_name,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let env_specular_info =
            resources.texture_info(ibl_textures.filtered_specular_environment_map_teture_name);
        let env_specular_mips = env_specular_info.mip_levels;
        let env_map_specular = resources.access_view(
            cmd_buffer,
            ibl_textures.filtered_specular_environment_map_teture_name,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                mip_level_length: env_specular_mips,
                format: None,
                plane: TexturePlane::Primary,
            },
            HistoryResourceEntry::Current,
        );

        let mut slices: SmallVec<[(String, BufferAccessor); 4]> =
            SmallVec::with_capacity(params.scene.scene.volume_mesh_instances().len());
        if !use_mesh_shader {
            for drawable in params.scene.scene.volume_mesh_instances() {
                let name = MarchingCubesPass::index_buffer_name(drawable.entity);
                let slice = resources.access_buffer(
                    cmd_buffer,
                    &MarchingCubesPass::index_buffer_name(drawable.entity),
                    BarrierSync::INDEX_INPUT,
                    BarrierAccess::INDEX_READ,
                    HistoryResourceEntry::Current,
                );
                slices.push((name, slice));
            }
        } else {
            for drawable in params.scene.scene.volume_mesh_instances() {
                let name = MarchingCubesPass::culling_buffer_name(drawable.entity);
                let slice = resources.access_buffer(
                    cmd_buffer,
                    &name,
                    BarrierSync::INDIRECT | BarrierSync::TASK_SHADER,
                    BarrierAccess::INDIRECT_READ | BarrierAccess::STORAGE_READ,
                    HistoryResourceEntry::Current,
                );
                slices.push((name, slice));
            }
        }
        slices.push((MarchingCubesPass::ATOMICS_BUFFER_NAME.to_string(), marchingcubes_indirect));

        cmd_buffer.flush_barriers();

        cmd_buffer.begin_label("Geometry");

        let has_opaque = params
            .scene
            .scene
            .volume_mesh_instances()
            .iter()
            .any(|d| d.transparent == VolumeDrawableTransparencyMode::Opaque);

        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[
                RenderTarget {
                    view: color_view.view(),
                    load_op: LoadOpColor::Load,
                    store_op: StoreOp::Store,
                },
                RenderTarget {
                    view: sss_view.view(),
                    load_op: LoadOpColor::Clear(ClearColor::BLACK),
                    store_op: StoreOp::Store,
                },
            ],
            depth_stencil: Some(&DepthStencilAttachment {
                view: dsv.view(),
                load_op: LoadOpDepthStencil::Clear(ClearDepthStencilValue {
                    depth: 1.0f32,
                    stencil: 0u32,
                }),
                store_op: StoreOp::Store,
            }),
            query_range: None,
            resume_suspend: RenderPassResumeSuspend::empty(),
        });

        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0.0f32, 0.0f32),
            extent: Vec2::new(color_tex_extent.x as f32, color_tex_extent.y as f32),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);
        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0, 0),
            extent: color_tex_extent,
        }]);
        cmd_buffer.set_stencil_reference(1u32);

        let base_pass = |cmd_buffer: &mut CommandBuffer, ray_march_normals: bool| {
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                4u32,
                env_map_diffuse.view(),
            );
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                5u32,
                env_map_specular.view(),
            );
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                6u32,
                integration_lut.view(),
            );

            for drawable in params.scene.scene.volume_mesh_instances() {
                if drawable.transparent != VolumeDrawableTransparencyMode::Opaque
                    && !(!has_opaque
                        && drawable.transparent
                            == VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque)
                {
                    continue;
                }

                if drawable.ray_march_normals != ray_march_normals {
                    continue;
                }
                self.draw(cmd_buffer, params.assets, color_tex_extent, marching_cubes_map, &slices,
                          use_mesh_shader, GeometryPassPipelineType::Opaque, options.culling, tris_table, drawable);
            }
        };

        // Iterate over the meshes twice to reduce pipeline binding changes.
        cmd_buffer.begin_label("Opaque");
        base_pass(cmd_buffer, false);
        cmd_buffer.end_label();
        cmd_buffer.begin_label("Opaque with ray marched normals");
        base_pass(cmd_buffer, true);
        cmd_buffer.end_label();

        // Geometry 2 - Non overlapping

        let mut transparent_drawables: SmallVec<[RendererVolumeDrawable; 2]> = params
            .scene
            .scene
            .volume_mesh_instances()
            .iter()
            .filter(|d| d.transparent != VolumeDrawableTransparencyMode::Opaque)
            .cloned()
            .collect();
        transparent_drawables.sort_by_key(|d| (d.min_threshold * 1000.0f32) as u32); // good enough

        // Decide pipeline per-mesh here because he have a fixed order.
        cmd_buffer.begin_label("Non overlapping");
        for drawable in &transparent_drawables {
            if !has_opaque {
                break;
            }
            if drawable.transparent != VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque {
                continue;
            }
            self.draw(cmd_buffer, params.assets, color_tex_extent, marching_cubes_map, &slices,
                      use_mesh_shader, GeometryPassPipelineType::NonOverlapping, options.culling, tris_table, drawable);
        }
        cmd_buffer.end_label();

        // Geometry 2 - Depth prepass

        // Decide pipeline per-mesh here because we have a fixed order.
        cmd_buffer.begin_label("Transparency depth prepass");
        for drawable in &transparent_drawables {
            if drawable.transparent == VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque {
                if !has_opaque {
                    continue;
                }
                cmd_buffer.set_stencil_reference(1u32);
            } else if drawable.transparent == VolumeDrawableTransparencyMode::Transparent {
                cmd_buffer.set_stencil_reference(0u32);
            }
            self.draw(cmd_buffer, params.assets, color_tex_extent, marching_cubes_map, &slices,
                      use_mesh_shader, GeometryPassPipelineType::TransparentPrepass, options.culling, tris_table, drawable);
        }
        cmd_buffer.end_label();

        // Geometry 2 - Transparent

        cmd_buffer.begin_label("Transparent");
        for drawable in &transparent_drawables {
            if drawable.transparent == VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque {
                if !has_opaque {
                    continue;
                }
                cmd_buffer.set_stencil_reference(1u32);
            } else if drawable.transparent == VolumeDrawableTransparencyMode::Transparent {
                cmd_buffer.set_stencil_reference(0u32);
            }
            self.draw(cmd_buffer, params.assets, color_tex_extent, marching_cubes_map, &slices,
                       use_mesh_shader, GeometryPassPipelineType::Transparent, options.culling, tris_table, drawable);
        }
        cmd_buffer.end_label();

        cmd_buffer.end_render_pass();

        cmd_buffer.end_label();
    }

    fn bind_material(cmd_buffer: &mut CommandBuffer, assets: &RendererAssetsReadOnly, material: MaterialHandle) {
        let material_opt = assets.get_material_opt(material);
        if material_opt.is_none() {
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                1u32,
                &assets.get_placeholder_texture_white().view,
            );
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                2u32,
                &assets.get_placeholder_texture_white().view,
            );
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                3u32,
                &assets.get_placeholder_texture_black().view,
            );
            return;
        }

        let material = material_opt.unwrap();
        match material {
            RendererMaterial::SimplePBR {
                albedo, roughness, metalness,
                albedo_color: _, roughness_factor: _, metalness_factor: _
            } => {
                let albedo_texture = albedo
                    .and_then(|t|
                        assets.get_texture_opt(t))
                    .unwrap_or(assets.get_placeholder_texture_white());

                cmd_buffer.bind_sampling_view(
                    BindingFrequency::Frequent,
                    1u32,
                    &albedo_texture.view,
                );

                let roughness_texture = roughness
                    .and_then(|t|
                        assets.get_texture_opt(t))
                    .unwrap_or(assets.get_placeholder_texture_white());

                cmd_buffer.bind_sampling_view(
                    BindingFrequency::Frequent,
                    2u32,
                    &roughness_texture.view,
                );

                let metalness_texture = metalness
                    .and_then(|t|
                        assets.get_texture_opt(t))
                    .unwrap_or(assets.get_placeholder_texture_black());

                cmd_buffer.bind_sampling_view(
                    BindingFrequency::Frequent,
                    3u32,
                    &metalness_texture.view,
                );
            },
            _ => panic!("Unexpected material"),
        }
    }

    fn draw(
        &self,
        cmd_buffer: &mut CommandBuffer,
        assets: &RendererAssetsReadOnly,
        rt_extent: Vec2UI,
        marching_cubes_map: &super::marching_cubes::OffsetsMap,
        buffers: &[(String, BufferAccessor)],
        use_mesh_shader: bool,
        pipeline_type: GeometryPassPipelineType,
        culling_enabled: bool,
        tris_table: &Arc<BufferSlice>,
        drawable: &RendererVolumeDrawable) -> bool {
        let volume_texture = assets.get_texture(drawable.volume_texture);
        let volume_texture_base_opt = volume_texture.view.texture();
        if volume_texture_base_opt.is_none() {
            log::error!("Could not load texture for volume geometry pass: {:?}", drawable.volume_texture);
            return false;
        }
        let volume_texture_base = volume_texture_base_opt.unwrap();
        let volume_texture_info = volume_texture_base.info();
        let volume_texture_lod_extents = Vec3UI::new(
            (volume_texture_info.width >> drawable.texture_lod).max(1u32),
            (volume_texture_info.height >> drawable.texture_lod).max(1u32),
            (volume_texture_info.depth >> drawable.texture_lod).max(1u32),
        );

        let drawable_culling = culling_enabled && MarchingCubesPass::use_culling(volume_texture_lod_extents);

        let mut model_matrix = drawable.transform.into();
        model_matrix *= Matrix4::from_scale(Vec3::new(
            (volume_texture_info.width as f32) / (volume_texture_lod_extents.x as f32),
            (volume_texture_info.height as f32) / (volume_texture_lod_extents.y as f32),
            (volume_texture_info.depth as f32) / (volume_texture_lod_extents.z as f32),
        ));

        let pipeline = if !use_mesh_shader {
            PipelineBinding::Graphics(self.pipelines
                .get(assets, pipeline_type, drawable.ray_march_normals)
                .expect("Pipeline is not compiled yet"))
        } else {
            PipelineBinding::MeshGraphics(self.mesh_pipelines.as_ref().unwrap()
                .get(assets, pipeline_type, drawable.ray_march_normals, drawable.render_as_cubes, drawable_culling)
                .expect("Pipeline is not compiled yet"))
        };
        cmd_buffer.set_pipeline(pipeline);

        cmd_buffer.bind_sampling_view(
            BindingFrequency::Frequent,
            0u32,
            &volume_texture.view,
        );
        Self::bind_material(cmd_buffer, assets, drawable.material_handle);

        cmd_buffer.set_push_constant_data(
            &[PushConstantData {
                model_matrix,
                threshold: drawable.min_threshold,
                lod_extents: volume_texture_lod_extents,
                lod: drawable.texture_lod,
                material_data: MaterialData {
                    roughness: 0.6f32,
                    metalness: 0.3f32,
                    //roughness: 0.1f32,
                    //metalness: 0.9f32,
                    f0: Vec3::new(0.04f32, 0.04f32, 0.04f32),
                    inv_model_matrix: Matrix4::inverse(&model_matrix),
                    lod: drawable.texture_lod,
                    width: rt_extent.x as f32,
                    height: rt_extent.y as f32,
                    threshold: drawable.min_threshold,
                    ..Zeroable::zeroed()
                },
                ..Zeroable::zeroed()
            }],
        );

        if !use_mesh_shader {
            let ibo = &(buffers.iter()
                .find(|(name, _)| name == &MarchingCubesPass::index_buffer_name(drawable.entity)).unwrap().1);
            let indirect_buffer = &(buffers.iter()
                .find(|(name, _)| name == MarchingCubesPass::ATOMICS_BUFFER_NAME).unwrap().1);

            let indirect_buffer_offset = *(marching_cubes_map.get(&drawable.entity).unwrap());
            cmd_buffer.set_index_buffer(ibo.buffer_ref(), 0u64, IndexFormat::U32);

            cmd_buffer.finish_binding();
            cmd_buffer.draw_indexed_indirect(
                indirect_buffer.buffer_ref(),
                indirect_buffer_offset as u64,
                1u32,
                std::mem::size_of::<MarchingCubesIndirectCall>() as u32,
            );
        } else {
            cmd_buffer.bind_uniform_buffer(BindingFrequency::Frequent, 7,
                                           BufferRef::Regular(tris_table), 0, WHOLE_BUFFER);

            let culling_buffer = &(buffers.iter()
                .find(|(name, _)| name == &MarchingCubesPass::culling_buffer_name(drawable.entity)).unwrap().1);
            cmd_buffer.bind_storage_buffer(BindingFrequency::Frequent, 8,
                                           culling_buffer.buffer_ref(), 0u64, WHOLE_BUFFER);
            cmd_buffer.finish_binding();

            if drawable_culling {
                cmd_buffer.draw_mesh_tasks_indirect(
                    culling_buffer.buffer_ref(),
                    0, 1, (3 * std::mem::size_of::<u32>()) as u32
                );
            } else {
                cmd_buffer.draw_mesh_tasks(
                    (volume_texture_lod_extents.x + 3) / 4,
                    (volume_texture_lod_extents.y + 3) / 4,
                    (volume_texture_lod_extents.z + 1) / 2,
                );
            }
        }

        true
    }
}