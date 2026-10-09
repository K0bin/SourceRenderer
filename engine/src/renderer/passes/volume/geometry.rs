use crate::graphics::*;
use crate::renderer::asset::{GraphicsPipelineHandle, GraphicsPipelineInfo, MeshGraphicsPipelineInfo, MeshGraphicsPipelineHandle, PathPipelineShaderStage, RendererAssets, RendererAssetsReadOnly, RendererMaterial};
use crate::renderer::drawable::{RendererVolumeDrawable, VolumeDrawableTransparencyMode};
use crate::renderer::passes::volume::ibl::ImageBasedLightingTextures;
use crate::renderer::passes::volume::marching_cubes::{
    MarchingCubesIndirectCall, MarchingCubesInfo, MarchingCubesKey, MarchingCubesPass,
};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use bytemuck::{Pod, Zeroable};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{SpecConstValue, StencilOp, TexturePlane};
use sourcerenderer_core::{Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec3UI};
use std::cell::Ref;
use std::collections::HashMap;
use std::default::Default;
use std::sync::Arc;
use crate::asset::{AssetHandle, MaterialHandle};

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

struct GeometryPassPipelines<T : Clone + Copy + From<AssetHandle> + Into<AssetHandle>> {
    opaque: T,
    non_overlapping: T,
    transparent: T,
    transparent_prepass: T,
}

impl<T: Clone + Copy + From<AssetHandle> + Into<AssetHandle>> GeometryPassPipelines<T> {
    fn is_ready(&self, assets: &RendererAssetsReadOnly) -> bool {
        assets.has_pipeline(self.opaque.into())
            && assets.has_pipeline(self.non_overlapping.into())
            && assets.has_pipeline(self.transparent.into())
            && assets.has_pipeline(self.transparent_prepass.into())
    }
}

struct GeometryPassPipelineRefs<'a, T> {
    opaque: &'a Arc<T>,
    non_overlapping: &'a Arc<T>,
    transparent: &'a Arc<T>,
    transparent_prepass: &'a Arc<T>,
}

impl<'a> GeometryPassPipelineRefs<'a, GraphicsPipeline> {
    fn load(handles: &GeometryPassPipelines<GraphicsPipelineHandle>, renderer_assets: &'a RendererAssetsReadOnly<'a>) -> Self {
        Self {
            opaque: renderer_assets.get_graphics_pipeline(handles.opaque)
                .expect("Pipeline is not compiled yet"),
            non_overlapping: renderer_assets.get_graphics_pipeline(handles.non_overlapping)
                .expect("Pipeline is not compiled yet"),
            transparent: renderer_assets.get_graphics_pipeline(handles.transparent)
                .expect("Pipeline is not compiled yet"),
            transparent_prepass: renderer_assets.get_graphics_pipeline(handles.transparent_prepass)
                .expect("Pipeline is not compiled yet"),
        }
    }
}

impl<'a> GeometryPassPipelineRefs<'a, MeshGraphicsPipeline> {
    fn load(handles: &GeometryPassPipelines<MeshGraphicsPipelineHandle>, renderer_assets: &'a RendererAssetsReadOnly<'a>) -> Self {
        Self {
            opaque: renderer_assets.get_mesh_graphics_pipeline(handles.opaque)
                .expect("Pipeline is not compiled yet"),
            non_overlapping: renderer_assets.get_mesh_graphics_pipeline(handles.non_overlapping)
                .expect("Pipeline is not compiled yet"),
            transparent: renderer_assets.get_mesh_graphics_pipeline(handles.transparent)
                .expect("Pipeline is not compiled yet"),
            transparent_prepass: renderer_assets.get_mesh_graphics_pipeline(handles.transparent_prepass)
                .expect("Pipeline is not compiled yet"),
        }
    }
}

pub struct GeometryPass {
    pipelines: GeometryPassPipelines<GraphicsPipelineHandle>,
    pipelines_non_raymarch: GeometryPassPipelines<GraphicsPipelineHandle>,
    mesh_pipelines: Option<GeometryPassPipelines<MeshGraphicsPipelineHandle>>,
    mesh_pipelines_non_raymarch: Option<GeometryPassPipelines<MeshGraphicsPipelineHandle>>,
    mesh_pipelines_cube: Option<GeometryPassPipelines<MeshGraphicsPipelineHandle>>,
    mesh_pipelines_non_raymarch_cube: Option<GeometryPassPipelines<MeshGraphicsPipelineHandle>>,
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

