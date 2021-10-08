use std::{io::Read, path::Path, sync::Arc};

use sourcerenderer_core::{Platform, graphics::{AttachmentBlendInfo, AttachmentInfo, Backend, BlendInfo, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Format, FrontFace, GraphicsPipelineInfo, InputAssemblerElement, InputRate, LoadOp, LogicOp, OutputAttachmentRef, PipelineBinding, PrimitiveType, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderPassInfo, RenderpassRecordingMode, SampleCount, ShaderInputElement, ShaderType, StencilInfo, StoreOp, SubpassInfo, Swapchain, TextureDepthStencilViewInfo, TextureInfo, TextureUsage, VertexLayoutInfo}, platform::io::IO};

use crate::{renderer::{drawable::View, renderer_scene::RendererScene}};

pub struct GeometryPass<B: Backend> {
  depth_buffer: Arc<B::TextureDepthStencilView>,
  swapchain: Arc<B::Swapchain>,
  pipeline: Arc<B::GraphicsPipeline>
}

impl<B: Backend> GeometryPass<B> {

  pub(super) fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>) -> Self {
    let ds = device.create_texture(&TextureInfo {
      format: Format::D32,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::DEPTH_READ | TextureUsage::DEPTH_WRITE,
    }, None);

    let dsv = device.create_depth_stencil_view(&ds, &TextureDepthStencilViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });

    let vertex_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("textured.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes, Some("textured.vert.spv"))
    };

    let fragment_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("textured.frag.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::FragmentShader, &bytes, Some("textured.frag.spv"))
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
          },
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 2,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 24,
            format: Format::RG32Float
          },
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 3,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 32,
            format: Format::RG32Float
          },
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 4,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 40,
            format: Format::R32Float
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
    };
    let pipeline = device.create_graphics_pipeline(&pipeline_info, &RenderPassInfo {
      attachments: vec![
        AttachmentInfo {
          format: swapchain.format(),
          samples: swapchain.sample_count(),
          load_op: LoadOp::DontCare,
          store_op: StoreOp::DontCare,
          stencil_load_op: LoadOp::DontCare,
          stencil_store_op: StoreOp::DontCare,
        },
        AttachmentInfo {
          format: Format::D24S8,
          samples: SampleCount::Samples1,
          load_op: LoadOp::DontCare,
          store_op: StoreOp::DontCare,
          stencil_load_op: LoadOp::DontCare,
          stencil_store_op: StoreOp::DontCare,
        }
      ],
      subpasses: vec![
        SubpassInfo {
          input_attachments: vec![],
          output_color_attachments: vec![
            OutputAttachmentRef {
              index: 0,
              resolve_attachment_index: None
            }
          ],
          depth_stencil_attachment: Some(DepthStencilAttachmentRef {
            index: 1,
            read_only: true,
          }),
        }
      ]
    }, 0);

    Self {
      depth_buffer: dsv,
      swapchain: swapchain.clone(),
      pipeline
    }
  }


  pub(super) fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    device: &Arc<B::Device>,
    scene: &RendererScene<B>,
    view: &View) -> Arc<B::Semaphore> {

    let semaphore = device.create_semaphore();
    let backbuffer = self.swapchain.prepare_back_buffer(&semaphore).unwrap();
    cmd_buffer.begin_render_pass_1(&RenderPassBeginInfo {
      attachments: &[
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&backbuffer),
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
          output_color_attachments: vec![OutputAttachmentRef {
            index: 0,
            resolve_attachment_index: None
          }],
          depth_stencil_attachment: Some(DepthStencilAttachmentRef {
            index: 1,
            read_only: false
          }),
        }
      ],
    }, RenderpassRecordingMode::Commands);

    cmd_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));

    let drawables = scene.static_drawables();
    let parts = &view.drawable_parts;
    for part in parts {
      let drawable = &drawables[part.drawable_index];
      let model = &drawable.model;
      let mesh = model.mesh();
      let materials = model.materials();
      let range = &mesh.parts[part.part_index];
      let _material = &materials[part.part_index];
      cmd_buffer.set_vertex_buffer(&mesh.vertices);
      if let Some(indices) = mesh.indices.as_ref() {
        cmd_buffer.set_index_buffer(indices);
        cmd_buffer.draw_indexed(1, 0, range.count, range.start, 0);
      } else {
        cmd_buffer.draw(range.count, range.start);
      }
    }
    cmd_buffer.end_render_pass();
    return semaphore;
  }
}