use sourcerenderer_core::platform::{Platform, PlatformEvent, GraphicsApi};
use job::{Scheduler, JobThreadContext};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::fs::File;
use std::io::*;
use sourcerenderer_core::graphics::SwapchainInfo;
use sourcerenderer_core::graphics::QueueType;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::MemoryUsage;
use sourcerenderer_core::graphics::BufferUsage;
use sourcerenderer_core::Vec2;
use sourcerenderer_core::Vec2I;
use sourcerenderer_core::Vec2UI;
use sourcerenderer_core::Vec3;
use sourcerenderer_core::graphics::*;
use std::rc::Rc;
use std::path::Path;
use sourcerenderer_core::platform::Window;

pub struct Engine<P: Platform> {
    platform: Box<P>,
    scheduler: Arc<Mutex<Scheduler>>
}

pub trait EngineSubsystem {
  fn init_contexts() -> Vec<Box<dyn JobThreadContext>>;
}

struct Vertex {
  pub position: Vec3,
  pub color: Vec3
}

impl<P: Platform> Engine<P> {
  pub fn new(platform: Box<P>) -> Engine<P> {
    return Engine {
      platform: platform,
      scheduler: Scheduler::new(0)
    };
  }

  pub fn run(&mut self) {
    self.init();
    //let renderer = self.platform.create_renderer();
    let graphics = self.platform.create_graphics(true).unwrap();
    let surface = self.platform.window().create_surface(graphics.clone());

    let mut adapters = graphics.list_adapters();
    println!("n devices: {}", adapters.len());

    let device = adapters.remove(0).create_device(surface.clone());
    let swapchain_info = SwapchainInfo {
      width: 1280,
      height: 720,
      vsync: true
    };
    let swapchain = self.platform.window().create_swapchain(swapchain_info, device.clone(), surface.clone());
    let queue = device.clone().create_queue(QueueType::Graphics).unwrap();
    let mut command_pool = queue.clone().create_command_pool();
    let mut command_buffer = command_pool.get_command_buffer(CommandBufferType::PRIMARY);
    let mut command_buffer_ref = command_buffer.borrow_mut();

    let buffer = device.clone().create_buffer(8096, MemoryUsage::CpuOnly, BufferUsage::VERTEX);
    let triangle = [
      Vertex {
        position: Vec3 {
          x: -1.0f32,
          y: 1.0f32,
          z: 0.0f32,
        },
        color: Vec3 {
          x: 1.0f32,
          y: 0.0f32,
          z: 0.0f32,
        }
      },
      Vertex {
        position: Vec3 {
          x: 0.0f32,
          y: -1.0f32,
          z: 0.0f32,
        },
        color: Vec3 {
          x: 0.0f32,
          y: 1.0f32,
          z: 0.0f32,
        }
      },
      Vertex {
        position: Vec3 {
          x: 1.0f32,
          y: 1.0f32,
          z: 0.0f32,
        },
        color: Vec3 {
          x: 0.0f32,
          y: 0.0f32,
          z: 1.0f32,
        }
      }
    ];
    let ptr = buffer.map().expect("failed to map buffer");
    unsafe {
      std::ptr::copy(triangle.as_ptr(), ptr as *mut Vertex, 3);
    }
    buffer.unmap();

    let vertex_shader = {
      //let mut file = File::open("..\\..\\core\\shaders\\simple.vert.spv").unwrap();
      let mut file = File::open(Path::new("../../engine/shaders/simple.vert.spv")).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.clone().create_shader(ShaderType::VertexShader, &bytes)
    };

    let fragment_shader = {
      //let mut file = File::open("..\\..\\core\\shaders\\simple.frag.spv")).unwrap();
      let mut file = File::open(Path::new("../../engine/shaders/simple.frag.spv")).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.clone().create_shader(ShaderType::FragmentShader, &bytes)
    };

    let render_pass_info = RenderPassLayoutInfo {
      attachments: vec![
        Attachment {
          format: Format::BGRA8UNorm,
          samples: SampleCount::Samples1,
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
          stencil_load_op: LoadOp::DontCare,
          stencil_store_op: StoreOp::DontCare,
          initial_layout: ImageLayout::Undefined,
          final_layout: ImageLayout::Present
        }
      ],
      subpasses: vec![
        Subpass {
          input_attachments: vec![],
          output_color_attachments: vec![
            OutputAttachmentRef {
              layout: ImageLayout::RenderTarget,
              index: 0u32,
              resolve_attachment_index: None
            },
          ],
          output_resolve_attachments: Vec::new(),
          depth_stencil_attachment: None,
          preserve_unused_attachments: Vec::new()
        }
      ]
    };
    let render_pass_layout = device.clone().create_renderpass_layout(&render_pass_info);

    let backbuffer_semaphore = device.clone().create_semaphore();
    let texture = swapchain.get_back_buffer(0, &backbuffer_semaphore);
    let rtv = device.clone().create_render_target_view(texture);

    let render_pass_info = RenderPassInfo {
      layout: render_pass_layout.clone(),
      width: 1280u32,
      height: 720u32,
      array_length: 1u32,
      attachments: vec![rtv]
    };
    let render_pass = device.clone().create_renderpass(&render_pass_info);

    let pipeline_info = PipelineInfo {
      vs: vertex_shader,
      fs: Some(fragment_shader),
      gs: None,
      tcs: None,
      tes: None,
      vertex_layout: VertexLayoutInfo {
        input_assembler: vec![
          InputAssemblerElement {
            binding: 0,
            stride: 24,
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
        cull_mode: CullMode::None,
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
      },
      renderpass: render_pass_layout,
      subpass: 0u32,
    };
    let pipeline = device.clone().create_pipeline(&pipeline_info);

    command_buffer_ref.begin();
    command_buffer_ref.begin_render_pass(&*render_pass, RenderpassRecordingMode::Commands);
    command_buffer_ref.set_pipeline(pipeline.clone());
    command_buffer_ref.set_vertex_buffer(&*buffer);
    command_buffer_ref.set_viewports(&[Viewport {
      position: Vec2 { x: 0.0f32, y: 0.0f32 },
      extent: Vec2 { x: 1280.0f32, y: 720.0f32 },
      min_depth: 0.0f32,
      max_depth: 1.0f32
    }]);
    command_buffer_ref.set_scissors(&[Scissor {
      position: Vec2I { x: 0, y: 0 },
      extent: Vec2UI { x: 9999, y: 9999 },
    }]);
    command_buffer_ref.draw(6, 0);
    command_buffer_ref.end_render_pass();
    command_buffer_ref.end();

    let cmd_buffer_semaphore = device.clone().create_semaphore();
    queue.submit(&command_buffer_ref, None, &[ &backbuffer_semaphore ], &[ &cmd_buffer_semaphore ]);

    queue.present(&swapchain, 0, &[ &cmd_buffer_semaphore ]);

    device.wait_for_idle();

    'main_loop: loop {
      let event = self.platform.handle_events();
      if event == PlatformEvent::Quit {
          break 'main_loop;
      }
      //renderer.render();
      std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
    }
  }

  fn init(&mut self) {

  }
}