use sourcerenderer_core::graphics::{Backend as GraphicsBackend, PassInfo, Format, SampleCount, SubpassOutput, GraphicsSubpassInfo, PassInput, PassType, GraphicsPipelineInfo, VertexLayoutInfo, InputAssemblerElement, InputRate, ShaderInputElement, RasterizerInfo, FillMode, CullMode, FrontFace, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, Device, RenderPassCallbacks, PipelineBinding, BufferUsage, Viewport, Scissor, BindingFrequency, CommandBuffer, ShaderType, PrimitiveType, DepthStencil, PipelineStage, StoreAction, LoadAction, RenderPassTextureExtent};
use std::sync::Arc;
use crate::renderer::drawable::{View, RDrawable};
use sourcerenderer_core::{Matrix4, Platform, Vec2, Vec2I, Vec2UI};
use crate::renderer::drawable::RDrawableType;
use std::path::Path;
use std::io::Read;
use crate::renderer::passes::late_latching::OUTPUT_CAMERA as LATE_LATCHING_CAMERA;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::platform::io::IO;

pub(super) const PASS_NAME: &str = "Prepass";
pub(super) const OUTPUT_DS: &str = "PrepassDS";

pub(super) const OUTPUT_NORMALS: &str = "PrepassNormals";
pub(super) const OUTPUT_MOTION: &str = "PrepassMotion";

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

pub(crate) fn build_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: PASS_NAME.to_string(),
    pass_type: PassType::Graphics {
      subpasses: vec![
        GraphicsSubpassInfo {
          outputs: vec![
            SubpassOutput::RenderTarget {
              name: OUTPUT_NORMALS.to_string(),
              format: Format::RGBA32Float,
              samples: SampleCount::Samples1,
              extent: RenderPassTextureExtent::RelativeToSwapchain {
                width: 1f32, height: 1f32
              },
              depth: 1,
              levels: 1,
              external: false,
              load_action: LoadAction::Clear,
              store_action: StoreAction::Store
            },
            SubpassOutput::RenderTarget {
              name: OUTPUT_MOTION.to_string(),
              format: Format::RGBA32Float,
              samples: SampleCount::Samples1,
              extent: RenderPassTextureExtent::RelativeToSwapchain {
                width: 1f32, height: 1f32
              },
              depth: 1,
              levels: 1,
              external: false,
              load_action: LoadAction::Clear,
              store_action: StoreAction::Store
            },
          ],
          inputs: vec![
            PassInput {
              name: LATE_LATCHING_CAMERA.to_string(),
              is_local: false,
              is_history: false,
              stage: PipelineStage::VertexShader
            },
            PassInput {
              name: LATE_LATCHING_CAMERA.to_string(),
              is_local: false,
              is_history: true,
              stage: PipelineStage::VertexShader
            }
          ],
          depth_stencil: DepthStencil::Output {
            name: OUTPUT_DS.to_string(),
            format: Format::D24S8,
            samples: SampleCount::Samples1,
            extent: RenderPassTextureExtent::RelativeToSwapchain {
              width: 1f32, height: 1f32
            },
            depth_load_action: LoadAction::Clear,
            depth_store_action: StoreAction::Store,
            stencil_load_action: LoadAction::DontCare,
            stencil_store_action: StoreAction::DontCare
          }
        }
      ],
    }
  }
}

pub(in super::super::super) fn build_pass<P: Platform>(device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>, graph_template: &Arc<<P::GraphicsBackend as GraphicsBackend>::RenderGraphTemplate>, _view: &Arc<AtomicRefCell<View>>, drawables: &Arc<AtomicRefCell<Vec<RDrawable<P::GraphicsBackend>>>>) -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  let vertex_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("prepass.vert.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::VertexShader, &bytes, Some("textured.vert.spv"))
  };

  let fragment_shader = {
    let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("prepass.frag.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::FragmentShader, &bytes, Some("textured.frag.spv"))
  };

  let pipeline_info: GraphicsPipelineInfo<P::GraphicsBackend> = GraphicsPipelineInfo {
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
  let pipeline = device.create_graphics_pipeline(&pipeline_info, &graph_template, PASS_NAME, 0);

  let c_drawables = drawables.clone();

  (PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        let drawables = c_drawables.borrow();

        let transform_constant_buffer = command_buffer.upload_dynamic_data(&[*graph_resources.swapchain_transform()], BufferUsage::CONSTANT);
        command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 2, &transform_constant_buffer);

        command_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
        let dimensions = graph_resources.texture_dimensions(OUTPUT_DS).unwrap();
        command_buffer.set_viewports(&[Viewport {
          position: Vec2::new(0.0f32, 0.0f32),
          extent: Vec2::new(dimensions.width as f32, dimensions.height as f32),
          min_depth: 0.0f32,
          max_depth: 1.0f32
        }]);
        command_buffer.set_scissors(&[Scissor {
          position: Vec2I::new(0, 0),
          extent: Vec2UI::new(9999, 9999),
        }]);

        command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, graph_resources.get_buffer(LATE_LATCHING_CAMERA, false).expect("Failed to get graph resource"));
        command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 1, graph_resources.get_buffer(LATE_LATCHING_CAMERA, true).expect("Failed to get graph resource"));
        //command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, &camera_constant_buffer);
        for drawable in drawables.iter() {
          let model_constant_buffer = command_buffer.upload_dynamic_data(&[PrepassModelCB {
            model: drawable.transform,
            old_model: drawable.old_transform
          }], BufferUsage::CONSTANT);
          command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, &model_constant_buffer);
          command_buffer.finish_binding();

          if let RDrawableType::Static {
            model, ..
          } = &drawable.drawable_type {
            let mesh = &model.mesh;

            command_buffer.set_vertex_buffer(&mesh.vertices);
            if mesh.indices.is_some() {
              command_buffer.set_index_buffer(mesh.indices.as_ref().unwrap());
            }

            for i in 0..mesh.parts.len() {
              let range = &mesh.parts[i];

              if mesh.indices.is_some() {
                command_buffer.draw_indexed(1, 0, range.count, range.start, 0);
              } else {
                command_buffer.draw(range.count, range.start);
              }
            }
          }
        }
      })
    ]))
}
