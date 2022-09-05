use sourcerenderer_core::{graphics::{AttachmentBlendInfo, AttachmentInfo, Backend as GraphicsBackend, BlendInfo, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, FillMode, Format, FrontFace, InputAssemblerElement, InputRate, LoadOp, LogicOp, OutputAttachmentRef, PipelineBinding, PrimitiveType, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderPassInfo, RenderpassRecordingMode, SampleCount, Scissor, ShaderInputElement, StencilInfo, StoreOp, SubpassInfo, Texture, TextureInfo, TextureRenderTargetView, TextureViewInfo, TextureUsage, VertexLayoutInfo, Viewport, TextureLayout, BarrierSync, BarrierAccess, IndexFormat, TextureDimension}};
use std::sync::Arc;
use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, shader_manager::{GraphicsPipelineInfo, ShaderManager, GraphicsPipelineHandle}};
use sourcerenderer_core::{Platform, Vec2, Vec2I, Vec2UI};

use super::{draw_prep::DrawPrepPass, gpu_scene::DRAW_CAPACITY};

pub struct VisibilityBufferPass {
  pipeline: GraphicsPipelineHandle
}

impl VisibilityBufferPass {
  pub const BARYCENTRICS_TEXTURE_NAME: &'static str = "barycentrics";
  pub const PRIMITIVE_ID_TEXTURE_NAME: &'static str = "primitive";
  pub const DEPTH_TEXTURE_NAME: &'static str = "depth";

  pub fn new<P: Platform>(resolution: Vec2UI, resources: &mut RendererResources<P::GraphicsBackend>, shader_manager: &mut ShaderManager<P>) -> Self {
    let barycentrics_texture_info = TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RG16UNorm,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC | TextureUsage::STORAGE,
      supports_srgb: false,
    };
    resources.create_texture(Self::BARYCENTRICS_TEXTURE_NAME, &barycentrics_texture_info, false);

    let primitive_id_texture_info = TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::R32UInt,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC | TextureUsage::STORAGE,
      supports_srgb: false,
    };
    resources.create_texture(Self::PRIMITIVE_ID_TEXTURE_NAME, &primitive_id_texture_info, false);

    let depth_texture_info = TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::D24,
      width: resolution.x,
      height: resolution.y,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::DEPTH_STENCIL,
      supports_srgb: false,
    };
    resources.create_texture(Self::DEPTH_TEXTURE_NAME, &depth_texture_info, true);

    let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
      vs: "shaders/visibility_buffer.vert.spv",
      fs: Some("shaders/visibility_buffer.frag.spv"),
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
        attachments: &[
          AttachmentBlendInfo::default(),
          AttachmentBlendInfo::default()
        ]
      }
    };
    let pipeline = shader_manager.request_graphics_pipeline(&pipeline_info, &RenderPassInfo {
      attachments: &[
        AttachmentInfo {
          format: primitive_id_texture_info.format,
          samples: primitive_id_texture_info.samples,
        },
        AttachmentInfo {
          format: barycentrics_texture_info.format,
          samples: barycentrics_texture_info.samples,
        },
        AttachmentInfo {
          format: depth_texture_info.format,
          samples: depth_texture_info.samples,
        }
      ],
      subpasses: &[
        SubpassInfo {
          input_attachments: &[],
          output_color_attachments: &[
            OutputAttachmentRef {
              index: 0,
              resolve_attachment_index: None
            },
            OutputAttachmentRef {
              index: 1,
              resolve_attachment_index: None
            }
          ],
          depth_stencil_attachment: Some(DepthStencilAttachmentRef {
            index: 2,
            read_only: false,
          }),
        }
      ]
    }, 0);

    Self {
      pipeline
    }
  }

  #[profiling::function]
  pub(super) fn execute<P: Platform>(
    &mut self,
    cmd_buffer: &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer,
    resources: &RendererResources<P::GraphicsBackend>,
    vertex_buffer: &Arc<<P::GraphicsBackend as GraphicsBackend>::Buffer>,
    index_buffer: &Arc<<P::GraphicsBackend as GraphicsBackend>::Buffer>,
    shader_manager: &ShaderManager<P>
  ) {
    cmd_buffer.begin_label("Visibility Buffer pass");
    let draw_buffer = resources.access_buffer(
      cmd_buffer,
      DrawPrepPass::INDIRECT_DRAW_BUFFER,
      BarrierSync::INDIRECT,
      BarrierAccess::INDIRECT_READ,
      HistoryResourceEntry::Current
    );

    let barycentrics_rtv = resources.access_render_target_view(
      cmd_buffer,
      Self::BARYCENTRICS_TEXTURE_NAME,
      BarrierSync::RENDER_TARGET,
      BarrierAccess::RENDER_TARGET_WRITE,
      TextureLayout::RenderTarget, true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let primitive_id_rtv = resources.access_render_target_view(
      cmd_buffer,
      Self::PRIMITIVE_ID_TEXTURE_NAME,
      BarrierSync::RENDER_TARGET,
      BarrierAccess::RENDER_TARGET_WRITE,
      TextureLayout::RenderTarget, true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    let dsv = resources.access_depth_stencil_view(
      cmd_buffer,
      Self::DEPTH_TEXTURE_NAME,
      BarrierSync::LATE_DEPTH | BarrierSync::EARLY_DEPTH,
      BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
      TextureLayout::DepthStencilReadWrite, true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );

    cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
      attachments: &[
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&primitive_id_rtv),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&barycentrics_rtv),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::DepthStencil(&dsv),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store
        }
      ],
      subpasses: &[
        SubpassInfo {
          input_attachments: &[],
          output_color_attachments: &[
            OutputAttachmentRef {
              index: 0,
              resolve_attachment_index: None
            },
            OutputAttachmentRef {
              index: 1,
              resolve_attachment_index: None
            }
          ],
          depth_stencil_attachment: Some(DepthStencilAttachmentRef {
            index: 2,
            read_only: false,
          }),
        }
      ]
    }, RenderpassRecordingMode::Commands);

    let rtv_info = barycentrics_rtv.texture().info();
    let pipeline = shader_manager.get_graphics_pipeline(self.pipeline);
    cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
    cmd_buffer.set_viewports(&[Viewport {
      position: Vec2::new(0.0f32, 0.0f32),
      extent: Vec2::new(rtv_info.width as f32, rtv_info.height as f32),
      min_depth: 0.0f32,
      max_depth: 1.0f32
    }]);
    cmd_buffer.set_scissors(&[Scissor {
      position: Vec2I::new(0, 0),
      extent: Vec2UI::new(9999, 9999),
    }]);

    cmd_buffer.set_vertex_buffer(vertex_buffer, 0);
    cmd_buffer.set_index_buffer(index_buffer, 0, IndexFormat::U32);

    cmd_buffer.finish_binding();
    cmd_buffer.draw_indexed_indirect(&draw_buffer, 4, &draw_buffer, 0, DRAW_CAPACITY, 20);

    cmd_buffer.end_render_pass();
    cmd_buffer.end_label();
  }
}
