use nalgebra::Vector2;
use sourcerenderer_core::{Matrix4, Vec4, graphics::{AddressMode, AttachmentBlendInfo, AttachmentInfo, Backend as GraphicsBackend, BindingFrequency, BlendInfo, BufferUsage, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Filter, Format, FrontFace, GraphicsPipelineInfo, InputAssemblerElement, InputRate, LoadOp, LogicOp, OutputAttachmentRef, PipelineBinding, PrimitiveType, Queue, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderPassInfo, RenderpassRecordingMode, SampleCount, SamplerInfo, Scissor, ShaderInputElement, ShaderType, StencilInfo, StoreOp, SubpassInfo, Swapchain, TextureInfo, TextureViewInfo, TextureUsage, VertexLayoutInfo, Viewport, TextureLayout, BarrierSync, BarrierAccess, IndexFormat, WHOLE_BUFFER, TextureDimension}};
use std::{sync::Arc, cell::Ref};
use crate::renderer::{drawable::View, renderer_scene::RendererScene, renderer_resources::{RendererResources, HistoryResourceEntry}, passes::{light_binning, ssao::SsaoPass, prepass::Prepass, rt_shadows::RTShadowPass}};
use sourcerenderer_core::{Platform, Vec2, Vec2I, Vec2UI};
use std::path::Path;
use std::io::Read;
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::platform::io::IO;
use rayon::prelude::*;

use super::desktop_renderer::{FrameBindings, ConservativeRenderer};

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
  point_light_count: u32,
  directional_light_count: u32
}

pub struct GeometryPass<B: GraphicsBackend> {
  sampler: Arc<B::Sampler>,
  pipeline: Arc<B::GraphicsPipeline>
}

impl<B: GraphicsBackend> GeometryPass<B> {
  pub const GEOMETRY_PASS_TEXTURE_NAME: &'static str = "geometry";

