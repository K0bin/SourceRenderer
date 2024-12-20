use std::{
    ffi::CString,
    hash::{
        Hash,
        Hasher,
    },
    os::raw::{c_char, c_void},
    sync::Arc,
};

use ash::vk::{self, Handle as _};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Buffer as _, Shader as _};

use super::*;

#[inline]
pub(super) fn input_rate_to_vk(input_rate: gpu::InputRate) -> vk::VertexInputRate {
    match input_rate {
        gpu::InputRate::PerVertex => vk::VertexInputRate::VERTEX,
        gpu::InputRate::PerInstance => vk::VertexInputRate::INSTANCE,
    }
}

pub struct VkShader {
    shader_type: gpu::ShaderType,
    shader_module: vk::ShaderModule,
    device: Arc<RawVkDevice>,
    descriptor_set_bindings: [SmallVec<[VkDescriptorSetEntryInfo; gpu::PER_SET_BINDINGS as usize]>; gpu::NON_BINDLESS_SET_COUNT as usize],
    push_constants_range: Option<vk::PushConstantRange>,
    uses_bindless_texture_set: bool,
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
    #[allow(clippy::size_of_in_element_count)]
    pub fn new(
        device: &Arc<RawVkDevice>,
        shader: gpu::PackedShader,
        name: Option<&str>,
    ) -> Self {
        assert_ne!(shader.shader_spirv.len(), 0);

        let create_info = vk::ShaderModuleCreateInfo {
            code_size: shader.shader_spirv.len(),
            p_code: shader.shader_spirv.as_ptr() as *const u32,
            ..Default::default()
        };
        let vk_device = &device.device;
        let shader_module = unsafe { vk_device.create_shader_module(&create_info, None).unwrap() };
        let mut sets: [SmallVec<[VkDescriptorSetEntryInfo; gpu::PER_SET_BINDINGS as usize]>; gpu::NON_BINDLESS_SET_COUNT as usize] = Default::default();
        let vk_shader_stage = shader_type_to_vk(shader.shader_type);

        for (set_index, set_metadata) in shader.resources.iter().enumerate() {
            let set = &mut sets[set_index];
            for binding_metadata in set_metadata.iter() {
                assert_eq!(binding_metadata.set, set_index as u32);
                set.push(VkDescriptorSetEntryInfo {
                    name: binding_metadata.name.clone(),
                    index: binding_metadata.binding,
                    descriptor_type: match binding_metadata.resource_type {
                        gpu::ResourceType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
                        gpu::ResourceType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
                        gpu::ResourceType::SubpassInput => vk::DescriptorType::INPUT_ATTACHMENT,
                        gpu::ResourceType::SampledTexture => vk::DescriptorType::SAMPLED_IMAGE,
                        gpu::ResourceType::StorageTexture => vk::DescriptorType::STORAGE_IMAGE,
                        gpu::ResourceType::Sampler => vk::DescriptorType::SAMPLER,
                        gpu::ResourceType::CombinedTextureSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        gpu::ResourceType::AccelerationStructure => vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                    },
                    shader_stage: vk_shader_stage,
                    count: binding_metadata.array_size,
                    writable: false,
                    flags: vk::DescriptorBindingFlags::empty(),
                });
            }
        }

        let push_constants_range = if shader.push_constant_size == 0 {
            None
        } else {
            Some(vk::PushConstantRange {
                stage_flags: match shader.shader_type {
                    gpu::ShaderType::VertexShader => vk::ShaderStageFlags::VERTEX,
                    gpu::ShaderType::FragmentShader => vk::ShaderStageFlags::FRAGMENT,
                    gpu::ShaderType::ComputeShader => vk::ShaderStageFlags::COMPUTE,
                    gpu::ShaderType::RayGen => vk::ShaderStageFlags::RAYGEN_KHR,
                    gpu::ShaderType::RayMiss => vk::ShaderStageFlags::MISS_KHR,
                    gpu::ShaderType::RayClosestHit => vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                    _ => unimplemented!(),
                },
                offset: 0u32,
                size: shader.push_constant_size as u32,
            })
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::SHADER_MODULE,
                                object_handle: shader_module.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }
        VkShader {
            shader_type: shader.shader_type,
            shader_module,
            device: device.clone(),
            descriptor_set_bindings: sets,
            push_constants_range,
            uses_bindless_texture_set: shader.uses_bindless_texture_set,
        }
    }

    fn shader_module(&self) -> vk::ShaderModule {
        self.shader_module
    }
}

