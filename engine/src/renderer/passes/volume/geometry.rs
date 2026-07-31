use crate::asset::{AssetLoadPriority, AssetType, TextureHandle};
use crate::graphics::*;
use crate::renderer::asset::{
    GraphicsPipelineHandle, GraphicsPipelineInfo, PathPipelineShaderStage, RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::drawable::View;
use crate::renderer::passes::marching_cubes::{MarchingCubesIndirectCall, MarchingCubesPass};
use crate::renderer::passes::volume::ibl::ImageBasedLightingPreparation;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use crate::renderer::renderer_scene::RendererScene;
use sourcerenderer_core::gpu::{StencilOp, TexturePlane};
use sourcerenderer_core::{Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec3UI, Vec4};
use std::default::Default;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone)]
struct PushConstantData {
    model_matrix: Matrix4,
    inv_model_matrix: Matrix4,
    lod_extents: Vec3UI,
    threshold: f32,
    lod: u32,
}

#[repr(C)]
#[derive(Clone)]
struct MaterialData {
    f0: Vec3,
    roughness: f32,
    inv_model_matrix: Matrix4,
    metalness: f32,
    lod: u32,
    threshold: f32,
    width: f32,
    height: f32,
}

pub struct GeometryPass {
    pipeline: GraphicsPipelineHandle,
    pipeline_non_overlapping: GraphicsPipelineHandle,
    pipeline_transparent: GraphicsPipelineHandle,
    pipeline_transparent_prepass: GraphicsPipelineHandle,
    sampler: Arc<crate::graphics::Sampler>,
    transfer_function_handle: TextureHandle,
}

impl GeometryPass {
    pub const COLOR_TEXTURE_NAME: &'static str = "GeometryColor";
    pub const DEPTH_TEXTURE_NAME: &'static str = "Depth";
    pub const SSS_INTENSITY_TEXTURE_NAME: &'static str = "SSSIntensity";

