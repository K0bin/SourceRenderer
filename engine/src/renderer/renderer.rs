use std::sync::{Arc, Mutex};
use std::fs::File;
use std::path::Path;
use std::collections::HashMap;
use std::io::Read as IORead;

use crossbeam_channel::{Sender, bounded, Receiver};

use nalgebra::Matrix4;

use sourcerenderer_core::platform::{Platform, Window, WindowState};
use sourcerenderer_core::graphics::{Instance, Adapter, Device, Backend, ShaderType, PipelineInfo, VertexLayoutInfo, InputAssemblerElement, InputRate, ShaderInputElement, Format, RasterizerInfo, FillMode, CullMode, FrontFace, SampleCount, DepthStencilInfo, CompareFunc, StencilInfo, BlendInfo, LogicOp, AttachmentBlendInfo, BufferUsage, CommandBuffer, Viewport, Scissor, BindingFrequency, Swapchain, RenderGraphTemplateInfo};
use sourcerenderer_core::graphics::{BACK_BUFFER_ATTACHMENT_NAME, RenderGraphInfo, RenderGraph, LoadAction, StoreAction, PassInfo, GraphicsPassInfo, OutputTextureAttachmentReference};
use sourcerenderer_core::{Vec2, Vec2I, Vec2UI};

use crate::asset::AssetKey;
use crate::asset::AssetManager;
use crate::renderer::renderable::{Renderables, StaticModelRenderable, Renderable, TransformedRenderable};

use async_std::task;
use sourcerenderer_core::job::{SystemJob, JobScheduler, JobCounterWait, JobQueue};
use std::sync::atomic::Ordering;
use sourcerenderer_vulkan::VkSwapchain;

pub struct Renderer<P: Platform> {
  sender: Sender<Renderables>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  window_state: Mutex<WindowState>
}

impl<P: Platform> Renderer<P> {
  fn new(sender: Sender<Renderables>, device: &Arc<<P::GraphicsBackend as Backend>::Device>, window: &P::Window) -> Self {
    Self {
      sender,
      device: device.clone(),
      window_state: Mutex::new(window.state())
    }
  }

  pub fn run(job_scheduler: &JobScheduler, window: &P::Window, device: &Arc<<P::GraphicsBackend as Backend>::Device>, swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>, asset_manager: &Arc<AssetManager<P>>) -> Arc<Renderer<P>> {
    let (sender, receiver) = bounded::<Renderables>(1);
    let renderer = Arc::new(Renderer::new(sender, device, window));
    let mut internal = RendererInternal::new(&renderer, &device, &swapchain, asset_manager, receiver);

    job_scheduler.enqueue_system_job(Box::new(move |queue| {
      internal.render(queue)
    }));
    //task::spawn(internal.render_loop());
    renderer
  }

  pub fn render(&self, renderables: Renderables) {
    self.sender.send(renderables);
  }

  pub fn set_window_state(&self, window_state: WindowState) {
    let mut guard = self.window_state.lock().unwrap();
    std::mem::replace(&mut *guard, window_state);
  }
}

struct RendererInternal<P: Platform> {
  renderer: Arc<Renderer<P>>,
  device: Arc<<P::GraphicsBackend as Backend>::Device>,
  graph: <P::GraphicsBackend as Backend>::RenderGraph,
  swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
}

impl<P: Platform> RendererInternal<P> {
  fn new(
    renderer: &Arc<Renderer<P>>,
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    receiver: Receiver<Renderables>) -> Self {
    let graph = RendererInternal::<P>::build_graph(device, swapchain, asset_manager, receiver);
    Self {
      renderer: renderer.clone(),
      device: device.clone(),
      graph,
      swapchain: swapchain.clone()
    }
  }

