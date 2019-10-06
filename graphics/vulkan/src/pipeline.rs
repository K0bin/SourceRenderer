use std::sync::Arc;
use std::ffi::CStr;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::InputRate;
use sourcerenderer_core::graphics::PipelineInfo;
use sourcerenderer_core::graphics::Pipeline;
use sourcerenderer_core::graphics::ShaderType;
use sourcerenderer_core::graphics::Shader;
use sourcerenderer_core::graphics::FillMode;
use sourcerenderer_core::graphics::CullMode;
use sourcerenderer_core::graphics::FrontFace;
use sourcerenderer_core::graphics::SampleCount;
use sourcerenderer_core::graphics::CompareFunc;
use sourcerenderer_core::graphics::StencilOp;
use sourcerenderer_core::graphics::LogicOp;
use sourcerenderer_core::graphics::BlendFactor;
use sourcerenderer_core::graphics::BlendOp;
use sourcerenderer_core::graphics::ColorComponents;
use sourcerenderer_core::graphics::StoreOp;
use sourcerenderer_core::graphics::LoadOp;
use sourcerenderer_core::graphics::ImageLayout;
use sourcerenderer_core::graphics::AttachmentRef;

use crate::VkDevice;
use crate::format::format_to_vk;
use crate::VkRenderPassLayout;

pub fn input_rate_to_vk(input_rate: InputRate) -> vk::VertexInputRate {
  return match input_rate {
    InputRate::PerVertex => vk::VertexInputRate::VERTEX,
    InputRate::PerInstance => vk::VertexInputRate::INSTANCE
  }
}

pub struct VkShader {
  shader_type: ShaderType,
  shader_module: vk::ShaderModule,
  device: ash::Device
}

impl VkShader {
  pub fn new(device: &VkDevice, shader_type: ShaderType, bytecode: &Vec<u8>) -> Self {
    let create_info = vk::ShaderModuleCreateInfo {
      code_size: bytecode.len(),
      p_code: bytecode.as_ptr() as *const u32,
      ..Default::default()
    };
    let vk_device = device.get_ash_device().clone();
    let shader_module = unsafe { vk_device.create_shader_module(&create_info, None).unwrap() };

    return VkShader {
      shader_type: shader_type,
      shader_module: shader_module,
      device: vk_device
    };
  }

  fn get_shader_module(&self) -> vk::ShaderModule {
    return self.shader_module.clone();
  }
}

impl Shader for VkShader {
  fn get_shader_type(&self) -> ShaderType {
    return self.shader_type;
  }
}

impl Drop for VkShader {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_shader_module(self.shader_module, None);
    }
  }
}

pub struct VkPipeline {
  pipeline: vk::Pipeline,
  device: Arc<VkDevice>
}

const SHADER_ENTRY_POINT_NAME: &str = "main";

pub fn shader_type_to_vk(shader_type: ShaderType) -> vk::ShaderStageFlags {
  return match shader_type {
    ShaderType::VertexShader => vk::ShaderStageFlags::VERTEX,
    ShaderType::FragmentShader => vk::ShaderStageFlags::FRAGMENT,
    ShaderType::GeometryShader => vk::ShaderStageFlags::GEOMETRY,
    ShaderType::TessellationControlShader => vk::ShaderStageFlags::TESSELLATION_CONTROL,
    ShaderType::TessellationEvaluationShader => vk::ShaderStageFlags::TESSELLATION_EVALUATION,
    ShaderType::ComputeShader => vk::ShaderStageFlags::COMPUTE
  };
}

pub fn samples_to_vk(samples: SampleCount) -> vk::SampleCountFlags {
  return match samples {
    SampleCount::Samples1 => vk::SampleCountFlags::TYPE_1,
    SampleCount::Samples2 => vk::SampleCountFlags::TYPE_2,
    SampleCount::Samples4 => vk::SampleCountFlags::TYPE_4,
    SampleCount::Samples8 => vk::SampleCountFlags::TYPE_8,
  };
}

