use sourcerenderer_core::platform::{Platform, PlatformEvent, GraphicsApi};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::*;
use sourcerenderer_core::graphics::SwapchainInfo;
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
use sourcerenderer_core::graphics::graph::{RenderGraph, RenderGraphInfo, RenderGraphAttachmentInfo, RenderPassInfo, BACK_BUFFER_ATTACHMENT_NAME, OutputAttachmentReference};
use std::collections::HashMap;
use image::{GenericImage, GenericImageView};
use nalgebra::{Matrix4, Point3, Vector3, Rotation3};
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicUsize;

pub struct Engine<P: Platform> {
    platform: Box<P>
}

struct Vertex {
  pub position: Vec3,
  pub color: Vec3,
  pub uv: Vec2
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

    println!("bla {}", Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("texture.png")).to_str().unwrap());

    //let image = image::open("texture.png").unwrap();
    //println!("img {}", image);


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
    let mut swapchain = Arc::new(self.platform.window().create_swapchain(swapchain_info, &device, &surface));

    let (texture_buffer, texture) = {
      let image = image::open(Path::new("..").join(Path::new("..")).join(Path::new("engine")).join(Path::new("texture.png"))).expect("Failed to open texture");
      let data = image.to_rgba();
      let buffer = device.upload_data_raw(&data);
      let texture = device.create_texture(&TextureInfo {
        format: Format::RGBA8,
        width: image.width(),
        height: image.height(),
        depth: 0,
        mip_levels: 1,
        array_length: 1,
        samples: SampleCount::Samples1
      });
      (Arc::new(buffer), Arc::new(texture))
    };
    let texture_view = Arc::new(device.create_shader_resource_view(&texture, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
      mag_filter: Filter::Linear,
      min_filter: Filter::Linear,
      mip_filter: Filter::Linear,
      address_mode_u: AddressMode::Repeat,
      address_mode_v: AddressMode::Repeat,
      address_mode_w: AddressMode::Repeat,
      mip_bias: 0.0,
      max_anisotropy: 1.0,
      compare_op: None,
      min_lod: 0.0,
      max_lod: 0.0
    }));

    //let buffer = Arc::new(device.create_buffer(8096, MemoryUsage::CpuOnly, BufferUsage::VERTEX));
    let triangle = [
      Vertex {
        position: Vec3 {
          x: -1.0f32,
          y: -1.0f32,
          z: -1.0f32,
        },
        color: Vec3 {
          x: 1.0f32,
          y: 0.0f32,
          z: 0.0f32,
        },
        uv: Vec2 {
          x: 0.0f32,
          y: 0.0f32
        }
      },
      Vertex {
        position: Vec3 {
          x: 1.0f32,
          y: -1.0f32,
          z: -1.0f32,
        },
        color: Vec3 {
          x: 0.0f32,
          y: 1.0f32,
          z: 0.0f32,
        },
        uv: Vec2 {
          x: 1.0f32,
          y: 0.0f32
        }
      },
      Vertex {
        position: Vec3 {
          x: 1.0f32,
          y: 1.0f32,
          z: -1.0f32,
        },
        color: Vec3 {
          x: 0.0f32,
          y: 0.0f32,
          z: 1.0f32,
        },
        uv: Vec2 {
          x: 1.0f32,
          y: 1.0f32
        }
      },
      Vertex {
        position: Vec3 {
          x: -1.0f32,
          y: 1.0f32,
          z: -1.0f32,
        },
        color: Vec3 {
          x: 1.0f32,
          y: 1.0f32,
          z: 1.0f32,
        },
        uv: Vec2 {
          x: 0.0f32,
          y: 1.0f32
        }
      },
      // face 2
      Vertex {
        position: Vec3 {
          x: -1.0f32,
          y: -1.0f32,
          z: 1.0f32,
        },
        color: Vec3 {
          x: 1.0f32,
          y: 0.0f32,
          z: 0.0f32,
        },
        uv: Vec2 {
          x: 0.0f32,
          y: 0.0f32
        }
      },
      Vertex {
        position: Vec3 {
          x: 1.0f32,
          y: -1.0f32,
          z: 1.0f32,
        },
        color: Vec3 {
          x: 0.0f32,
          y: 1.0f32,
          z: 0.0f32,
        },
        uv: Vec2 {
          x: 1.0f32,
          y: 0.0f32
        }
      },
      Vertex {
        position: Vec3 {
          x: 1.0f32,
          y: 1.0f32,
          z: 1.0f32,
        },
        color: Vec3 {
          x: 0.0f32,
          y: 0.0f32,
          z: 1.0f32,
        },
        uv: Vec2 {
          x: 1.0f32,
          y: 1.0f32
        }
      },
      Vertex {
        position: Vec3 {
          x: -1.0f32,
          y: 1.0f32,
          z: 1.0f32,
        },
        color: Vec3 {
          x: 1.0f32,
          y: 1.0f32,
          z: 1.0f32,
        },
        uv: Vec2 {
          x: 0.0f32,
          y: 1.0f32
        }
      }
    ];
    let indices = [0u32, 1u32, 2u32, 2u32, 3u32, 0u32, // front
                            6u32, 5u32, 4u32, 4u32, 7u32, 6u32, // back
                            5u32, 1u32, 0u32, 0u32, 4u32, 5u32, // top
                            3u32, 2u32, 6u32, 6u32, 7u32, 3u32, // bottom
                            7u32, 4u32, 0u32, 0u32, 3u32, 7u32, // left
                            1u32, 5u32, 6u32, 6u32, 2u32, 1u32]; // right
    /*let ptr = buffer.map().expect("failed to map buffer");
    unsafe {
      std::ptr::copy(triangle.as_ptr(), ptr as *mut Vertex, 3);
    }
    buffer.unmap();*/

    /*{
      let mut map = buffer.map().expect("failed to map buffer");
      let mut data = map.get_data();
      std::mem::replace(data, triangle);
    }*/

    let vertex_buffer = Arc::new(device.upload_data(triangle));
    let index_buffer = Arc::new(device.upload_data(indices));

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
      vs: Arc::new(vertex_shader),
      fs: Some(Arc::new(fragment_shader)),
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

    let camera: Arc<Mutex<Matrix4<f32>>> = Arc::new(Mutex::new(Matrix4::identity()));
    let pass_camera = camera.clone();

    let mut passes: Vec<RenderPassInfo<P::GraphicsBackend>> = Vec::new();
    passes.push(RenderPassInfo {
      outputs: vec![OutputAttachmentReference {
        name: BACK_BUFFER_ATTACHMENT_NAME.to_string()
      }],
      inputs: Vec::new(),
      render: Arc::new(move |command_buffer| {
        //command_buffer.init_texture_mip_level(&texture_buffer, &texture, 0, 0);

        let matrix = {
          let guard = pass_camera.lock().unwrap();
          guard.clone()
        };

        let constant_buffer = Arc::new(command_buffer.upload_data(matrix));
        command_buffer.set_pipeline(&pipeline_info);
        command_buffer.set_vertex_buffer(vertex_buffer.clone());
        command_buffer.set_index_buffer(index_buffer.clone());
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
        command_buffer.bind_buffer(BindingFrequency::PerDraw, 1, &constant_buffer);
        command_buffer.bind_texture_view(BindingFrequency::PerDraw, 0, &texture_view);
        command_buffer.finish_binding();
        //command_buffer.draw(6, 0);
        command_buffer.draw_indexed(1, 0, 6 * 6, 0, 0);

        0
      })
    });

    let mut graph = device.create_render_graph(&RenderGraphInfo {
      attachments: HashMap::new(),
      passes
    }, &swapchain);

    device.init_texture(&texture, &texture_buffer, 0, 0);
    device.flush_transfers();

    let counter = AtomicUsize::new(0);
    task::spawn(async move {
      'main_loop: loop {
        counter.fetch_add(1, Ordering::SeqCst);
        {
          let mut cam = camera.lock().unwrap();
          let new_mat =
            Matrix4::new_perspective(16f32 / 9f32, 1.02974f32, 0.001f32, 20.0f32)
                *
                Matrix4::look_at_rh(
                  &Point3::new(0.0f32, 2.0f32, -5.0f32),
                  &Point3::new(0.0f32, 0.0f32, 0.0f32),
                  &Vector3::new(0.0f32, 1.0f32, 0.0f32)
                )
              *
          Matrix4::from(Rotation3::from_axis_angle(&Vector3::y_axis(), (counter.load(Ordering::SeqCst) as f32) / 300.0f32));
          std::mem::replace(&mut *cam, new_mat);
        }

        graph.render();
        device.free_completed_transfers();

        std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
      }
    });

    'main_loop: loop {
      let event = self.platform.handle_events();
      if event == PlatformEvent::Quit {
        break 'main_loop;
      }
      std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
    }

  }

  fn init(&mut self) {

  }
}