        let mut cube_spec_consts = HashMap::<u32, SpecConstValue>::with_capacity(1);
        cube_spec_consts.insert(0, SpecConstValue::Bool(true));
        let ts_stage_path_opt = Some(PathPipelineShaderStage::empty_spec_consts(ts_path));
        let ms_stage_path = PathPipelineShaderStage::empty_spec_consts(ms_path);
        let ts_stage_path_cube_opt = Some(PathPipelineShaderStage { shader_path: ts_path, spec_consts: Some(&cube_spec_consts) });
        let ms_stage_path_cube = PathPipelineShaderStage { shader_path: ms_path, spec_consts: Some(&cube_spec_consts) };

        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: PathPipelineShaderStage::empty_spec_consts(&vs_path),
            fs: Some(PathPipelineShaderStage::empty_spec_consts(&fs_path)),
            primitive_type: PrimitiveType::Triangles,
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
        let pipeline = assets.request_graphics_pipeline(&pipeline_info);

        let mut pipeline_transparency_non_overlapping_info: GraphicsPipelineInfo =
            pipeline_info.clone();
        pipeline_transparency_non_overlapping_info
            .depth_stencil
            .stencil_front = StencilInfo {
            pass_op: StencilOp::Keep,
            fail_op: StencilOp::Keep,
            func: CompareFunc::Greater,
            depth_fail_op: StencilOp::Keep,
        };
        let pipeline_non_overlapping =
            assets.request_graphics_pipeline(&pipeline_transparency_non_overlapping_info);

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
        let pipeline_transparent_prepass =
            assets.request_graphics_pipeline(&pipeline_transparency_prepass_info);

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
        let pipeline_transparent = assets.request_graphics_pipeline(&pipeline_transparency_info);

        fn request_with_fs_spec_consts(
            assets: &RendererAssets,
            info: &GraphicsPipelineInfo,
            spec_consts: &HashMap<u32, SpecConstValue>,
        ) -> GraphicsPipelineHandle {
            let mut new_pipeline_info = info.clone();
            new_pipeline_info.fs = Some(info.fs.as_ref().unwrap().with_spec_consts(&spec_consts));
            assets.request_graphics_pipeline(&new_pipeline_info)
        }
        fn request_mesh_with_spec_consts(
            assets: &RendererAssets,
            info: &MeshGraphicsPipelineInfo,
            fs_spec_consts: Option<&HashMap<u32, SpecConstValue>>,
            ms_ts_spec_consts: Option<&HashMap<u32, SpecConstValue>>,
        ) -> MeshGraphicsPipelineHandle {
            let mut new_pipeline_info = info.clone();
            if let Some(fs) = info.fs.as_ref() {
                new_pipeline_info.fs = Some(fs.with_spec_consts_opt(fs_spec_consts));
            }
            new_pipeline_info.ms = info.ms.with_spec_consts_opt(ms_ts_spec_consts);
            new_pipeline_info.ts = Some(info.ts.as_ref().unwrap().with_spec_consts_opt(ms_ts_spec_consts));
            assets.request_mesh_graphics_pipeline(&new_pipeline_info)
        }

        let mut raymarch_spec_consts_hashmap = HashMap::<u32, SpecConstValue>::new();
        raymarch_spec_consts_hashmap.insert(0, SpecConstValue::Bool(false));

        let non_raymarch_pipeline =
            request_with_fs_spec_consts(assets, &pipeline_info, &raymarch_spec_consts_hashmap);
        let non_raymarch_non_overlapping_pipeline = request_with_fs_spec_consts(
            assets,
            &pipeline_transparency_non_overlapping_info,
            &raymarch_spec_consts_hashmap,
        );
        let non_raymarch_transparency_pipeline = request_with_fs_spec_consts(
            assets,
            &pipeline_transparency_info,
            &raymarch_spec_consts_hashmap,
        );