pub fn compare_func_to_vk(compare_func: CompareFunc) -> vk::CompareOp {
  return match compare_func {
    CompareFunc::Always => vk::CompareOp::ALWAYS,
    CompareFunc::NotEqual => vk::CompareOp::NOT_EQUAL,
    CompareFunc::Never => vk::CompareOp::NEVER,
    CompareFunc::Less => vk::CompareOp::LESS,
    CompareFunc::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
    CompareFunc::Equal => vk::CompareOp::EQUAL,
    CompareFunc::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
    CompareFunc::Greater => vk::CompareOp::GREATER,
  };
}

pub fn stencil_op_to_vk(stencil_op: StencilOp) -> vk::StencilOp {
  return match stencil_op {
    StencilOp::Decrease => vk::StencilOp::DECREMENT_AND_WRAP,
    StencilOp::Increase => vk::StencilOp::INCREMENT_AND_WRAP,
    StencilOp::DecreaseClamp => vk::StencilOp::DECREMENT_AND_CLAMP,
    StencilOp::IncreaseClamp => vk::StencilOp::INCREMENT_AND_CLAMP,
    StencilOp::Invert => vk::StencilOp::INVERT,
    StencilOp::Keep => vk::StencilOp::KEEP,
    StencilOp::Replace => vk::StencilOp::REPLACE,
    StencilOp::Zero => vk::StencilOp::ZERO
  };
}

pub fn logic_op_to_vk(logic_op: LogicOp) -> vk::LogicOp {
  return match logic_op {
    LogicOp::And => vk::LogicOp::AND,
    LogicOp::AndInverted => vk::LogicOp::AND_INVERTED,
    LogicOp::AndReversed => vk::LogicOp::AND_REVERSE,
    LogicOp::Clear => vk::LogicOp::CLEAR,
    LogicOp::Copy => vk::LogicOp::COPY,
    LogicOp::CopyInverted => vk::LogicOp::COPY_INVERTED,
    LogicOp::Equivalent => vk::LogicOp::EQUIVALENT,
    LogicOp::Invert => vk::LogicOp::INVERT,
    LogicOp::Nand => vk::LogicOp::NAND,
    LogicOp::Noop => vk::LogicOp::NO_OP,
    LogicOp::Nor => vk::LogicOp::NOR,
    LogicOp::Or => vk::LogicOp::OR,
    LogicOp::OrInverted => vk::LogicOp::OR_INVERTED,
    LogicOp::OrReverse => vk::LogicOp::OR_REVERSE,
    LogicOp::Set => vk::LogicOp::SET,
    LogicOp::Xor => vk::LogicOp::XOR,
  };
}

pub fn blend_factor_to_vk(blend_factor: BlendFactor) -> vk::BlendFactor {
  return match blend_factor {
    BlendFactor::ConstantColor => vk::BlendFactor::CONSTANT_COLOR,
    BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
    BlendFactor::DstColor => vk::BlendFactor::DST_COLOR,
    BlendFactor::One => vk::BlendFactor::ONE,
    BlendFactor::OneMinusConstantColor => vk::BlendFactor::ONE_MINUS_CONSTANT_COLOR,
    BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
    BlendFactor::OneMinusDstColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
    BlendFactor::OneMinusSrc1Alpha => vk::BlendFactor::ONE_MINUS_SRC1_ALPHA,
    BlendFactor::OneMinusSrc1Color => vk::BlendFactor::ONE_MINUS_SRC1_COLOR,
    BlendFactor::OneMinusSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
    BlendFactor::Src1Alpha => vk::BlendFactor::SRC1_ALPHA,
    BlendFactor::Src1Color => vk::BlendFactor::SRC1_COLOR,
    BlendFactor::SrcAlphaSaturate => vk::BlendFactor::SRC_ALPHA_SATURATE,
    BlendFactor::SrcColor => vk::BlendFactor::SRC_COLOR,
    BlendFactor::Zero => vk::BlendFactor::ZERO,
  };
}

