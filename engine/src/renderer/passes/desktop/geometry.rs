use nalgebra::Vector2;
use sourcerenderer_core::{Matrix4, graphics::{AddressMode, AttachmentBlendInfo, Backend as GraphicsBackend, BindingFrequency, BlendInfo, BufferUsage, CommandBuffer, CompareFunc, CullMode, DepthStencil, DepthStencilInfo, Device, FillMode, Filter, Format, FrontFace, GraphicsPipelineInfo, GraphicsSubpassInfo, InputAssemblerElement, InputRate, InputUsage, LogicOp, PassInfo, PassInput, PassType, PipelineBinding, PipelineStage, PrimitiveType, RasterizerInfo, RenderPassCallbacks, RenderPassTextureExtent, SampleCount, SamplerInfo, Scissor, ShaderInputElement, ShaderType, StencilInfo, SubpassOutput, VertexLayoutInfo, Viewport}};
use std::sync::Arc;
use crate::renderer::{drawable::View, passes::desktop::{light_binning::OUTPUT_LIGHT_BITMASKS}, renderer_scene::RendererScene};
use sourcerenderer_core::{Platform, Vec2, Vec2I, Vec2UI};
use crate::renderer::passes::desktop::taa::scaled_halton_point;
use std::path::Path;
use std::io::Read;
use crate::renderer::passes::late_latching::OUTPUT_CAMERA as LATE_LATCHING_CAMERA;
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::atomic_refcell::AtomicRefCell;
use sourcerenderer_core::platform::io::IO;
use rayon::prelude::*;

const PASS_NAME: &str = "Geometry";
const OUTPUT_DS: &str = "DS";
pub const OUTPUT_IMAGE: &str = "OutputImage";

