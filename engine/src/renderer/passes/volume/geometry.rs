use std::sync::Arc;

use crate::asset::{AssetLoadPriority, AssetType, TextureHandle};
use crate::graphics::*;
use crate::renderer::asset::{
    GraphicsPipelineHandle, GraphicsPipelineInfo, RendererAssets, RendererAssetsReadOnly,
    RendererMaterial, RendererMaterialValue,
};
use crate::renderer::drawable::View;
use crate::renderer::passes::marching_cubes::{MarchingCubesIndirectCall, MarchingCubesPass};
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use crate::renderer::renderer_scene::RendererScene;
use smallvec::SmallVec;
use sourcerenderer_core::{HalfVec3, Matrix4, Vec2, Vec2I, Vec2UI, Vec3, Vec4};

#[repr(C)]
#[derive(Clone)]
struct PushConstantData {
    model_matrix: Matrix4,
    size: Vec3,
}

pub struct GeometryPass {
    pipeline: GraphicsPipelineHandle,
    sampler: Arc<crate::graphics::Sampler>,
    transfer_function_handle: TextureHandle,
}

impl GeometryPass {
    pub const DEPTH_TEXTURE_NAME: &'static str = "Depth";

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

        let shader_file_extension = "json";

        let fs_name = format!("shaders/volume_geometry.web.frag.{}", shader_file_extension);
        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: &format!("shaders/volume_geometry.web.vert.{}", shader_file_extension),
            fs: Some(&fs_name),
            primitive_type: PrimitiveType::Triangles,
            vertex_layout: VertexLayoutInfo {
                input_assembler: &[InputAssemblerElement {
                    binding: 0,
                    stride: std::mem::size_of::<HalfVec3>(),
                    input_rate: InputRate::PerVertex,
                }],
                shader_inputs: &[ShaderInputElement {
                    input_assembler_binding: 0,
                    location_vk_mtl: 0,
                    semantic_name_d3d: String::from(""),
                    semantic_index_d3d: 0,
                    offset: 0,
                    format: Format::RGB16Float,
                }],
            },
            rasterizer: RasterizerInfo {
                fill_mode: FillMode::Fill,
                cull_mode: CullMode::None,
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
            render_target_formats: &[swapchain.format()],
            depth_stencil_format: Format::D32,
        };
        let pipeline = assets.request_graphics_pipeline(&pipeline_info);

        let (transfer_function_handle, _) = assets.asset_manager().request_asset(
            "assets/transferfunction.png",
            AssetType::Texture,
            AssetLoadPriority::Normal,
        );

        Self {
            pipeline,
            sampler: Arc::new(sampler),
            transfer_function_handle: TextureHandle::from(transfer_function_handle),
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
        resources: &RendererResources,
        backbuffer: &Arc<TextureView>,
        backbuffer_handle: &BackendTexture,
        width: u32,
        height: u32,
        assets: &RendererAssetsReadOnly<'_>,
        volume_texture: TextureHandle,
        spacing: Vec3,
    ) {
        cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::RENDER_TARGET,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::RenderTarget,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None,
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

        let marchingcubes_vbo = resources.access_buffer(
            cmd_buffer,
            MarchingCubesPass::VERTICES_BUFFER_NAME,
            BarrierSync::VERTEX_INPUT,
            BarrierAccess::VERTEX_INPUT_READ,
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
        cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
            render_targets: &[RenderTarget {
                view: &backbuffer,
                load_op: LoadOpColor::Clear(ClearColor::from_u32([0, 0, 0, 255])),
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
            extent: Vec2::new(width as f32, height as f32),
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }]);
        cmd_buffer.set_scissors(&[Scissor {
            position: Vec2I::new(0, 0),
            extent: Vec2UI::new(width, height),
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
        let texture_info = volume_texture.view.texture().unwrap().info();

        let transfer_function = assets.get_texture(self.transfer_function_handle);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            1u32,
            &transfer_function.view,
            resources.linear_sampler(),
        );

        cmd_buffer.set_push_constant_data(
            &[PushConstantData {
                model_matrix: Matrix4::from_rotation_z(-1.57f32)
                    * Matrix4::from_rotation_y(-1.57f32),
                size: Vec3::new(
                    texture_info.width as f32,
                    texture_info.height as f32,
                    texture_info.depth as f32,
                ) * spacing,
            }],
            ShaderType::VertexShader,
        );
        cmd_buffer.set_vertex_buffer(0u32, BufferRef::Regular(&*marchingcubes_vbo), 0u64);
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

        cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::RENDER_TARGET,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::RENDER_TARGET_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::RenderTarget,
            new_layout: TextureLayout::Present,
            texture: backbuffer_handle,
            queue_ownership: None,
            range: BarrierTextureRange::default(),
        }]);
    }
}
