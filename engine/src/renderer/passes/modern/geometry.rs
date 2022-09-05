use nalgebra::Vector2;
use smallvec::SmallVec;
use sourcerenderer_core::{Matrix4, graphics::{AddressMode, AttachmentBlendInfo, AttachmentInfo, Backend as GraphicsBackend, BindingFrequency, BlendInfo, BufferUsage, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Filter, Format, FrontFace, InputAssemblerElement, InputRate, LoadOp, LogicOp, OutputAttachmentRef, PipelineBinding, PrimitiveType, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderPassInfo, RenderpassRecordingMode, SampleCount, SamplerInfo, Scissor, ShaderInputElement, StencilInfo, StoreOp, SubpassInfo, Swapchain, Texture, TextureInfo, TextureRenderTargetView, TextureViewInfo, TextureUsage, VertexLayoutInfo, Viewport, TextureLayout, BarrierSync, BarrierAccess, IndexFormat, WHOLE_BUFFER, TextureDimension}};
use std::{sync::Arc, cell::Ref};
use crate::renderer::{PointLight, drawable::View, light::DirectionalLight, renderer_scene::RendererScene, renderer_resources::{RendererResources, HistoryResourceEntry}, passes::{light_binning, ssao::SsaoPass, rt_shadows::RTShadowPass}, shader_manager::{ShaderManager, PipelineHandle, GraphicsPipelineInfo}};
use sourcerenderer_core::{Platform, Vec2, Vec2I, Vec2UI, graphics::Backend};
use crate::renderer::passes::taa::scaled_halton_point;
use crate::renderer::renderer_assets::*;

use super::{draw_prep::DrawPrepPass, gpu_scene::DRAW_CAPACITY};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FrameData {
  swapchain_transform: Matrix4,
  jitter: Vec2,
  z_near: f32,
  z_far: f32,
  rt_size: Vector2::<u32>,
  cluster_z_bias: f32,
  cluster_z_scale: f32,
  cluster_count: nalgebra::Vector3::<u32>,
  point_light_count: u32,
  directional_light_count: u32
}

pub struct GeometryPass<P: Platform> {
  sampler: Arc<<P::GraphicsBackend as GraphicsBackend>::Sampler>,
  pipeline: PipelineHandle
}

impl<P: Platform> GeometryPass<P> {
  pub const GEOMETRY_PASS_TEXTURE_NAME: &'static str = "geometry";

