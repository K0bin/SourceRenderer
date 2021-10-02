use nalgebra::Vector2;
use sourcerenderer_core::{Matrix4, graphics::{AddressMode, AttachmentBlendInfo, AttachmentInfo, Backend as GraphicsBackend, Barrier, BindingFrequency, BlendInfo, BufferUsage, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Filter, Format, FrontFace, GraphicsPipelineInfo, InputAssemblerElement, InputRate, LoadOp, LogicOp, OutputAttachmentRef, PipelineBinding, PrimitiveType, Queue, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderPassInfo, RenderpassRecordingMode, SampleCount, SamplerInfo, Scissor, ShaderInputElement, ShaderType, StencilInfo, StoreOp, SubpassInfo, Swapchain, Texture, TextureDepthStencilView, TextureInfo, TextureRenderTargetView, TextureRenderTargetViewInfo, TextureShaderResourceView, TextureShaderResourceViewInfo, TextureUsage, VertexLayoutInfo, Viewport}};
use std::sync::Arc;
use crate::renderer::{drawable::View, renderer_scene::RendererScene};
use sourcerenderer_core::{Platform, Vec2, Vec2I, Vec2UI};
use crate::renderer::passes::desktop::taa::scaled_halton_point;
use std::path::Path;
use std::io::Read;
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::platform::io::IO;
use rayon::prelude::*;

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

pub struct GeometryPass<B: GraphicsBackend> {
  rtv: Arc<B::TextureRenderTargetView>,
  srv: Arc<B::TextureShaderResourceView>,
  sampler: Arc<B::Sampler>,
  pipeline: Arc<B::GraphicsPipeline>
}