    pub(crate) fn new(
        device: &Arc<crate::graphics::Device>,
        assets: &RendererAssets,
        swapchain: &crate::graphics::Swapchain,
        _init_cmd_buffer: &mut crate::graphics::CommandBuffer,
        resources: &mut RendererResources,
    ) -> Self {
        let sampler = device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::ClampToEdge,
            mip_bias: 0.0f32,
            max_anisotropy: 1f32,
            compare_op: None,
            min_lod: 0.0f32,
            max_lod: None,
        });

        resources.create_texture(
            Self::COLOR_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA16UNorm,
                width: swapchain.width(),
                height: swapchain.height(),
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
                width: swapchain.width(),
                height: swapchain.height(),
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
                width: swapchain.width(),
                height: swapchain.height(),
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET | TextureUsage::STORAGE,
                supports_srgb: false,
            },
            false,
        );

        let shader_file_extension = "json";

        let vs_path = format!("shaders/volume_geometry.vert.{}", shader_file_extension);
        let fs_path = format!("shaders/volume_geometry.frag.{}", shader_file_extension);
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
            func: CompareFunc::NotEqual,
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
            func: CompareFunc::Equal,
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
            func: CompareFunc::Equal,
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

        let (transfer_function_handle, _) = assets.asset_manager().request_asset(
            //"assets/transferfunction_colorful.png",
            "assets/transferfunction.png",
            AssetType::Texture,
            AssetLoadPriority::Normal,
        );

        Self {
            pipeline,
            pipeline_transparent,
            pipeline_non_overlapping,
            sampler: Arc::new(sampler),
            transfer_function_handle: TextureHandle::from(transfer_function_handle),
            pipeline_transparent_prepass,
        }
    }

    #[inline(always)]
    pub(crate) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
            && assets
                .get_graphics_pipeline(self.pipeline_transparent)
                .is_some()
            && assets
                .get_graphics_pipeline(self.pipeline_transparent_prepass)
                .is_some()
    }

    pub(crate) fn transfer_function_handle(&self) -> TextureHandle {
        self.transfer_function_handle
    }

    pub(crate) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        scene: &RendererScene,
        view: &View,
        camera_buffer: &TransientBufferSlice,
        params: &RenderPassParameters,
        assets: &RendererAssetsReadOnly<'_>,
        volume_texture: TextureHandle,
        model_matrix: Matrix4,
        threshold: f32,
        threshold_transparency: f32,
        lod: u32,
    ) {
        let resources = &params.resources;

        if !resources.has_resource(
            ImageBasedLightingPreparation::FILTERED_DIFFUSE_ENVIRONMENT_MAP_TEXTURE_NAME,
        ) || !resources.has_resource(
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
        ) {
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

        let marchingcubes_ibo = resources.access_buffer(
            cmd_buffer,
            MarchingCubesPass::INDICES_BUFFER_NAME,
            BarrierSync::INDEX_INPUT,
            BarrierAccess::INDEX_READ,
            HistoryResourceEntry::Current,
        );

        let marchingcubes_transparent_ibo = resources.access_buffer(
            cmd_buffer,
            MarchingCubesPass::TRANSPARENT_INDICES_BUFFER_NAME,
            BarrierSync::INDEX_INPUT,
            BarrierAccess::INDEX_READ,
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
            ImageBasedLightingPreparation::PREINTEGRATION_MAP_TEXTURE_NAME,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let env_map_diffuse = resources.access_view(
            cmd_buffer,
            ImageBasedLightingPreparation::FILTERED_DIFFUSE_ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let env_specular_info = resources.texture_info(
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
        );
        let env_specular_mips = env_specular_info.mip_levels;
        std::mem::drop(env_specular_info);
        let env_map_specular = resources.access_view(
            cmd_buffer,
            ImageBasedLightingPreparation::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
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

        cmd_buffer.flush_barriers();

        cmd_buffer.begin_label("Geometry");

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
        });

        let pipeline: &Arc<GraphicsPipeline> = assets
            .get_graphics_pipeline(self.pipeline)
            .expect("Pipeline is not compiled yet");
        let pipeline_transparent: &Arc<GraphicsPipeline> = assets
            .get_graphics_pipeline(self.pipeline_transparent)
            .expect("Pipeline is not compiled yet");
        let pipeline_non_overlapping: &Arc<GraphicsPipeline> = assets
            .get_graphics_pipeline(self.pipeline_non_overlapping)
            .expect("Pipeline is not compiled yet");
        let pipeline_transparent_prepass: &Arc<GraphicsPipeline> = assets
            .get_graphics_pipeline(self.pipeline_transparent_prepass)
            .expect("Pipeline is not compiled yet");
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
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

        //let camera_buffer = cmd_buffer.upload_dynamic_data(&[view.proj_matrix * view.view_matrix], BufferUsage::CONSTANT);
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::Frame,
            0,
            BufferRef::Transient(camera_buffer),
            0,
            WHOLE_BUFFER,
        );

        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            2u32,
            &env_map_diffuse,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            3u32,
            &env_map_specular,
            resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            4u32,
            &integration_lut,
            resources.linear_sampler(),
        );

        let volume_texture = assets.get_texture(volume_texture);
        let volume_texture_base = volume_texture.view.texture().unwrap();
        let volume_texture_info = volume_texture_base.info();
        let volume_texture_lod_extents = Vec3UI::new(
            volume_texture_info.width >> lod,
            volume_texture_info.height >> lod,
            volume_texture_info.depth >> lod,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            0u32,
            &volume_texture.view,
            resources.linear_sampler(),
        );

        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::Frame,
            0,
            BufferRef::Transient(camera_buffer),
            0,
            WHOLE_BUFFER,
        );

        let transfer_function = assets.get_texture(self.transfer_function_handle);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            1u32,
            &transfer_function.view,
            resources.linear_sampler(),
        );

        cmd_buffer.set_push_constant_data(
            &[PushConstantData {
                model_matrix,
                inv_model_matrix: Matrix4::inverse(&model_matrix),
                lod_extents: volume_texture_lod_extents,
                threshold,
                lod,
            }],
            ShaderType::VertexShader,
        );
        cmd_buffer.set_push_constant_data(
            &[MaterialData {
                roughness: 0.6f32,
                metalness: 0.3f32,
                //roughness: 0.1f32,
                //metalness: 0.9f32,
                f0: Vec3::new(0.04f32, 0.04f32, 0.04f32),
                inv_model_matrix: Matrix4::inverse(&model_matrix),
                lod,
                width: color_tex_extent.x as f32,
                height: color_tex_extent.y as f32,
                threshold,
            }],
            ShaderType::FragmentShader,
        );
        cmd_buffer.set_index_buffer(
            BufferRef::Regular(&*marchingcubes_ibo),
            0u64,
            IndexFormat::U32,
        );
        cmd_buffer.finish_binding();
        cmd_buffer.draw_indexed_indirect(
            BufferRef::Regular(&*marchingcubes_indirect),
            0u64,
            1u32,
            std::mem::size_of::<MarchingCubesIndirectCall>() as u32,
        );

        // Geometry 2 - Non overlapping

        cmd_buffer.set_pipeline(PipelineBinding::Graphics(pipeline_non_overlapping));
        cmd_buffer.set_push_constant_data(
            &[PushConstantData {
                model_matrix,
                inv_model_matrix: Matrix4::inverse(&model_matrix),
                lod_extents: volume_texture_lod_extents,
                threshold: threshold_transparency,
                lod,
            }],
            ShaderType::VertexShader,
        );
        cmd_buffer.set_push_constant_data(
            &[MaterialData {
                roughness: 0.6f32,
                metalness: 0.3f32,
                //roughness: 0.1f32,
                //metalness: 0.9f32,
                inv_model_matrix: Matrix4::inverse(&model_matrix),
                lod,
                width: color_tex_extent.x as f32,
                height: color_tex_extent.y as f32,
                f0: Vec3::new(0.04f32, 0.04f32, 0.04f32),
                threshold,
            }],
            ShaderType::FragmentShader,
        );
        cmd_buffer.set_index_buffer(
            BufferRef::Regular(&*marchingcubes_transparent_ibo),
            0u64,
            IndexFormat::U32,
        );
        cmd_buffer.finish_binding();
        cmd_buffer.draw_indexed_indirect(
            BufferRef::Regular(&*marchingcubes_indirect),
            std::mem::size_of::<MarchingCubesIndirectCall>() as u64,
            1u32,
            std::mem::size_of::<MarchingCubesIndirectCall>() as u32,
        );

        // Geometry 2 - Depth prepass

        cmd_buffer.set_pipeline(PipelineBinding::Graphics(pipeline_transparent_prepass));
        cmd_buffer.set_push_constant_data(
            &[PushConstantData {
                model_matrix,
                inv_model_matrix: Matrix4::inverse(&model_matrix),
                lod_extents: volume_texture_lod_extents,
                threshold: threshold_transparency,
                lod,
            }],
            ShaderType::VertexShader,
        );
        cmd_buffer.set_index_buffer(
            BufferRef::Regular(&*marchingcubes_transparent_ibo),
            0u64,
            IndexFormat::U32,
        );
        cmd_buffer.finish_binding();
        cmd_buffer.draw_indexed_indirect(
            BufferRef::Regular(&*marchingcubes_indirect),
            std::mem::size_of::<MarchingCubesIndirectCall>() as u64,
            1u32,
            std::mem::size_of::<MarchingCubesIndirectCall>() as u32,
        );

        // Geometry 2 - Transparent

        cmd_buffer.set_pipeline(PipelineBinding::Graphics(pipeline_transparent));
        cmd_buffer.set_push_constant_data(
            &[PushConstantData {
                model_matrix,
                inv_model_matrix: Matrix4::inverse(&model_matrix),
                threshold: threshold_transparency,
                lod_extents: volume_texture_lod_extents,
                lod,
            }],
            ShaderType::VertexShader,
        );
        cmd_buffer.set_push_constant_data(
            &[MaterialData {
                roughness: 0.6f32,
                metalness: 0.3f32,
                //roughness: 0.1f32,
                //metalness: 0.9f32,
                f0: Vec3::new(0.04f32, 0.04f32, 0.04f32),
                inv_model_matrix: Matrix4::inverse(&model_matrix),
                lod,
                width: color_tex_extent.x as f32,
                height: color_tex_extent.y as f32,
                threshold: threshold_transparency,
            }],
            ShaderType::FragmentShader,
        );
        cmd_buffer.set_index_buffer(
            BufferRef::Regular(&*marchingcubes_transparent_ibo),
            0u64,
            IndexFormat::U32,
        );
        cmd_buffer.finish_binding();
        cmd_buffer.draw_indexed_indirect(
            BufferRef::Regular(&*marchingcubes_indirect),
            std::mem::size_of::<MarchingCubesIndirectCall>() as u64,
            1u32,
            std::mem::size_of::<MarchingCubesIndirectCall>() as u32,
        );

        cmd_buffer.end_render_pass();

        cmd_buffer.end_label();
    }
}
