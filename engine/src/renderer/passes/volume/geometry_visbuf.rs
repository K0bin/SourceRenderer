use std::sync::Arc;

use crate::asset::{AssetLoadPriority, AssetType, TextureHandle};
use crate::graphics::*;
use crate::renderer::asset::{
    GraphicsPipelineHandle, GraphicsPipelineInfo, RendererAssets, RendererAssetsReadOnly,
    RendererMaterial, RendererMaterialValue,
};
use crate::renderer::drawable::View;
use crate::renderer::passes::marching_cubes::{MarchingCubesIndirectCall, MarchingCubesPass};
use crate::renderer::passes::volume::ibl::ImageBasedLightingPreparation;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use crate::renderer::renderer_scene::RendererScene;
use smallvec::SmallVec;
use sourcerenderer_core::{HalfVec3, Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec4};

#[repr(C)]
#[derive(Clone)]
struct PushConstantData {
    model_matrix: Matrix4,
    threshold: f32,
    lod: u32,
}

pub struct GeometryVisibilityBufferPass {
    pipeline: GraphicsPipelineHandle,
    sampler: Arc<crate::graphics::Sampler>,
    depth_texture_name: &'static str,
}

impl GeometryVisibilityBufferPass {
    pub const VISIBILITY_BUFFER_NAME: &'static str = "VisBuf";

    pub(crate) fn new(
        device: &Arc<crate::graphics::Device>,
        assets: &RendererAssets,
        swapchain: &crate::graphics::Swapchain,
        _init_cmd_buffer: &mut crate::graphics::CommandBuffer,
        resources: &mut RendererResources,
        depth_texture_name: &'static str,
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
            Self::VISIBILITY_BUFFER_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::R32UInt,
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

        /*resources.create_texture(
            Self::DEPTH_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::D16,
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
        );*/

        let shader_file_extension = "json";

        let fs_name = format!(
            "shaders/volume_geometry_visbuf.frag.{}",
            shader_file_extension
        );
        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: &format!(
                "shaders/volume_geometry_visbuf.vert.{}",
                shader_file_extension
            ),
            fs: Some(&fs_name),
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
                attachments: &[AttachmentBlendInfo {
                    blend_enabled: false,
                    src_color_blend_factor: BlendFactor::SrcAlpha,
                    dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                    color_blend_op: BlendOp::Add,
                    src_alpha_blend_factor: BlendFactor::Zero,
                    dst_alpha_blend_factor: BlendFactor::One,
                    alpha_blend_op: BlendOp::Add,
                    write_mask: ColorComponents::all(),
                }],
            },
            render_target_formats: &[Format::R32UInt],
            depth_stencil_format: Format::D32S8,
        };
        let pipeline = assets.request_graphics_pipeline(&pipeline_info);

        Self {
            pipeline,
            sampler: Arc::new(sampler),
            depth_texture_name,
        }
    }

    #[inline(always)]
    pub(crate) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
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
        lod: u32,
    ) {
        let resources = &params.resources;

        let vis_buf_texture_info = resources.texture_info(Self::VISIBILITY_BUFFER_NAME);
        let vis_buf_tex_extent =
            Vec2UI::new(vis_buf_texture_info.width, vis_buf_texture_info.height);
        std::mem::drop(vis_buf_texture_info);

        let rt = resources.access_view(
            cmd_buffer,
            Self::VISIBILITY_BUFFER_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let dsv = resources.access_view(
            cmd_buffer,
            self.depth_texture_name,
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
        let marchingcubes_indirect = resources.access_buffer(
            cmd_buffer,
            MarchingCubesPass::ATOMICS_BUFFER_NAME,
            BarrierSync::INDIRECT,
            BarrierAccess::INDIRECT_READ,
            HistoryResourceEntry::Current,
        );

        cmd_buffer.flush_barriers();

        cmd_buffer.begin_label("Geometry (Visiblity buffer)");

        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[RenderTarget {
                view: &rt,
                load_op: LoadOpColor::Clear(ClearColor::from_u32([!0u32, !0u32, !0u32, !0u32])),
                store_op: StoreOp::Store,
            }],
            depth_stencil: Some(&DepthStencilAttachment {
                view: &dsv,
                load_op: LoadOpDepthStencil::Clear(ClearDepthStencilValue::DEPTH_ONE),
                store_op: StoreOp::Store,
            }),
            query_range: None,
        });

        let pipeline: &Arc<GraphicsPipeline> = assets
            .get_graphics_pipeline(self.pipeline)
            .expect("Pipeline is not compiled yet");
        cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
        cmd_buffer.set_viewports(&[Viewport {
            position: Vec2::new(0.0f32, 0.0f32),
            extent: Vec2::new(vis_buf_tex_extent.x as f32, vis_buf_tex_extent.y as f32),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);
        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0, 0),
            extent: vis_buf_tex_extent,
        }]);

        //let camera_buffer = cmd_buffer.upload_dynamic_data(&[view.proj_matrix * view.view_matrix], BufferUsage::CONSTANT);
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::Frame,
            0,
            BufferRef::Transient(camera_buffer),
            0,
            WHOLE_BUFFER,
        );

        let volume_texture = assets.get_texture(volume_texture);
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

        cmd_buffer.set_push_constant_data(
            &[PushConstantData {
                model_matrix,
                threshold,
                lod,
            }],
            ShaderType::VertexShader,
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

        cmd_buffer.end_render_pass();

        cmd_buffer.end_label();
    }
}
