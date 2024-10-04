use std::cell::Ref;
use std::sync::Arc;

use nalgebra::Vector2;
use smallvec::SmallVec;
use sourcerenderer_core::{
    Matrix4,
    Platform,
    Vec2,
    Vec2I,
    Vec2UI,
};

use super::draw_prep::DrawPrepPass;
use super::gpu_scene::DRAW_CAPACITY;
use crate::renderer::drawable::View;
use crate::renderer::light::DirectionalLight;
use crate::renderer::passes::light_binning;
use crate::renderer::passes::rt_shadows::RTShadowPass;
use crate::renderer::passes::ssao::SsaoPass;
use crate::renderer::passes::taa::scaled_halton_point;
use crate::renderer::renderer_assets::*;
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
use crate::renderer::PointLight;

use crate::graphics::*;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FrameData {
    swapchain_transform: Matrix4,
    jitter: Vec2,
    z_near: f32,
    z_far: f32,
    rt_size: Vector2<u32>,
    cluster_z_bias: f32,
    cluster_z_scale: f32,
    cluster_count: nalgebra::Vector3<u32>,
    point_light_count: u32,
    directional_light_count: u32,
}

pub struct GeometryPass<P: Platform> {
    sampler: Arc<Sampler<P::GPUBackend>>,
    pipeline: GraphicsPipelineHandle,
}