pub fn blend_op_to_vk(blend_op: BlendOp) -> vk::BlendOp {
  return match blend_op {
    BlendOp::Add => vk::BlendOp::ADD,
    BlendOp::Max => vk::BlendOp::MAX,
    BlendOp::Min => vk::BlendOp::MIN,
    BlendOp::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
    BlendOp::Subtract => vk::BlendOp::SUBTRACT
  };
}

pub fn color_components_to_vk(color_components: ColorComponents) -> vk::ColorComponentFlags {
  let components_bits = color_components.bits() as u32;
  let mut colors = 0u32;
  colors |= components_bits.rotate_left(ColorComponents::RED.bits().trailing_zeros() - vk::ColorComponentFlags::R.as_raw().trailing_zeros()) & vk::ColorComponentFlags::R.as_raw();
  colors |= components_bits.rotate_left(ColorComponents::GREEN.bits().trailing_zeros() - vk::ColorComponentFlags::G.as_raw().trailing_zeros()) & vk::ColorComponentFlags::G.as_raw();
  colors |= components_bits.rotate_left(ColorComponents::BLUE.bits().trailing_zeros() - vk::ColorComponentFlags::B.as_raw().trailing_zeros()) & vk::ColorComponentFlags::B.as_raw();
  colors |= components_bits.rotate_left(ColorComponents::ALPHA.bits().trailing_zeros() - vk::ColorComponentFlags::A.as_raw().trailing_zeros()) & vk::ColorComponentFlags::A.as_raw();
  return vk::ColorComponentFlags::from_raw(colors);
}


