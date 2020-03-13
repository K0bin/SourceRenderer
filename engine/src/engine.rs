use sourcerenderer_core::platform::{Platform, PlatformEvent, GraphicsApi};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
use async_std::task;
use async_std::prelude::*;
use async_std::future;
use std::thread::{Thread};
use std::future::Future;
use async_std::task::JoinHandle;
use std::cell::RefCell;

pub struct Engine<P: Platform> {
    platform: Box<P>
}

struct Vertex {
  pub position: Vec3,
  pub color: Vec3
}

impl<P: Platform> Engine<P> {
  pub fn new(platform: Box<P>) -> Engine<P> {
    return Engine {
      platform
    };
  }

  pub fn run(&mut self) {
    self.init();

    //let pool = crossbeam_workstealing_pool::small_pool(n_workers);
    //pool.execute()

    task::spawn(async {

      let start = Instant::now();
      let task1 = task::spawn(async {
        let id = std::thread::current().id();
        let mut sum = 0f64;
        for i in 0..100000000  {
          sum += (i as f64).sqrt();
        }
        println!("a - {:?} - thread: {:?}", sum, id);
      });
      //task1.await;
      let task2 = task::spawn(async {
        let id = std::thread::current().id();
        let mut sum = 0f64;
        for i in 0..100000000  {
          sum += (i as f64).sqrt();
        }
        println!("b - {:?} - thread: {:?}", sum, id);
      });
      //task2.await;
      task1.join(task2).await;
      //task1.await;

      //let result = task::spawn(fib(50)).await;
      //println!("Fib is {:?}", result);

      let after = Instant::now();
      let duration = after - start;
      println!("Took: {:?}", duration);
      //join!(task1, task2);
    });

    //let renderer = self.platform.create_renderer();
    let graphics = self.platform.create_graphics(true).unwrap();
    let surface = self.platform.window().create_surface(graphics.clone());

    let mut adapters = graphics.list_adapters();
    println!("n devices: {}", adapters.len());

    let device = adapters.remove(0).create_device(&surface);
    let swapchain_info = SwapchainInfo {
      width: 1280,
      height: 720,
      vsync: true
    };
    let mut swapchain = self.platform.window().create_swapchain(swapchain_info, &device, &surface);
    let queue = device.get_queue(QueueType::Graphics).unwrap();
    let mut command_pool = queue.create_command_pool();

    let buffer = Arc::new(device.create_buffer(8096, MemoryUsage::CpuOnly, BufferUsage::VERTEX));
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
    /*let ptr = buffer.map().expect("failed to map buffer");
    unsafe {
      std::ptr::copy(triangle.as_ptr(), ptr as *mut Vertex, 3);
    }
    buffer.unmap();*/

    {
      let mut map = buffer.map().expect("failed to map buffer");
      let mut data = map.get_data();
      std::mem::replace(data, triangle);
    }

    let vertex_shader = {
      let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("core")).join(Path::new("shaders")).join(Path::new("simple.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes)
    };

    let fragment_shader = {
      let mut file = File::open(Path::new("..").join(Path::new("..")).join(Path::new("core")).join(Path::new("shaders")).join(Path::new("simple.frag.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::FragmentShader, &bytes)
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
    let render_pass_layout = Arc::new(device.create_renderpass_layout(&render_pass_info));

    let pipeline_info = PipelineInfo2 {
      vs: Arc::new(vertex_shader),
      fs: Some(Arc::new(fragment_shader)),
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
      }
    };
    //let pipeline = Arc::new(device.create_pipeline(&pipeline_info));

    'main_loop: loop {
      let event = self.platform.handle_events();
      if event == PlatformEvent::Quit {
          break 'main_loop;
      }

      let backbuffer_semaphore = device.create_semaphore();
      let (backbuffer, swapchain_image_index) = swapchain.prepare_back_buffer(&backbuffer_semaphore);
      let rtv = device.create_render_target_view(backbuffer);

      let render_pass_info = RenderPassInfo {
        layout: render_pass_layout.clone(),
        width: 1280u32,
        height: 720u32,
        array_length: 1u32,
        attachments: vec![Arc::new(rtv)]
      };
      let render_pass = device.create_renderpass(&render_pass_info);

      let mut command_buffer = command_pool.get_command_buffer(CommandBufferType::PRIMARY);
      command_buffer.begin_render_pass(&render_pass, RenderpassRecordingMode::Commands);
      command_buffer.set_pipeline2(&pipeline_info);
      command_buffer.set_vertex_buffer(buffer.clone());
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
      command_buffer.draw(6, 0);
      command_buffer.end_render_pass();
      let submission = command_buffer.finish();

      let cmd_buffer_semaphore = device.create_semaphore();
      queue.submit(submission, None, &[ &backbuffer_semaphore ], &[ &cmd_buffer_semaphore ]);

      queue.present(&swapchain, swapchain_image_index, &[ &cmd_buffer_semaphore ]);

      device.wait_for_idle();

      command_pool.reset();

      //renderer.render();
      std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
    }
  }

  fn init(&mut self) {

  }
}