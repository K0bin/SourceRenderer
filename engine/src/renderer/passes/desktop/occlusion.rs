use std::{sync::{Arc, atomic::{AtomicU32, Ordering}, Mutex}, io::Read, path::Path, collections::HashMap};

use bitset_core::BitSet;
use rayon::{slice::ParallelSlice, iter::ParallelIterator};
use smallvec::SmallVec;
use sourcerenderer_core::{graphics::{Backend, BufferInfo, BufferUsage, MemoryUsage, Device, Buffer, CommandBuffer, Barrier, BarrierSync, BarrierAccess, RenderPassInfo, GraphicsPipelineInfo, ShaderType, VertexLayoutInfo, PrimitiveType, ShaderInputElement, InputAssemblerElement, InputRate, Format, RasterizerInfo, FillMode, CullMode, SampleCount, FrontFace, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, LoadOp, AttachmentInfo, StoreOp, SubpassInfo, DepthStencilAttachmentRef, RenderPassBeginInfo, RenderPassAttachment, RenderPassAttachmentView, RenderpassRecordingMode, PipelineBinding, Scissor, Viewport, TextureDepthStencilView, Texture, BindingFrequency, TextureLayout, Queue}, Vec4, Platform, platform::io::IO, Vec2UI, Vec2I, Vec2, Matrix4, Vec3, atomic_refcell::AtomicRefCell};

use crate::renderer::{drawable::View, renderer_scene::RendererScene};

const QUERY_COUNT: usize = 16384;
const OCCLUDED_FRAME_COUNT: u32 = 5;
const QUERY_PING_PONG_FRAMES: u32 = 5;

pub struct OcclusionPass<B: Backend> {
  query_buffers: Vec<Arc<B::Buffer>>,
  occluder_vb: Arc<B::Buffer>,
  occluder_ib: Arc<B::Buffer>,
  pipeline: Arc<B::GraphicsPipeline>,
  drawable_occluded_frames: AtomicRefCell<HashMap<u32, u32>>,
  occlusion_query_maps: Vec<HashMap<u32, u32>>,
  visible_drawable_indices: Vec<u32>
}

impl<B: Backend> OcclusionPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>) -> Self {
    let buffer_info = BufferInfo {
      size: std::mem::size_of::<u32>() * QUERY_COUNT,
      usage: BufferUsage::COPY_DST,
    };

    let ring_size = device.prerendered_frames() as usize + 2;
    let mut query_buffers = Vec::with_capacity(ring_size);
    let mut occlusion_query_maps = Vec::with_capacity(ring_size);
    for i in 0..ring_size {
      let name = format!("QueryBuffer{}", i);
      let buffer = device.create_buffer(&buffer_info, MemoryUsage::GpuToCpu, Some(&name));
      {
        let mut map = buffer.map_mut::<[u32; QUERY_COUNT]>().unwrap();
        *map = [!0u32; 16384];
      }
      query_buffers.push(buffer);
      occlusion_query_maps.push(HashMap::new());
    }

    let occluder_vb = device.create_buffer(&BufferInfo {
      size: std::mem::size_of::<Vec4>() * 8,
      usage: BufferUsage::COPY_DST | BufferUsage::VERTEX,
    }, MemoryUsage::GpuOnly, Some("OccluderVB"));

    let occluder_ib = device.create_buffer(&BufferInfo {
      size: std::mem::size_of::<u32>() * 36,
      usage: BufferUsage::COPY_DST | BufferUsage::INDEX,
    }, MemoryUsage::GpuOnly, Some("OccluderIB"));

