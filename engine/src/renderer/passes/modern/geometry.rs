use nalgebra::Vector2;
use smallvec::SmallVec;
use sourcerenderer_core::{Matrix4, graphics::{AddressMode, AttachmentBlendInfo, AttachmentInfo, Backend as GraphicsBackend, BindingFrequency, BlendInfo, BufferUsage, CommandBuffer, CompareFunc, CullMode, DepthStencilAttachmentRef, DepthStencilInfo, Device, FillMode, Filter, Format, FrontFace, GraphicsPipelineInfo, InputAssemblerElement, InputRate, LoadOp, LogicOp, OutputAttachmentRef, PipelineBinding, PrimitiveType, RasterizerInfo, RenderPassAttachment, RenderPassAttachmentView, RenderPassBeginInfo, RenderPassInfo, RenderpassRecordingMode, SampleCount, SamplerInfo, Scissor, ShaderInputElement, ShaderType, StencilInfo, StoreOp, SubpassInfo, Swapchain, Texture, TextureInfo, TextureRenderTargetView, TextureRenderTargetViewInfo, TextureSamplingViewInfo, TextureUsage, VertexLayoutInfo, Viewport, TextureLayout, BarrierSync, BarrierAccess, IndexFormat, TextureDepthStencilViewInfo, WHOLE_BUFFER}};
use std::{sync::Arc, cell::Ref};
use crate::renderer::{PointLight, drawable::View, light::DirectionalLight, renderer_scene::RendererScene, renderer_resources::{RendererResources, HistoryResourceEntry}, passes::{light_binning, ssao::SsaoPass, prepass::Prepass, rt_shadows::RTShadowPass}};
use sourcerenderer_core::{Platform, Vec2, Vec2I, Vec2UI};
use crate::renderer::passes::taa::scaled_halton_point;
use std::path::Path;
use std::io::Read;
use crate::renderer::renderer_assets::*;
use sourcerenderer_core::platform::io::IO;

use super::{draw_prep::DrawPrepPass, gpu_scene::DRAW_CAPACITY};

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

  pub fn new<P: Platform>(device: &Arc<B::Device>, swapchain: &Arc<B::Swapchain>, barriers: &mut RendererResources<B>) -> Self {
    let texture_info = TextureInfo {
      format: Format::RGBA8,
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
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("geometry_bindless.vert.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::VertexShader, &bytes, Some("geometry_bindless.vert.spv"))
    };

    let fragment_shader = {
      let mut file = <P::IO as IO>::open_asset(Path::new("shaders").join(Path::new("geometry_bindless.frag.spv"))).unwrap();
      let mut bytes: Vec<u8> = Vec::new();
      file.read_to_end(&mut bytes).unwrap();
      device.create_shader(ShaderType::FragmentShader, &bytes, Some("geometry_bindless.frag.spv"))
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
        attachments: vec![
          AttachmentBlendInfo::default()
        ]
      }
    };
    let pipeline = device.create_graphics_pipeline(&pipeline_info, &RenderPassInfo {
      attachments: vec![
        AttachmentInfo {
          format: texture_info.format,
          samples: texture_info.samples,
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
    }, 0, Some("BindlessGeometry"));

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
    gpu_scene: &Arc<B::Buffer>,
    zero_texture_view: &Arc<B::TextureSamplingView>,
    _zero_texture_view_black: &Arc<B::TextureSamplingView>,
    lightmap: &Arc<RendererTexture<B>>,
    swapchain_transform: Matrix4,
    frame: u64,
    barriers: &RendererResources<B>,
    camera_buffer: &Arc<B::Buffer>,
    vertex_buffer: &Arc<B::Buffer>,
    index_buffer: &Arc<B::Buffer>,
  ) {
    cmd_buffer.begin_label("Geometry pass");
    let draw_buffer = barriers.access_buffer(
      cmd_buffer,
      DrawPrepPass::<B>::INDIRECT_DRAW_BUFFER,
      BarrierSync::INDIRECT,
      BarrierAccess::INDIRECT_READ,
      HistoryResourceEntry::Current
    );

    let rtv_ref = barriers.access_rtv(
      cmd_buffer,
      Self::GEOMETRY_PASS_TEXTURE_NAME,
      BarrierSync::RENDER_TARGET,
      BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
      TextureLayout::RenderTarget, true,
      &TextureRenderTargetViewInfo::default(),
      HistoryResourceEntry::Current
    );
    let rtv = &*rtv_ref;

    let prepass_depth_ref = barriers.access_dsv(
      cmd_buffer,
      Prepass::<B>::DEPTH_TEXTURE_NAME,
      BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH,
      BarrierAccess::DEPTH_STENCIL_READ,
      TextureLayout::DepthStencilRead,
      false,
      &TextureDepthStencilViewInfo::default(),
      HistoryResourceEntry::Current
    );
    let prepass_depth = &*prepass_depth_ref;

    let ssao_ref = barriers.access_srv(
      cmd_buffer,
      SsaoPass::<B>::SSAO_TEXTURE_NAME,
      BarrierSync::FRAGMENT_SHADER | BarrierSync::COMPUTE_SHADER,
      BarrierAccess::SHADER_RESOURCE_READ,
      TextureLayout::Sampled,
      false,
      &TextureSamplingViewInfo::default(),
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
      rt_shadows = barriers.access_srv(
        cmd_buffer,
        RTShadowPass::<B>::SHADOWS_TEXTURE_NAME,
        BarrierSync::FRAGMENT_SHADER,
        BarrierAccess::SHADER_RESOURCE_READ,
        TextureLayout::Sampled,
        false,
        &TextureSamplingViewInfo::default(),
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
          store_op: StoreOp::DontCare
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
    }, RenderpassRecordingMode::Commands);

    let rtv_info = rtv.texture().info();
    let cluster_count = nalgebra::Vector3::<u32>::new(16, 9, 24);
    let near = view.near_plane;
    let far = view.far_plane;
    let cluster_z_scale = (cluster_count.z as f32) / (far / near).log2();
    let cluster_z_bias = -(cluster_count.z as f32) * (near).log2() / (far / near).log2();
    let per_frame = FrameData {
      swapchain_transform,
      halton_point: scaled_halton_point(rtv_info.width, rtv_info.height, (frame % 8) as u32),
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

    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 3, &per_frame_buffer, 0, WHOLE_BUFFER);

    cmd_buffer.set_pipeline(PipelineBinding::Graphics(&self.pipeline));
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

    //command_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 7, clusters);
    cmd_buffer.bind_uniform_buffer(BindingFrequency::PerFrame, 0, camera_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 1, &point_light_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 2, &light_bitmask_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerFrame, 4, &ssao, &self.sampler);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 5, &directional_light_buffer, 0, WHOLE_BUFFER);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerFrame, 6, &lightmap.view, &self.sampler);
    cmd_buffer.bind_sampler(BindingFrequency::PerFrame, 7, &self.sampler);
    cmd_buffer.bind_sampling_view_and_sampler(BindingFrequency::PerFrame, 8,  &shadows, &self.sampler);
    cmd_buffer.bind_storage_buffer(BindingFrequency::PerFrame, 9, gpu_scene, 0, WHOLE_BUFFER);

    cmd_buffer.set_vertex_buffer(vertex_buffer, 0);
    cmd_buffer.set_index_buffer(index_buffer, 0, IndexFormat::U32);

    cmd_buffer.finish_binding();
    cmd_buffer.draw_indexed_indirect(&draw_buffer, 4, &draw_buffer, 0, DRAW_CAPACITY, 20);

    cmd_buffer.end_render_pass();
    cmd_buffer.end_label();
  }
}