impl<B: GraphicsBackend> GeometryPass<B> {
  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let output = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::COMPUTE_SHADER_SAMPLED | TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
    }, Some("GeometryPassOutput"));
    let rtv = device.create_render_target_view(&output, &TextureRenderTargetViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });
    let srv = device.create_shader_resource_view(&output, &TextureShaderResourceViewInfo {
      base_mip_level: 0,
      mip_level_length: 1,
      base_array_level: 0,
      array_level_length: 1,
    });


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
        format: output.get_info().format,
        samples: output.get_info().samples,
        load_op: LoadOp::DontCare,
        store_op: StoreOp::DontCare,
        stencil_load_op: LoadOp::DontCare,
        stencil_store_op: StoreOp::DontCare,
      },
      AttachmentInfo {
        format: Format::D24S8,
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

  init_cmd_buffer.barrier(&[
    Barrier::TextureBarrier {
      old_primary_usage: TextureUsage::UNINITIALIZED,
      new_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
      old_usages: TextureUsage::empty(),
      new_usages: TextureUsage::empty(),
      texture: rtv.texture(),
    }
  ]);

    Self {
      srv,
      rtv,
      sampler,
      pipeline
    }
  }

  pub(super) fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    device: &Arc<B::Device>,
    scene: &RendererScene<B>,
    view: &View,
    lightmap: &Arc<RendererTexture<B>>,
    swapchain_transform: Matrix4,
    frame: u64,
    prepass_depth: &Arc<B::TextureDepthStencilView>,
    light_bitmask_buffer: &Arc<B::Buffer>,
    camera_buffer: &Arc<B::Buffer>,
    ssao: &Arc<B::TextureShaderResourceView>
  ) {
    let static_drawables = scene.static_drawables();

    cmd_buffer.barrier(&[
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        new_primary_usage: TextureUsage::RENDER_TARGET,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: self.rtv.texture(),
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::COMPUTE_SHADER_SAMPLED,
        new_primary_usage: TextureUsage::DEPTH_READ,
        old_usages: TextureUsage::empty(),
        new_usages: TextureUsage::empty(),
        texture: prepass_depth.texture()
      },
      Barrier::BufferBarrier {
        old_primary_usage: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_primary_usage: BufferUsage::FRAGMENT_SHADER_STORAGE_READ,
        old_usages: BufferUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_usages: BufferUsage::FRAGMENT_SHADER_STORAGE_READ,
        buffer: light_bitmask_buffer,
      },
      Barrier::TextureBarrier {
        old_primary_usage: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_primary_usage: TextureUsage::FRAGMENT_SHADER_SAMPLED | TextureUsage::COMPUTE_SHADER_SAMPLED,
        old_usages: TextureUsage::COMPUTE_SHADER_STORAGE_WRITE,
        new_usages: TextureUsage::FRAGMENT_SHADER_SAMPLED,
        texture: ssao.texture()
      },
    ]);

    cmd_buffer.begin_render_pass_1(&RenderPassBeginInfo {
      attachments: &[
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&self.rtv),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::DepthStencil(prepass_depth),
          load_op: LoadOp::Load,
          store_op: StoreOp::Store
        }
      ],
      subpasses: &[
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
    }, RenderpassRecordingMode::CommandBuffers);

    let rtv_info = self.rtv.texture().get_info();
    let cluster_count = nalgebra::Vector3::<u32>::new(16, 9, 24);
    let near = view.near_plane;
    let far = view.far_plane;
    let cluster_z_scale = (cluster_count.z as f32) / (far / near).log2();
    let cluster_z_bias = -(cluster_count.z as f32) * (near).log2() / (far / near).log2();
    let per_frame = FrameData {
      swapchain_transform: swapchain_transform,
      halton_point: scaled_halton_point(rtv_info.width, rtv_info.height, (frame % 8) as u32),
      z_near: view.near_plane,
      z_far: view.far_plane,
      rt_size: Vector2::<u32>::new(rtv_info.width, rtv_info.height),
      cluster_z_bias,
      cluster_z_scale,
      cluster_count,
      point_light_count: scene.point_lights().len() as u32
    };
    let per_frame_buffer = cmd_buffer.upload_dynamic_data(&[per_frame], BufferUsage::FRAGMENT_SHADER_CONSTANT | BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::COMPUTE_SHADER_CONSTANT);
    let point_light_buffer = cmd_buffer.upload_dynamic_data(scene.point_lights(), BufferUsage::FRAGMENT_SHADER_STORAGE_READ | BufferUsage::VERTEX_SHADER_STORAGE_READ);

    let inheritance = cmd_buffer.inheritance();
    const CHUNK_SIZE: usize = 128;
    let chunks = view.drawable_parts.par_chunks(CHUNK_SIZE);
    let inner_cmd_buffers: Vec::<B::CommandBufferSubmission> = chunks.map(|chunk| {
      let mut command_buffer = device.graphics_queue().create_inner_command_buffer(inheritance);

      command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 3, &per_frame_buffer);

      command_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));
      command_buffer.set_viewports(&[Viewport {
        position: Vec2::new(0.0f32, 0.0f32),
        extent: Vec2::new(rtv_info.width as f32, rtv_info.height as f32),
        min_depth: 0.0f32,
        max_depth: 1.0f32
      }]);
      command_buffer.set_scissors(&[Scissor {
        position: Vec2I::new(0, 0),
        extent: Vec2UI::new(9999, 9999),
      }]);

      //command_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 4, graph_resources.get_buffer(OUTPUT_CLUSTERS, false).expect("Failed to get graph resource"));
      command_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, camera_buffer);
      command_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 1, &point_light_buffer);
      command_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 2, light_bitmask_buffer);
      command_buffer.bind_texture_view(BindingFrequency::PerFrame, 4, ssao, &self.sampler);
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
        command_buffer.bind_texture_view(BindingFrequency::PerMaterial, 0, &albedo_view, &self.sampler);

        let lightmap_ref = lightmap.view.borrow();
        command_buffer.bind_texture_view(BindingFrequency::PerMaterial, 1, &lightmap_ref, &self.sampler);
        command_buffer.finish_binding();

        if mesh.indices.is_some() {
          command_buffer.draw_indexed(1, 0, range.count, range.start, 0);
        } else {
          command_buffer.draw(range.count, range.start);
        }
      }
      command_buffer.finish()
    }).collect();

    cmd_buffer.execute_inner(inner_cmd_buffers);
    cmd_buffer.end_render_pass();
  }

  pub fn output_srv(&self) -> &Arc<B::TextureShaderResourceView> {
    &self.srv
  }
}
