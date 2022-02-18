use sourcerenderer_core::graphics::{Barrier, OutputAttachmentRef, Queue, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderpassRecordingMode, Texture, TextureDepthStencilView, TextureDepthStencilViewInfo, TextureRenderTargetView, TextureRenderTargetViewInfo, TextureShaderResourceView, TextureShaderResourceViewInfo, TextureLayout, BarrierAccess, BarrierSync, IndexFormat};
use sourcerenderer_core::graphics::{AttachmentBlendInfo, AttachmentInfo, Backend as GraphicsBackend, BindingFrequency, BlendInfo, BufferUsage, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Format, FrontFace, GraphicsPipelineInfo, InputAssemblerElement, InputRate, LoadOp, LogicOp, PipelineBinding, PrimitiveType, RasterizerInfo, RenderPassInfo, SampleCount, Scissor, ShaderInputElement, ShaderType, StencilInfo, StoreOp, SubpassInfo, Swapchain, TextureInfo, TextureUsage, VertexLayoutInfo, Viewport};
use std::sync::Arc;
use crate::renderer::{RendererScene, drawable::View, passes::desktop::taa::scaled_halton_point};
use sourcerenderer_core::{Matrix4, Platform, Vec2, Vec2I, Vec2UI};
use std::path::Path;
use std::io::Read;
use sourcerenderer_core::platform::io::IO;
use rayon::prelude::*;

