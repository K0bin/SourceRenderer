use std::cell::Ref;
use std::sync::Arc;

use bevy_tasks::ParallelSlice;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::Submission;
use sourcerenderer_core::{
    Matrix4, Platform, Vec2, Vec2I, Vec2UI, Vec3UI, Vec4
};

use super::desktop_renderer::FrameBindings;
use crate::renderer::passes::clustering::ClusteringPass;
use crate::renderer::passes::conservative::desktop_renderer::setup_frame;
use crate::renderer::passes::light_binning;
use crate::renderer::passes::rt_shadows::RTShadowPass;
use crate::renderer::passes::ssao::SsaoPass;
use crate::renderer::render_path::{
    RenderPassParameters,
};
use crate::renderer::asset::*;
use crate::asset::*;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    GraphicsPipelineHandle,
    GraphicsPipelineInfo,
    ShaderManager,
};

use crate::graphics::*;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FrameData {
    swapchain_transform: Matrix4,
    halton_point: Vec2,
    z_near: f32,
    z_far: f32,
    rt_size: Vec2UI,
    cluster_z_bias: f32,
    cluster_z_scale: f32,
    cluster_count: Vec3UI,
    point_light_count: u32,
    directional_light_count: u32,
}

pub struct GeometryPass<P: Platform> {
    sampler: Sampler<P::GPUBackend>,
    pipeline: GraphicsPipelineHandle,
}

impl<P: Platform> GeometryPass<P> {
    pub const GEOMETRY_PASS_TEXTURE_NAME: &'static str = "geometry";

    const DRAWABLE_LABELS: bool = false;

    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
        resolution: Vec2UI,
        barriers: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let texture_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::RGBA8UNorm,
            width: resolution.x,
            height: resolution.y,
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
        barriers.create_texture(Self::GEOMETRY_PASS_TEXTURE_NAME, &texture_info, false);