  fn build_graph(
    device: &<P::GraphicsBackend as Backend>::Device,
    swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    asset_manager: &Arc<AssetManager<P>>,
    receiver: Receiver<Renderables>)
    -> <P::GraphicsBackend as Backend>::RenderGraph {
    let vertex_shader = {
      let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("shaders")).join(Path::new("textured.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes)
    };

    let fragment_shader = {
      let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("shaders")).join(Path::new("textured.frag.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::FragmentShader, &bytes)
    };

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

    let asset_manager_ref = asset_manager.clone();
    let mut passes: Vec<PassInfo> = Vec::new();
    passes.push(PassInfo::Graphics(GraphicsPassInfo {
      outputs: vec![OutputTextureAttachmentReference {
        name: BACK_BUFFER_ATTACHMENT_NAME.to_owned(),
        load_action: LoadAction::Clear,
        store_action: StoreAction::Store
      }],
      inputs: Vec::new()
    }));

    let graph_template = Arc::new(device.create_render_graph_template(&RenderGraphTemplateInfo {
      attachments: HashMap::new(),
      passes,
      swapchain_sample_count: swapchain.sample_count(),
      swapchain_format: swapchain.format()
    }));

    let graph = device.create_render_graph(&graph_template, &RenderGraphInfo {
      pass_callbacks: vec![
        Arc::new(move |command_buffer| {

          let state = receiver.recv().unwrap(); // async
          let assets_lookup = asset_manager_ref.lookup_graphics();

          let camera_constant_buffer = command_buffer.upload_dynamic_data(state.camera, BufferUsage::CONSTANT);
          command_buffer.set_pipeline(&pipeline_info);
          command_buffer.set_viewports(&[Viewport {
            position: Vec2 { x: 0.0f32, y: 0.0f32 },
            extent: Vec2 { x: 1280.0f32, y: 720.0f32 },
            min_depth: 0.0f32,
            max_depth: 1.0f32
          }]);
          command_buffer.set_scissors(&[Scissor {
            position: Vec2I { x: 0, y: 0 },
            extent: Vec2UI { x: 9999, y: 9999 },
          }]);

          command_buffer.bind_buffer(BindingFrequency::PerModel, 0, &camera_constant_buffer);
          for renderable in state.elements {
            let model_constant_buffer = command_buffer.upload_dynamic_data(renderable.transform, BufferUsage::CONSTANT);
            command_buffer.bind_buffer(BindingFrequency::PerDraw, 0, &model_constant_buffer);

            if let Renderable::Static(static_renderable) = &renderable.renderable {
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
          0
        })
      ]
    }, swapchain);

    graph
  }

  fn render(&mut self, queue: &dyn JobQueue) -> JobCounterWait {
    let state = {
      let state_guard = self.renderer.window_state.lock().unwrap();
      state_guard.clone()
    };

    let mut swapchain_width = 0u32;
    let mut swapchain_height = 0u32;

    match state {
      WindowState::Minimized => {
        let counter = JobScheduler::new_counter();
        counter.inc();
        return JobCounterWait {
          counter,
          value: 1
        };
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

    let result = self.graph.render(queue);
    if result.is_err() {
      self.device.wait_for_idle();

      let new_swapchain_result = <P::GraphicsBackend as Backend>::Swapchain::recreate(&self.swapchain, swapchain_width, swapchain_height);
      if new_swapchain_result.is_err() {
        let counter = JobScheduler::new_counter();
        counter.inc();
        return JobCounterWait {
          counter,
          value: 1
        };
      }
      let new_swapchain = new_swapchain_result.unwrap();
      if new_swapchain.format() != self.swapchain.format() || new_swapchain.sample_count() != self.swapchain.sample_count() {
        panic!("Swapchain format or sample count changed. Can not recreate render graph.");
      }

      let new_graph = <P::GraphicsBackend as Backend>::RenderGraph::recreate(&self.graph, &new_swapchain);
      std::mem::replace(&mut self.swapchain, new_swapchain);
      std::mem::replace(&mut self.graph, new_graph);
      return self.graph.render(queue).unwrap();
    }
    return result.unwrap();
  }
}


