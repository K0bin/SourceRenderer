use std::collections::HashMap;
use std::sync::atomic::{
    AtomicU32,
    Ordering,
};
use std::sync::Arc;

use bitset_core::BitSet;
use smallvec::SmallVec;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::{
    Matrix4,
    Platform,
    Vec2,
    Vec2I,
    Vec2UI,
    Vec3,
    Vec4,
};

use crate::graphics::*;
use crate::renderer::render_path::{
    RenderPassParameters,
    SceneInfo,
};
use crate::renderer::renderer_assets::RendererAssets;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    GraphicsPipelineHandle,
    GraphicsPipelineInfo,
    ShaderManager,
};
use crate::Mutex;

const QUERY_COUNT: usize = 16384;
const OCCLUDED_FRAME_COUNT: u32 = 5;
const QUERY_PING_PONG_FRAMES: u32 = 5;

pub struct OcclusionPass {
    query_buffers: Vec<Arc<BufferSlice>>,
    occluder_vb: Arc<BufferSlice>,
    occluder_ib: Arc<BufferSlice>,
    pipeline: GraphicsPipelineHandle,
    drawable_occluded_frames: AtomicRefCell<HashMap<u32, u32>>,
    occlusion_query_maps: Vec<HashMap<u32, u32>>,
    visible_drawable_indices: Vec<u32>,
}