impl VkPipeline {
  pub fn new(device: Arc<VkDevice>, info: &PipelineInfo) -> Self {
    let vk_device = device.get_ash_device();

    let pipeline = unsafe {
      let mut shader_stages: Vec<vk::PipelineShaderStageCreateInfo> = Vec::new();

      {
        let shader = info.vs.clone();
        let shader_ptr = Arc::into_raw(shader) as *const VkShader;
        let shader_vk: Arc<VkShader> = Arc::from_raw(shader_ptr);
        let shader_stage = vk::PipelineShaderStageCreateInfo {
          module: shader_vk.get_shader_module(),
          p_name: SHADER_ENTRY_POINT_NAME.as_ptr() as *const i8,
          stage: shader_type_to_vk(shader_vk.get_shader_type()),
          ..Default::default()
        };
        shader_stages.push(shader_stage);
      }

      if let Some(shader) = info.fs.clone() {
        let shader_ptr = Arc::into_raw(shader) as *const VkShader;
        let shader_vk: Arc<VkShader> = Arc::from_raw(shader_ptr);
        let shader_stage = vk::PipelineShaderStageCreateInfo {
          module: shader_vk.get_shader_module(),
          p_name: SHADER_ENTRY_POINT_NAME.as_ptr() as *const i8,
          stage: shader_type_to_vk(shader_vk.get_shader_type()),
          ..Default::default()
        };
        shader_stages.push(shader_stage);
      }

      if let Some(shader) = info.gs.clone() {
        let shader_ptr = Arc::into_raw(shader) as *const VkShader;
        let shader_vk: Arc<VkShader> = Arc::from_raw(shader_ptr);
        let shader_stage = vk::PipelineShaderStageCreateInfo {
          module: shader_vk.get_shader_module(),
          p_name: SHADER_ENTRY_POINT_NAME.as_ptr() as *const i8,
          stage: shader_type_to_vk(shader_vk.get_shader_type()),
          ..Default::default()
        };
        shader_stages.push(shader_stage);
      }

      if let Some(shader) = info.tes.clone() {
        let shader_ptr = Arc::into_raw(shader) as *const VkShader;
        let shader_vk: Arc<VkShader> = Arc::from_raw(shader_ptr);
        let shader_stage = vk::PipelineShaderStageCreateInfo {
          module: shader_vk.get_shader_module(),
          p_name: SHADER_ENTRY_POINT_NAME.as_ptr() as *const i8,
          stage: shader_type_to_vk(shader_vk.get_shader_type()),
          ..Default::default()
        };
        shader_stages.push(shader_stage);
      }

      if let Some(shader) = info.tcs.clone() {
        let shader_ptr = Arc::into_raw(shader) as *const VkShader;
        let shader_vk: Arc<VkShader> = Arc::from_raw(shader_ptr);
        let shader_stage = vk::PipelineShaderStageCreateInfo {
          module: shader_vk.get_shader_module(),
          p_name: SHADER_ENTRY_POINT_NAME.as_ptr() as *const i8,
          stage: shader_type_to_vk(shader_vk.get_shader_type()),
          ..Default::default()
        };
        shader_stages.push(shader_stage);
      }

      let mut attribute_descriptions: Vec<vk::VertexInputAttributeDescription> = Vec::new();
      let mut binding_descriptions: Vec<vk::VertexInputBindingDescription> = Vec::new();
      for i in 0..info.vertex_layout.elements.len() {
        let element = &info.vertex_layout.elements[i];
        attribute_descriptions.push(vk::VertexInputAttributeDescription {
          location: i as u32,
          binding: i as u32,
          format: format_to_vk(element.format),
          offset: element.offset as u32
        });

        binding_descriptions.push(vk::VertexInputBindingDescription {
          binding: i as u32,
          stride: element.stride as u32,
          input_rate: input_rate_to_vk(element.input_rate)
        });
      }
      let vertex_input_create_info = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: binding_descriptions.len() as u32,
        p_vertex_binding_descriptions: binding_descriptions.as_ptr(),
        vertex_attribute_description_count: attribute_descriptions.len() as u32,
        p_vertex_attribute_descriptions: attribute_descriptions.as_ptr(),
        ..Default::default()
      };

      let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo {
        topology: vk::PrimitiveTopology::TRIANGLE_LIST,
        primitive_restart_enable: false as u32,
        ..Default::default()
      };

      let rasterizer_create_info = vk::PipelineRasterizationStateCreateInfo {
        polygon_mode: match &info.rasterizer.fill_mode {
          FillMode::Fill => vk::PolygonMode::FILL,
          FillMode::Line => vk::PolygonMode::LINE
        },
        cull_mode: match &info.rasterizer.cull_mode {
          CullMode::Back => vk::CullModeFlags::BACK,
          CullMode::Front => vk::CullModeFlags::FRONT,
          CullMode::None => vk::CullModeFlags::NONE
        },
        front_face: match &info.rasterizer.front_face {
          FrontFace::Clockwise => vk::FrontFace::CLOCKWISE,
          FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE
        },
        line_width: 1.0f32,
        ..Default::default()
      };

      let multisample_create_info = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: samples_to_vk(info.rasterizer.sample_count),
        alpha_to_coverage_enable: info.blend.alpha_to_coverage_enabled as u32,
        ..Default::default()
      };