impl<P: Platform> GeometryPass<P> {
    pub const GEOMETRY_PASS_TEXTURE_NAME: &'static str = "geometry";
    pub const MOTION_TEXTURE_NAME: &'static str = "Motion";
    pub const NORMALS_TEXTURE_NAME: &'static str = "Normals";

    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
        swapchain: &Arc<Swapchain<P::GPUBackend>>,
        resources: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let texture_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::RGBA8UNorm,
            width: swapchain.width(),
            height: swapchain.height(),
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::SAMPLED
                | TextureUsage::RENDER_TARGET
                | TextureUsage::COPY_SRC
                | TextureUsage::STORAGE,
            supports_srgb: false,
        };
        resources.create_texture(Self::GEOMETRY_PASS_TEXTURE_NAME, &texture_info, false);

        resources.create_texture(
            Self::MOTION_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RG32Float,
                width: swapchain.width(),
                height: swapchain.height(),
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            true,
        );

        resources.create_texture(
            Self::NORMALS_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA32Float,
                width: swapchain.width(),
                height: swapchain.height(),
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            false,
        );

        let sampler = Arc::new(device.create_sampler(&SamplerInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mip_filter: Filter::Linear,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mip_bias: 0.0,
            max_anisotropy: 1f32,
            compare_op: None,
            min_lod: 0.0,
            max_lod: None,
        }));

        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: "shaders/geometry_bindless.vert.json",
            fs: Some("shaders/geometry_bindless.frag.json"),
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
                depth_func: CompareFunc::LessEqual,
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
                        format: texture_info.format,
                        samples: texture_info.samples,
                    },
                    AttachmentInfo {
                        format: Format::D24S8,
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
                        read_only: true,
                    }),
                }],
            },
            0,
        );

        Self { sampler, pipeline }
    }

    #[profiling::function]
    pub(super) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        barriers: &RendererResources<P::GPUBackend>,
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
        depth_name: &str,
        scene: &RendererScene<P::GPUBackend>,
        view: &View,
        gpu_scene: &TransientBufferSlice<P::GPUBackend>,
        zero_texture_view: &Arc<TextureView<P::GPUBackend>>,
        _zero_texture_view_black: &Arc<TextureView<P::GPUBackend>>,
        lightmap: &Arc<RendererTexture<P::GPUBackend>>,
        swapchain_transform: Matrix4,
        frame: u64,
        camera_buffer: &Arc<BufferSlice<P::GPUBackend>>,
        vertex_buffer: &Arc<BufferSlice<P::GPUBackend>>,
        index_buffer: &Arc<BufferSlice<P::GPUBackend>>,
        shader_manager: &ShaderManager<P>,
    ) {
        cmd_buffer.begin_label("Geometry pass");
        let draw_buffer = barriers.access_buffer(
            cmd_buffer,
            DrawPrepPass::INDIRECT_DRAW_BUFFER,
            BarrierSync::INDIRECT,
            BarrierAccess::INDIRECT_READ,
            HistoryResourceEntry::Current,
        );

        let rtv_ref = barriers.access_view(
            cmd_buffer,
            Self::GEOMETRY_PASS_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let rtv = &*rtv_ref;

        let motion = barriers.access_view(
            cmd_buffer,
            Self::MOTION_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let normals = barriers.access_view(
            cmd_buffer,
            Self::NORMALS_TEXTURE_NAME,
            BarrierSync::RENDER_TARGET,
            BarrierAccess::RENDER_TARGET_WRITE,
            TextureLayout::RenderTarget,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let prepass_depth_ref = barriers.access_view(
            cmd_buffer,
            depth_name,
            BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
            BarrierAccess::DEPTH_STENCIL_READ,
            TextureLayout::DepthStencilRead,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let prepass_depth = &*prepass_depth_ref;

        let ssao_ref = barriers.access_view(
            cmd_buffer,
            SsaoPass::<P>::SSAO_TEXTURE_NAME,
            BarrierSync::FRAGMENT_SHADER | BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let ssao = &*ssao_ref;

        let light_bitmask_buffer_ref = barriers.access_buffer(
            cmd_buffer,
            light_binning::LightBinningPass::LIGHT_BINNING_BUFFER_NAME,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );
        let light_bitmask_buffer = &*light_bitmask_buffer_ref;

        let rt_shadows: Ref<Arc<TextureView<P::GPUBackend>>>;
        let shadows = if device.supports_ray_tracing() {
            rt_shadows = barriers.access_view(
                cmd_buffer,
                RTShadowPass::SHADOWS_TEXTURE_NAME,
                BarrierSync::FRAGMENT_SHADER,
                BarrierAccess::SAMPLING_READ,
                TextureLayout::Sampled,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            );
            &*rt_shadows
        } else {
            zero_texture_view
        };

        cmd_buffer.begin_render_pass(
            &RenderPassBeginInfo {
                attachments: &[
                    RenderPassAttachment {
                        view: RenderPassAttachmentView::RenderTarget(&rtv),
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::Store,
                    },
                    RenderPassAttachment {
                        view: RenderPassAttachmentView::RenderTarget(&*motion),
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::Store,
                    },
                    RenderPassAttachment {
                        view: RenderPassAttachmentView::RenderTarget(&*normals),
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::Store,
                    },
                    RenderPassAttachment {
                        view: RenderPassAttachmentView::DepthStencil(&prepass_depth),
                        load_op: LoadOp::Load,
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
                        read_only: true,
                    }),
                }],
            },
            RenderpassRecordingMode::Commands,
        );

        let rtv_info = rtv.texture().unwrap().info();
        let cluster_count = nalgebra::Vector3::<u32>::new(16, 9, 24);
        let near = view.near_plane;
        let far = view.far_plane;
        let cluster_z_scale = (cluster_count.z as f32) / (far / near).log2();
        let cluster_z_bias = -(cluster_count.z as f32) * (near).log2() / (far / near).log2();
        let per_frame = FrameData {
            swapchain_transform,
            jitter: scaled_halton_point(rtv_info.width, rtv_info.height, (frame % 8) as u32 + 1),
            z_near: near,
            z_far: far,
            rt_size: Vector2::<u32>::new(rtv_info.width, rtv_info.height),
            cluster_z_bias,
            cluster_z_scale,
            cluster_count,
            point_light_count: scene.point_lights().len() as u32,
            directional_light_count: scene.directional_lights().len() as u32,
        };
        let mut point_lights = SmallVec::<[PointLight; 16]>::new();
        for point_light in scene.point_lights() {
            point_lights.push(PointLight {
                position: point_light.position,
                intensity: point_light.intensity,
            });
        }
        let mut directional_lights = SmallVec::<[DirectionalLight; 16]>::new();
        for directional_light in scene.directional_lights() {
            directional_lights.push(DirectionalLight {
                direction: directional_light.direction,
                intensity: directional_light.intensity,
            });
        }
        let per_frame_buffer = cmd_buffer.upload_dynamic_data(&[per_frame], BufferUsage::CONSTANT).unwrap();
        let point_light_buffer =
            cmd_buffer.upload_dynamic_data(&point_lights[..], BufferUsage::STORAGE).unwrap();
        let directional_light_buffer =
            cmd_buffer.upload_dynamic_data(&directional_lights[..], BufferUsage::STORAGE).unwrap();

        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::Frequent,
            3,
            BufferRef::Transient(&per_frame_buffer),
            0,
            WHOLE_BUFFER,
        );

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
            extent: Vec2UI::new(rtv_info.width, rtv_info.height),
        }]);

        //command_buffer.bind_storage_buffer(BindingFrequency::Frequent, 7, clusters);
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::Frequent,
            0,
            BufferRef::Regular(camera_buffer),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::Frequent,
            1,
            BufferRef::Transient(&point_light_buffer),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::Frequent,
            2,
            BufferRef::Regular(light_bitmask_buffer),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            4,
            &ssao,
            &self.sampler,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::Frequent,
            5,
            BufferRef::Transient(&directional_light_buffer),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            6,
            &lightmap.view,
            &self.sampler,
        );
        cmd_buffer.bind_sampler(BindingFrequency::Frequent, 7, &self.sampler);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::Frequent,
            8,
            &shadows,
            &self.sampler,
        );
        cmd_buffer.bind_storage_buffer(BindingFrequency::Frequent, 9, BufferRef::Transient(gpu_scene), 0, WHOLE_BUFFER);

        cmd_buffer.set_vertex_buffer(BufferRef::Regular(vertex_buffer), 0);
        cmd_buffer.set_index_buffer(BufferRef::Regular(index_buffer), 0, IndexFormat::U32);

        cmd_buffer.finish_binding();
        cmd_buffer.draw_indexed_indirect(BufferRef::Regular(&draw_buffer), 4, BufferRef::Regular(&draw_buffer), 0, DRAW_CAPACITY, 20);

        cmd_buffer.end_render_pass();
        cmd_buffer.end_label();
    }
}