impl gpu::Shader for VkShader {
    fn shader_type(&self) -> gpu::ShaderType {
        self.shader_type
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VkPipelineType {
    Graphics,
    Compute,
    RayTracing,
}

pub struct VkPipeline {
    pipeline: vk::Pipeline,
    layout: Arc<VkPipelineLayout>,
    device: Arc<RawVkDevice>,
    pipeline_type: VkPipelineType,
    uses_bindless_texture_set: bool,
    sbt: Option<VkShaderBindingTables>,
}

struct VkShaderBindingTables {
    buffer: vk::Buffer,
    buffer_offset: u64,
    raygen_region: vk::StridedDeviceAddressRegionKHR,
    closest_hit_region: vk::StridedDeviceAddressRegionKHR,
    miss_region: vk::StridedDeviceAddressRegionKHR,
}

impl PartialEq for VkPipeline {
    fn eq(&self, other: &Self) -> bool {
        self.pipeline == other.pipeline
    }
}

const SHADER_ENTRY_POINT_NAME: &str = "main";

pub(super) fn shader_type_to_vk(shader_type: gpu::ShaderType) -> vk::ShaderStageFlags {
    match shader_type {
        gpu::ShaderType::VertexShader => vk::ShaderStageFlags::VERTEX,
        gpu::ShaderType::FragmentShader => vk::ShaderStageFlags::FRAGMENT,
        gpu::ShaderType::GeometryShader => vk::ShaderStageFlags::GEOMETRY,
        gpu::ShaderType::TessellationControlShader => vk::ShaderStageFlags::TESSELLATION_CONTROL,
        gpu::ShaderType::TessellationEvaluationShader => vk::ShaderStageFlags::TESSELLATION_EVALUATION,
        gpu::ShaderType::ComputeShader => vk::ShaderStageFlags::COMPUTE,
        gpu::ShaderType::RayClosestHit => vk::ShaderStageFlags::CLOSEST_HIT_KHR,
        gpu::ShaderType::RayGen => vk::ShaderStageFlags::RAYGEN_KHR,
        gpu::ShaderType::RayMiss => vk::ShaderStageFlags::MISS_KHR,
    }
}

pub(super) fn samples_to_vk(samples: gpu::SampleCount) -> vk::SampleCountFlags {
    match samples {
        gpu::SampleCount::Samples1 => vk::SampleCountFlags::TYPE_1,
        gpu::SampleCount::Samples2 => vk::SampleCountFlags::TYPE_2,
        gpu::SampleCount::Samples4 => vk::SampleCountFlags::TYPE_4,
        gpu::SampleCount::Samples8 => vk::SampleCountFlags::TYPE_8,
    }
}

pub(super) fn compare_func_to_vk(compare_func: gpu::CompareFunc) -> vk::CompareOp {
    match compare_func {
        gpu::CompareFunc::Always => vk::CompareOp::ALWAYS,
        gpu::CompareFunc::NotEqual => vk::CompareOp::NOT_EQUAL,
        gpu::CompareFunc::Never => vk::CompareOp::NEVER,
        gpu::CompareFunc::Less => vk::CompareOp::LESS,
        gpu::CompareFunc::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
        gpu::CompareFunc::Equal => vk::CompareOp::EQUAL,
        gpu::CompareFunc::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
        gpu::CompareFunc::Greater => vk::CompareOp::GREATER,
    }
}

pub(super) fn stencil_op_to_vk(stencil_op: gpu::StencilOp) -> vk::StencilOp {
    match stencil_op {
        gpu::StencilOp::Decrease => vk::StencilOp::DECREMENT_AND_WRAP,
        gpu::StencilOp::Increase => vk::StencilOp::INCREMENT_AND_WRAP,
        gpu::StencilOp::DecreaseClamp => vk::StencilOp::DECREMENT_AND_CLAMP,
        gpu::StencilOp::IncreaseClamp => vk::StencilOp::INCREMENT_AND_CLAMP,
        gpu::StencilOp::Invert => vk::StencilOp::INVERT,
        gpu::StencilOp::Keep => vk::StencilOp::KEEP,
        gpu::StencilOp::Replace => vk::StencilOp::REPLACE,
        gpu::StencilOp::Zero => vk::StencilOp::ZERO,
    }
}

pub(super) fn logic_op_to_vk(logic_op: gpu::LogicOp) -> vk::LogicOp {
    match logic_op {
        gpu::LogicOp::And => vk::LogicOp::AND,
        gpu::LogicOp::AndInverted => vk::LogicOp::AND_INVERTED,
        gpu::LogicOp::AndReversed => vk::LogicOp::AND_REVERSE,
        gpu::LogicOp::Clear => vk::LogicOp::CLEAR,
        gpu::LogicOp::Copy => vk::LogicOp::COPY,
        gpu::LogicOp::CopyInverted => vk::LogicOp::COPY_INVERTED,
        gpu::LogicOp::Equivalent => vk::LogicOp::EQUIVALENT,
        gpu::LogicOp::Invert => vk::LogicOp::INVERT,
        gpu::LogicOp::Nand => vk::LogicOp::NAND,
        gpu::LogicOp::Noop => vk::LogicOp::NO_OP,
        gpu::LogicOp::Nor => vk::LogicOp::NOR,
        gpu::LogicOp::Or => vk::LogicOp::OR,
        gpu::LogicOp::OrInverted => vk::LogicOp::OR_INVERTED,
        gpu::LogicOp::OrReverse => vk::LogicOp::OR_REVERSE,
        gpu::LogicOp::Set => vk::LogicOp::SET,
        gpu::LogicOp::Xor => vk::LogicOp::XOR,
    }
}

pub(super) fn blend_factor_to_vk(blend_factor: gpu::BlendFactor) -> vk::BlendFactor {
    match blend_factor {
        gpu::BlendFactor::ConstantColor => vk::BlendFactor::CONSTANT_COLOR,
        gpu::BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
        gpu::BlendFactor::DstColor => vk::BlendFactor::DST_COLOR,
        gpu::BlendFactor::One => vk::BlendFactor::ONE,
        gpu::BlendFactor::OneMinusConstantColor => vk::BlendFactor::ONE_MINUS_CONSTANT_COLOR,
        gpu::BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        gpu::BlendFactor::OneMinusDstColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
        gpu::BlendFactor::OneMinusSrc1Alpha => vk::BlendFactor::ONE_MINUS_SRC1_ALPHA,
        gpu::BlendFactor::OneMinusSrc1Color => vk::BlendFactor::ONE_MINUS_SRC1_COLOR,
        gpu::BlendFactor::OneMinusSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        gpu::BlendFactor::Src1Alpha => vk::BlendFactor::SRC1_ALPHA,
        gpu::BlendFactor::Src1Color => vk::BlendFactor::SRC1_COLOR,
        gpu::BlendFactor::SrcAlphaSaturate => vk::BlendFactor::SRC_ALPHA_SATURATE,
        gpu::BlendFactor::SrcColor => vk::BlendFactor::SRC_COLOR,
        gpu::BlendFactor::Zero => vk::BlendFactor::ZERO,
        gpu::BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
        gpu::BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
    }
}

pub(super) fn blend_op_to_vk(blend_op: gpu::BlendOp) -> vk::BlendOp {
    match blend_op {
        gpu::BlendOp::Add => vk::BlendOp::ADD,
        gpu::BlendOp::Max => vk::BlendOp::MAX,
        gpu::BlendOp::Min => vk::BlendOp::MIN,
        gpu::BlendOp::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
        gpu::BlendOp::Subtract => vk::BlendOp::SUBTRACT,
    }
}

pub(super) fn color_components_to_vk(color_components: gpu::ColorComponents) -> vk::ColorComponentFlags {
    let components_bits = color_components.bits() as u32;
    let mut colors = 0u32;
    colors |= components_bits.rotate_left(
        gpu::ColorComponents::RED.bits().trailing_zeros()
            - vk::ColorComponentFlags::R.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::R.as_raw();
    colors |= components_bits.rotate_left(
        gpu::ColorComponents::GREEN.bits().trailing_zeros()
            - vk::ColorComponentFlags::G.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::G.as_raw();
    colors |= components_bits.rotate_left(
        gpu::ColorComponents::BLUE.bits().trailing_zeros()
            - vk::ColorComponentFlags::B.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::B.as_raw();
    colors |= components_bits.rotate_left(
        gpu::ColorComponents::ALPHA.bits().trailing_zeros()
            - vk::ColorComponentFlags::A.as_raw().trailing_zeros(),
    ) & vk::ColorComponentFlags::A.as_raw();
    vk::ColorComponentFlags::from_raw(colors)
}

#[derive(Default)]
struct DescriptorSetLayoutSetupContext {
    descriptor_set_layouts: [VkDescriptorSetLayoutKey; gpu::TOTAL_SET_COUNT as usize],
    dynamic_storage_buffers: [u32; gpu::TOTAL_SET_COUNT as usize],
    dynamic_uniform_buffers: [u32; gpu::TOTAL_SET_COUNT as usize],
    push_constants_ranges: [Option<VkConstantRange>; 3],
    uses_bindless_texture_set: bool,
    shader_stages: vk::ShaderStageFlags
}

fn add_shader_to_descriptor_set_layout_setup(device: &Arc<RawVkDevice>, shader: &VkShader, context: &mut DescriptorSetLayoutSetupContext) {
    for (index, shader_set) in shader.descriptor_set_bindings.iter().enumerate() {
        let set = &mut context.descriptor_set_layouts[index as usize];
        for binding in shader_set {
            let existing_binding_option = set
                .bindings
                .iter_mut()
                .find(|existing_binding| existing_binding.index == binding.index);
            if let Some(existing_binding) = existing_binding_option {
                if existing_binding.descriptor_type
                    == vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                {
                    assert_eq!(binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
                } else if existing_binding.descriptor_type
                    == vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                {
                    assert_eq!(binding.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
                } else {
                    assert_eq!(existing_binding.descriptor_type, binding.descriptor_type);
                }
                assert!(!existing_binding.writable);
                assert!(!binding.writable);
                assert_eq!(existing_binding.count, binding.count);
                existing_binding.shader_stage |= binding.shader_stage;
                existing_binding.flags |= binding.flags;
            } else {
                let mut binding_clone = binding.clone();
                if binding_clone.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                    && context.dynamic_storage_buffers[index as usize] + binding_clone.count
                        < device
                            .properties
                            .limits
                            .max_descriptor_set_storage_buffers_dynamic
                {
                    context.dynamic_storage_buffers[index as usize] += binding_clone.count;
                    binding_clone.descriptor_type =
                        vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
                }
                if binding_clone.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER
                    && context.dynamic_uniform_buffers[index as usize] + binding_clone.count
                        < device
                            .properties
                            .limits
                            .max_descriptor_set_uniform_buffers_dynamic
                {
                    context.dynamic_uniform_buffers[index as usize] += binding_clone.count;
                    binding_clone.descriptor_type =
                        vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
                }
                set.bindings.push(binding_clone);
            }
        }
    }
    let shader_stage_flags = shader_type_to_vk(shader.shader_type());
    if let Some(push_constants_range) = &shader.push_constants_range {
        if let Some(index) = VkPipelineLayout::push_constant_range_index(shader.shader_type()) {
            context.push_constants_ranges[index] = Some(VkConstantRange {
                offset: push_constants_range.offset,
                size: push_constants_range.size,
                shader_stage: shader_stage_flags,
            });
        }
    }
    context.uses_bindless_texture_set |= shader.uses_bindless_texture_set;
    context.shader_stages |= shader_stage_flags;
}

fn add_bindless_set_if_used(device: &Arc<RawVkDevice>, context: &mut DescriptorSetLayoutSetupContext, pipeline_name: Option<&str>) {
    if !context.uses_bindless_texture_set {
        return;
    }

    if !device.features.contains(VkFeatures::DESCRIPTOR_INDEXING) {
        panic!("Pipeline {:?} is trying to use the bindless texture descriptor set but the Vulkan device does not support descriptor indexing.", pipeline_name);
    }

    let mut bindless_bindings = SmallVec::<[VkDescriptorSetEntryInfo; PER_SET_BINDINGS]>::new();
    bindless_bindings.push(VkDescriptorSetEntryInfo {
        name: "bindless_textures".to_string(),
        shader_stage: vk::ShaderStageFlags::VERTEX
            | vk::ShaderStageFlags::FRAGMENT
            | vk::ShaderStageFlags::COMPUTE,
        index: 0,
        descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
        count: BINDLESS_TEXTURE_COUNT,
        writable: false,
        flags: vk::DescriptorBindingFlags::UPDATE_AFTER_BIND_EXT
            | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING_EXT
            | vk::DescriptorBindingFlags::PARTIALLY_BOUND_EXT,
    });

    context.descriptor_set_layouts[gpu::BINDLESS_TEXTURE_SET_INDEX as usize] =
        VkDescriptorSetLayoutKey {
            bindings: bindless_bindings,
            flags: vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL_EXT,
        };
}

fn remap_push_constant_ranges(context: &mut DescriptorSetLayoutSetupContext) {let mut offset = 0u32;
    let mut remapped_push_constant_ranges = <[Option<VkConstantRange>; 3]>::default();
    for i in 0..context.push_constants_ranges.len() {
        if let Some(range) = &context.push_constants_ranges[i] {
            remapped_push_constant_ranges[i] = Some(VkConstantRange {
                offset,
                size: range.size,
                shader_stage: range.shader_stage,
            });
            offset += range.size;
        }
    }
    context.push_constants_ranges = remapped_push_constant_ranges;
}

impl VkPipeline {
    pub fn new_graphics(
        device: &Arc<RawVkDevice>,
        info: &gpu::GraphicsPipelineInfo<VkBackend>,
        shared: &VkShared,
        name: Option<&str>,
    ) -> Self {
        let vk_device = &device.device;
        let mut shader_stages: Vec<vk::PipelineShaderStageCreateInfo> = Vec::new();

        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();
        let mut context = DescriptorSetLayoutSetupContext::default();

        {
            let shader = info.vs;
            let shader_stage = vk::PipelineShaderStageCreateInfo {
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                stage: shader_type_to_vk(shader.shader_type()),
                ..Default::default()
            };
            shader_stages.push(shader_stage);
            add_shader_to_descriptor_set_layout_setup(device, shader, &mut context);
        }

        if let Some(shader) = info.fs.clone() {
            let shader_stage = vk::PipelineShaderStageCreateInfo {
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                stage: shader_type_to_vk(shader.shader_type()),
                ..Default::default()
            };
            shader_stages.push(shader_stage);
            add_shader_to_descriptor_set_layout_setup(device, shader, &mut context);
        }

        let mut attribute_descriptions: Vec<vk::VertexInputAttributeDescription> = Vec::new();
        let mut binding_descriptions: Vec<vk::VertexInputBindingDescription> = Vec::new();
        for element in info.vertex_layout.shader_inputs {
            attribute_descriptions.push(vk::VertexInputAttributeDescription {
                location: element.location_vk_mtl,
                binding: element.input_assembler_binding,
                format: format_to_vk(element.format, false),
                offset: element.offset as u32,
            });
        }

        for element in info.vertex_layout.input_assembler {
            binding_descriptions.push(vk::VertexInputBindingDescription {
                binding: element.binding,
                stride: element.stride as u32,
                input_rate: input_rate_to_vk(element.input_rate),
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
            topology: match info.primitive_type {
                gpu::PrimitiveType::Triangles => vk::PrimitiveTopology::TRIANGLE_LIST,
                gpu::PrimitiveType::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
                gpu::PrimitiveType::Lines => vk::PrimitiveTopology::LINE_LIST,
                gpu::PrimitiveType::LineStrip => vk::PrimitiveTopology::LINE_STRIP,
                gpu::PrimitiveType::Points => vk::PrimitiveTopology::POINT_LIST,
            },
            primitive_restart_enable: false as u32,
            ..Default::default()
        };

        let rasterizer_create_info = vk::PipelineRasterizationStateCreateInfo {
            flags: vk::PipelineRasterizationStateCreateFlags::empty(),
            depth_clamp_enable: vk::FALSE,
            rasterizer_discard_enable: vk::FALSE,
            polygon_mode: match &info.rasterizer.fill_mode {
                gpu::FillMode::Fill => vk::PolygonMode::FILL,
                gpu::FillMode::Line => vk::PolygonMode::LINE,
            },
            cull_mode: match &info.rasterizer.cull_mode {
                gpu::CullMode::Back => vk::CullModeFlags::BACK,
                gpu::CullMode::Front => vk::CullModeFlags::FRONT,
                gpu::CullMode::None => vk::CullModeFlags::NONE,
            },
            front_face: match &info.rasterizer.front_face {
                gpu::FrontFace::Clockwise => vk::FrontFace::CLOCKWISE,
                gpu::FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE,
            },
            depth_bias_enable: vk::FALSE,
            depth_bias_constant_factor: 0.0f32,
            depth_bias_clamp: 0.0f32,
            depth_bias_slope_factor: 0.0f32,
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
            depth_bounds_test_enable: vk::FALSE,
            stencil_test_enable: info.depth_stencil.stencil_enable as u32,
            front: vk::StencilOpState {
                pass_op: stencil_op_to_vk(info.depth_stencil.stencil_front.pass_op),
                fail_op: stencil_op_to_vk(info.depth_stencil.stencil_front.fail_op),
                depth_fail_op: stencil_op_to_vk(
                    info.depth_stencil.stencil_front.depth_fail_op,
                ),
                compare_op: compare_func_to_vk(info.depth_stencil.stencil_front.func),
                write_mask: info.depth_stencil.stencil_write_mask as u32,
                compare_mask: info.depth_stencil.stencil_read_mask as u32,
                reference: 0u32,
            },
            back: vk::StencilOpState {
                pass_op: stencil_op_to_vk(info.depth_stencil.stencil_back.pass_op),
                fail_op: stencil_op_to_vk(info.depth_stencil.stencil_back.fail_op),
                depth_fail_op: stencil_op_to_vk(info.depth_stencil.stencil_back.depth_fail_op),
                compare_op: compare_func_to_vk(info.depth_stencil.stencil_back.func),
                write_mask: info.depth_stencil.stencil_write_mask as u32,
                compare_mask: info.depth_stencil.stencil_read_mask as u32,
                reference: 0u32,
            },
            min_depth_bounds: 0.0,
            max_depth_bounds: 0.0,
            ..Default::default()
        };

        let mut blend_attachments: Vec<vk::PipelineColorBlendAttachmentState> = Vec::new();
        for blend in info.blend.attachments {
            blend_attachments.push(vk::PipelineColorBlendAttachmentState {
                blend_enable: blend.blend_enabled as u32,
                src_color_blend_factor: blend_factor_to_vk(blend.src_color_blend_factor),
                dst_color_blend_factor: blend_factor_to_vk(blend.dst_color_blend_factor),
                color_blend_op: blend_op_to_vk(blend.color_blend_op),
                src_alpha_blend_factor: blend_factor_to_vk(blend.src_alpha_blend_factor),
                dst_alpha_blend_factor: blend_factor_to_vk(blend.dst_alpha_blend_factor),
                alpha_blend_op: blend_op_to_vk(blend.alpha_blend_op),
                color_write_mask: color_components_to_vk(blend.write_mask),
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

        let dynamic_state = [
            vk::DynamicState::VIEWPORT,
            vk::DynamicState::SCISSOR,
            vk::DynamicState::STENCIL_REFERENCE,
        ];
        let dynamic_state_create_info = vk::PipelineDynamicStateCreateInfo {
            p_dynamic_states: dynamic_state.as_ptr(),
            dynamic_state_count: dynamic_state.len() as u32,
            ..Default::default()
        };

        add_bindless_set_if_used(device, &mut context, name);
        remap_push_constant_ranges(&mut context);

        let layout = shared.get_pipeline_layout(&VkPipelineLayoutKey {
            descriptor_set_layouts: context.descriptor_set_layouts,
            push_constant_ranges: context.push_constants_ranges,
        });

        let viewport_info = vk::PipelineViewportStateCreateInfo {
            viewport_count: 1,
            p_viewports: &vk::Viewport {
                x: 0f32,
                y: 0f32,
                width: 0f32,
                height: 0f32,
                min_depth: 0f32,
                max_depth: 1f32,
            },
            scissor_count: 1,
            p_scissors: &vk::Rect2D {
                offset: vk::Offset2D { x: 0i32, y: 0i32 },
                extent: vk::Extent2D {
                    width: 0u32,
                    height: 0u32,
                },
            },
            ..Default::default()
        };

        let color_attachment_formats: SmallVec<[vk::Format; 8]> = info.render_target_formats
            .iter()
            .map(|f| format_to_vk(*f, false))
            .collect();

        let dsv_format: vk::Format = format_to_vk(info.depth_stencil_format, device.supports_d24);

        let pipeline_rendering_create_info = vk::PipelineRenderingCreateInfo {
            view_mask: 0u32,
            color_attachment_count: color_attachment_formats.len() as u32,
            p_color_attachment_formats: color_attachment_formats.as_ptr(),
            depth_attachment_format: if info.depth_stencil_format.is_depth() { dsv_format } else { vk::Format::UNDEFINED },
            stencil_attachment_format: if info.depth_stencil_format.is_stencil() { dsv_format } else { vk::Format::UNDEFINED },
            ..Default::default()
        };

        let pipeline_create_info = vk::GraphicsPipelineCreateInfo {
            p_next: &pipeline_rendering_create_info as *const vk::PipelineRenderingCreateInfo as *const c_void,
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
            layout: layout.handle(),
            render_pass: vk::RenderPass::null(),
            subpass: 0,
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0i32,
            ..Default::default()
        };

        let pipeline = unsafe {
            vk_device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_create_info], None)
                .unwrap()[0]
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::PIPELINE,
                                object_handle: pipeline.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        Self {
            pipeline,
            device: device.clone(),
            layout,
            pipeline_type: VkPipelineType::Graphics,
            uses_bindless_texture_set: context.uses_bindless_texture_set,
            sbt: None,
        }
    }

    pub fn new_compute(
        device: &Arc<RawVkDevice>,
        shader: &VkShader,
        shared: &VkShared,
        name: Option<&str>,
    ) -> Self {
        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();
        let mut context = DescriptorSetLayoutSetupContext::default();

        let shader_stage = vk::PipelineShaderStageCreateInfo {
            module: shader.shader_module(),
            p_name: entry_point.as_ptr() as *const c_char,
            stage: shader_type_to_vk(shader.shader_type()),
            ..Default::default()
        };

        add_shader_to_descriptor_set_layout_setup(device, shader, &mut context);
        add_bindless_set_if_used(device, &mut context, name);
        remap_push_constant_ranges(&mut context);

        let layout = shared.get_pipeline_layout(&VkPipelineLayoutKey {
            descriptor_set_layouts: context.descriptor_set_layouts,
            push_constant_ranges: context.push_constants_ranges,
        });

        let pipeline_create_info = vk::ComputePipelineCreateInfo {
            flags: vk::PipelineCreateFlags::empty(),
            stage: shader_stage,
            layout: layout.handle(),
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0,
            ..Default::default()
        };
        let pipeline = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[pipeline_create_info], None)
                .unwrap()[0]
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::PIPELINE,
                                object_handle: pipeline.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        VkPipeline {
            pipeline,
            device: device.clone(),
            layout,
            pipeline_type: VkPipelineType::Compute,
            uses_bindless_texture_set: shader.uses_bindless_texture_set,
            sbt: None,
        }
    }

    pub fn new_compute_meta(
        device: &Arc<RawVkDevice>,
        shader: &VkShader,
        name: Option<&str>,
    ) -> Self {
        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();
        let mut context = DescriptorSetLayoutSetupContext::default();

        let shader_stage = vk::PipelineShaderStageCreateInfo {
            module: shader.shader_module(),
            p_name: entry_point.as_ptr() as *const c_char,
            stage: shader_type_to_vk(shader.shader_type()),
            ..Default::default()
        };

        add_shader_to_descriptor_set_layout_setup(device, shader, &mut context);

        let mut descriptor_set_layouts: [Option<Arc<VkDescriptorSetLayout>>; 5] =
            Default::default();
        for (i, set_key) in context.descriptor_set_layouts.iter().enumerate() {
            descriptor_set_layouts[i] = Some(Arc::new(VkDescriptorSetLayout::new(
                &set_key.bindings,
                set_key.flags,
                device,
            )));
        }

        let layout = Arc::new(VkPipelineLayout::new(
            &descriptor_set_layouts,
            &context.push_constants_ranges,
            device,
        ));

        let pipeline_create_info = vk::ComputePipelineCreateInfo {
            flags: vk::PipelineCreateFlags::empty(),
            stage: shader_stage,
            layout: layout.handle(),
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0,
            ..Default::default()
        };
        let pipeline = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[pipeline_create_info], None)
                .unwrap()[0]
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::PIPELINE,
                                object_handle: pipeline.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        VkPipeline {
            pipeline,
            device: device.clone(),
            layout,
            pipeline_type: VkPipelineType::Compute,
            uses_bindless_texture_set: shader.uses_bindless_texture_set,
            sbt: None,
        }
    }

    pub fn ray_tracing_buffer_size(
        device: &Arc<RawVkDevice>,
        info: &gpu::RayTracingPipelineInfo<VkBackend>,
        _shared: &VkShared
    ) -> u64 {
        let shader_count = 1 + info.closest_hit_shaders.len() + info.miss_shaders.len();

        let rt = device.rt.as_ref().unwrap();
        let handle_size = rt.rt_pipeline_properties.shader_group_handle_size;
        let handle_alignment = rt.rt_pipeline_properties.shader_group_handle_alignment;
        let handle_stride = align_up_32(handle_size, handle_alignment);
        let group_alignment = rt.rt_pipeline_properties.shader_group_base_alignment as u64;

        align_up_32(handle_stride, group_alignment as u32) as u64 * shader_count as u64
    }

    pub fn new_ray_tracing(
        device: &Arc<RawVkDevice>,
        info: &gpu::RayTracingPipelineInfo<VkBackend>,
        shared: &VkShared,
        buffer: &VkBuffer,
        buffer_offset: u64,
        name: Option<&str>
    ) -> Self {
        let rt = device.rt.as_ref().unwrap();
        let entry_point = CString::new(SHADER_ENTRY_POINT_NAME).unwrap();

        let mut stages = SmallVec::<[vk::PipelineShaderStageCreateInfo; 4]>::new();
        let mut groups = SmallVec::<[vk::RayTracingShaderGroupCreateInfoKHR; 4]>::new();

        let mut context = DescriptorSetLayoutSetupContext::default();

        {
            let shader = info.ray_gen_shader;
            let stage_info = vk::PipelineShaderStageCreateInfo {
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::RAYGEN_KHR,
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                ..Default::default()
            };
            let group_info = vk::RayTracingShaderGroupCreateInfoKHR {
                ty: vk::RayTracingShaderGroupTypeKHR::GENERAL,
                general_shader: stages.len() as u32,
                closest_hit_shader: vk::SHADER_UNUSED_KHR,
                any_hit_shader: vk::SHADER_UNUSED_KHR,
                intersection_shader: vk::SHADER_UNUSED_KHR,
                p_shader_group_capture_replay_handle: std::ptr::null(),
                ..Default::default()
            };
            stages.push(stage_info);
            groups.push(group_info);
            add_shader_to_descriptor_set_layout_setup(device, shader, &mut context);
        }

        for shader in info.closest_hit_shaders.iter() {
            let stage_info = vk::PipelineShaderStageCreateInfo {
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                ..Default::default()
            };
            let group_info = vk::RayTracingShaderGroupCreateInfoKHR {
                ty: vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP,
                general_shader: vk::SHADER_UNUSED_KHR,
                closest_hit_shader: stages.len() as u32,
                any_hit_shader: vk::SHADER_UNUSED_KHR,
                intersection_shader: vk::SHADER_UNUSED_KHR,
                p_shader_group_capture_replay_handle: std::ptr::null(),
                ..Default::default()
            };
            stages.push(stage_info);
            groups.push(group_info);
            add_shader_to_descriptor_set_layout_setup(device, shader, &mut context);
        }

        for shader in info.miss_shaders.iter() {
            let stage_info = vk::PipelineShaderStageCreateInfo {
                flags: vk::PipelineShaderStageCreateFlags::empty(),
                stage: vk::ShaderStageFlags::MISS_KHR,
                module: shader.shader_module(),
                p_name: entry_point.as_ptr() as *const c_char,
                ..Default::default()
            };
            let group_info = vk::RayTracingShaderGroupCreateInfoKHR {
                ty: vk::RayTracingShaderGroupTypeKHR::GENERAL,
                general_shader: stages.len() as u32,
                closest_hit_shader: vk::SHADER_UNUSED_KHR,
                any_hit_shader: vk::SHADER_UNUSED_KHR,
                intersection_shader: vk::SHADER_UNUSED_KHR,
                p_shader_group_capture_replay_handle: std::ptr::null(),
                ..Default::default()
            };
            stages.push(stage_info);
            groups.push(group_info);
            add_shader_to_descriptor_set_layout_setup(device, shader, &mut context);
        }
        add_bindless_set_if_used(device, &mut context, name);

        let layout = shared.get_pipeline_layout(&VkPipelineLayoutKey {
            descriptor_set_layouts: context.descriptor_set_layouts,
            push_constant_ranges: context.push_constants_ranges,
        });

        let vk_info = vk::RayTracingPipelineCreateInfoKHR {
            flags: vk::PipelineCreateFlags::empty(),
            stage_count: stages.len() as u32,
            p_stages: stages.as_ptr(),
            group_count: groups.len() as u32,
            p_groups: groups.as_ptr(),
            max_pipeline_ray_recursion_depth: 2,
            p_library_info: std::ptr::null(),
            p_library_interface: std::ptr::null(),
            p_dynamic_state: std::ptr::null(),
            layout: layout.handle(),
            base_pipeline_handle: vk::Pipeline::null(),
            base_pipeline_index: 0,
            ..Default::default()
        };
        let pipeline = unsafe {
            rt.rt_pipelines.create_ray_tracing_pipelines(
                vk::DeferredOperationKHR::null(),
                vk::PipelineCache::null(),
                &[vk_info],
                None,
            )
        }
        .unwrap()
        .pop()
        .unwrap();

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(
                            &vk::DebugUtilsObjectNameInfoEXT {
                                object_type: vk::ObjectType::PIPELINE,
                                object_handle: pipeline.as_raw(),
                                p_object_name: name_cstring.as_ptr(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        }

        // SBT
        let handle_size = rt.rt_pipeline_properties.shader_group_handle_size;
        let handle_alignment = rt.rt_pipeline_properties.shader_group_handle_alignment;
        let handle_stride = align_up_32(handle_size, handle_alignment);
        let group_alignment = rt.rt_pipeline_properties.shader_group_base_alignment as u64;

        let size = Self::ray_tracing_buffer_size(device, info, shared);
        assert!(buffer.info().size - buffer_offset >= size);

        let handles = unsafe {
            rt.rt_pipelines.get_ray_tracing_shader_group_handles(
                pipeline,
                0,
                groups.len() as u32,
                handle_size as usize * groups.len(),
            )
        }
        .unwrap();

        let sbt = buffer;
        let map = unsafe { sbt.map(buffer_offset, size, false).unwrap() as *mut u8 };

        let mut src_offset = 0u64;
        let mut dst_offset = 0u64;
        let raygen_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt.va_offset(buffer_offset).unwrap(),
            stride: align_up_64(handle_stride as u64, group_alignment),
            size: align_up_64(handle_stride as u64, group_alignment),
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                (handles.as_ptr() as *const u8).add(src_offset as usize),
                map.add(dst_offset as usize),
                handle_size as usize,
            );
        }
        src_offset += handle_size as u64;
        dst_offset += handle_stride as u64;

        dst_offset = align_up_64(dst_offset as u64, group_alignment);
        let closest_hit_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt.va_offset(buffer_offset).unwrap() + dst_offset,
            stride: handle_stride as u64,
            size: align_up_64(
                info.closest_hit_shaders.len() as u64 * handle_stride as u64,
                group_alignment,
            ),
        };
        for _ in 0..info.closest_hit_shaders.len() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (handles.as_ptr() as *const u8).add(src_offset as usize),
                    map.add(dst_offset as usize),
                    handle_size as usize,
                );
            }
            src_offset += handle_size as u64;
            dst_offset += handle_stride as u64;
        }

        dst_offset = align_up_64(dst_offset as u64, group_alignment);
        let miss_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt.va_offset(buffer_offset).unwrap() + dst_offset,
            stride: handle_stride as u64,
            size: align_up_64(
                info.miss_shaders.len() as u64 * handle_stride as u64,
                group_alignment,
            ),
        };
        for _ in 0..info.miss_shaders.len() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (handles.as_ptr() as *const u8).add(src_offset as usize),
                    map.add(dst_offset as usize),
                    handle_size as usize,
                );
            }
            src_offset += handle_size as u64;
            dst_offset += handle_stride as u64;
        }

        unsafe {
            sbt.unmap(buffer_offset, size, true);
        }

        Self {
            pipeline,
            layout,
            device: device.clone(),
            pipeline_type: VkPipelineType::RayTracing,
            uses_bindless_texture_set: context.uses_bindless_texture_set,
            sbt: Some(VkShaderBindingTables {
                buffer: sbt.handle(),
                buffer_offset,
                raygen_region,
                closest_hit_region,
                miss_region,
            }),
        }
    }

    #[inline]
    pub(super) fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }

    #[inline]
    pub(super) fn layout(&self) -> &Arc<VkPipelineLayout> {
        &self.layout
    }

    pub(super) fn pipeline_type(&self) -> VkPipelineType {
        self.pipeline_type
    }

    #[inline]
    pub(super) fn uses_bindless_texture_set(&self) -> bool {
        self.uses_bindless_texture_set
    }

    #[inline]
    pub(super) fn sbt_buffer_handle(&self) -> vk::Buffer {
        self.sbt.as_ref().unwrap().buffer
    }

    #[inline]
    pub(super) fn sbt_buffer_offset(&self) -> vk::Buffer {
        self.sbt.as_ref().unwrap().buffer
    }

    #[inline]
    pub(super) fn raygen_sbt_region(&self) -> &vk::StridedDeviceAddressRegionKHR {
        &self.sbt.as_ref().unwrap().raygen_region
    }

    #[inline]
    pub(super) fn closest_hit_sbt_region(&self) -> &vk::StridedDeviceAddressRegionKHR {
        &self.sbt.as_ref().unwrap().closest_hit_region
    }

    #[inline]
    pub(super) fn miss_sbt_region(&self) -> &vk::StridedDeviceAddressRegionKHR {
        &self.sbt.as_ref().unwrap().miss_region
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

impl gpu::ComputePipeline for VkPipeline {
    fn binding_info(&self, set: gpu::BindingFrequency, slot: u32) -> Option<gpu::BindingInfo> {
        self.layout
            .descriptor_set_layouts
            .get(set as usize)
            .unwrap()
            .as_ref()
            .and_then(|layout| layout.binding(slot))
            .map(|i| gpu::BindingInfo {
                name: i.name.as_str(),
                binding_type: match i.descriptor_type {
                    vk::DescriptorType::STORAGE_BUFFER_DYNAMIC
                    | vk::DescriptorType::STORAGE_BUFFER => gpu::BindingType::StorageTexture,
                    vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC
                    | vk::DescriptorType::UNIFORM_BUFFER => gpu::BindingType::ConstantBuffer,
                    vk::DescriptorType::STORAGE_IMAGE => gpu::BindingType::StorageTexture,
                    vk::DescriptorType::SAMPLED_IMAGE => gpu::BindingType::SampledTexture,
                    vk::DescriptorType::SAMPLER => gpu::BindingType::Sampler,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER => gpu::BindingType::TextureAndSampler,
                    _ => unreachable!(),
                },
            })
    }
}

pub(super) struct VkPipelineLayout {
    device: Arc<RawVkDevice>,
    layout: vk::PipelineLayout,
    descriptor_set_layouts: [Option<Arc<VkDescriptorSetLayout>>; 5],
    push_constant_ranges: [Option<VkConstantRange>; 3],
}

impl VkPipelineLayout {
    pub fn new(
        descriptor_set_layouts: &[Option<Arc<VkDescriptorSetLayout>>; 5],
        push_constant_ranges: &[Option<VkConstantRange>; 3],
        device: &Arc<RawVkDevice>,
    ) -> Self {
        let layouts: Vec<vk::DescriptorSetLayout> = descriptor_set_layouts
            .iter()
            .filter(|descriptor_set_layout| descriptor_set_layout.is_some())
            .map(|descriptor_set_layout| descriptor_set_layout.as_ref().unwrap().handle())
            .collect();

        let ranges: Vec<vk::PushConstantRange> = push_constant_ranges
            .iter()
            .filter(|r| r.is_some())
            .map(|r| {
                let r = r.as_ref().unwrap();
                vk::PushConstantRange {
                    stage_flags: r.shader_stage,
                    offset: r.offset,
                    size: r.size,
                }
            })
            .collect();

        let info = vk::PipelineLayoutCreateInfo {
            p_set_layouts: layouts.as_ptr(),
            set_layout_count: layouts.len() as u32,
            p_push_constant_ranges: ranges.as_ptr(),
            push_constant_range_count: ranges.len() as u32,
            ..Default::default()
        };

        unsafe {
            if info.push_constant_range_count != 0 && (*(info.p_push_constant_ranges)).size == 0 {
                panic!("Empty push constant range in pipeline layout");
            }
        }

        let layout = unsafe { device.create_pipeline_layout(&info, None) }.unwrap();
        Self {
            device: device.clone(),
            layout,
            descriptor_set_layouts: descriptor_set_layouts.clone(),
            push_constant_ranges: push_constant_ranges.clone(),
        }
    }

    #[inline]
    pub(super) fn handle(&self) -> vk::PipelineLayout {
        self.layout
    }

    #[inline]
    pub(super) fn descriptor_set_layout(&self, index: u32) -> Option<&Arc<VkDescriptorSetLayout>> {
        self.descriptor_set_layouts[index as usize].as_ref()
    }

    pub(super) fn push_constant_range_index(shader_type: gpu::ShaderType) -> Option<usize> {
        match shader_type {
            gpu::ShaderType::VertexShader => Some(0),
            gpu::ShaderType::FragmentShader => Some(1),
            gpu::ShaderType::ComputeShader => Some(0),
            gpu::ShaderType::RayGen => Some(0),
            gpu::ShaderType::RayClosestHit => Some(1),
            gpu::ShaderType::RayMiss => Some(2),
            _ => None,
        }
    }

    pub(super) fn push_constant_range(&self, shader_type: gpu::ShaderType) -> Option<&VkConstantRange> {
        Self::push_constant_range_index(shader_type).and_then(|index| self.push_constant_ranges[index].as_ref())
    }
}

impl Drop for VkPipelineLayout {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}