        let (mesh_pipelines, mesh_pipelines_non_raymarch, mesh_pipelines_cube, mesh_pipelines_non_raymarch_cube) = if device.supports_indirect_count_mesh_shader() {
            let pipeline_info_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_mesh = assets.request_mesh_graphics_pipeline(&pipeline_info_mesh);
            let pipeline_mesh_cube = request_mesh_with_spec_consts(assets, &pipeline_info_mesh, None, Some(&cube_spec_consts));

            let pipeline_non_overlapping_info_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_transparency_non_overlapping_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_non_overlapping_mesh = assets.request_mesh_graphics_pipeline(&pipeline_non_overlapping_info_mesh);
            let pipeline_non_overlapping_mesh_cube = request_mesh_with_spec_consts(assets, &pipeline_non_overlapping_info_mesh, None, Some(&cube_spec_consts));

            let pipeline_transparency_prepass_info_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_transparency_prepass_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_transparent_prepass_mesh = assets.request_mesh_graphics_pipeline(&pipeline_transparency_prepass_info_mesh);
            let pipeline_transparent_prepass_mesh_cube = request_mesh_with_spec_consts(assets, &pipeline_transparency_prepass_info_mesh, None, Some(&cube_spec_consts));

            let pipeline_info_transparency_mesh = MeshGraphicsPipelineInfo::from_legacy(&pipeline_transparency_info, &ts_stage_path_opt, &ms_stage_path);
            let pipeline_transparent_mesh = assets.request_mesh_graphics_pipeline(&pipeline_info_transparency_mesh);
            let pipeline_transparent_mesh_cube = request_mesh_with_spec_consts(assets, &pipeline_info_transparency_mesh, None, Some(&cube_spec_consts));

            let non_raymarch_pipeline_mesh =
                request_mesh_with_spec_consts(assets, &pipeline_info_mesh, Some(&raymarch_spec_consts_hashmap), None);
            let non_raymarch_pipeline_non_overlapping_mesh = request_mesh_with_spec_consts(
                assets,
                &pipeline_non_overlapping_info_mesh,
                Some(&raymarch_spec_consts_hashmap),
                None,
            );
            let non_raymarch_transparency_pipeline_mesh = request_mesh_with_spec_consts(
                assets,
                &pipeline_info_transparency_mesh,
                Some(&raymarch_spec_consts_hashmap),
                None,
            );

            let non_raymarch_pipeline_non_overlapping_mesh_cube = request_mesh_with_spec_consts(
                assets,
                &pipeline_non_overlapping_info_mesh,
                Some(&raymarch_spec_consts_hashmap),
                Some(&cube_spec_consts),
            );
            let non_raymarch_pipeline_mesh_cube = request_mesh_with_spec_consts(
                assets,
                &pipeline_info_mesh,
                Some(&raymarch_spec_consts_hashmap),
                Some(&cube_spec_consts),
            );
            let non_raymarch_transparency_pipeline_mesh_cube = request_mesh_with_spec_consts(
                assets,
                &pipeline_info_transparency_mesh,
                Some(&raymarch_spec_consts_hashmap),
                Some(&cube_spec_consts),
            );

            (Some(GeometryPassPipelines {
                opaque: pipeline_mesh,
                non_overlapping: pipeline_non_overlapping_mesh,
                transparent: pipeline_transparent_mesh,
                transparent_prepass: pipeline_transparent_prepass_mesh,
            }),
             Some(GeometryPassPipelines {
                 opaque: non_raymarch_pipeline_mesh,
                 non_overlapping: non_raymarch_pipeline_non_overlapping_mesh,
                 transparent: non_raymarch_transparency_pipeline_mesh,
                 transparent_prepass: pipeline_transparent_prepass_mesh,
             }),
             Some(GeometryPassPipelines {
                 opaque: pipeline_mesh_cube,
                 non_overlapping: pipeline_non_overlapping_mesh_cube,
                 transparent: pipeline_transparent_mesh_cube,
                 transparent_prepass: pipeline_transparent_prepass_mesh_cube,
             }),
             Some(GeometryPassPipelines {
                 opaque:non_raymarch_pipeline_mesh_cube,
                 non_overlapping: non_raymarch_pipeline_non_overlapping_mesh_cube,
                 transparent: non_raymarch_transparency_pipeline_mesh_cube,
                 transparent_prepass: pipeline_transparent_prepass_mesh_cube,
             }))
        } else {
            log::error!("MESH SHADERS NOT SUPPORTED!");
            (None, None, None, None)
        };

