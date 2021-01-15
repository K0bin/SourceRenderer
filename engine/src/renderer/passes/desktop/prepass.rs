use sourcerenderer_core::graphics::{Backend as GraphicsBackend, PassInfo, Format, SampleCount, RenderPassTextureExtent, LoadAction, StoreAction, SubpassOutput, GraphicsSubpassInfo, PassInput, PassType, GraphicsPipelineInfo, VertexLayoutInfo, InputAssemblerElement, InputRate, ShaderInputElement, RasterizerInfo, FillMode, CullMode, FrontFace, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, Device, RenderPassCallbacks, PipelineBinding, BufferUsage, Viewport, Scissor, BindingFrequency, CommandBuffer, ShaderType, PrimitiveType, DepthStencil};
use std::sync::{Arc, Mutex};
use crate::renderer::drawable::View;
use sourcerenderer_core::{Matrix4, Platform, Vec2, Vec2I, Vec2UI};
use crate::renderer::DrawableType;
use crate::renderer::drawable::RDrawableType;
use crate::asset::AssetManager;
use std::fs::File;
use std::path::Path;
use std::io::Read;
use crate::renderer::passes::late_latching::OUTPUT_CAMERA as LATE_LATCHING_CAMERA;
use crate::renderer::renderer_assets::*;

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
              is_local: false
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

pub(crate) fn build_pass<P: Platform>(device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>, graph_template: &Arc<<P::GraphicsBackend as GraphicsBackend>::RenderGraphTemplate>, view: &Arc<Mutex<View<P::GraphicsBackend>>>) -> (String, RenderPassCallbacks<P::GraphicsBackend>) {
  let vertex_shader = {
    let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("shaders")).join(Path::new("prepass.vert.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::VertexShader, &bytes, Some("textured.vert.spv"))
  };

  let fragment_shader = {
    let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("shaders")).join(Path::new("prepass.frag.spv"))).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    device.create_shader(ShaderType::FragmentShader, &bytes, Some("textured.frag.spv"))
  };

  let pipeline_info: GraphicsPipelineInfo<P::GraphicsBackend> = GraphicsPipelineInfo {
    vs: vertex_shader.clone(),
    fs: Some(fragment_shader.clone()),
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
          format: Format::RGB32Float
        },
        ShaderInputElement {
          input_assembler_binding: 0,
          location_vk_mtl: 3,
          semantic_name_d3d: String::from(""),
          semantic_index_d3d: 0,
          offset: 36,
          format: Format::RG32Float
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

  let c_view = view.clone();

  (PASS_NAME.to_string(), RenderPassCallbacks::Regular(
    vec![
      Arc::new(move |command_buffer_a, graph_resources| {
        let command_buffer = command_buffer_a as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer;
        let state = c_view.lock().unwrap();

        let camera_constant_buffer: Arc<<P::GraphicsBackend as GraphicsBackend>::Buffer> = (command_buffer as &mut <P::GraphicsBackend as GraphicsBackend>::CommandBuffer).upload_dynamic_data::<PrepassCameraCB>(PrepassCameraCB {
            view_projection: state.camera_matrix,
            old_view_projection: state.old_camera_matrix
          }, BufferUsage::CONSTANT);
        command_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
        command_buffer.set_viewports(&[Viewport {
          position: Vec2::new(0.0f32, 0.0f32),
          extent: Vec2::new(1280.0f32, 720.0f32),
          min_depth: 0.0f32,
          max_depth: 1.0f32
        }]);
        command_buffer.set_scissors(&[Scissor {
          position: Vec2I::new(0, 0),
          extent: Vec2UI::new(9999, 9999),
        }]);

        //command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, graph_resources.get_buffer(LATE_LATCHING_CAMERA).expect("Failed to get graph resource"));
        command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, &camera_constant_buffer);
        for renderable in &state.elements {
          let model_constant_buffer = command_buffer.upload_dynamic_data(PrepassModelCB {
            model: renderable.transform,
            old_model: renderable.old_transform
          }, BufferUsage::CONSTANT);
          command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, &model_constant_buffer);
          command_buffer.finish_binding();

          if let RDrawableType::Static {
            model, ..
          } = &renderable.drawable_type {
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