  pub fn new(device: &Arc<<P::GraphicsBackend as Backend>::Device>, swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>, barriers: &mut RendererResources<P::GraphicsBackend>, shader_manager: &mut ShaderManager<P>) -> Self {
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
      supports_srgb: false,
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

    let pipeline_info: GraphicsPipelineInfo = GraphicsPipelineInfo {
      vs: "shaders/geometry_bindless.vert.spv",
      fs: Some("shaders/geometry_bindless.frag.spv"),
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
        depth_write_enabled: true,
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

    let pipeline = shader_manager.request_graphics_pipeline(&pipeline_info, &RenderPassInfo {
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
    }, 0);

    Self {
      sampler,
      pipeline
    }
  }

  #[profiling::function]
  pub(super) fn execute(
    &mut self,
    cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
    barriers: &RendererResources<P::GraphicsBackend>,
    device: &Arc<<P::GraphicsBackend as Backend>::Device>,
    depth_name: &str,
    scene: &RendererScene<P::GraphicsBackend>,
    view: &View,
    gpu_scene: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    zero_texture_view: &Arc<<P::GraphicsBackend as Backend>::TextureSamplingView>,
    _zero_texture_view_black: &Arc<<P::GraphicsBackend as Backend>::TextureSamplingView>,
    lightmap: &Arc<RendererTexture<P::GraphicsBackend>>,
    swapchain_transform: Matrix4,
    frame: u64,
    camera_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    vertex_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    index_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
    shader_manager: &ShaderManager<P>
  ) {
    cmd_buffer.begin_label("Geometry pass");
    let draw_buffer = barriers.access_buffer(
      cmd_buffer,
      DrawPrepPass::INDIRECT_DRAW_BUFFER,
      BarrierSync::INDIRECT,
      BarrierAccess::INDIRECT_READ,
      HistoryResourceEntry::Current
    );

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
      depth_name,
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
      SsaoPass::<P>::SSAO_TEXTURE_NAME,
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
      light_binning::LightBinningPass::LIGHT_BINNING_BUFFER_NAME,
      BarrierSync::FRAGMENT_SHADER,
      BarrierAccess::STORAGE_READ,
      HistoryResourceEntry::Current
    );
    let light_bitmask_buffer = &*light_bitmask_buffer_ref;

    let rt_shadows: Ref<Arc<<P::GraphicsBackend as Backend>::TextureSamplingView>>;
    let shadows = if device.supports_ray_tracing() {
      rt_shadows = barriers.access_sampling_view(
        cmd_buffer,
        RTShadowPass::<P::GraphicsBackend>::SHADOWS_TEXTURE_NAME,
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
    }, RenderpassRecordingMode::Commands);

    let rtv_info = rtv.texture().info();
    let cluster_count = nalgebra::Vector3::<u32>::new(16, 9, 24);
    let near = view.near_plane;
    let far = view.far_plane;
    let cluster_z_scale = (cluster_count.z as f32) / (far / near).log2();
    let cluster_z_bias = -(cluster_count.z as f32) * (near).log2() / (far / near).log2();
    let per_frame = FrameData {
      swapchain_transform,
      jitter: scaled_halton_point(rtv_info.width, rtv_info.height, (frame % 8) as u32 + 1),
      z_near: near,
      z_far: far,
      rt_size: Vector2::<u32>::new(rtv_info.width, rtv_info.height),
      cluster_z_bias,
      cluster_z_scale,
      cluster_count,
      point_light_count: scene.point_lights().len() as u32,
      directional_light_count: scene.directional_lights().len() as u32
    };
    let mut point_lights = SmallVec::<[PointLight; 16]>::new();
    for point_light in scene.point_lights() {
      point_lights.push(PointLight {
        position: point_light.position,
        intensity: point_light.intensity
      });
    }
    let mut directional_lights = SmallVec::<[DirectionalLight; 16]>::new();
    for directional_light in scene.directional_lights() {
      directional_lights.push(DirectionalLight {
        direction: directional_light.direction,
        intensity: directional_light.intensity
      });
    }
    let per_frame_buffer = cmd_buffer.upload_dynamic_data(&[per_frame], BufferUsage::CONSTANT);
    let point_light_buffer = cmd_buffer.upload_dynamic_data(&point_lights[..], BufferUsage::STORAGE);
    let directional_light_buffer = cmd_buffer.upload_dynamic_data(&directional_lights[..], BufferUsage::STORAGE);

    cmd_buffer.bind_uniform_buffer(BindingFrequency::Frequent, 3, &per_frame_buffer, 0, WHOLE_BUFFER);

    let pipeline = shader_manager.get_graphics_pipeline(self.pipeline);
    cmd_buffer.set_pipeline(PipelineBinding::Graphics(&pipeline));
    cmd_buffer.set_viewports(&[Viewport {
      position: Vec2::new(0.0f32, 0.0f32),
      extent: Vec2::new(rtv_info.width as f32, rtv_info.height as f32),
      min_depth: 0.0f32,
      max_depth: 1.0f32
    }]);
    cmd_buffer.set_scissors(&[Scissor {
      position: Vec2I::new(0, 0),
      extent: Vec2UI::new(9999, 9999),
    }]);

    //command_buffer.bind_storage_buffer(BindingFrequency::Frequent, 7, clusters);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::Frequent, 0, camera_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::Frequent, 1, &point_light_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::Frequent, 2, &light_bitmask_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 4, &ssao, &self.sampler);
    cmd_buffer.bind_storage_buffer(BindingFrequency::Frequent, 5, &directional_light_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 6, &lightmap.view, &self.sampler);
    cmd_buffer.bind_sampler(BindingFrequency::Frequent, 7, &self.sampler);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::Frequent, 8,  &shadows, &self.sampler);
    cmd_buffer.bind_storage_buffer(BindingFrequency::Frequent, 9, gpu_scene, 0, WHOLE_BUFFER);

    cmd_buffer.set_vertex_buffer(vertex_buffer, 0);
    cmd_buffer.set_index_buffer(index_buffer, 0, IndexFormat::U32);

    cmd_buffer.finish_binding();
    cmd_buffer.draw_indexed_indirect(&draw_buffer, 4, &draw_buffer, 0, DRAW_CAPACITY, 20);

    cmd_buffer.end_render_pass();
    cmd_buffer.end_label();
  }
}