#[derive(Clone, Copy)]
#[repr(C)]
struct PrepassCameraCB {
  view_projection: Matrix4,
  old_view_projection: Matrix4
}
#[derive(Clone, Copy)]
#[repr(C)]
struct PrepassModelCB {
  model: Matrix4,
  old_model: Matrix4
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FrameData {
  swapchain_transform: Matrix4,
  halton_point: Vec2
}

pub struct Prepass<B: GraphicsBackend> {
  depth_buffer: Arc<B::TextureDepthStencilView>,
  depth_buffer_b: Arc<B::TextureDepthStencilView>,
  depth_srv: Arc<B::TextureShaderResourceView>,
  depth_srv_b: Arc<B::TextureShaderResourceView>,
  motion: Arc<B::TextureRenderTargetView>,
  motion_srv: Arc<B::TextureShaderResourceView>,
  normals: Arc<B::TextureRenderTargetView>,
  normals_srv: Arc<B::TextureShaderResourceView>,
  pipeline: Arc<B::GraphicsPipeline>
}

impl<B: GraphicsBackend> Prepass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let depth_info = TextureInfo {
      format: Format::D24S8,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::DEPTH_STENCIL | TextureUsage::SAMPLED,
    };
    let depth_buffer = device.create_texture(&depth_info, Some("PrepassDepth"));
    let depth_buffer_b = device.create_texture(&depth_info, Some("PrepassDepthB"));
    let dsv_info = TextureDepthStencilViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    };
    let dsv = device.create_depth_stencil_view(&depth_buffer, &dsv_info);
    let dsv_b = device.create_depth_stencil_view(&depth_buffer_b, &dsv_info);
    let depth_srv_info = TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    };
    let depth_srv = device.create_shader_resource_view(&depth_buffer, &depth_srv_info);
    let depth_srv_b = device.create_shader_resource_view(&depth_buffer_b, &depth_srv_info);

    let motion = device.create_texture(&TextureInfo {
      format: Format::RG32Float,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
    }, Some("Motion"));
    let motion_view = device.create_render_target_view(&motion, &TextureRenderTargetViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });
    let motion_srv = device.create_shader_resource_view(&motion, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });

    let normals = device.create_texture(&TextureInfo {
      format: Format::RGBA32Float,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
    }, Some("Normals"));
    let normals_view = device.create_render_target_view(&normals, &TextureRenderTargetViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });
    let normals_srv = device.create_shader_resource_view(&normals, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });


    let vertex_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("prepass.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes, Some("prepass.vert.spv"))
    };

    let fragment_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("prepass.frag.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::FragmentShader, &bytes, Some("prepass.frag.spv"))
    };
    let pipeline_info: GraphicsPipelineInfo<B> = GraphicsPipelineInfo {
      vs: vertex_shader,
      fs: Some(fragment_shader),
      gs: None,
      tcs: None,
      tes: None,
      primitive_type: PrimitiveType::Triangles,
      vertex_layout: VertexLayoutInfo {
        input_assembler: vec![
          InputAssemblerElement {
            binding: 0,
            stride: 44,
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
          },
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 1,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 12,
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
        depth_write_enabled: true,
        depth_func: CompareFunc::Less,
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
          AttachmentBlendInfo::default(),
          AttachmentBlendInfo::default()
        ]
      }
    };
    let pipeline = device.create_graphics_pipeline(&pipeline_info, &RenderPassInfo {
      attachments: vec![
        AttachmentInfo {
          format: Format::RG32Float,
          samples: SampleCount::Samples1,
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
          stencil_load_op: LoadOp::DontCare,
          stencil_store_op: StoreOp::DontCare
        },
        AttachmentInfo {
          format: Format::RGBA32Float,
          samples: SampleCount::Samples1,
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
          stencil_load_op: LoadOp::DontCare,
          stencil_store_op: StoreOp::DontCare
        },
        AttachmentInfo {
          format: Format::D24S8,
          samples: SampleCount::Samples1,
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
          stencil_load_op: LoadOp::DontCare,
          stencil_store_op: StoreOp::DontCare
        }
      ],
      subpasses: vec![
        SubpassInfo {
          input_attachments: vec![],
          output_color_attachments: vec![
            OutputAttachmentRef {
              index: 1,
              resolve_attachment_index: None
            },
            OutputAttachmentRef {
              index: 0,
              resolve_attachment_index: None
            },
          ],
          depth_stencil_attachment: Some(DepthStencilAttachmentRef {
            index: 2,
            read_only: false
          })
        }
      ],
    }, 0);

    init_cmd_buffer.barrier(&[Barrier::TextureBarrier {
      old_sync: BarrierSync::empty(),
      new_sync: BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
      old_layout: TextureLayout::Undefined,
      new_layout: TextureLayout::DepthStencilRead,
      old_access: BarrierAccess::empty(),
      new_access: BarrierAccess::DEPTH_STENCIL_READ,
      texture: &depth_buffer_b,
    }]);

    Self {
      depth_buffer: dsv,
      depth_buffer_b: dsv_b,
      motion: motion_view,
      motion_srv,
      depth_srv,
      depth_srv_b,
      normals: normals_view,
      normals_srv,
      pipeline
    }
  }

  pub(super) fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    device: &Arc<B::Device>,
    scene: &RendererScene<B>,
    view: &View,
    swapchain_transform: Matrix4,
    frame: u64,
    camera_buffer: &Arc<B::Buffer>,
    camera_history_buffer: &Arc<B::Buffer>
  ) {
    cmd_buffer.begin_label("Depth prepass");
    let static_drawables = scene.static_drawables();

    cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_sync: BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
        new_sync: BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
        old_layout: TextureLayout::Undefined,
        new_layout: TextureLayout::DepthStencilReadWrite,
        texture: self.depth_buffer.texture()
      },
      Barrier::TextureBarrier {
        old_sync: BarrierSync::COMPUTE_SHADER,
        new_sync: BarrierSync::RENDER_TARGET,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::empty(),
        old_layout: TextureLayout::Undefined,
        new_layout: TextureLayout::RenderTarget,
        texture: self.motion.texture(),
      },
      Barrier::TextureBarrier {
        old_sync: BarrierSync::COMPUTE_SHADER,
        new_sync: BarrierSync::RENDER_TARGET,
        old_access: BarrierAccess::empty(),
        new_access: BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ,
        old_layout: TextureLayout::Undefined,
        new_layout: TextureLayout::RenderTarget,
        texture: self.normals.texture(),
      },
    ]);

    cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
      attachments: &[
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&self.motion),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&self.normals),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::DepthStencil(&self.depth_buffer),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store
        }
      ],
      subpasses: &[
        SubpassInfo {
          input_attachments: vec![],
          output_color_attachments: vec![
            OutputAttachmentRef {
              index: 1,
              resolve_attachment_index: None
            },
            OutputAttachmentRef {
              index: 0,
              resolve_attachment_index: None
            }
          ],
          depth_stencil_attachment: Some(DepthStencilAttachmentRef {
            index: 2,
            read_only: false,
        }),
        }
      ],
    }, RenderpassRecordingMode::CommandBuffers);

    let info = self.motion.texture().get_info();
    let per_frame = FrameData {
      swapchain_transform,
      halton_point: scaled_halton_point(info.width, info.height, (frame % 8) as u32)
    };
    let transform_constant_buffer = cmd_buffer.upload_dynamic_data(&[per_frame], BufferUsage::CONSTANT);

    let inheritance = cmd_buffer.inheritance();
    const CHUNK_SIZE: usize = 128;
    let chunks = view.drawable_parts.par_chunks(CHUNK_SIZE);
    let inner_cmd_buffers: Vec<B::CommandBufferSubmission> = chunks.map(|chunk| {
      let mut command_buffer = device.graphics_queue().create_inner_command_buffer(inheritance);

      command_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));
      command_buffer.set_viewports(&[Viewport {
        position: Vec2::new(0.0f32, 0.0f32),
        extent: Vec2::new(info.width as f32, info.height as f32),
        min_depth: 0.0f32,
        max_depth: 1.0f32
      }]);
      command_buffer.set_scissors(&[Scissor {
        position: Vec2I::new(0, 0),
        extent: Vec2UI::new(9999, 9999),
      }]);
      command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 2, &transform_constant_buffer);

      command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, camera_buffer);
      command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 1, camera_history_buffer);
      command_buffer.finish_binding();

      for part in chunk.iter() {
        let drawable = &static_drawables[part.drawable_index];
        let model = &drawable.model;

        command_buffer.upload_dynamic_data_inline(&[PrepassModelCB {
          model: drawable.transform,
          old_model: drawable.old_transform
        }], ShaderType::VertexShader);

        let mesh = &model.mesh();

        command_buffer.set_vertex_buffer(&mesh.vertices);
        if mesh.indices.is_some() {
          command_buffer.set_index_buffer(mesh.indices.as_ref().unwrap(), IndexFormat::U32);
        }

        let range = &mesh.parts[part.part_index];

        if mesh.indices.is_some() {
          command_buffer.draw_indexed(1, 0, range.count, range.start, 0);
        } else {
          command_buffer.draw(range.count, range.start);
        }
      }
      command_buffer.finish()
    }).collect();

    cmd_buffer.execute_inner(inner_cmd_buffers);
    cmd_buffer.end_render_pass();
    cmd_buffer.end_label();
  }

  pub fn swap_history_resources(&mut self) {
    std::mem::swap(&mut self.depth_buffer, &mut self.depth_buffer_b);
    std::mem::swap(&mut self.depth_srv, &mut self.depth_srv_b);
  }

  pub fn depth_dsv(&self) -> &Arc<B::TextureDepthStencilView> {
    &self.depth_buffer
  }

  pub fn depth_srv(&self) -> &Arc<B::TextureShaderResourceView> {
    &self.depth_srv
  }

  pub fn depth_dsv_history(&self) -> &Arc<B::TextureDepthStencilView> {
    &self.depth_buffer_b
  }

  pub fn motion_srv(&self) -> &Arc<B::TextureShaderResourceView> {
    &self.motion_srv
  }

  pub fn normals_srv(&self) -> &Arc<B::TextureShaderResourceView> {
    &self.normals_srv
  }
}