    let occluder_vb_data = device.upload_data(&[
      Vec3::new(-0.5f32, -0.5f32, 0.5f32),
      Vec3::new(0.5f32, -0.5f32, 0.5f32),
      Vec3::new(0.5f32, 0.5f32, 0.5f32),
      Vec3::new(-0.5f32, 0.5f32, 0.5f32),
      Vec3::new(-0.5f32, -0.5f32, -0.5f32),
      Vec3::new(0.5f32, -0.5f32, -0.5f32),
      Vec3::new(0.5f32, 0.5f32, -0.5f32),
      Vec3::new(-0.5f32, 0.5f32, -0.5f32),
    ], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    let occluder_ib_data = device.upload_data(&[
      1u32, 2u32, 3u32, 3u32, 0u32, 1u32,
      5u32, 6u32, 2u32, 2u32, 1u32, 5u32,
      7u32, 3u32, 2u32, 2u32, 6u32, 7u32,
      4u32, 5u32, 1u32, 1u32, 0u32, 4u32,
      7u32, 4u32, 0u32, 0u32, 3u32, 7u32,
      5u32, 4u32, 7u32, 7u32, 6u32, 5u32
    ], MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);

    device.init_buffer(&occluder_vb_data, &occluder_vb);
    device.init_buffer(&occluder_ib_data, &occluder_ib);

    let vertex_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("occlusion.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes, Some("occlusion.vert.spv"))
    };
    let pipeline = device.create_graphics_pipeline(&GraphicsPipelineInfo {
      vs: vertex_shader,
      fs: None,
      gs: None,
      tcs: None,
      tes: None,primitive_type: PrimitiveType::Triangles,
      vertex_layout: VertexLayoutInfo {
        input_assembler: vec![
          InputAssemblerElement {
            binding: 0,
            stride: 12,
            input_rate: InputRate::PerVertex
          }
        ],
        shader_inputs: vec![
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 0,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 0,
            format: Format::RGB32Float
          }
        ]
      },
      rasterizer: RasterizerInfo {
        fill_mode: FillMode::Fill,
        cull_mode: CullMode::Back,
        front_face: FrontFace::CounterClockwise,
        sample_count: SampleCount::Samples1
      },
      depth_stencil: DepthStencilInfo {
        depth_test_enabled: true,
        depth_write_enabled: false,
        depth_func: CompareFunc::LessEqual,
        stencil_enable: false,
        stencil_read_mask: 0u8,
        stencil_write_mask: 0u8,
        stencil_front: StencilInfo::default(),
        stencil_back: StencilInfo::default()
      },
      blend: BlendInfo {
        alpha_to_coverage_enabled: false,
        logic_op_enabled: false,
        logic_op: LogicOp::And,
        constants: [0f32, 0f32, 0f32, 0f32],
        attachments: vec![
          AttachmentBlendInfo::default()
        ]
      }
    }, &RenderPassInfo {
      attachments: vec![AttachmentInfo {
        format: Format::D24S8,
        samples: SampleCount::Samples1,
        load_op: LoadOp::Load,
        store_op: StoreOp::Store,
        stencil_load_op: LoadOp::DontCare,
        stencil_store_op: StoreOp::DontCare,
      }],
      subpasses: vec![
        SubpassInfo {
          input_attachments: vec![],
          output_color_attachments: vec![],
          depth_stencil_attachment: Some(
            DepthStencilAttachmentRef {
              index: 0,
              read_only: true
            }
          )
        }
      ],
    }, 0);

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

  pub fn execute(
    &mut self,
    device: &B::Device,
    command_buffer: &mut B::CommandBuffer,
    frame: u64,
    history_depth_buffer: &Arc<B::TextureDepthStencilView>,
    camera_history_buffer: &Arc<B::Buffer>,
    scene: &RendererScene<B>,
    view: &View
  ) {
    let query_buffer_index = (frame % self.query_buffers.len() as u64) as usize;
    let mut occlusion_query_map = std::mem::take(&mut self.occlusion_query_maps[query_buffer_index]);
    occlusion_query_map.clear();
    let occlusion_query_map_lock = Mutex::new(occlusion_query_map);

    let static_meshes = scene.static_drawables();

    let mut map = self.drawable_occluded_frames.borrow_mut();
    self.visible_drawable_indices.clear();
    let mut visible_count = 0;
    for i in 0..static_meshes.len() {
      if view.visible_drawables_bitset.bit_test(i) {
        visible_count += 1;
        let entry = map.entry(i as u32).or_default();
        if (visible_count % QUERY_PING_PONG_FRAMES) != (frame % (QUERY_PING_PONG_FRAMES as u64)) as u32 && *entry < OCCLUDED_FRAME_COUNT {
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
        view: RenderPassAttachmentView::DepthStencil(history_depth_buffer),
        load_op: LoadOp::Load,
        store_op: StoreOp::Store,
      }],
      subpasses: &[SubpassInfo {
        input_attachments: vec![],
        output_color_attachments: vec![],
        depth_stencil_attachment: Some(DepthStencilAttachmentRef {
          index: 0,
          read_only: true
        }),
      }],
    }, RenderpassRecordingMode::CommandBuffers);