  const DRAWABLE_LABELS: bool = false;

  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, barriers: &mut RendererResources<B>) -> Self {
    let texture_info = TextureInfo {
      dimension: TextureDimension::Dim2D,
      format: Format::RGBA8UNorm,
      width: swapchain.width(),
      height: swapchain.height(),
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC | TextureUsage::STORAGE,
    };
    barriers.create_texture(Self::GEOMETRY_PASS_TEXTURE_NAME, &texture_info, false);

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
      max_lod: None,
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
      vs: &vertex_shader,
      fs: Some(&fragment_shader),
      primitive_type: PrimitiveType::Triangles,
      vertex_layout: VertexLayoutInfo {
        input_assembler: &[
          InputAssemblerElement {
            binding: 0,
            stride: 64,
            input_rate: InputRate::PerVertex
          }
        ],
        shader_inputs: &[
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
            offset: 16,
            format: Format::RGB32Float
          },
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 2,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 32,
            format: Format::RG32Float
          },
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 3,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 40,
            format: Format::RG32Float
          },
          ShaderInputElement {
            input_assembler_binding: 0,
            location_vk_mtl: 4,
            semantic_name_d3d: String::from(""),
            semantic_index_d3d: 0,
            offset: 48,
            format: Format::R32Float
          }
        ]
      },
      rasterizer: RasterizerInfo {
        fill_mode: FillMode::Fill,
        cull_mode: CullMode::Back,
        front_face: FrontFace::Clockwise,
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
        attachments: &[
          AttachmentBlendInfo::default()
        ]
      }
    };
    let pipeline = device.create_graphics_pipeline(&pipeline_info, &RenderPassInfo {
      attachments: &[
        AttachmentInfo {
          format: texture_info.format,
          samples: texture_info.samples,
        },
        AttachmentInfo {
          format: Format::D24,
          samples: SampleCount::Samples1,
        }
      ],
      subpasses: &[
        SubpassInfo {
          input_attachments: &[],
          output_color_attachments: &[
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
    }, 0, Some("Geometry"));

    Self {
      sampler,
      pipeline
    }
  }

  #[profiling::function]
  pub(super) fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    device: &Arc<B::Device>,
    scene: &RendererScene<B>,
    view: &View,
    bindings: &FrameBindings<B>,
    zero_texture_view: &Arc<B::TextureSamplingView>,
    zero_texture_view_black: &Arc<B::TextureSamplingView>,
    lightmap: &Arc<RendererTexture<B>>,
    barriers: &RendererResources<B>,
  ) {
    cmd_buffer.begin_label("Geometry pass");
    let static_drawables = scene.static_drawables();

    let (width, height) = {
      let info = barriers.texture_info(Self::GEOMETRY_PASS_TEXTURE_NAME);
      (info.width, info.height)
    };

    let rtv_ref = barriers.access_render_target_view(
      cmd_buffer,
      Self::GEOMETRY_PASS_TEXTURE_NAME,
      BarrierSync::RENDER_TARGET,
      BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
      TextureLayout::RenderTarget, true,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );
    let rtv = &*rtv_ref;

    let prepass_depth_ref = barriers.access_depth_stencil_view(
      cmd_buffer,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
      BarrierAccess::DEPTH_STENCIL_READ,
      TextureLayout::DepthStencilRead,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );
    let prepass_depth = &*prepass_depth_ref;

    let ssao_ref = barriers.access_sampling_view(
      cmd_buffer,
      SsaoPass::<B>::SSAO_TEXTURE_NAME,
      BarrierSync::FRAGMENT_SHADER | BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SAMPLING_READ,
      TextureLayout::Sampled,
      false,
      &TextureViewInfo::default(),
      HistoryResourceEntry::Current
    );
    let ssao = &*ssao_ref;

    let light_bitmask_buffer_ref = barriers.access_buffer(
      cmd_buffer,
      light_binning::LightBinningPass::<B>::LIGHT_BINNING_BUFFER_NAME,
      BarrierSync::FRAGMENT_SHADER,
      BarrierAccess::STORAGE_READ,
      HistoryResourceEntry::Current
    );
    let light_bitmask_buffer = &*light_bitmask_buffer_ref;

    let rt_shadows: Ref<Arc<B::TextureSamplingView>>;
    let shadows = if device.supports_ray_tracing() {
      rt_shadows = barriers.access_sampling_view(
        cmd_buffer,
        RTShadowPass::<B>::SHADOWS_TEXTURE_NAME,
        BarrierSync::FRAGMENT_SHADER,
        BarrierAccess::SAMPLING_READ,
        TextureLayout::Sampled,
        false,
        &TextureViewInfo::default(),
        HistoryResourceEntry::Current
      );
      &*rt_shadows
    } else {
      zero_texture_view
    };

    /*let clusters = barriers.access_buffer(
      cmd_buffer,
      ClusteringPass::<B>::CLUSTERS_BUFFER_NAME,
      BarrierSync::FRAGMENT_SHADER,
      BarrierAccess::STORAGE_READ,
      HistoryResourceEntry::Current
    ).clone();*/

    cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
      attachments: &[
        RenderPassAttachment {
          view: RenderPassAttachmentView::RenderTarget(&rtv),
          load_op: LoadOp::Clear,
          store_op: StoreOp::Store,
        },
        RenderPassAttachment {
          view: RenderPassAttachmentView::DepthStencil(&prepass_depth),
          load_op: LoadOp::Load,
          store_op: StoreOp::Store
        }
      ],
      subpasses: &[
        SubpassInfo {
          input_attachments: &[],
          output_color_attachments: &[
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

    let inheritance = cmd_buffer.inheritance();
    const CHUNK_SIZE: usize = 128;
    let chunks = view.drawable_parts.par_chunks(CHUNK_SIZE);
    let inner_cmd_buffers: Vec::<B::CommandBufferSubmission> = chunks.map(|chunk| {
      let mut command_buffer = device.graphics_queue().create_inner_command_buffer(inheritance);

      command_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));
      command_buffer.set_viewports(&[Viewport {
        position: Vec2::new(0.0f32, 0.0f32),
        extent: Vec2::new(width as f32, height as f32),
        min_depth: 0.0f32,
        max_depth: 1.0f32
      }]);
      command_buffer.set_scissors(&[Scissor {
        position: Vec2I::new(0, 0),
        extent: Vec2UI::new(9999, 9999),
      }]);

      command_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 0, &lightmap.view, &self.sampler);
      command_buffer.bind_sampler(BindingFrequency::Frequent, 1, &self.sampler);
      command_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 2,  &shadows, &self.sampler);
      command_buffer.bind_storage_buffer(BindingFrequency::Frequent, 3, &light_bitmask_buffer, 0, WHOLE_BUFFER);
      command_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 4, &ssao, &self.sampler);
      // command_buffer.bind_storage_buffer(BindingFrequency::Frequent, 5, &clusters, 0, WHOLE_BUFFER);

      command_buffer.track_texture_view(zero_texture_view);
      command_buffer.track_texture_view(zero_texture_view_black);

      let mut last_material = Option::<Arc<RendererMaterial<B>>>::None;

      for part in chunk.iter() {
        let drawable = &static_drawables[part.drawable_index];
        if Self::DRAWABLE_LABELS {
          command_buffer.begin_label(&format!("Drawable {}", part.drawable_index));
        }

        ConservativeRenderer::<B>::setup_frame(&mut command_buffer, bindings);

        command_buffer.upload_dynamic_data_inline(&[drawable.transform], ShaderType::VertexShader);

        let model = &drawable.model;
        let mesh = model.mesh();
        let materials = model.materials();

        command_buffer.set_vertex_buffer(mesh.vertices.buffer(), mesh.vertices.offset() as usize);
        if let Some(indices) = mesh.indices.as_ref() {
          command_buffer.set_index_buffer(indices.buffer(), indices.offset() as usize, IndexFormat::U32);
        }

        let range = &mesh.parts[part.part_index];
        let material = &materials[part.part_index];

        if last_material.as_ref() != Some(material) {
          #[repr(C)]
          #[derive(Clone, Copy)]
          struct MaterialInfo {
            albedo: Vec4,
            roughness_factor: f32,
            metalness_factor: f32,
            albedo_texture_index: u32
          }
          let mut material_info = MaterialInfo {
            albedo: Vec4::new(1f32, 1f32, 1f32, 1f32),
            roughness_factor: 0f32,
            metalness_factor: 0f32,
            albedo_texture_index: 0u32
          };

          command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, zero_texture_view, &self.sampler);
          command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 1, zero_texture_view, &self.sampler);
          command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 2, zero_texture_view, &self.sampler);

          let albedo_value = material.get("albedo").unwrap();
          match albedo_value {
            RendererMaterialValue::Texture(texture) => {
              let albedo_view = &texture.view;
              command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 0, albedo_view, &self.sampler);
              command_buffer.track_texture_view(albedo_view);
              material_info.albedo_texture_index = 0;
            },
            RendererMaterialValue::Vec4(val) => {
              material_info.albedo = *val
            },
            RendererMaterialValue::Float(_) => unimplemented!()
          }
          let roughness_value = material.get("roughness");
          match roughness_value {
            Some(RendererMaterialValue::Texture(texture)) => {
              let roughness_view = &texture.view;
              command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 1, roughness_view, &self.sampler);
            }
            Some(RendererMaterialValue::Vec4(_)) => unimplemented!(),
            Some(RendererMaterialValue::Float(val)) => {
              material_info.roughness_factor = *val;
            },
            None => {}
          }
          let metalness_value = material.get("metalness");
          match metalness_value {
            Some(RendererMaterialValue::Texture(texture)) => {
              let metalness_view = &texture.view;
              command_buffer.bind_sampling_view_and_sampler(BindingFrequency::VeryFrequent, 2, metalness_view, &self.sampler);
            }
            Some(RendererMaterialValue::Vec4(_)) => unimplemented!(),
            Some(RendererMaterialValue::Float(val)) => {
              material_info.metalness_factor = *val;
            },
            None => {}
          }
          let material_info_buffer = command_buffer.upload_dynamic_data(&[material_info], BufferUsage::CONSTANT);
          command_buffer.bind_uniform_buffer(BindingFrequency::VeryFrequent, 3, &material_info_buffer, 0, WHOLE_BUFFER);
          last_material = Some(material.clone());
        }

        command_buffer.finish_binding();

        if mesh.indices.is_some() {
          command_buffer.draw_indexed(1, 0, range.count, range.start, 0);
        } else {
          command_buffer.draw(range.count, range.start);
        }
        if Self::DRAWABLE_LABELS {
          command_buffer.end_label();
        }
      }
      command_buffer.finish()
    }).collect();

    cmd_buffer.execute_inner(inner_cmd_buffers);
    cmd_buffer.end_render_pass();
    cmd_buffer.end_label();
  }
}
