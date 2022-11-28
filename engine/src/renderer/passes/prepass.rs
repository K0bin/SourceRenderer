use sourcerenderer_core::graphics::{OutputAttachmentRef, Queue, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderpassRecordingMode, TextureViewInfo, TextureLayout, BarrierAccess, BarrierSync, IndexFormat, TextureView, Texture, WHOLE_BUFFER, TextureDimension};
use sourcerenderer_core::graphics::{AttachmentBlendInfo, AttachmentInfo, Backend as GraphicsBackend, BindingFrequency, BlendInfo, BufferUsage, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Format, FrontFace, InputAssemblerElement, InputRate, LoadOp, LogicOp, PipelineBinding, PrimitiveType, RasterizerInfo, RenderPassInfo, SampleCount, Scissor, ShaderInputElement, ShaderType, StencilInfo, StoreOp, SubpassInfo, TextureInfo, TextureUsage, VertexLayoutInfo, Viewport};
use std::sync::Arc;
use crate::renderer::passes::taa::scaled_halton_point;
use crate::renderer::renderer_assets::RendererAssets;
use crate::renderer::renderer_resources::{RendererResources, HistoryResourceEntry};
use crate::renderer::shader_manager::{ShaderManager, GraphicsPipelineInfo, GraphicsPipelineHandle};
use crate::renderer::{RendererScene, drawable::View};
use sourcerenderer_core::{Matrix4, Platform, Vec2, Vec2I, Vec2UI};
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

pub struct Prepass {
  pipeline: GraphicsPipelineHandle
}

impl Prepass {
  pub const DEPTH_TEXTURE_NAME: &'static str = "PrepassDepth";
  pub const MOTION_TEXTURE_NAME: &'static str = "Motion";
  pub const NORMALS_TEXTURE_NAME: &'static str = "Normals";

  const DRAWABLE_LABELS: bool = false;

  pub fn new<P: Platform>(
    resources: &mut RendererResources<P::GraphicsBackend>,
    shader_manager: &mut ShaderManager<P>,
    resolution: Vec2UI
  ) -> Self {
    let depth_info = TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::D24,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::DEPTH_STENCIL | TextureUsage::SAMPLED,
      supports_srgb: false,
    };
    resources.create_texture(Self::DEPTH_TEXTURE_NAME, &depth_info, true);

