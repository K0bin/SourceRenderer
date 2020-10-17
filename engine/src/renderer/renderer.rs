use std::sync::{Arc, Mutex};
use std::fs::File;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use std::io::Read as IORead;

use crossbeam_channel::{Sender, bounded, Receiver, unbounded};

use nalgebra::Transform;

use sourcerenderer_core::platform::{Platform, Window, WindowState};
use sourcerenderer_core::graphics::{Instance, Adapter, Device, Backend, ShaderType, PipelineInfo, VertexLayoutInfo, InputAssemblerElement, InputRate, ShaderInputElement, Format, RasterizerInfo, FillMode, CullMode, FrontFace, SampleCount, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, BufferUsage, CommandBuffer, Viewport, Scissor, BindingFrequency, Swapchain, RenderGraphTemplateInfo, GraphicsSubpassInfo, PassType, RenderPassCallback, PipelineBinding};
use sourcerenderer_core::graphics::{BACK_BUFFER_ATTACHMENT_NAME, RenderGraphInfo, RenderGraph, LoadAction, StoreAction, PassInfo, OutputTextureAttachmentReference};
use sourcerenderer_core::{Vec2, Vec2I, Vec2UI, Matrix4};

use crate::asset::AssetKey;
use crate::asset::AssetManager;
use crate::renderer::renderable::{Renderables, StaticModelRenderable, Renderable, RenderableType};

use async_std::task;
use sourcerenderer_core::job::{JobScheduler};
use std::sync::atomic::{Ordering, AtomicUsize};
use sourcerenderer_vulkan::VkSwapchain;
use crate::renderer::command::RendererCommand;
use legion::{World, Resources, Schedule, Entity};
use legion::systems::{Builder as SystemBuilder, Builder};

pub struct Renderer<P: Platform> {
  sender: Sender<RendererCommand>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  window_state: Mutex<WindowState>,
  queued_frames_counter: AtomicUsize,
  max_prequeued_frames: usize
}

impl<P: Platform> Renderer<P> {
  fn new(sender: Sender<RendererCommand>, device: &Arc<<P::GraphicsBackend as Backend>::Device>, window: &P::Window) -> Self {
    Self {
      sender,
      device: device.clone(),
      window_state: Mutex::new(window.state()),
      queued_frames_counter: AtomicUsize::new(0),
      max_prequeued_frames: 1
    }
  }

  pub fn run(window: &P::Window,
             device: &Arc<<P::GraphicsBackend as Backend>::Device>,
             swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
             asset_manager: &Arc<AssetManager<P>>) -> Arc<Renderer<P>> {
    let (sender, receiver) = unbounded::<RendererCommand>();
    let renderer = Arc::new(Renderer::new(sender.clone(), device, window));
    let mut internal = RendererInternal::new(&renderer, &device, &swapchain, asset_manager, sender, receiver);

    std::thread::spawn(move || {
      'render_loop: loop {
        internal.render();
      }
    });
    renderer
  }

  pub fn set_window_state(&self, window_state: WindowState) {
    let mut guard = self.window_state.lock().unwrap();
    std::mem::replace(&mut *guard, window_state);
  }

  pub fn install(self: &Arc<Renderer<P>>, world: &mut World, resources: &mut Resources, systems: &mut Builder) {
    crate::renderer::ecs::install(systems, self);
  }

  pub fn register_static_renderable(&self, renderable: Renderable) {
    let result = self.sender.send(RendererCommand::Register(renderable));
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn unregister_static_renderable(&self, entity: Entity) {
    let result = self.sender.send(RendererCommand::UnregisterStatic(entity));
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn update_camera(&self, camera_matrix: Matrix4) {
    let result = self.sender.send(RendererCommand::UpdateCamera(camera_matrix));
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn update_transform(&self, entity: Entity, transform: Matrix4) {
    let result = self.sender.send(RendererCommand::UpdateTransform(entity, transform));
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn end_frame(&self) {
    self.queued_frames_counter.fetch_add(1, Ordering::SeqCst);
    let result = self.sender.send(RendererCommand::EndFrame);
    if result.is_err() {
      panic!("Sending message to render thread failed");
    }
  }

  pub fn is_saturated(&self) -> bool {
    self.queued_frames_counter.load(Ordering::SeqCst) > self.max_prequeued_frames
  }
}


pub struct RendererInternal<P: Platform> {
  renderer: Arc<Renderer<P>>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  graph: <P::GraphicsBackend as Backend>::RenderGraph,
  swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
  renderables: Arc<Mutex<Renderables>>,
  sender: Sender<RendererCommand>,
  receiver: Receiver<RendererCommand>
}

impl<P: Platform> RendererInternal<P> {
  fn new(
    renderer: &Arc<Renderer<P>>,
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    sender: Sender<RendererCommand>,
    receiver: Receiver<RendererCommand>) -> Self {

    let renderables = Arc::new(Mutex::new(Renderables::default()));
    let graph = RendererInternal::<P>::build_graph(device, swapchain, asset_manager, &renderables);
    Self {
      renderer: renderer.clone(),
      device: device.clone(),
      graph,
      swapchain: swapchain.clone(),
      renderables,
      sender,
      receiver
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
              inputs: Vec::new()
            }
          ],
        }
      }
    ];

    let graph_template = device.create_render_graph_template(&RenderGraphTemplateInfo {
      attachments: HashMap::new(),
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
    let mut callbacks : HashMap<String, Vec<Arc<RenderPassCallback<P::GraphicsBackend>>>>= HashMap::new();
    callbacks.insert("Geometry".to_string(), vec![
      Arc::new(move |command_buffer| {
        let state = c_renderables.lock().unwrap();

        let assets_lookup = c_asset_manager.lookup_graphics();

        let camera_constant_buffer = command_buffer.upload_dynamic_data(state.camera, BufferUsage::CONSTANT);
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
          let model_constant_buffer = command_buffer.upload_dynamic_data(renderable.transform, BufferUsage::CONSTANT);
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

  fn render(&mut self) {
    let state = {
      let state_guard = self.renderer.window_state.lock().unwrap();
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
      }
    }

    {
      let mut guard = self.renderables.lock().unwrap();

      let mut message_opt: Option<RendererCommand> = None;

      let message_result = self.receiver.recv();
      if message_result.is_err() {
        panic!("Rendering channel closed");
      } else {
        message_opt = message_result.ok();
      }

      loop {
        let message = std::mem::replace(&mut message_opt, None).unwrap();
        match message {
          RendererCommand::EndFrame => {
            let frame = self.renderer.queued_frames_counter.fetch_sub(1, Ordering::SeqCst);
            break;
          },

          RendererCommand::UpdateCamera(camera_mat) => {
            guard.old_camera = guard.camera.clone();
            guard.camera = camera_mat;
          },

          RendererCommand::UpdateTransform(entity, transform_mat) => {
            let mut element = guard.elements.iter_mut()
              .find(|r| r.entity == entity);
            // TODO optimize

            if let Some(element) = element {
              element.old_transform = element.transform;
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

        let message_result = self.receiver.recv();
        if message_result.is_err() {
          panic!("Rendering channel closed");
        } else {
          message_opt = message_result.ok();
        }
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