impl OcclusionPass {
    pub fn new(device: &Arc<crate::graphics::Device>, assets: &RendererAssets) -> Self {
        let buffer_info = BufferInfo {
            size: (std::mem::size_of::<u32>() * QUERY_COUNT) as u64,
            usage: BufferUsage::COPY_DST,
            sharing_mode: QueueSharingMode::Exclusive,
        };

        let ring_size = device.prerendered_frames() as usize + 2;
        let mut query_buffers = Vec::with_capacity(ring_size);
        let mut occlusion_query_maps = Vec::with_capacity(ring_size);
        for i in 0..ring_size {
            let name = format!("QueryBuffer{}", i);
            let buffer = device
                .create_buffer(&buffer_info, MemoryUsage::GPUMemory, Some(&name))
                .unwrap();
            {
                let mut map = buffer.map_mut::<[u32; QUERY_COUNT]>().unwrap();
                *map = [!0u32; 16384];
            }
            query_buffers.push(buffer);
            occlusion_query_maps.push(HashMap::new());
        }

        let occluder_vb = device
            .create_buffer(
                &BufferInfo {
                    size: (std::mem::size_of::<Vec4>() * 8) as u64,
                    usage: BufferUsage::INITIAL_COPY | BufferUsage::VERTEX,
                    sharing_mode: QueueSharingMode::Exclusive,
                },
                MemoryUsage::GPUMemory,
                Some("OccluderVB"),
            )
            .unwrap();

        let occluder_ib = device
            .create_buffer(
                &BufferInfo {
                    size: (std::mem::size_of::<u32>() * 36) as u64,
                    usage: BufferUsage::INITIAL_COPY | BufferUsage::INDEX,
                    sharing_mode: QueueSharingMode::Exclusive,
                },
                MemoryUsage::GPUMemory,
                Some("OccluderIB"),
            )
            .unwrap();

        device
            .init_buffer(
                &[
                    Vec3::new(-0.5f32, -0.5f32, 0.5f32),
                    Vec3::new(0.5f32, -0.5f32, 0.5f32),
                    Vec3::new(0.5f32, 0.5f32, 0.5f32),
                    Vec3::new(-0.5f32, 0.5f32, 0.5f32),
                    Vec3::new(-0.5f32, -0.5f32, -0.5f32),
                    Vec3::new(0.5f32, -0.5f32, -0.5f32),
                    Vec3::new(0.5f32, 0.5f32, -0.5f32),
                    Vec3::new(-0.5f32, 0.5f32, -0.5f32),
                ],
                &occluder_vb,
                0,
            )
            .unwrap();
        device
            .init_buffer(
                &[
                    1u32, 2u32, 3u32, 3u32, 0u32, 1u32, 5u32, 6u32, 2u32, 2u32, 1u32, 5u32, 7u32,
                    3u32, 2u32, 2u32, 6u32, 7u32, 4u32, 5u32, 1u32, 1u32, 0u32, 4u32, 7u32, 4u32,
                    0u32, 0u32, 3u32, 7u32, 5u32, 4u32, 7u32, 7u32, 6u32, 5u32,
                ],
                &occluder_ib,
                0,
            )
            .unwrap();

        let pipeline = shader_manager.request_graphics_pipeline(
            &GraphicsPipelineInfo {
                vs: "shaders/occlusion.vert.json",
                fs: None,
                primitive_type: PrimitiveType::Triangles,
                vertex_layout: VertexLayoutInfo {
                    input_assembler: &[InputAssemblerElement {
                        binding: 0,
                        stride: 12,
                        input_rate: InputRate::PerVertex,
                    }],
                    shader_inputs: &[ShaderInputElement {
                        input_assembler_binding: 0,
                        location_vk_mtl: 0,
                        semantic_name_d3d: String::from(""),
                        semantic_index_d3d: 0,
                        offset: 0,
                        format: Format::RGB32Float,
                    }],
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
            },
            &RenderPassInfo {
                attachments: &[AttachmentInfo {
                    format: Format::D24S8,
                    samples: SampleCount::Samples1,
                }],
                subpasses: &[SubpassInfo {
                    input_attachments: &[],
                    output_color_attachments: &[],
                    depth_stencil_attachment: Some(DepthStencilAttachmentRef {
                        index: 0,
                        read_only: true,
                    }),
                }],
            },
            0,
        );

        assert_eq!(query_buffers.len(), occlusion_query_maps.len());

        Self {
            query_buffers,
            occluder_vb,
            occluder_ib,
            pipeline,
            occlusion_query_maps,
            visible_drawable_indices: Vec::new(),
            drawable_occluded_frames: AtomicRefCell::new(HashMap::new()),
        }
    }

    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        context: &GraphicsContext,
        command_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters<'_>,
        frame: u64,
        camera_history_buffer: &Arc<BufferSlice>,
        depth_name: &str,
    ) {
        let history_depth_buffer_ref = pass_params.resources.access_view(
            command_buffer,
            depth_name,
            BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
            BarrierAccess::DEPTH_STENCIL_READ,
            TextureLayout::DepthStencilRead,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Past,
        );

        let history_depth_buffer = &*history_depth_buffer_ref;

        let query_buffer_index = (frame % self.query_buffers.len() as u64) as usize;
        let mut occlusion_query_map =
            std::mem::take(&mut self.occlusion_query_maps[query_buffer_index]);
        occlusion_query_map.clear();
        let occlusion_query_map_lock = Mutex::new(occlusion_query_map);

        let static_meshes = pass_params.scene.scene.static_drawables();
        let view = &pass_params.scene.views[pass_params.scene.active_view_index];

        let mut map = self.drawable_occluded_frames.borrow_mut();
        self.visible_drawable_indices.clear();
        let mut visible_count = 0;
        for i in 0..static_meshes.len() {
            if view.visible_drawables_bitset.bit_test(i) {
                visible_count += 1;
                let entry = map.entry(i as u32).or_default();
                if (visible_count % QUERY_PING_PONG_FRAMES)
                    != (frame % (QUERY_PING_PONG_FRAMES as u64)) as u32
                    && *entry < OCCLUDED_FRAME_COUNT
                {
                    // Spread occlusion testing across multiple frames
                    continue;
                }

                self.visible_drawable_indices.push(i as u32);
            }
        }

        command_buffer.begin_label("Occlusion query tests");
        let query_range = command_buffer.create_query_range(QUERY_COUNT as u32);
        command_buffer.begin_render_pass(&RenderPassBeginInfo {
            attachments: &[RenderPassAttachment {
                view: RenderPassAttachmentView::DepthStencil(&*history_depth_buffer),
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
            }],
            subpasses: &[SubpassInfo {
                input_attachments: &[],
                output_color_attachments: &[],
                depth_stencil_attachment: Some(DepthStencilAttachmentRef {
                    index: 0,
                    read_only: true,
                }),
            }],
            query_range: None,
        });

        let device = pass_params.device;
        let assets = pass_params.assets;

        let pipeline = pass_params
            .shader_manager
            .get_graphics_pipeline(self.pipeline);
        let query_count = AtomicU32::new(0);
        const CHUNK_SIZE: usize = 256;
        let chunks = self.visible_drawable_indices.par_chunks(CHUNK_SIZE);
        let inner_cmd_buffers: Vec<FinishedCommandBuffer> = chunks
            .map(|chunk| {
                let mut pairs: SmallVec<[(u32, u32); CHUNK_SIZE]> = SmallVec::new();
                let mut command_buffer = context.get_inner_command_buffer(inheritance);
                command_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
                command_buffer.set_scissors(&[Scissor {
                    position: Vec2I::new(0i32, 0i32),
                    extent: Vec2UI::new(
                        history_depth_buffer.texture().unwrap().info().width,
                        history_depth_buffer.texture().unwrap().info().height,
                    ),
                }]);
                command_buffer.set_viewports(&[Viewport {
                    position: Vec2::new(0f32, 0f32),
                    extent: Vec2::new(
                        history_depth_buffer.texture().unwrap().info().width as f32,
                        history_depth_buffer.texture().unwrap().info().height as f32,
                    ),
                    min_depth: 0f32,
                    max_depth: 1f32,
                }]);
                command_buffer.set_vertex_buffer(0, BufferRef::Regular(&self.occluder_vb), 0);
                command_buffer.set_index_buffer(
                    BufferRef::Regular(&self.occluder_ib),
                    0,
                    IndexFormat::U32,
                );
                command_buffer.bind_uniform_buffer(
                    BindingFrequency::VeryFrequent,
                    0,
                    BufferRef::Regular(&camera_history_buffer),
                    0,
                    WHOLE_BUFFER,
                );
                command_buffer.finish_binding();

                for drawable_index in chunk {
                    let drawable_index = *drawable_index;
                    let drawable = &static_meshes[drawable_index as usize];

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

                    let bb = mesh.bounding_box.as_ref();
                    if bb.is_none() {
                        continue;
                    }

                    let drawable_query_index = query_count.fetch_add(1, Ordering::SeqCst);
                    pairs.push((drawable_index as u32, drawable_query_index));

                    let bb = bb.unwrap();
                    let mut bb_scale = bb.max - bb.min;
                    let bb_translation = bb.min + bb_scale / 2.0f32;
                    bb_scale *= 1.1f32; // make bounding box 10% bigger to avoid getting culled by the actual geometry
                    bb_scale.x = bb_scale.x.max(0.4f32);
                    bb_scale.y = bb_scale.y.max(0.4f32);
                    bb_scale.z = bb_scale.z.max(0.4f32);
                    let bb_transform = Matrix4::new_translation(&bb_translation)
                        * Matrix4::new_nonuniform_scaling(&bb_scale);

                    command_buffer.set_push_constant_data(
                        &[drawable.old_transform * bb_transform],
                        ShaderType::VertexShader,
                    );
                    command_buffer.begin_query(&query_range, drawable_query_index);
                    command_buffer.draw_indexed(1, 0, 36, 0, 0);
                    command_buffer.end_query(&query_range, drawable_query_index);
                }
                let inner_cmd_buffer = command_buffer.finish();

                {
                    let mut guard = occlusion_query_map_lock.lock().unwrap();
                    for (drawable_index, query_index) in pairs {
                        guard.insert(drawable_index, query_index);
                    }
                }

                inner_cmd_buffer
            })
            .collect();

        command_buffer.execute_inner(inner_cmd_buffers);
        command_buffer.end_render_pass();

        let final_query_count = query_count.load(Ordering::SeqCst);
        if final_query_count != 0 {
            let query_buffer = &self.query_buffers[query_buffer_index];
            command_buffer.barrier(&[Barrier::BufferBarrier {
                old_sync: BarrierSync::FRAGMENT_SHADER
                    | BarrierSync::VERTEX_SHADER
                    | BarrierSync::LATE_DEPTH
                    | BarrierSync::EARLY_DEPTH
                    | BarrierSync::RENDER_TARGET,
                new_sync: BarrierSync::COPY,
                old_access: BarrierAccess::empty(),
                new_access: BarrierAccess::empty(),
                buffer: BufferRef::Regular(query_buffer),
                queue_ownership: None,
            }]);
            command_buffer.flush_barriers();
            command_buffer.copy_query_results_to_buffer(
                &query_range,
                &query_buffer,
                0,
                final_query_count,
            );
        }

        command_buffer.end_label();
        self.occlusion_query_maps[query_buffer_index] =
            occlusion_query_map_lock.into_inner().unwrap();
    }

    pub fn write_occlusion_query_results(&self, frame: u64, bitset: &mut Vec<u32>) {
        bitset.fill(!0u32);
        let frame_diff = self.query_buffers.len() as u64 - 1;
        if frame < frame_diff {
            return;
        }
        let query_buffer_index = ((frame - frame_diff) % self.query_buffers.len() as u64) as usize;
        let occlusion_query_map = &self.occlusion_query_maps[query_buffer_index];
        let mapped_buffer = self.query_buffers[query_buffer_index]
            .map::<[u32; QUERY_COUNT]>()
            .unwrap();

        let mut occluded_frames_map = self.drawable_occluded_frames.borrow_mut();
        for (drawable_index, occluded_frames) in occluded_frames_map.iter_mut() {
            let query_index = occlusion_query_map.get(drawable_index);

            if let Some(query_index) = query_index {
                let samples = mapped_buffer[*query_index as usize];
                let visible = samples > 0;

                if visible {
                    *occluded_frames = 0;
                } else {
                    *occluded_frames += 1;
                }
            }
            bitset.bit_cond(
                *drawable_index as usize,
                *occluded_frames <= OCCLUDED_FRAME_COUNT,
            );
        }
    }
}