    resources.create_texture(Self::MOTION_TEXTURE_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RG32Float,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
      supports_srgb: false,
    }, true);

    resources.create_texture(Self::NORMALS_TEXTURE_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA32Float,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
      supports_srgb: false,
    }, false);

    let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
      vs: &("shaders/prepass.vert.spv"),
      fs: Some("shaders/prepass.frag.spv"),
      primitive_type: PrimitiveType::Triangles,
      vertex_layout: VertexLayoutInfo {
        input_assembler: &[
          InputAssemblerElement {
            binding: 0,
            stride: 64,
            input_rate: InputRate::PerVertex
          }
        ],
        shader_inputs: &[
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
            offset: 16,
            format: Format::RGB32Float
          }
        ]
      },
      rasterizer: RasterizerInfo {
        fill_mode: FillMode::Fill,
        cull_mode: CullMode::Back,
        front_face: FrontFace::Clockwise,
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
        attachments: &[
          AttachmentBlendInfo::default(),
          AttachmentBlendInfo::default()
        ]
      }
    };
    let pipeline = shader_manager.request_graphics_pipeline(&pipeline_info, &RenderPassInfo {
      attachments: &[
        AttachmentInfo {
          format: Format::RG32Float,
          samples: SampleCount::Samples1,
        },
        AttachmentInfo {
          format: Format::RGBA32Float,
          samples: SampleCount::Samples1,
        },
        AttachmentInfo {
          format: Format::D24,
          samples: SampleCount::Samples1,
        }
      ],
      subpasses: &[
        SubpassInfo {
          input_attachments: &[],
          output_color_attachments: &[
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

    Self {
      pipeline
    }
  }

  #[profiling::function]
  pub(super) fn execute<P: Platform>(
    &mut self,
    cmd_buffer: &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer,
    device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>,
    scene: &RendererScene<P::GraphicsBackend>,
    view: &View,
    swapchain_transform: Matrix4,
    frame: u64,
    camera_buffer: &Arc<<P::GraphicsBackend as GraphicsBackend>::Buffer>,
    camera_history_buffer: &Arc<<P::GraphicsBackend as GraphicsBackend>::Buffer>,
    resources: &RendererResources<P::GraphicsBackend>,
    shader_manager: &ShaderManager<P>,
    assets: &RendererAssets<P>
  ) {
    cmd_buffer.begin_label("Depth prepass");
    let static_drawables = scene.static_drawables();

    let depth_buffer = resources.access_view(
      cmd_buffer,
      Self::DEPTH_TEXTURE_NAME,
      BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
      BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
      TextureLayout::DepthStencilReadWrite,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let motion = resources.access_view(
      cmd_buffer,
      Self::MOTION_TEXTURE_NAME,
      BarrierSync::RENDER_TARGET,
      BarrierAccess::RENDER_TARGET_WRITE,
      TextureLayout::RenderTarget,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let normals = resources.access_view(
      cmd_buffer,
      Self::NORMALS_TEXTURE_NAME,
      BarrierSync::RENDER_TARGET,
      BarrierAccess::RENDER_TARGET_WRITE,
      TextureLayout::RenderTarget,
      true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
      attachments: &[
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&*motion),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&*normals),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::DepthStencil(&*depth_buffer),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store
        }
      ],
      subpasses: &[
        SubpassInfo {
          input_attachments: &[],
          output_color_attachments: &[
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

    let info = motion.texture().info();
    let per_frame = FrameData {
      swapchain_transform,
      halton_point: scaled_halton_point(info.width, info.height, (frame % 8) as u32 + 1)
    };
    let transform_constant_buffer = cmd_buffer.upload_dynamic_data(&[per_frame], BufferUsage::CONSTANT);

    let inheritance = cmd_buffer.inheritance();
    const CHUNK_SIZE: usize = 128;
    let chunks = view.drawable_parts.par_chunks(CHUNK_SIZE);
    let pipeline = shader_manager.get_graphics_pipeline(self.pipeline);
    let inner_cmd_buffers: Vec<<P::GraphicsBackend as GraphicsBackend>::CommandBufferSubmission> = chunks.map(|chunk| {
      let mut command_buffer = device.graphics_queue().create_inner_command_buffer(inheritance);

      command_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
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
      command_buffer.bind_uniform_buffer(BindingFrequency::Frequent, 2, &transform_constant_buffer, 0, WHOLE_BUFFER);

      command_buffer.bind_uniform_buffer(BindingFrequency::Frequent, 0, camera_buffer, 0, WHOLE_BUFFER);
      command_buffer.bind_uniform_buffer(BindingFrequency::Frequent, 1, camera_history_buffer, 0, WHOLE_BUFFER);
      command_buffer.finish_binding();

      for part in chunk.iter() {
        let drawable = &static_drawables[part.drawable_index];
        if Self::DRAWABLE_LABELS {
          command_buffer.begin_label(&format!("Drawable {}", part.drawable_index));
        }

        command_buffer.upload_dynamic_data_inline(&[PrepassModelCB {
          model: drawable.transform,
          old_model: drawable.old_transform
        }], ShaderType::VertexShader);

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

        command_buffer.set_vertex_buffer(mesh.vertices.buffer(), mesh.vertices.offset() as usize);
        if let Some(indices) = mesh.indices.as_ref() {
          command_buffer.set_index_buffer(indices.buffer(), indices.offset() as usize, IndexFormat::U32);
        }

        let range = &mesh.parts[part.part_index];

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
    }).collect();

    cmd_buffer.execute_inner(inner_cmd_buffers);
    cmd_buffer.end_render_pass();
    cmd_buffer.end_label();
  }
}