      let depth_stencil_create_info = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: info.depth_stencil.depth_test_enabled as u32,
        depth_write_enable: info.depth_stencil.depth_write_enabled as u32,
        depth_compare_op: compare_func_to_vk(info.depth_stencil.depth_func),
        stencil_test_enable: info.depth_stencil.stencil_enable as u32,
        front: vk::StencilOpState {
          pass_op: stencil_op_to_vk(info.depth_stencil.stencil_front.pass_op),
          fail_op: stencil_op_to_vk(info.depth_stencil.stencil_front.fail_op),
          depth_fail_op: stencil_op_to_vk(info.depth_stencil.stencil_front.depth_fail_op),
          compare_op: compare_func_to_vk(info.depth_stencil.stencil_front.func),
          write_mask: info.depth_stencil.stencil_write_mask as u32,
          compare_mask: info.depth_stencil.stencil_read_mask as u32,
          reference: 0u32
        },
        back: vk::StencilOpState {
          pass_op: stencil_op_to_vk(info.depth_stencil.stencil_back.pass_op),
          fail_op: stencil_op_to_vk(info.depth_stencil.stencil_back.fail_op),
          depth_fail_op: stencil_op_to_vk(info.depth_stencil.stencil_back.depth_fail_op),
          compare_op: compare_func_to_vk(info.depth_stencil.stencil_back.func),
          write_mask: info.depth_stencil.stencil_write_mask as u32,
          compare_mask: info.depth_stencil.stencil_read_mask as u32,
          reference: 0u32
        },
        ..Default::default()
      };

      let mut blend_attachments: Vec<vk::PipelineColorBlendAttachmentState> = Vec::new();
      for blend in &info.blend.attachments {
        blend_attachments.push(vk::PipelineColorBlendAttachmentState {
          blend_enable: blend.blend_enabled as u32,
          src_color_blend_factor: blend_factor_to_vk(blend.src_color_blend_factor),
          dst_color_blend_factor: blend_factor_to_vk(blend.dst_color_blend_factor),
          color_blend_op: blend_op_to_vk(blend.color_blend_op),
          src_alpha_blend_factor: blend_factor_to_vk(blend.src_alpha_blend_factor),
          dst_alpha_blend_factor: blend_factor_to_vk(blend.dst_alpha_blend_factor),
          alpha_blend_op: blend_op_to_vk(blend.alpha_blend_op),
          color_write_mask: color_components_to_vk(blend.write_mask)
        });
      }
      let blend_create_info = vk::PipelineColorBlendStateCreateInfo {
        logic_op_enable: info.blend.logic_op_enabled as u32,
        logic_op: logic_op_to_vk(info.blend.logic_op),
        p_attachments: blend_attachments.as_ptr(),
        attachment_count: blend_attachments.len() as u32,
        blend_constants: info.blend.constants,
        ..Default::default()
      };

      let dynamic_state = [ vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR, vk::DynamicState::STENCIL_REFERENCE ];
      let dynamic_state_create_info = vk::PipelineDynamicStateCreateInfo {
        p_dynamic_states: dynamic_state.as_ptr(),
        dynamic_state_count: dynamic_state.len() as u32,
        ..Default::default()
      };

      let layout_create_info = vk::PipelineLayoutCreateInfo {
        ..Default::default()
      };
      let layout = vk_device.create_pipeline_layout(&layout_create_info, None).unwrap();

      let viewport_info = vk::PipelineViewportStateCreateInfo {
        viewport_count: 1,
        p_viewports: &vk::Viewport {
          x: 0f32,
          y: 0f32,
          width: 0f32,
          height: 0f32,
          min_depth: 0f32,
          max_depth: 1f32
        },
        scissor_count: 1,
        p_scissors: &vk::Rect2D {
          offset: vk::Offset2D {
            x: 0i32,
            y: 0i32
          },
          extent: vk::Extent2D {
            width: 0u32,
            height: 0u32
          }
        },
        ..Default::default()
      };

      let vk_renderpass_layout = Arc::from_raw(Arc::into_raw(info.renderpass.clone()) as *const VkRenderPassLayout);

      let pipeline_create_info = vk::GraphicsPipelineCreateInfo {
        stage_count: shader_stages.len() as u32,
        p_stages: shader_stages.as_ptr(),
        p_vertex_input_state: &vertex_input_create_info,
        p_input_assembly_state: &input_assembly_info,
        p_rasterization_state: &rasterizer_create_info,
        p_multisample_state: &multisample_create_info,
        p_depth_stencil_state: &depth_stencil_create_info,
        p_color_blend_state: &blend_create_info,
        p_viewport_state: &viewport_info,
        p_tessellation_state: &vk::PipelineTessellationStateCreateInfo::default(),
        p_dynamic_state: &dynamic_state_create_info,
        layout: layout,
        render_pass: *vk_renderpass_layout.get_handle(),
        subpass: info.subpass,
        base_pipeline_handle: vk::Pipeline::null(),
        base_pipeline_index: 0i32,
        ..Default::default()
      };

      vk_device.create_graphics_pipelines(vk::PipelineCache::null(), &[ pipeline_create_info ], None).unwrap()[0]
    };
    return VkPipeline {
      pipeline: pipeline,
      device: device
    };
  }
}

impl Drop for VkPipeline {
  fn drop(&mut self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.destroy_pipeline(self.pipeline, None);
    }
  }
}

impl Pipeline for VkPipeline {

}
