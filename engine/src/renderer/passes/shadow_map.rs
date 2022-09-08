use std::{sync::Arc, path::Path, io::Read};

use sourcerenderer_core::{graphics::{Backend, TextureInfo, TextureUsage, SampleCount, Format, TextureDimension, Device, GraphicsPipelineInfo, RenderPassInfo, RasterizerInfo, FillMode, CullMode, FrontFace, DepthStencilInfo, CompareFunc, StencilInfo, AttachmentInfo, SubpassInfo, DepthStencilAttachmentRef, BlendInfo, LogicOp, PrimitiveType, ShaderType, VertexLayoutInfo, ShaderInputElement, InputAssemblerElement, InputRate, BarrierSync, TextureViewInfo, TextureLayout, BarrierAccess}, Platform, platform::IO};

use crate::renderer::{renderer_resources::{RendererResources, HistoryResourceEntry}, Vertex};

pub struct ShadowMapPass<B: Backend> {
  pipeline: Arc<B::GraphicsPipeline>,
}

impl<B: Backend> ShadowMapPass<B> {
  pub const SHADOW_MAP_NAME: &'static str = "ShadowMap";
  pub fn new<P: Platform>(device: &Arc<B::Device>, resources: &mut RendererResources<B>, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    resources.create_texture(&Self::SHADOW_MAP_NAME, &TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::D24,
      width: 4096,
      height: 4096,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::DEPTH_STENCIL | TextureUsage::SAMPLED,
      supports_srgb: false,
    }, false);

    let vertex_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("shadow_map.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes, Some("shadow_map.vert.spv"))
    };

    let pipeline = device.create_graphics_pipeline(
      &GraphicsPipelineInfo {
        vs: &vertex_shader,
        fs: None,
        vertex_layout: VertexLayoutInfo {
          shader_inputs: &[
            ShaderInputElement {
              input_assembler_binding: 0,
              location_vk_mtl: 0,
              semantic_name_d3d: "pos".to_string(),
              semantic_index_d3d: 0,
              offset: 0,
              format: Format::RGB32Float,
            }
          ],
          input_assembler: &[InputAssemblerElement {
            binding: 0,
            input_rate: InputRate::PerVertex,
            stride: std::mem::size_of::<Vertex>(),
        }],
        },
        rasterizer: RasterizerInfo {
          fill_mode: FillMode::Fill,
          cull_mode: CullMode::Back,
          front_face: FrontFace::CounterClockwise,
          sample_count: SampleCount::Samples1,
        },
        depth_stencil: DepthStencilInfo {
          depth_test_enabled: true,
          depth_write_enabled: true,
          depth_func: CompareFunc::Less,
          stencil_enable: false,
          stencil_read_mask: 0,
          stencil_write_mask: 0,
          stencil_front: StencilInfo::default(),
          stencil_back: StencilInfo::default(),
        },
        blend: BlendInfo {
          alpha_to_coverage_enabled: false,
          logic_op_enabled: false,
          logic_op: LogicOp::And,
          attachments: &[],
          constants: [0f32; 4],
      },
      primitive_type: PrimitiveType::Triangles,
    }, &RenderPassInfo {
      attachments: &[
        AttachmentInfo {
          format: Format::D24,
          samples: SampleCount::Samples1,
        }
      ],
      subpasses: &[SubpassInfo {
        input_attachments: &[],
        output_color_attachments: &[],
        depth_stencil_attachment: Some(DepthStencilAttachmentRef {
          index: 0,
          read_only: false,
        }),
      }],
    }, 0, Some("ShadowMap"));

    Self {
      pipeline
    }
  }

  pub fn execute(cmd_buffer: &mut B::CommandBuffer, resources: &RendererResources<B>, gpu_driven: bool) {
    resources.access_depth_stencil_view(cmd_buffer,
      Self::SHADOW_MAP_NAME,
      BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
      BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
      TextureLayout::DepthStencilReadWrite,
      true,
      &TextureViewInfo {
        base_mip_level: 0,
        mip_level_length: 1,
        base_array_layer: 0,
        array_layer_length: 1,
        format: None,
    }, HistoryResourceEntry::Current);
  }
}