    let query_count = AtomicU32::new(0);
    const CHUNK_SIZE: usize = 256;
    let chunks = self.visible_drawable_indices.par_chunks(CHUNK_SIZE);
    let inheritance = command_buffer.inheritance();
    let inner_cmd_buffers: Vec::<B::CommandBufferSubmission> = chunks.map(|chunk| {
      let mut pairs: SmallVec<[(u32, u32); CHUNK_SIZE]> = SmallVec::new();
      let mut command_buffer = device.graphics_queue().create_inner_command_buffer(inheritance);
      command_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));
      command_buffer.set_scissors(&[Scissor {
        position: Vec2I::new(0i32, 0i32),
        extent: Vec2UI::new(99999u32, 99999u32),
      }]);
      command_buffer.set_viewports(&[Viewport {
        position: Vec2::new(0f32, 0f32),
        extent: Vec2::new(history_depth_buffer.texture().get_info().width as f32, history_depth_buffer.texture().get_info().height as f32),
        min_depth: 0f32,
        max_depth: 1f32,
      }]);
      command_buffer.set_vertex_buffer(&self.occluder_vb);
      command_buffer.set_index_buffer(&self.occluder_ib);
      command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, &camera_history_buffer);
      command_buffer.finish_binding();

      for drawable_index in chunk {
        let drawable_index = *drawable_index;
        let drawable = &static_meshes[drawable_index as usize];
        let mesh = drawable.model.mesh();
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

        command_buffer.upload_dynamic_data_inline(&[drawable.old_transform * bb_transform], ShaderType::VertexShader);
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
    }).collect();

    command_buffer.execute_inner(inner_cmd_buffers);
    command_buffer.end_render_pass();

    let final_query_count = query_count.load(Ordering::SeqCst);
    if final_query_count != 0 {
      let query_buffer = &self.query_buffers[query_buffer_index];
      command_buffer.barrier(&[Barrier::BufferBarrier {
        old_sync: BarrierSync::FRAGMENT_SHADER | BarrierSync::VERTEX_SHADER | BarrierSync::LATE_DEPTH | BarrierSync::EARLY_DEPTH | BarrierSync::RENDER_TARGET,
        new_sync: BarrierSync::COPY,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::empty(),
        buffer: query_buffer,
      }]);
      command_buffer.flush_barriers();
      command_buffer.copy_query_results_to_buffer(&query_range, &query_buffer, 0, final_query_count);
    }

    command_buffer.end_label();
    self.occlusion_query_maps[query_buffer_index] = occlusion_query_map_lock.into_inner().unwrap();
  }

  pub fn write_occlusion_query_results(&self, frame: u64, bitset: &mut Vec<u32>) {
    bitset.fill(!0u32);
    let frame_diff = self.query_buffers.len() as u64 - 1;
    if frame < frame_diff {
      return;
    }
    let query_buffer_index = ((frame - frame_diff) % self.query_buffers.len() as u64) as usize;
    let occlusion_query_map = &self.occlusion_query_maps[query_buffer_index];
    let mapped_buffer = self.query_buffers[query_buffer_index].map::<[u32; QUERY_COUNT]>().unwrap();

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
      bitset.bit_cond(*drawable_index as usize, *occluded_frames <= OCCLUDED_FRAME_COUNT);
    }
  }
}