        let sampler = device.create_sampler(&SamplerInfo {
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
        });

        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: "shaders/textured.vert.json",
            fs: Some("shaders/textured.frag.json"),
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
                depth_write_enabled: false,
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
            render_target_formats: &[texture_info.format],
            depth_stencil_format: Format::D24S8
        };
        let pipeline = shader_manager.request_graphics_pipeline(&pipeline_info);

        Self { sampler, pipeline }
    }

    #[profiling::function]
    pub(super) fn execute(
        &mut self,
        context: &mut GraphicsContext<P::GPUBackend>,
        cmd_buffer: &mut crate::graphics::CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>,
        depth_name: &str,
        bindings: &FrameBindings<P::GPUBackend>,
    ) {
        cmd_buffer.begin_label("Geometry pass");
        let static_drawables = pass_params.scene.scene.static_drawables();

        let (width, height) = {
            let info = pass_params.resources.texture_info(Self::GEOMETRY_PASS_TEXTURE_NAME);
            (info.width, info.height)
        };

        let rtv_ref = pass_params.resources.access_view(
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

        let prepass_depth_ref = pass_params.resources.access_view(
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

        let ssao_ref = pass_params.resources.access_view(
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

        let light_bitmask_buffer_ref = pass_params.resources.access_buffer(
            cmd_buffer,
            light_binning::LightBinningPass::LIGHT_BINNING_BUFFER_NAME,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );
        let light_bitmask_buffer = &*light_bitmask_buffer_ref;

        let rt_shadows: Ref<Arc<TextureView<P::GPUBackend>>>;
        let shadows = if pass_params.device.supports_ray_tracing() {
            rt_shadows = pass_params.resources.access_view(
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
            pass_params.zero_textures.zero_texture_view
        };

        let clusters = pass_params.resources.access_buffer(
          cmd_buffer,
          ClusteringPass::CLUSTERS_BUFFER_NAME,
          BarrierSync::FRAGMENT_SHADER,
          BarrierAccess::STORAGE_READ,
          HistoryResourceEntry::Current
        ).clone();

        cmd_buffer.begin_render_pass(
            &RenderPassBeginInfo {
                render_targets: &[
                    RenderTarget {
                        view: &rtv,
                        load_op: LoadOpColor::Clear(ClearColor::BLACK),
                        store_op: StoreOp::<P::GPUBackend>::Store
                    }
                ],
                depth_stencil: Some(&DepthStencilAttachment {
                    view: prepass_depth,
                    load_op: LoadOpDepthStencil::Load,
                    store_op: StoreOp::<P::GPUBackend>::Store
                })
            },
            RenderpassRecordingMode::CommandBuffers,
        );

        let assets = pass_params.assets;
        let zero_textures = pass_params.zero_textures;
        let lightmap = pass_params.scene.lightmap;

        let inheritance = cmd_buffer.inheritance();
        const CHUNK_SIZE: usize = 128;
        let view = &pass_params.scene.scene.views()[pass_params.scene.active_view_index];
        let chunk_size = (view.drawable_parts.len() / 15).max(CHUNK_SIZE);
        let pipeline = pass_params.shader_manager.get_graphics_pipeline(self.pipeline);
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        let inner_cmd_buffers: Vec<FinishedCommandBuffer<P::GPUBackend>> = view.drawable_parts.par_chunk_map(task_pool, chunk_size, |_index, chunk| {
                P::thread_memory_management_pool(|| {
                    let mut command_buffer = context.get_inner_command_buffer(inheritance);

                    command_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
                    command_buffer.set_viewports(&[Viewport {
                        position: Vec2::new(0.0f32, 0.0f32),
                        extent: Vec2::new(width as f32, height as f32),
                        min_depth: 0.0f32,
                        max_depth: 1.0f32,
                    }]);
                    command_buffer.set_scissors(&[Scissor {
                        position: Vec2I::new(0, 0),
                        extent: Vec2UI::new(width, height),
                    }]);

                    command_buffer.bind_sampling_view_and_sampler(
                        BindingFrequency::Frequent,
                        0,
                        if let Some(lightmap) = lightmap { &lightmap.view } else { zero_textures.zero_texture_view },
                        &self.sampler,
                    );
                    command_buffer.bind_sampler(BindingFrequency::Frequent, 1, &self.sampler);
                    command_buffer.bind_sampling_view_and_sampler(
                        BindingFrequency::Frequent,
                        2,
                        &shadows,
                        &self.sampler,
                    );
                    command_buffer.bind_storage_buffer(
                        BindingFrequency::Frequent,
                        3,
                        BufferRef::Regular(&light_bitmask_buffer),
                        0,
                        WHOLE_BUFFER,
                    );
                    command_buffer.bind_sampling_view_and_sampler(
                        BindingFrequency::Frequent,
                        4,
                        &ssao,
                        &self.sampler,
                    );
                    command_buffer.bind_storage_buffer(BindingFrequency::Frequent, 5, BufferRef::Regular(&clusters), 0, WHOLE_BUFFER);

                    let mut last_material = Option::<&RendererMaterial>::None;

                    for part in chunk.iter() {
                        let drawable = &static_drawables[part.drawable_index];
                        if Self::DRAWABLE_LABELS {
                            command_buffer.begin_label(&format!("Drawable {}", part.drawable_index));
                        }

                        setup_frame::<P::GPUBackend>(&mut command_buffer, bindings);

                        command_buffer.set_push_constant_data(
                            &[drawable.transform],
                            ShaderType::VertexShader,
                        );

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
                        let materials: SmallVec<[&RendererMaterial; 8]> = model
                            .material_handles()
                            .iter()
                            .map(|handle| assets.get_material(*handle))
                            .collect();

                        command_buffer
                            .set_vertex_buffer(0, BufferRef::Regular(mesh.vertices.buffer()), mesh.vertices.offset() as u64);
                        if let Some(indices) = mesh.indices.as_ref() {
                            command_buffer.set_index_buffer(
                                BufferRef::Regular(indices.buffer()),
                                indices.offset() as u64,
                                IndexFormat::U32,
                            );
                        }

                        let range = &mesh.parts[part.part_index];
                        let material = &materials[part.part_index];

                        if last_material.as_ref() != Some(material) {
                            #[repr(C)]
                            #[derive(Clone, Copy)]
                            struct MaterialInfo {
                                albedo: Vec4,
                                roughness_factor: f32,
                                metalness_factor: f32,
                                albedo_texture_index: u32,
                            }
                            let mut material_info = MaterialInfo {
                                albedo: Vec4::new(1f32, 1f32, 1f32, 1f32),
                                roughness_factor: 0f32,
                                metalness_factor: 0f32,
                                albedo_texture_index: 0u32,
                            };

                            command_buffer.bind_sampling_view_and_sampler(
                                BindingFrequency::VeryFrequent,
                                0,
                                zero_textures.zero_texture_view,
                                &self.sampler,
                            );
                            command_buffer.bind_sampling_view_and_sampler(
                                BindingFrequency::VeryFrequent,
                                1,
                                zero_textures.zero_texture_view,
                                &self.sampler,
                            );
                            command_buffer.bind_sampling_view_and_sampler(
                                BindingFrequency::VeryFrequent,
                                2,
                                zero_textures.zero_texture_view,
                                &self.sampler,
                            );

                            let albedo_value = material.get("albedo").unwrap();
                            match albedo_value {
                                RendererMaterialValue::Texture(handle) => {
                                    let albedo_view = &assets.get_texture(*handle).view;
                                    command_buffer.bind_sampling_view_and_sampler(
                                        BindingFrequency::VeryFrequent,
                                        0,
                                        albedo_view,
                                        &self.sampler,
                                    );
                                    material_info.albedo_texture_index = 0;
                                }
                                RendererMaterialValue::Vec4(val) => material_info.albedo = *val,
                                RendererMaterialValue::Float(_) => unimplemented!(),
                            }
                            let roughness_value = material.get("roughness");
                            match roughness_value {
                                Some(RendererMaterialValue::Texture(handle)) => {
                                    let roughness_view = &assets.get_texture(*handle).view;
                                    command_buffer.bind_sampling_view_and_sampler(
                                        BindingFrequency::VeryFrequent,
                                        1,
                                        roughness_view,
                                        &self.sampler,
                                    );
                                }
                                Some(RendererMaterialValue::Vec4(_)) => unimplemented!(),
                                Some(RendererMaterialValue::Float(val)) => {
                                    material_info.roughness_factor = *val;
                                }
                                None => {}
                            }
                            let metalness_value = material.get("metalness");
                            match metalness_value {
                                Some(RendererMaterialValue::Texture(handle)) => {
                                    let metalness_view = &assets.get_texture(*handle).view;
                                    command_buffer.bind_sampling_view_and_sampler(
                                        BindingFrequency::VeryFrequent,
                                        2,
                                        metalness_view,
                                        &self.sampler,
                                    );
                                }
                                Some(RendererMaterialValue::Vec4(_)) => unimplemented!(),
                                Some(RendererMaterialValue::Float(val)) => {
                                    material_info.metalness_factor = *val;
                                }
                                None => {}
                            }
                            let material_info_buffer = command_buffer
                                .upload_dynamic_data(&[material_info], BufferUsage::CONSTANT).unwrap();
                            command_buffer.bind_uniform_buffer(
                                BindingFrequency::VeryFrequent,
                                3,
                                BufferRef::Transient(&material_info_buffer),
                                0,
                                WHOLE_BUFFER,
                            );
                            last_material = Some(material.clone());
                        }

                        command_buffer.finish_binding();

                        if mesh.indices.is_some() {
                            command_buffer.draw_indexed(1, 0, range.count, range.start, 0);
                        } else {
                            command_buffer.draw(range.count, range.start);
                        }
                        if Self::DRAWABLE_LABELS {
                            command_buffer.end_label();
                        }
                    }
                    command_buffer.finish()
                })
            });

        cmd_buffer.execute_inner(inner_cmd_buffers);
        cmd_buffer.end_render_pass();
        cmd_buffer.end_label();
    }
}