pub(crate) fn build_pass_template<B: GraphicsBackend>() -> PassInfo {
  PassInfo {
    name: PASS_NAME.to_string(),
    pass_type: PassType::Graphics {
      subpasses: vec![
        GraphicsSubpassInfo {
          outputs: vec![
            SubpassOutput::RenderTarget {
            name: OUTPUT_IMAGE.to_string(),
            format: Format::RGBA8,
            samples: SampleCount::Samples1,
            extent: RenderPassTextureExtent::RelativeToSwapchain {
              width: 1f32,
              height: 1f32
            },
            depth: 1,
            levels: 1,
            external: false,
            clear: true
          }],
          inputs: vec![
            PassInput {
              name: LATE_LATCHING_CAMERA.to_string(),
              usage: InputUsage::Storage,
              is_history: false,
              stage: PipelineStage::GraphicsShaders
            },
            /*PassInput {
              name: super::clustering::OUTPUT_CLUSTERS.to_string(),
              usage: InputUsage::Storage,
              is_history: false,
              stage: PipelineStage::FragmentShader
            },*/
            PassInput {
              name: super::light_binning::OUTPUT_LIGHT_BITMASKS.to_string(),
              usage: InputUsage::Storage,
              is_history: false,
              stage: PipelineStage::FragmentShader
            }
          ],
          depth_stencil: DepthStencil::Input {
            name: super::prepass::OUTPUT_DS.to_string(),
            is_history: false
          }
        }
      ],
    }
  }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FrameData {
  swapchain_transform: Matrix4,
  halton_point: Vec2,
  z_near: f32,
  z_far: f32,
  rt_size: Vector2::<u32>,
  cluster_z_bias: f32,
  cluster_z_scale: f32,
  cluster_count: nalgebra::Vector3::<u32>,
  point_light_count: u32
}

pub(in super::super::super) fn build_pass<P: Platform>(
  device: &Arc<<P::GraphicsBackend as GraphicsBackend>::Device>,
  graph_template: &Arc<<P::GraphicsBackend as GraphicsBackend>::RenderGraphTemplate>,
  view: &Arc<AtomicRefCell<View>>,
  scene: &Arc<AtomicRefCell<RendererScene<P::GraphicsBackend>>>,
  lightmap: &Arc<RendererTexture<P::GraphicsBackend>>) -> (String, RenderPassCallbacks<P::GraphicsBackend>) {

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
  let pipeline = device.create_graphics_pipeline(&pipeline_info, &graph_template, PASS_NAME, 0);

  let sampler = device.create_sampler(&SamplerInfo {
    mag_filter: Filter::Linear,
    min_filter: Filter::Linear,
    mip_filter: Filter::Linear,
    address_mode_u: AddressMode::Repeat,
    address_mode_v: AddressMode::Repeat,
    address_mode_w: AddressMode::Repeat,
    mip_bias: 0.0,
    max_anisotropy: 0.0,
    compare_op: None,
    min_lod: 0.0,
    max_lod: 1.0,
  });

  let c_scene = scene.clone();
  let c_lightmap = lightmap.clone();
  let c_view = view.clone();

  (PASS_NAME.to_string(), RenderPassCallbacks::InternallyThreaded(
    vec![
      Arc::new(move |command_buffer_provider, graph_resources, frame_counter| {
        let scene = c_scene.borrow();
        let static_drawables = scene.static_drawables();
        let view_ref = c_view.borrow();
        const CHUNK_SIZE: usize = 128;
        let chunks = view_ref.drawable_parts.par_chunks(CHUNK_SIZE);
        chunks.map(|chunk| {
          let mut command_buffer = command_buffer_provider.get_inner_command_buffer();
          let dimensions = graph_resources.texture_dimensions(OUTPUT_IMAGE).unwrap();

          let cluster_count = nalgebra::Vector3::<u32>::new(16, 9, 24);
          let near = view_ref.near_plane;
          let far = view_ref.far_plane;
          let cluster_z_scale = (cluster_count.z as f32) / (far / near).log2();
          let cluster_z_bias = -(cluster_count.z as f32) * (near).log2() / (far / near).log2();
          let per_frame = FrameData {
            swapchain_transform: *graph_resources.swapchain_transform(),
            halton_point: scaled_halton_point(dimensions.width, dimensions.height, (frame_counter % 8) as u32),
            z_near: view_ref.near_plane,
            z_far: view_ref.far_plane,
            rt_size: Vector2::<u32>::new(dimensions.width, dimensions.height),
            cluster_z_bias,
            cluster_z_scale,
            cluster_count,
            point_light_count: scene.point_lights().len() as u32
          };
          let transform_constant_buffer = command_buffer.upload_dynamic_data(&[per_frame], BufferUsage::CONSTANT);
          command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 3, &transform_constant_buffer);

          command_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
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

          let point_light_buffer = command_buffer.upload_dynamic_data(scene.point_lights(), BufferUsage::STORAGE);

          //command_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 4, graph_resources.get_buffer(OUTPUT_CLUSTERS, false).expect("Failed to get graph resource"));
          command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, graph_resources.get_buffer(LATE_LATCHING_CAMERA, false).expect("Failed to get graph resource"));
          command_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 1, &point_light_buffer);
          command_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 2, graph_resources.get_buffer(OUTPUT_LIGHT_BITMASKS, false).expect("Failed to get graph resource"));
          for part in chunk.into_iter() {
            let drawable = &static_drawables[part.drawable_index];

            /*let model_constant_buffer = command_buffer.upload_dynamic_data(&[drawable.transform], BufferUsage::CONSTANT);
            command_buffer.bind_uniform_buffer(BindingFrequency::PerDraw, 0, &model_constant_buffer);*/
            command_buffer.upload_dynamic_data_inline(&[drawable.transform], ShaderType::VertexShader);

            let model = &drawable.model;
            let mesh = &model.mesh;

            command_buffer.set_vertex_buffer(&mesh.vertices);
            if mesh.indices.is_some() {
              command_buffer.set_index_buffer(mesh.indices.as_ref().unwrap());
            }

            let range = &mesh.parts[part.part_index];
            let material = &model.materials[part.part_index];
            let texture = material.albedo.borrow();
            let albedo_view = texture.view.borrow();
            command_buffer.bind_texture_view(BindingFrequency::PerMaterial, 0, &albedo_view, &sampler);

            let lightmap_ref = c_lightmap.view.borrow();
            command_buffer.bind_texture_view(BindingFrequency::PerMaterial, 1, &lightmap_ref, &sampler);
            command_buffer.finish_binding();

            if mesh.indices.is_some() {
              command_buffer.draw_indexed(1, 0, range.count, range.start, 0);
            } else {
              command_buffer.draw(range.count, range.start);
            }
          }
          command_buffer.finish()
        }).collect()
      })
    ]))
}
