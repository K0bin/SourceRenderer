use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::{
    Matrix4, Platform, Vec2, Vec2I, Vec2UI
};

use crate::asset::AssetManager;
use crate::renderer::asset::{RendererAssetsReadOnly, RendererMaterial, RendererMaterialValue};
use crate::renderer::drawable::View;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::renderer_scene::RendererScene;
use crate::renderer::asset::{GraphicsPipelineHandle, GraphicsPipelineInfo};

use crate::graphics::*;

pub struct GeometryPass<P: Platform> {
    pipeline: GraphicsPipelineHandle,
    sampler: Arc<crate::graphics::Sampler<P::GPUBackend>>,
}

impl<P: Platform> GeometryPass<P> {
    pub const DEPTH_TEXTURE_NAME: &'static str = "Depth";

    pub(super) fn new(
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
        asset_manager: &Arc<AssetManager<P>>,
        swapchain: &crate::graphics::Swapchain<P::GPUBackend>,
        _init_cmd_buffer: &mut crate::graphics::CommandBufferRecorder<P::GPUBackend>,
        resources: &mut RendererResources<P::GPUBackend>,
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

        let fs_name = format!("shaders/web_geometry.web.frag.{}", shader_file_extension);
        let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
            vs: &format!("shaders/web_geometry.web.vert.{}", shader_file_extension),
            fs: Some(&fs_name),
            primitive_type: PrimitiveType::Triangles,
            vertex_layout: VertexLayoutInfo {
                input_assembler: &[InputAssemblerElement {
                    binding: 0,
                    stride: std::mem::size_of::<crate::asset::Vertex>(),
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
                        offset: 12,
                        format: Format::RG32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 2,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 20,
                        format: Format::RGB32Float,
                    },
                    ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 3,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 32,
                        format: Format::R32UInt,
                    },
                ],
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
            depth_stencil_format: Format::D32
        };
        let pipeline = asset_manager.request_graphics_pipeline(&pipeline_info);

        Self { pipeline, sampler: Arc::new(sampler) }
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_, P>) -> bool {
        assets.get_graphics_pipeline(self.pipeline).is_some()
    }

    pub(super) fn execute(
        &mut self,
        cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        scene: &RendererScene<P::GPUBackend>,
        view: &View,
        camera_buffer: &TransientBufferSlice<P::GPUBackend>,
        resources: &RendererResources<P::GPUBackend>,
        backbuffer: &Arc<TextureView<P::GPUBackend>>,
        backbuffer_handle: &<P::GPUBackend as GPUBackend>::Texture,
        width: u32,
        height: u32,
        assets: &RendererAssetsReadOnly<'_, P>
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
            queue_ownership: None
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

        cmd_buffer.flush_barriers();
        cmd_buffer.begin_render_pass(
            &RenderPassBeginInfo {
                render_targets: &[RenderTarget {
                    view: &backbuffer,
                    load_op: LoadOpColor::Clear(ClearColor::from_u32([0, 0, 0, 255])),
                    store_op: StoreOp::<P::GPUBackend>::Store,
                }],
                depth_stencil: Some(&DepthStencilAttachment {
                    view: &dsv,
                    load_op: LoadOpDepthStencil::Clear(ClearDepthStencilValue::DEPTH_ONE),
                    store_op: StoreOp::<P::GPUBackend>::Store,
                })
            },
            RenderpassRecordingMode::Commands,
        );

        let pipeline: &Arc<GraphicsPipeline<<P as Platform>::GPUBackend>> = assets.get_graphics_pipeline(self.pipeline).expect("Pipeline is not compiled yet");
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
        cmd_buffer.bind_uniform_buffer(BindingFrequency::Frame, 0, BufferRef::Transient(camera_buffer), 0, WHOLE_BUFFER);

        let drawables = scene.static_drawables();
        let parts = &view.drawable_parts;
        for part in parts {
            let drawable = &drawables[part.drawable_index];
            cmd_buffer.set_push_constant_data(&[Matrix4::from(drawable.transform)], ShaderType::VertexShader);
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
            let materials: SmallVec<[&RendererMaterial; 4]> = model
                .material_handles()
                .iter()
                .map(|handle| assets.get_material(*handle))
                .collect();
            let range = &mesh.parts[part.part_index];
            let material = &materials[part.part_index];
            let albedo_value = material.get("albedo").unwrap();
            match albedo_value {
                RendererMaterialValue::Texture(handle) => {
                    let texture = assets.get_texture(*handle);
                    let albedo_view = &texture.view;
                    cmd_buffer.bind_sampling_view_and_sampler(
                        BindingFrequency::Frequent,
                        0,
                        albedo_view,
                        &self.sampler,
                    );
                }
                _ => {
                    let texture = assets.get_placeholder_texture_white();
                    let albedo_view = &texture.view;
                    cmd_buffer.bind_sampling_view_and_sampler(
                        BindingFrequency::Frequent,
                        0,
                        albedo_view,
                        &self.sampler,
                    );
                }
                //_ => unimplemented!(),
            }
            cmd_buffer.finish_binding();

            cmd_buffer.set_vertex_buffer(0, BufferRef::Regular(mesh.vertices.buffer()), mesh.vertices.offset() as u64);
            if let Some(indices) = mesh.indices.as_ref() {
                cmd_buffer.set_index_buffer(
                    BufferRef::Regular(indices.buffer()),
                    indices.offset() as u64,
                    IndexFormat::U32,
                );
                cmd_buffer.draw_indexed(1, 0, range.count, range.start, 0);
            } else {
                cmd_buffer.draw(range.count, range.start);
            }
        }
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
