use std::sync::{Arc, Mutex};
use crate::renderer::Renderer;
use crossbeam_channel::{Sender, Receiver, TryRecvError};
use crate::renderer::command::RendererCommand;
use std::time::SystemTime;
use crate::asset::AssetManager;
use sourcerenderer_core::{Platform, Matrix4, Vec2, Vec3, Quaternion, Vec2UI, Vec2I};
use sourcerenderer_core::graphics::{Backend, ShaderType, AttachmentInfo, SampleCount, AttachmentSizeClass, Format, PassInfo, PassType, GraphicsSubpassInfo, OutputTextureAttachmentReference, BACK_BUFFER_ATTACHMENT_NAME, LoadAction, StoreAction, Device, RenderGraphTemplateInfo, PipelineInfo, Swapchain, VertexLayoutInfo, InputAssemblerElement, InputRate, ShaderInputElement, FillMode, CullMode, FrontFace, RasterizerInfo, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, RenderPassCallback, BufferUsage, CommandBuffer, PipelineBinding, Viewport, Scissor, BindingFrequency, RenderGraphInfo, RenderGraph};
use std::path::Path;
use std::collections::HashMap;
use std::fs::File;
use crate::renderer::renderable::{RenderableType, Renderables};
use sourcerenderer_core::platform::WindowState;
use nalgebra::Matrix3;
use std::sync::atomic::Ordering;
use std::io::Read;

pub(super) struct RendererInternal<P: Platform> {
  renderer: Arc<Renderer<P>>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  graph: <P::GraphicsBackend as Backend>::RenderGraph,
  swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
  renderables: Arc<Mutex<Renderables>>,
  sender: Sender<RendererCommand>,
  receiver: Receiver<RendererCommand>,
  simulation_tick_rate: u32,
  last_tick: SystemTime,
}

impl<P: Platform> RendererInternal<P> {
  pub(super) fn new(
    renderer: &Arc<Renderer<P>>,
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    sender: Sender<RendererCommand>,
    receiver: Receiver<RendererCommand>,
    simulation_tick_rate: u32) -> Self {

    let renderables = Arc::new(Mutex::new(Renderables::default()));
    let graph = RendererInternal::<P>::build_graph(device, swapchain, asset_manager, &renderables);
    Self {
      renderer: renderer.clone(),
      device: device.clone(),
      graph,
      swapchain: swapchain.clone(),
      renderables,
      sender,
      receiver,
      simulation_tick_rate,
      last_tick: SystemTime::now()
    }
  }

