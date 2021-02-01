use std::sync::Arc;
use std::ffi::{CString};

use ash::vk;
use ash::version::DeviceV1_0;

use spirv_cross::{spirv, glsl};

use sourcerenderer_core::graphics::{InputRate, BindingFrequency};
use sourcerenderer_core::graphics::GraphicsPipelineInfo;
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
use sourcerenderer_core::graphics::PrimitiveType;

use crate::raw::RawVkDevice;
use crate::format::format_to_vk;
use crate::VkBackend;
use std::hash::{Hasher, Hash};
use crate::VkRenderPass;
use spirv_cross::spirv::Decoration;
use ash::vk::{Handle, PipelineRasterizationStateCreateFlags};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use crate::descriptor::{VkDescriptorSetLayout, VkDescriptorSetBindingInfo};
use crate::VkShared;
use std::os::raw::c_char;

#[inline]
pub(crate) fn input_rate_to_vk(input_rate: InputRate) -> vk::VertexInputRate {
  return match input_rate {
    InputRate::PerVertex => vk::VertexInputRate::VERTEX,
    InputRate::PerInstance => vk::VertexInputRate::INSTANCE
  }
}

pub struct VkShader {
  shader_type: ShaderType,
  shader_module: vk::ShaderModule,
  device: Arc<RawVkDevice>,
  descriptor_set_bindings: HashMap<u32, Vec<VkDescriptorSetBindingInfo>>
}

impl PartialEq for VkShader {
  fn eq(&self, other: &Self) -> bool {
    self.shader_module == other.shader_module
  }
}

impl Eq for VkShader {}

impl Hash for VkShader {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.shader_module.hash(state);
  }
}

impl VkShader {
  pub fn new(device: &Arc<RawVkDevice>, shader_type: ShaderType, bytecode: &[u8], name: Option<&str>) -> Self {
    let create_info = vk::ShaderModuleCreateInfo {
      code_size: bytecode.len(),
      p_code: bytecode.as_ptr() as *const u32,
      ..Default::default()
    };
    let vk_device = &device.device;
    let shader_module = unsafe { vk_device.create_shader_module(&create_info, None).unwrap() };

    let module = spirv::Module::from_words(unsafe { std::slice::from_raw_parts(bytecode.as_ptr() as *const u32, bytecode.len() / std::mem::size_of::<u32>()) });
    let ast = spirv::Ast::<glsl::Target>::parse(&module).expect("Failed to parse shader with SPIR-V Cross");
    let resources = ast.get_shader_resources().expect("Failed to get resources");

    let mut sets: HashMap<u32, Vec<VkDescriptorSetBindingInfo>> = HashMap::new();
    for resource in resources.sampled_images {
      let set_index = ast.get_decoration(resource.id, Decoration::DescriptorSet).unwrap();
      let set = sets.entry(set_index).or_insert(Vec::new());
      set.push(VkDescriptorSetBindingInfo {
        index: ast.get_decoration(resource.id, Decoration::Binding).unwrap(),
        descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        shader_stage: shader_type_to_vk(shader_type)
      });
    }
    for resource in resources.subpass_inputs {
      let set_index = ast.get_decoration(resource.id, Decoration::DescriptorSet).unwrap();
      let set = sets.entry(set_index).or_insert(Vec::new());
      set.push(VkDescriptorSetBindingInfo {
        index: ast.get_decoration(resource.id, Decoration::Binding).unwrap(),
        descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        shader_stage: shader_type_to_vk(shader_type)
      });
    }
    for resource in resources.uniform_buffers {
      let set_index = ast.get_decoration(resource.id, Decoration::DescriptorSet).unwrap();
      let set = sets.entry(set_index).or_insert(Vec::new());
      set.push(VkDescriptorSetBindingInfo {
        index: ast.get_decoration(resource.id, Decoration::Binding).unwrap(),
        descriptor_type: if set_index == BindingFrequency::PerDraw as u32 { vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC } else { vk::DescriptorType::UNIFORM_BUFFER },
        shader_stage: shader_type_to_vk(shader_type)
      });
    }
    for resource in resources.storage_buffers {
      let set_index = ast.get_decoration(resource.id, Decoration::DescriptorSet).unwrap();
      let set = sets.entry(set_index).or_insert(Vec::new());
      set.push(VkDescriptorSetBindingInfo {
        index: ast.get_decoration(resource.id, Decoration::Binding).unwrap(),
        descriptor_type: if set_index == BindingFrequency::PerDraw as u32 { vk::DescriptorType::STORAGE_BUFFER_DYNAMIC } else { vk::DescriptorType::STORAGE_BUFFER },
        shader_stage: shader_type_to_vk(shader_type)
      });
    }

    if let Some(name) = name {
      if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
        let name_cstring = CString::new(name).unwrap();
        unsafe {
          debug_utils.debug_utils_loader.debug_utils_set_object_name(device.handle(), &vk::DebugUtilsObjectNameInfoEXT {
            object_type: vk::ObjectType::SHADER_MODULE,
            object_handle: shader_module.as_raw(),
            p_object_name: name_cstring.as_ptr(),
            ..Default::default()
          });
        }
      }
    }

