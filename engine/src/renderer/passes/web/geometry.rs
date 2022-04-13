use std::{io::Read, path::Path, sync::Arc};

use sourcerenderer_core::{Platform, Vec2, Vec2I, Vec2UI, graphics::{AddressMode, AttachmentBlendInfo, AttachmentInfo, Backend, Barrier, BindingFrequency, BlendInfo, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Filter, Format, FrontFace, GraphicsPipelineInfo, InputAssemblerElement, InputRate, LoadOp, LogicOp, OutputAttachmentRef, PipelineBinding, PrimitiveType, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderPassInfo, RenderpassRecordingMode, SampleCount, SamplerInfo, Scissor, ShaderInputElement, ShaderType, StencilInfo, StoreOp, SubpassInfo, Swapchain, Texture, TextureDepthStencilViewInfo, TextureInfo, TextureRenderTargetView, TextureUsage, VertexLayoutInfo, Viewport, BarrierSync, BarrierAccess, TextureLayout, IndexFormat, WHOLE_BUFFER}, platform::io::IO};

use crate::{renderer::{drawable::View, renderer_assets::RendererMaterialValue, renderer_scene::RendererScene}};

pub struct GeometryPass<B: Backend> {
  depth_buffer: Arc<B::TextureDepthStencilView>,
  pipeline: Arc<B::GraphicsPipeline>,
  sampler: Arc<B::Sampler>
}

impl<B: Backend> GeometryPass<B> {

  pub(super) fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let ds = device.create_texture(&TextureInfo {
      format: Format::D32,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::DEPTH_STENCIL,
    }, None);

    let dsv = device.create_depth_stencil_view(&ds, &TextureDepthStencilViewInfo::default(), None);

    let shader_file_extension = if cfg!(target_family = "wasm") {
      "glsl"
    } else {
      "spv"
    };

    let vertex_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new(&format!("web_geometry.vert.{}", shader_file_extension)))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes, Some("web_geometry.vert.glsl"))
    };

    let fragment_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new(&format!("web_geometry.frag.{}", shader_file_extension)))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::FragmentShader, &bytes, Some("web_geometry.frag.glsl"))
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
          format: ds.info().format,
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

    let sampler = device.create_sampler(&SamplerInfo {
      mag_filter: Filter::Linear,
      min_filter: Filter::Linear,
      mip_filter: Filter::Linear,
      address_mode_u: AddressMode::ClampToEdge,
      address_mode_v: AddressMode::ClampToEdge,
      address_mode_w: AddressMode::ClampToEdge,
      mip_bias: 0.0f32,
      max_anisotropy: 0.0f32,
      compare_op: None,
      min_lod: 0.0f32,
      max_lod: None,
    });

    init_cmd_buffer.barrier(&[Barrier::TextureBarrier {
      old_sync: BarrierSync::empty(),
      new_sync: BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
      old_access: BarrierAccess::empty(),
      new_access: BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE,
      old_layout: TextureLayout::Undefined,
      new_layout: TextureLayout::DepthStencilReadWrite,
      texture: &ds,
    }]);

    Self {
      depth_buffer: dsv,
      pipeline,
      sampler
    }
  }


  pub(super) fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    device: &Arc<B::Device>,
    scene: &RendererScene<B>,
    view: &View,
    camera_buffer: &Arc<B::Buffer>,
    backbuffer: &Arc<B::TextureRenderTargetView>) {

    cmd_buffer.barrier(&[Barrier::TextureBarrier {
      old_sync: BarrierSync::empty(),
      new_sync: BarrierSync::RENDER_TARGET,
      old_access: BarrierAccess::empty(),
      new_access: BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ,
      old_layout: TextureLayout::Undefined,
      new_layout: TextureLayout::RenderTarget,
      texture: backbuffer.texture(),
    }]);

    cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
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

    let rtv_info = backbuffer.texture().info();

    cmd_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));
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

    //let camera_buffer = cmd_buffer.upload_dynamic_data(&[view.proj_matrix * view.view_matrix], BufferUsage::CONSTANT);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, camera_buffer, 0, WHOLE_BUFFER);

    let drawables = scene.static_drawables();
    let parts = &view.drawable_parts;
    for part in parts {
      let drawable = &drawables[part.drawable_index];
      cmd_buffer.upload_dynamic_data_inline(&[drawable.transform], ShaderType::VertexShader);
      let model = &drawable.model;
      let mesh = model.mesh();
      let materials = model.materials();
      let range = &mesh.parts[part.part_index];
      let material = &materials[part.part_index];
      let albedo_value = material.get("albedo").unwrap();
      match albedo_value {
        RendererMaterialValue::Texture(texture) => {
          let albedo_view = &texture.view;
          cmd_buffer.bind_texture_view(BindingFrequency::PerMaterial, 0, albedo_view, &self.sampler);
        },
        _ => unimplemented!()
      }
      cmd_buffer.finish_binding();

      cmd_buffer.set_vertex_buffer(mesh.vertices.buffer(), mesh.vertices.offset() as usize);
      if let Some(indices) = mesh.indices.as_ref() {
        cmd_buffer.set_index_buffer(indices.buffer(), indices.offset() as usize, IndexFormat::U32);
        cmd_buffer.draw_indexed(1, 0, range.count, range.start, 0);
      } else {
        cmd_buffer.draw(range.count, range.start);
      }
    }
    cmd_buffer.end_render_pass();

    cmd_buffer.barrier(&[Barrier::TextureBarrier {
      old_sync: BarrierSync::RENDER_TARGET,
      new_sync: BarrierSync::empty(),
      old_access: BarrierAccess::RENDER_TARGET_WRITE,
      new_access: BarrierAccess::empty(),
      old_layout: TextureLayout::RenderTarget,
      new_layout: TextureLayout::Present,
      texture: backbuffer.texture(),
    }]);
  }
}