        Self {
            pipelines: GeometryPassPipelines {
                opaque: pipeline,
                non_overlapping: pipeline_non_overlapping,
                transparent: pipeline_transparent,
                transparent_prepass: pipeline_transparent_prepass,
            },
            pipelines_non_raymarch: GeometryPassPipelines {
                opaque: non_raymarch_pipeline,
                non_overlapping: non_raymarch_non_overlapping_pipeline,
                transparent: non_raymarch_transparency_pipeline,
                transparent_prepass: pipeline_transparent_prepass,
            },
            mesh_pipelines,
            mesh_pipelines_non_raymarch,
            mesh_pipelines_cube,
            mesh_pipelines_non_raymarch_cube,
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
            && self.pipelines_non_raymarch.is_ready(assets)
            && self.mesh_pipelines.as_ref().map(|p| p.is_ready(assets)).unwrap_or(true)
            && self.mesh_pipelines_cube.as_ref().map(|p| p.is_ready(assets)).unwrap_or(true)
            && self.mesh_pipelines_non_raymarch.as_ref().map(|p| p.is_ready(assets)).unwrap_or(true)
            && self.mesh_pipelines_non_raymarch_cube.as_ref().map(|p| p.is_ready(assets)).unwrap_or(true)
    }

    pub(crate) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        params: &RenderPassParameters,
        marching_cubes_map: &HashMap<MarchingCubesKey, MarchingCubesInfo>,
        tris_table: &Arc<BufferSlice>,
        ibl_textures: &ImageBasedLightingTextures,
    ) {
        cmd_buffer.clear_all_bindings(BindingFrequency::Frequent);
        cmd_buffer.clear_all_bindings(BindingFrequency::VeryFrequent);

        let resources = &params.resources;

        if !resources.has_resource(ibl_textures.filtered_diffuse_environment_map_texture_name)
            || !resources.has_resource(ibl_textures.filtered_specular_environment_map_teture_name)
        {
            return;
        }

        let color_tex_info = resources.texture_info(Self::COLOR_TEXTURE_NAME);
        let color_tex_extent = Vec2UI::new(color_tex_info.width, color_tex_info.height);
        std::mem::drop(color_tex_info);

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
        std::mem::drop(env_specular_info);
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

        let mut slices: SmallVec<[Ref<Arc<BufferSlice>>; 4]> =
            SmallVec::with_capacity(params.scene.scene.volume_mesh_instances().len());
        for drawable in params.scene.scene.volume_mesh_instances() {
            let key = MarchingCubesKey::new(
                drawable.volume_texture,
                drawable.texture_lod,
                drawable.entity,
            );
            let buffer_info = marching_cubes_map.get(&key).unwrap();
            slices.push(resources.access_buffer(
                cmd_buffer,
                &buffer_info.buffer_name,
                BarrierSync::INDEX_INPUT,
                BarrierAccess::INDEX_READ,
                HistoryResourceEntry::Current,
            ));
        }

        cmd_buffer.flush_barriers();
        std::mem::drop(slices);

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
                    view: &color_view,
                    load_op: LoadOpColor::Load,
                    store_op: StoreOp::Store,
                },
                RenderTarget {
                    view: &sss_view,
                    load_op: LoadOpColor::Clear(ClearColor::BLACK),
                    store_op: StoreOp::Store,
                },
            ],
            depth_stencil: Some(&DepthStencilAttachment {
                view: &dsv,
                load_op: LoadOpDepthStencil::Clear(ClearDepthStencilValue {
                    depth: 1.0f32,
                    stencil: 0u32,
                }),
                store_op: StoreOp::Store,
            }),
            query_range: None,
            resume_suspend: RenderPassResumeSuspend::empty(),
        });

        let pipelines = GeometryPassPipelineRefs::<GraphicsPipeline>::load(&self.pipelines, params.assets);
        let pipelines_non_raymarch = GeometryPassPipelineRefs::<GraphicsPipeline>::load(&self.pipelines_non_raymarch, params.assets);

        let pipelines_mesh = self.mesh_pipelines.as_ref().map(|p| GeometryPassPipelineRefs::<MeshGraphicsPipeline>::load(&p, params.assets));
        let pipelines_mesh_cube = self.mesh_pipelines_cube.as_ref().map(|p| GeometryPassPipelineRefs::<MeshGraphicsPipeline>::load(&p, params.assets));
        let pipelines_mesh_non_raymarch = self.mesh_pipelines_non_raymarch.as_ref().map(|p| GeometryPassPipelineRefs::<MeshGraphicsPipeline>::load(&p, params.assets));
        let pipelines_mesh_non_raymarch_cube = self.mesh_pipelines_non_raymarch_cube.as_ref().map(|p| GeometryPassPipelineRefs::<MeshGraphicsPipeline>::load(&p, params.assets));

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

        let mut base_pass = |ray_march_normals: bool| {
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                4u32,
                &env_map_diffuse,
            );
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                5u32,
                &env_map_specular,
            );
            cmd_buffer.bind_sampling_view(
                BindingFrequency::Frequent,
                6u32,
                &integration_lut,
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

                let pipeline: PipelineBinding = if params.device.supports_mesh_shader() {
                    if drawable.render_as_cubes {
                        if drawable.ray_march_normals {
                            pipelines_mesh_cube.as_ref().unwrap().opaque
                        } else {
                            pipelines_mesh_non_raymarch_cube.as_ref().unwrap().opaque
                        }.as_ref().into()
                    } else {
                        if drawable.ray_march_normals {
                            pipelines_mesh.as_ref().unwrap().opaque
                        } else {
                            pipelines_mesh_non_raymarch.as_ref().unwrap().opaque
                        }.as_ref().into()
                    }
                } else {
                    if drawable.ray_march_normals {
                        pipelines.opaque
                    } else {
                        pipelines_non_raymarch.opaque
                    }.as_ref().into()
                };
                Self::draw(cmd_buffer, params.assets, color_tex_extent, params.resources,
                           marching_cubes_map, &marchingcubes_indirect, pipeline, tris_table, drawable);
            }
        };

        // Iterate over the meshes twice to reduce pipeline binding changes.
        base_pass(true);
        base_pass(false);

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
        for drawable in &transparent_drawables {
            if !has_opaque {
                break;
            }
            if drawable.transparent != VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque {
                continue;
            }

            let pipeline: PipelineBinding = if params.device.supports_mesh_shader() {
                if drawable.render_as_cubes {
                    if drawable.ray_march_normals {
                        pipelines_mesh_cube.as_ref().unwrap().non_overlapping
                    } else {
                        pipelines_mesh_non_raymarch_cube.as_ref().unwrap().non_overlapping
                    }.as_ref().into()
                } else {
                    if drawable.ray_march_normals {
                        pipelines_mesh.as_ref().unwrap().non_overlapping
                    } else {
                        pipelines_mesh_non_raymarch.as_ref().unwrap().non_overlapping
                    }.as_ref().into()
                }
            } else {
                if drawable.ray_march_normals {
                    pipelines.non_overlapping
                } else {
                    pipelines_non_raymarch.non_overlapping
                }.as_ref().into()
            };
            Self::draw(cmd_buffer, params.assets, color_tex_extent, params.resources,
                       marching_cubes_map, &marchingcubes_indirect, pipeline, tris_table, drawable);
        }

        // Geometry 2 - Depth prepass

        // Decide pipeline per-mesh here because we have a fixed order.
        for drawable in &transparent_drawables {
            if drawable.transparent == VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque {
                cmd_buffer.set_stencil_reference(1u32);
            } else if drawable.transparent == VolumeDrawableTransparencyMode::Transparent {
                cmd_buffer.set_stencil_reference(0u32);
            }
            let pipeline: PipelineBinding = if params.device.supports_mesh_shader() {
                if drawable.render_as_cubes {
                    pipelines_mesh_cube.as_ref().unwrap().transparent_prepass.as_ref().into()
                } else {
                    pipelines_mesh.as_ref().unwrap().transparent_prepass.as_ref().into()
                }
            } else {
                pipelines.transparent_prepass.as_ref().into()
            };

            Self::draw(cmd_buffer, params.assets, color_tex_extent, params.resources, marching_cubes_map, &marchingcubes_indirect,
                       pipeline, tris_table, drawable);
        }

        // Geometry 2 - Transparent

        for drawable in &transparent_drawables {
            if drawable.transparent == VolumeDrawableTransparencyMode::TransparentInFrontOfOpaque {
                cmd_buffer.set_stencil_reference(1u32);
            } else if drawable.transparent == VolumeDrawableTransparencyMode::Transparent {
                cmd_buffer.set_stencil_reference(0u32);
            }

            let pipeline: PipelineBinding = if params.device.supports_mesh_shader() {
                if drawable.render_as_cubes {
                    if drawable.ray_march_normals {
                        pipelines_mesh_cube.as_ref().unwrap().transparent
                    } else {
                        pipelines_mesh_non_raymarch_cube.as_ref().unwrap().transparent
                    }.as_ref().into()
                } else {
                    if drawable.ray_march_normals {
                        pipelines_mesh.as_ref().unwrap().transparent
                    } else {
                        pipelines_mesh_non_raymarch.as_ref().unwrap().transparent
                    }.as_ref().into()
                }
            } else {
                if drawable.ray_march_normals {
                    pipelines.transparent
                } else {
                    pipelines_non_raymarch.transparent
                }.as_ref().into()
            };
            Self::draw(cmd_buffer, params.assets, color_tex_extent, params.resources,
                       marching_cubes_map, &marchingcubes_indirect, pipeline, tris_table, drawable);
        }

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
        cmd_buffer: &mut CommandBuffer,
        assets: &RendererAssetsReadOnly,
        rt_extent: Vec2UI,
        resources: &RendererResources,
        marching_cubes_map: &HashMap<MarchingCubesKey, MarchingCubesInfo>,
        indirect_buffer: &Arc<BufferSlice>,
        pipeline: PipelineBinding,
        tris_table: &Arc<BufferSlice>,
        drawable: &RendererVolumeDrawable) -> bool {
        let volume_texture = assets.get_texture(drawable.volume_texture);
        let volume_texture_base_opt = volume_texture.view.texture();
        if volume_texture_base_opt.is_none() {
            return false;
        }
        let volume_texture_base = volume_texture_base_opt.unwrap();
        let volume_texture_info = volume_texture_base.info();
        let volume_texture_lod_extents = Vec3UI::new(
            (volume_texture_info.width >> drawable.texture_lod).max(1u32),
            (volume_texture_info.height >> drawable.texture_lod).max(1u32),
            (volume_texture_info.depth >> drawable.texture_lod).max(1u32),
        );

        let mut model_matrix = drawable.transform.into();
        model_matrix *= Matrix4::from_scale(Vec3::new(
            (volume_texture_info.width as f32) / (volume_texture_lod_extents.x as f32),
            (volume_texture_info.height as f32) / (volume_texture_lod_extents.y as f32),
            (volume_texture_info.depth as f32) / (volume_texture_lod_extents.z as f32),
        ));

        let is_mesh = if let &PipelineBinding::MeshGraphics(_) = &pipeline { true } else { false };

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

        let key = MarchingCubesKey::new(
            drawable.volume_texture,
            drawable.texture_lod,
            drawable.entity,
        );
        let buffer_info = marching_cubes_map.get(&key).unwrap();
        let ibo = resources.access_buffer(
            cmd_buffer,
            &buffer_info.buffer_name,
            BarrierSync::INDEX_INPUT,
            BarrierAccess::INDEX_READ,
            HistoryResourceEntry::Current,
        );
        if !is_mesh {
            cmd_buffer.set_index_buffer(BufferRef::Regular(&*ibo), 0u64, IndexFormat::U32);
        } else {
            cmd_buffer.bind_uniform_buffer(BindingFrequency::Frequent, 7, BufferRef::Regular(tris_table), 0, WHOLE_BUFFER);
        }
        cmd_buffer.finish_binding();

        if !is_mesh {
            cmd_buffer.draw_indexed_indirect(
                BufferRef::Regular(&*indirect_buffer),
                buffer_info.indirect_buffer_offset as u64,
                1u32,
                std::mem::size_of::<MarchingCubesIndirectCall>() as u32,
            );
        } else {
            cmd_buffer.draw_mesh_tasks(
                (volume_texture_lod_extents.x + 3) / 4,
                (volume_texture_lod_extents.y + 3) / 4,
                (volume_texture_lod_extents.z + 1) / 2,
            );
        }

        true
    }
}