    return VkShader {
      shader_type,
      shader_module,
      device: device.clone(),
      descriptor_set_bindings: sets
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
      let vk_device = &self.device.device;
      vk_device.destroy_shader_module(self.shader_module, None);
    }
  }
}

pub struct VkPipeline {
  pipeline: vk::Pipeline,
  layout: Arc<VkPipelineLayout>,
  device: Arc<RawVkDevice>,
  is_graphics: bool
}

impl PartialEq for VkPipeline {
  fn eq(&self, other: &Self) -> bool {
    self.pipeline == other.pipeline
  }
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

#[derive(Hash, Eq, PartialEq)]
pub struct VkGraphicsPipelineInfo<'a> {
  pub info: &'a GraphicsPipelineInfo<VkBackend>,
  pub render_pass: &'a Arc<VkRenderPass>,
  pub sub_pass: u32
}

impl VkPipeline {
  pub fn new_graphics(device: &Arc<RawVkDevice>, info: &VkGraphicsPipelineInfo, shared: &VkShared) -> Self {
    let vk_device = &device.device;
    let mut shader_stages: Vec<vk::PipelineShaderStageCreateInfo> = Vec::new();
    let mut descriptor_set_layout_bindings: [Vec<VkDescriptorSetBindingInfo>; 4] = Default::default();

    let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();

    {
      let shader = info.info.vs.clone();
      let shader_stage = vk::PipelineShaderStageCreateInfo {
        module: shader.get_shader_module(),
        p_name: entry_point.as_ptr() as *const c_char,
        stage: shader_type_to_vk(shader.get_shader_type()),
        ..Default::default()
      };
      shader_stages.push(shader_stage);
      for (index, shader_set) in &shader.descriptor_set_bindings {
        let set = &mut descriptor_set_layout_bindings[*index as usize];
        for binding in shader_set {
          let existing_binding_option = set.iter_mut().find(|existing_binding| existing_binding.index == binding.index);
          if let Some(existing_binding) = existing_binding_option {
            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
            existing_binding.shader_stage |= binding.shader_stage;
          } else {
            set.push(binding.clone());
          }
        }
      }
    }

    if let Some(shader) = info.info.fs.clone() {
      let shader_stage = vk::PipelineShaderStageCreateInfo {
        module: shader.get_shader_module(),
        p_name: entry_point.as_ptr() as *const c_char,
        stage: shader_type_to_vk(shader.get_shader_type()),
        ..Default::default()
      };
      shader_stages.push(shader_stage);
      for (index, shader_set) in &shader.descriptor_set_bindings {
        let set = &mut descriptor_set_layout_bindings[*index as usize];
        for binding in shader_set {
          let existing_binding_option = set.iter_mut().find(|existing_binding| existing_binding.index == binding.index);
          if let Some(existing_binding) = existing_binding_option {
            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
            existing_binding.shader_stage |= binding.shader_stage;
          } else {
            set.push(binding.clone());
          }
        }
      }
    }

    if let Some(shader) = info.info.gs.clone() {
      let shader_stage = vk::PipelineShaderStageCreateInfo {
        module: shader.get_shader_module(),
        p_name: entry_point.as_ptr() as *const c_char,
        stage: shader_type_to_vk(shader.get_shader_type()),
        ..Default::default()
      };
      shader_stages.push(shader_stage);
      for (index, shader_set) in &shader.descriptor_set_bindings {
        let set = &mut descriptor_set_layout_bindings[*index as usize];
        for binding in shader_set {
          let existing_binding_option = set.iter_mut().find(|existing_binding| existing_binding.index == binding.index);
          if let Some(existing_binding) = existing_binding_option {
            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
            existing_binding.shader_stage |= binding.shader_stage;
          } else {
            set.push(binding.clone());
          }
        }
      }
    }

    if let Some(shader) = info.info.tes.clone() {
      let shader_stage = vk::PipelineShaderStageCreateInfo {
        module: shader.get_shader_module(),
        p_name: entry_point.as_ptr() as *const c_char,
        stage: shader_type_to_vk(shader.get_shader_type()),
        ..Default::default()
      };
      shader_stages.push(shader_stage);
      for (index, shader_set) in &shader.descriptor_set_bindings {
        let set = &mut descriptor_set_layout_bindings[*index as usize];
        for binding in shader_set {
          let existing_binding_option = set.iter_mut().find(|existing_binding| existing_binding.index == binding.index);
          if let Some(existing_binding) = existing_binding_option {
            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
            existing_binding.shader_stage |= binding.shader_stage;
          } else {
            set.push(binding.clone());
          }
        }
      }
    }

    if let Some(shader) = info.info.tcs.clone() {
      let shader_stage = vk::PipelineShaderStageCreateInfo {
        module: shader.get_shader_module(),
        p_name: entry_point.as_ptr() as *const c_char,
        stage: shader_type_to_vk(shader.get_shader_type()),
        ..Default::default()
      };
      shader_stages.push(shader_stage);
      for (index, shader_set) in &shader.descriptor_set_bindings {
        let set = &mut descriptor_set_layout_bindings[*index as usize];
        for binding in shader_set {
          let existing_binding_option = set.iter_mut().find(|existing_binding| existing_binding.index == binding.index);
          if let Some(existing_binding) = existing_binding_option {
            assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
            existing_binding.shader_stage |= binding.shader_stage;
          } else {
            set.push(binding.clone());
          }
        }
      }
    }

    let mut attribute_descriptions: Vec<vk::VertexInputAttributeDescription> = Vec::new();
    let mut binding_descriptions: Vec<vk::VertexInputBindingDescription> = Vec::new();
    for element in &info.info.vertex_layout.shader_inputs {
      attribute_descriptions.push(vk::VertexInputAttributeDescription {
        location: element.location_vk_mtl,
        binding: element.input_assembler_binding,
        format: format_to_vk(element.format),
        offset: element.offset as u32
      });
    }

    for element in &info.info.vertex_layout.input_assembler {
      binding_descriptions.push(vk::VertexInputBindingDescription {
        binding: element.binding,
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
      topology: match info.info.primitive_type {
        PrimitiveType::Triangles => vk::PrimitiveTopology::TRIANGLE_LIST,
        PrimitiveType::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
        PrimitiveType::Lines => vk::PrimitiveTopology::LINE_LIST,
        PrimitiveType::LineStrip => vk::PrimitiveTopology::LINE_STRIP,
        PrimitiveType::Points => vk::PrimitiveTopology::POINT_LIST,
      },
      primitive_restart_enable: false as u32,
      ..Default::default()
    };

    let rasterizer_create_info = vk::PipelineRasterizationStateCreateInfo {
      flags: PipelineRasterizationStateCreateFlags::empty(),
      depth_clamp_enable: vk::FALSE,
      rasterizer_discard_enable: vk::FALSE,
      polygon_mode: match &info.info.rasterizer.fill_mode {
        FillMode::Fill => vk::PolygonMode::FILL,
        FillMode::Line => vk::PolygonMode::LINE
      },
      cull_mode: match &info.info.rasterizer.cull_mode {
        CullMode::Back => vk::CullModeFlags::BACK,
        CullMode::Front => vk::CullModeFlags::FRONT,
        CullMode::None => vk::CullModeFlags::NONE
      },
      front_face: match &info.info.rasterizer.front_face {
        FrontFace::Clockwise => vk::FrontFace::CLOCKWISE,
        FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE
      },
      depth_bias_enable: vk::FALSE,
      depth_bias_constant_factor: 0.0f32,
      depth_bias_clamp: 0.0f32,
      depth_bias_slope_factor: 0.0f32,
      line_width: 1.0f32,
      ..Default::default()
    };

    let multisample_create_info = vk::PipelineMultisampleStateCreateInfo {
      rasterization_samples: samples_to_vk(info.info.rasterizer.sample_count),
      alpha_to_coverage_enable: info.info.blend.alpha_to_coverage_enabled as u32,
      ..Default::default()
    };

    let depth_stencil_create_info = vk::PipelineDepthStencilStateCreateInfo {
      depth_test_enable: info.info.depth_stencil.depth_test_enabled as u32,
      depth_write_enable: info.info.depth_stencil.depth_write_enabled as u32,
      depth_compare_op: compare_func_to_vk(info.info.depth_stencil.depth_func),
      depth_bounds_test_enable: vk::FALSE,
      stencil_test_enable: info.info.depth_stencil.stencil_enable as u32,
      front: vk::StencilOpState {
        pass_op: stencil_op_to_vk(info.info.depth_stencil.stencil_front.pass_op),
        fail_op: stencil_op_to_vk(info.info.depth_stencil.stencil_front.fail_op),
        depth_fail_op: stencil_op_to_vk(info.info.depth_stencil.stencil_front.depth_fail_op),
        compare_op: compare_func_to_vk(info.info.depth_stencil.stencil_front.func),
        write_mask: info.info.depth_stencil.stencil_write_mask as u32,
        compare_mask: info.info.depth_stencil.stencil_read_mask as u32,
        reference: 0u32
      },
      back: vk::StencilOpState {
        pass_op: stencil_op_to_vk(info.info.depth_stencil.stencil_back.pass_op),
        fail_op: stencil_op_to_vk(info.info.depth_stencil.stencil_back.fail_op),
        depth_fail_op: stencil_op_to_vk(info.info.depth_stencil.stencil_back.depth_fail_op),
        compare_op: compare_func_to_vk(info.info.depth_stencil.stencil_back.func),
        write_mask: info.info.depth_stencil.stencil_write_mask as u32,
        compare_mask: info.info.depth_stencil.stencil_read_mask as u32,
        reference: 0u32
      },
      min_depth_bounds: 0.0,
      max_depth_bounds: 0.0,
      ..Default::default()
    };

    let mut blend_attachments: Vec<vk::PipelineColorBlendAttachmentState> = Vec::new();
    for blend in &info.info.blend.attachments {
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
      logic_op_enable: info.info.blend.logic_op_enabled as u32,
      logic_op: logic_op_to_vk(info.info.blend.logic_op),
      p_attachments: blend_attachments.as_ptr(),
      attachment_count: blend_attachments.len() as u32,
      blend_constants: info.info.blend.constants,
      ..Default::default()
    };

    let dynamic_state = [ vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR, vk::DynamicState::STENCIL_REFERENCE ];
    let dynamic_state_create_info = vk::PipelineDynamicStateCreateInfo {
      p_dynamic_states: dynamic_state.as_ptr(),
      dynamic_state_count: dynamic_state.len() as u32,
      ..Default::default()
    };

    let mut descriptor_set_layouts: [Option<Arc<VkDescriptorSetLayout>>; 4] = Default::default();
    for (index, bindings) in descriptor_set_layout_bindings.iter().enumerate() {
      let mut hasher = DefaultHasher::new();
      bindings.hash(&mut hasher);
      let hash = hasher.finish();

      let cache_lock = shared.get_descriptor_set_layouts();
      let existing_set_layout = {
        let cache = cache_lock.read().unwrap();
        cache.get(&hash).map(|entry| entry.clone())
      };
      let set_layout = existing_set_layout.unwrap_or_else(|| {
        let mut cache = cache_lock.write().unwrap();
        cache.insert(hash, Arc::new(VkDescriptorSetLayout::new(&bindings, device)));
        cache.get(&hash).unwrap().clone()
      });
      descriptor_set_layouts[index] = Some(set_layout);
      if index > 0 && descriptor_set_layouts[index - 1].is_none() {
        panic!("Non continous descriptor set ranges are unsupported.");
      }
    }

    let mut hasher = DefaultHasher::new();
    for (index, bindings) in descriptor_set_layout_bindings.iter().enumerate() {
      index.hash(&mut hasher);
      bindings.hash(&mut hasher);
    }
    let hash = hasher.finish();
    let cache_lock = shared.get_pipeline_layouts();
    let existing_handle = {
      let cache = cache_lock.read().unwrap();
      cache.get(&hash).map(|entry| entry.clone())
    };
    let layout = existing_handle.unwrap_or_else(|| {
      let mut cache = cache_lock.write().unwrap();
      cache.insert(hash, Arc::new(VkPipelineLayout::new(&descriptor_set_layouts, device)));
      cache.get(&hash).unwrap().clone()
    });

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
      layout: *layout.get_handle(),
      render_pass: *info.render_pass.get_handle(),
      subpass: info.sub_pass,
      base_pipeline_handle: vk::Pipeline::null(),
      base_pipeline_index: 0i32,
      ..Default::default()
    };

    let pipeline = unsafe {
      vk_device.create_graphics_pipelines(vk::PipelineCache::null(), &[ pipeline_create_info ], None).unwrap()[0]
    };
    return VkPipeline {
      pipeline,
      device: device.clone(),
      layout,
      is_graphics: true
    };
  }

  pub fn new_compute(device: &Arc<RawVkDevice>, shader: &Arc<VkShader>, shared: &VkShared) -> Self {
    let mut descriptor_set_layout_bindings: [Vec<VkDescriptorSetBindingInfo>; 4] = Default::default();
    let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();

    let shader_stage = vk::PipelineShaderStageCreateInfo {
      module: shader.get_shader_module(),
      p_name: entry_point.as_ptr() as *const c_char,
      stage: shader_type_to_vk(shader.get_shader_type()),
      ..Default::default()
    };

    for (index, shader_set) in &shader.descriptor_set_bindings {
      let set = &mut descriptor_set_layout_bindings[*index as usize];
      for binding in shader_set {
        let existing_binding_option = set.iter_mut().find(|existing_binding| existing_binding.index == binding.index);
        if let Some(existing_binding) = existing_binding_option {
          assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
          existing_binding.shader_stage |= binding.shader_stage;
        } else {
          set.push(binding.clone());
        }
      }
    }

    let mut descriptor_set_layouts: [Option<Arc<VkDescriptorSetLayout>>; 4] = Default::default();
    for (index, bindings) in descriptor_set_layout_bindings.iter().enumerate() {
      let mut hasher = DefaultHasher::new();
      bindings.hash(&mut hasher);
      let hash = hasher.finish();

      let cache_lock = shared.get_descriptor_set_layouts();
      let existing_set_layout = {
        let cache = cache_lock.read().unwrap();
        cache.get(&hash).map(|entry| entry.clone())
      };
      let set_layout = existing_set_layout.unwrap_or_else(|| {
        let mut cache = cache_lock.write().unwrap();
        cache.insert(hash, Arc::new(VkDescriptorSetLayout::new(&bindings, device)));
        cache.get(&hash).unwrap().clone()
      });
      descriptor_set_layouts[index] = Some(set_layout);
      if index > 0 && descriptor_set_layouts[index - 1].is_none() {
        panic!("Non continous descriptor set ranges are unsupported.");
      }
    }

    let mut hasher = DefaultHasher::new();
    for (index, bindings) in descriptor_set_layout_bindings.iter().enumerate() {
      index.hash(&mut hasher);
      bindings.hash(&mut hasher);
    }
    let hash = hasher.finish();
    let cache_lock = shared.get_pipeline_layouts();
    let existing_handle = {
      let cache = cache_lock.read().unwrap();
      cache.get(&hash).map(|entry| entry.clone())
    };
    let layout = existing_handle.unwrap_or_else(|| {
      let mut cache = cache_lock.write().unwrap();
      cache.insert(hash, Arc::new(VkPipelineLayout::new(&descriptor_set_layouts, device)));
      cache.get(&hash).unwrap().clone()
    });

    let pipeline_create_info = vk::ComputePipelineCreateInfo {
      flags: vk::PipelineCreateFlags::empty(),
      stage: shader_stage,
      layout: *layout.get_handle(),
      base_pipeline_handle: vk::Pipeline::null(),
      base_pipeline_index: 0,
      ..Default::default()
    };
    let pipeline = unsafe {
      device.create_compute_pipelines(vk::PipelineCache::null(), &[ pipeline_create_info ], None).unwrap()[0]
    };

    return VkPipeline {
      pipeline,
      device: device.clone(),
      layout,
      is_graphics: false
    };
  }

  #[inline]
  pub(crate) fn get_handle(&self) -> &vk::Pipeline {
    return &self.pipeline;
  }

  #[inline]
  pub(crate) fn get_layout(&self) -> &VkPipelineLayout {
    &self.layout
  }

  #[inline]
  pub(crate) fn is_graphics(&self) -> bool {
    self.is_graphics
  }
}

impl Drop for VkPipeline {
  fn drop(&mut self) {
    unsafe {
      let vk_device = &self.device.device;
      vk_device.destroy_pipeline(self.pipeline, None);
    }
  }
}

pub(crate) struct VkPipelineLayout {
  device: Arc<RawVkDevice>,
  layout: vk::PipelineLayout,
  descriptor_set_layouts: [Option<Arc<VkDescriptorSetLayout>>; 4]
}

impl VkPipelineLayout {
  pub fn new(descriptor_set_layouts: &[Option<Arc<VkDescriptorSetLayout>>; 4], device: &Arc<RawVkDevice>) -> Self {
    let layouts: Vec<vk::DescriptorSetLayout> = descriptor_set_layouts.iter()
      .filter(|descriptor_set_layout| descriptor_set_layout.is_some())
      .map(|descriptor_set_layout| {
        *descriptor_set_layout.as_ref().unwrap().get_handle()
      })
      .collect();
    let info = vk::PipelineLayoutCreateInfo {
      p_set_layouts: layouts.as_ptr(),
      set_layout_count: layouts.len() as u32,
      ..Default::default()
    };
    let layout = unsafe {
      device.create_pipeline_layout(&info, None)
    }.unwrap();
    Self {
      device: device.clone(),
      layout,
      descriptor_set_layouts: descriptor_set_layouts.clone()
    }
  }

  #[inline]
  pub(crate) fn get_handle(&self) -> &vk::PipelineLayout {
    &self.layout
  }

  #[inline]
  pub(crate) fn get_descriptor_set_layout(&self, index: u32) -> Option<&Arc<VkDescriptorSetLayout>> {
    self.descriptor_set_layouts[index as usize].as_ref()
  }
}

impl Drop for VkPipelineLayout {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_pipeline_layout(self.layout, None);
    }
  }
}