  fn build_graph(
    device: &<P::GraphicsBackend as Backend>::Device,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    renderables: &Arc<Mutex<Renderables>>)
    -> <P::GraphicsBackend as Backend>::RenderGraph {
    let vertex_shader = {
      let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("shaders")).join(Path::new("textured.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes, Some("textured.vert.spv"))
    };

    let fragment_shader = {
      let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("shaders")).join(Path::new("textured.frag.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::FragmentShader, &bytes, Some("textured.frag.spv"))
    };

    let mut attachments: HashMap<String, AttachmentInfo> = HashMap::new();
    attachments.insert("DS".to_string(), AttachmentInfo::Texture {
      format: Format::D24S8,
      samples: SampleCount::Samples1,
      size_class: AttachmentSizeClass::RelativeToSwapchain,
      width: 1.0,
      height: 1.0,
      levels: 1,
      external: false
    });

    let mut passes: Vec<PassInfo> = vec![
      PassInfo {
        name: "Geometry".to_string(),
        pass_type: PassType::Graphics {
          subpasses: vec![
            GraphicsSubpassInfo {
              outputs: vec![OutputTextureAttachmentReference {
                name: BACK_BUFFER_ATTACHMENT_NAME.to_owned(),
                load_action: LoadAction::Clear,
                store_action: StoreAction::Store
              }],
              inputs: Vec::new(),
              depth_stencil: Some(OutputTextureAttachmentReference {
                name: "DS".to_string(),
                store_action: StoreAction::DontCare,
                load_action: LoadAction::Clear
              })
            }
          ],
        }
      }
    ];

    let graph_template = device.create_render_graph_template(&RenderGraphTemplateInfo {
      attachments,
      passes,
      swapchain_sample_count: swapchain.sample_count(),
      swapchain_format: swapchain.format()
    });

    let pipeline_info: PipelineInfo<P::GraphicsBackend> = PipelineInfo {
      vs: vertex_shader.clone(),
      fs: Some(fragment_shader.clone()),
      gs: None,
      tcs: None,
      tes: None,
      vertex_layout: VertexLayoutInfo {
        input_assembler: vec![
          InputAssemblerElement {
            binding: 0,
            stride: 32,
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
        depth_test_enabled: false,
        depth_write_enabled: false,
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
          AttachmentBlendInfo::default()
        ]
      }
    };
    let pipeline = device.create_graphics_pipeline(&pipeline_info, &graph_template, "Geometry", 0);

    let c_asset_manager = asset_manager.clone();
    let c_renderables = renderables.clone();
    let mut callbacks: HashMap<String, Vec<Arc<RenderPassCallback<P::GraphicsBackend>>>> = HashMap::new();
    callbacks.insert("Geometry".to_string(), vec![
      Arc::new(move |command_buffer| {
        let state = c_renderables.lock().unwrap();

        let assets_lookup = c_asset_manager.lookup_graphics();

        let camera_constant_buffer = command_buffer.upload_dynamic_data(state.interpolated_camera, BufferUsage::CONSTANT);
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

        command_buffer.bind_buffer(BindingFrequency::PerFrame, 0, &camera_constant_buffer);
        for renderable in &state.elements {
          let model_constant_buffer = command_buffer.upload_dynamic_data(renderable.interpolated_transform, BufferUsage::CONSTANT);
          command_buffer.bind_buffer(BindingFrequency::PerDraw, 0, &model_constant_buffer);

          if let RenderableType::Static(static_renderable) = &renderable.renderable_type {
            let model = assets_lookup.get_model(&static_renderable.model);
            let mesh = assets_lookup.get_mesh(&model.mesh);

            command_buffer.set_vertex_buffer(&mesh.vertices);
            if mesh.indices.is_some() {
              command_buffer.set_index_buffer(mesh.indices.as_ref().unwrap());
            }

            for i in 0..mesh.parts.len() {
              let range = &mesh.parts[i];
              let material_key = &model.materials[i];

              let material = assets_lookup.get_material(material_key);
              let albedo_view = assets_lookup.get_texture(&material.albedo);
              command_buffer.bind_texture_view(BindingFrequency::PerMaterial, 0, &albedo_view);
              command_buffer.finish_binding();

              if mesh.indices.is_some() {
                command_buffer.draw_indexed(1, 0, range.count, range.start, 0);
              } else {
                command_buffer.draw(range.count, range.start);
              }
            }
          }
        }



      })]);
    let graph = device.create_render_graph(&graph_template, &RenderGraphInfo {
      pass_callbacks: callbacks
    }, swapchain);

    graph
  }

  pub(super) fn render(&mut self) {
    let state = {
      let state_guard = self.renderer.window_state().lock().unwrap();
      state_guard.clone()
    };

    let mut swapchain_width = 0u32;
    let mut swapchain_height = 0u32;

    match state {
      WindowState::Minimized => {
        return;
      },
      WindowState::FullScreen {
        width, height
      } => {
        swapchain_width = width;
        swapchain_height = height;
      },
      WindowState::Visible {
        width, height, focussed: _focussed
      } => {
        swapchain_width = width;
        swapchain_height = height;
      },
      WindowState::Exited => {
        return;
      }
    }

    {
      let mut guard = self.renderables.lock().unwrap();

      let message_res = self.receiver.try_recv();
      if let Some(err) = message_res.as_ref().err() {
        if let TryRecvError::Disconnected = err {
          panic!("Rendering channel closed");
        }
      }
      let mut message_opt = message_res.ok();

      while message_opt.is_some() {
        let message = std::mem::replace(&mut message_opt, None).unwrap();
        match message {
          RendererCommand::EndFrame => {
            self.renderer.dec_queued_frames_counter();
            self.last_tick = SystemTime::now();

            for element in &mut guard.elements {
              element.older_transform = element.old_transform;
              element.old_transform = element.transform;
            }
            guard.older_camera = guard.old_camera;
            guard.old_camera = guard.camera;
          },

          RendererCommand::UpdateCamera(camera_mat) => {
            guard.camera = camera_mat;
          },

          RendererCommand::UpdateTransform(entity, transform_mat) => {
            let mut element = guard.elements.iter_mut()
              .find(|r| r.entity == entity);
            // TODO optimize

            if let Some(element) = element {
              element.transform = transform_mat;
            }
          },

          RendererCommand::Register(renderable) => {
            guard.elements.push(renderable);
          },

          RendererCommand::UnregisterStatic(entity) => {
            let index = guard.elements.iter()
              .position(|r| r.entity == entity);

            if let Some(index) = index {
              guard.elements.remove(index);
            }
          },

          _ => {
            println!("Unimplemented RenderCommand");
          }
        }

        let message_res = self.receiver.try_recv();
        if let Some(err) = message_res.as_ref().err() {
          if let TryRecvError::Disconnected = err {
            panic!("Rendering channel closed");
          }
        }
        message_opt = message_res.ok();
      }

      let now = SystemTime::now();
      let delta = now.duration_since(self.last_tick).unwrap().as_millis() as f32;
      let frac = f32::max(0f32, f32::min(1f32, delta / (1000f32 / self.simulation_tick_rate as f32)));

      guard.interpolated_camera = guard.old_camera;
      for element in &mut guard.elements {
        element.interpolated_transform = interpolate_transform_matrix(element.older_transform, element.old_transform, frac);
      }
    }

    let result = self.graph.render();
    if result.is_err() {
      self.device.wait_for_idle();

      let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate(&self.swapchain, swapchain_width, swapchain_height);
      if new_swapchain_result.is_err() {
        return;
      }
      let new_swapchain = new_swapchain_result.unwrap();
      if new_swapchain.format() != self.swapchain.format() || new_swapchain.sample_count() != self.swapchain.sample_count() {
        panic!("Swapchain format or sample count changed. Can not recreate render graph.");
      }

      let new_graph = <P::GraphicsBackend as Backend>::RenderGraph::recreate(&self.graph, &new_swapchain);
      std::mem::replace(&mut self.swapchain, new_swapchain);
      std::mem::replace(&mut self.graph, new_graph);
      self.graph.render();
    }
  }
}

fn deconstruct_transform(transform_mat: Matrix4) -> (Vec3, Quaternion, Vec3) {
  let scale = Vec3::new(transform_mat.column(0).xyz().magnitude(),
                        transform_mat.column(1).xyz().magnitude(),
                        transform_mat.column(2).xyz().magnitude());
  let translation: Vec3 = transform_mat.column(3).xyz();
  let rotation = Quaternion::from_matrix(&Matrix3::<f32>::from_columns(&[
    transform_mat.column(0).xyz() / scale.x,
    transform_mat.column(1).xyz() / scale.y,
    transform_mat.column(2).xyz() / scale.z
  ]));
  (translation, rotation, scale)
}

fn interpolate_transform_matrix(from: Matrix4, to: Matrix4, frac: f32) -> Matrix4 {
  let (from_position, from_rotation, from_scale) = deconstruct_transform(from);
  let (to_position, to_rotation, to_scale) = deconstruct_transform(to);
  let position = from_position.lerp(&to_position, frac);
  let rotation: Quaternion = Quaternion::from_quaternion(from_rotation.lerp(&to_rotation, frac));
  let scale = from_scale.lerp(&to_scale, frac);

  Matrix4::new_translation(&position)
    * Matrix4::new_rotation(rotation.axis_angle().map_or(Vec3::new(0.0f32, 0.0f32, 0.0f32), |(axis, amount)| *axis * amount))
    * Matrix4::new_nonuniform_scaling(&scale)
}
