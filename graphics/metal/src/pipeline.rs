use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use metal;
use metal::foreign_types::ForeignType;

use sourcerenderer_core::gpu;

use super::*;

#[derive(Clone, Default)]
pub(crate) struct MSLBinding {
    pub(crate) buffer_binding: Option<u32>,
    pub(crate) texture_binding: Option<u32>,
    pub(crate) sampler_binding: Option<u32>,
    pub(crate) array_count: u32,
}

#[derive(Clone)]
pub(crate) struct ShaderPushConstantInfo {
    pub(crate) binding: u32,
    pub(crate) size: u32,
}
pub(crate) struct ShaderResourceMap {
    pub(crate) resources: HashMap<(u32, u32), MSLBinding>,
    pub(crate) push_constants: Option<ShaderPushConstantInfo>,
    pub(crate) bindless_argument_buffer_binding: Option<u32>
}

pub(crate) struct PipelineResourceMap {
    pub(crate) resources: HashMap<(gpu::ShaderType, u32, u32), MSLBinding>,
    pub(crate) push_constants: HashMap<gpu::ShaderType, ShaderPushConstantInfo>,
    pub(crate) bindless_argument_buffer_binding: HashMap<gpu::ShaderType, u32>
}

pub struct MTLShader {
    shader_type: gpu::ShaderType,
    library: metal::Library,
    resource_map: ShaderResourceMap,
    name: Option<String>
}

impl MTLShader {
    pub(crate) fn new(device: &metal::DeviceRef, shader: gpu::PackedShader, name: Option<&str>) -> Self {
        println!("New shader {:?}", name);
        let library = device.new_library_with_data(&shader.shader_air).unwrap();
        if let Some(name) = name {
            library.set_label(name);
        }

        let mut resource_map = ShaderResourceMap {
            resources: HashMap::new(),
            push_constants: None,
            bindless_argument_buffer_binding: None
        };
        let mut buffer_count: u32 = 0;
        let mut texture_count: u32 = 0;
        let mut sampler_count: u32 = 0;
        for set in shader.resources.iter() {
            for resource in set.iter() {
                let mut binding = MSLBinding::default();
                binding.array_count = resource.array_size;
                match resource.resource_type {
                    gpu::ResourceType::UniformBuffer | gpu::ResourceType::StorageBuffer
                        | gpu::ResourceType::AccelerationStructure => {
                        binding.buffer_binding = Some(buffer_count);
                        buffer_count += resource.array_size;
                    },
                    gpu::ResourceType::SubpassInput | gpu::ResourceType::SampledTexture
                        | gpu::ResourceType::StorageTexture => {
                        binding.texture_binding = Some(texture_count);
                        texture_count += resource.array_size;
                    },
                    gpu::ResourceType::Sampler => {
                        binding.sampler_binding = Some(sampler_count);
                        sampler_count += resource.array_size;
                    },
                    gpu::ResourceType::CombinedTextureSampler => {
                        binding.texture_binding = Some(texture_count);
                        binding.sampler_binding = Some(sampler_count);
                        sampler_count += resource.array_size;
                        texture_count += resource.array_size;
                    },
                }
                resource_map.resources.insert((resource.set, resource.binding), binding);
            }
        }
        if shader.push_constant_size != 0 {
            resource_map.push_constants = Some(ShaderPushConstantInfo {
                binding: buffer_count,
                size: shader.push_constant_size
            });
            buffer_count += 1;
        }
        if shader.uses_bindless_texture_set {
            resource_map.bindless_argument_buffer_binding = Some(buffer_count);
            buffer_count += 1;
        }

        Self {
            shader_type: shader.shader_type,
            library,
            resource_map,
            name: name.map(|name| name.to_string())
        }
    }

    pub(crate) fn handle(&self) -> &metal::LibraryRef {
        &self.library
    }
}

impl gpu::Shader for MTLShader {
    fn shader_type(&self) -> gpu::ShaderType {
        self.shader_type
    }
}

impl PartialEq<MTLShader> for MTLShader {
    fn eq(&self, other: &MTLShader) -> bool {
        self.library.as_ptr() == other.library.as_ptr()
    }
}

impl Eq for MTLShader {}

impl Hash for MTLShader {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.library.as_ptr().hash(state);
    }
}

const SHADER_ENTRY_POINT_NAME: &str = "main0";

pub(crate) fn samples_to_mtl(samples: gpu::SampleCount) -> u64 {
    match samples {
        gpu::SampleCount::Samples1 => 1,
        gpu::SampleCount::Samples2 => 2,
        gpu::SampleCount::Samples4 => 4,
        gpu::SampleCount::Samples8 => 8,
    }
}

pub(crate) fn compare_func_to_mtl(compare_func: gpu::CompareFunc) -> metal::MTLCompareFunction {
    match compare_func {
        gpu::CompareFunc::Always => metal::MTLCompareFunction::Always,
        gpu::CompareFunc::NotEqual => metal::MTLCompareFunction::NotEqual,
        gpu::CompareFunc::Never => metal::MTLCompareFunction::Never,
        gpu::CompareFunc::Less => metal::MTLCompareFunction::Less,
        gpu::CompareFunc::LessEqual => metal::MTLCompareFunction::LessEqual,
        gpu::CompareFunc::Equal => metal::MTLCompareFunction::Equal,
        gpu::CompareFunc::GreaterEqual => metal::MTLCompareFunction::GreaterEqual,
        gpu::CompareFunc::Greater => metal::MTLCompareFunction::Greater,
    }
}

pub(crate) fn stencil_op_to_mtl(stencil_op: gpu::StencilOp) -> metal::MTLStencilOperation {
    match stencil_op {
        gpu::StencilOp::Decrease => metal::MTLStencilOperation::DecrementWrap,
        gpu::StencilOp::Increase => metal::MTLStencilOperation::IncrementWrap,
        gpu::StencilOp::DecreaseClamp => metal::MTLStencilOperation::DecrementClamp,
        gpu::StencilOp::IncreaseClamp => metal::MTLStencilOperation::IncrementClamp,
        gpu::StencilOp::Invert => metal::MTLStencilOperation::Invert,
        gpu::StencilOp::Keep => metal::MTLStencilOperation::Keep,
        gpu::StencilOp::Replace => metal::MTLStencilOperation::Replace,
        gpu::StencilOp::Zero => metal::MTLStencilOperation::Zero,
    }
}

pub(super) fn blend_factor_to_mtl(blend_factor: gpu::BlendFactor) -> metal::MTLBlendFactor {
    match blend_factor {
        gpu::BlendFactor::ConstantColor => metal::MTLBlendFactor::BlendColor,
        gpu::BlendFactor::DstAlpha => metal::MTLBlendFactor::DestinationAlpha,
        gpu::BlendFactor::DstColor => metal::MTLBlendFactor::DestinationColor,
        gpu::BlendFactor::One => metal::MTLBlendFactor::One,
        gpu::BlendFactor::OneMinusConstantColor => metal::MTLBlendFactor::OneMinusBlendColor,
        gpu::BlendFactor::OneMinusDstAlpha => metal::MTLBlendFactor::OneMinusDestinationAlpha,
        gpu::BlendFactor::OneMinusDstColor => metal::MTLBlendFactor::OneMinusDestinationColor,
        gpu::BlendFactor::OneMinusSrc1Alpha => metal::MTLBlendFactor::OneMinusSource1Alpha,
        gpu::BlendFactor::OneMinusSrc1Color => metal::MTLBlendFactor::OneMinusSource1Color,
        gpu::BlendFactor::OneMinusSrcColor => metal::MTLBlendFactor::OneMinusSourceColor,
        gpu::BlendFactor::Src1Alpha => metal::MTLBlendFactor::Source1Alpha,
        gpu::BlendFactor::Src1Color => metal::MTLBlendFactor::Source1Color,
        gpu::BlendFactor::SrcAlphaSaturate => metal::MTLBlendFactor::SourceAlphaSaturated,
        gpu::BlendFactor::SrcColor => metal::MTLBlendFactor::SourceColor,
        gpu::BlendFactor::Zero => metal::MTLBlendFactor::Zero,
        gpu::BlendFactor::SrcAlpha => metal::MTLBlendFactor::SourceAlpha,
        gpu::BlendFactor::OneMinusSrcAlpha => metal::MTLBlendFactor::OneMinusSourceAlpha,
    }
}

pub(crate) fn blend_op_to_mtl(blend_op: gpu::BlendOp) -> metal::MTLBlendOperation {
    match blend_op {
        gpu::BlendOp::Add => metal::MTLBlendOperation::Add,
        gpu::BlendOp::Subtract => metal::MTLBlendOperation::Subtract,
        gpu::BlendOp::ReverseSubtract => metal::MTLBlendOperation::ReverseSubtract,
        gpu::BlendOp::Min => metal::MTLBlendOperation::Min,
        gpu::BlendOp::Max => metal::MTLBlendOperation::Max,
    }
}

pub(super) fn color_components_to_mtl(color_components: gpu::ColorComponents) -> metal::MTLColorWriteMask {
    let components_bits = color_components.bits() as u64;
    let mut colors = metal::MTLColorWriteMask::empty();
    if color_components.contains(gpu::ColorComponents::RED) {
        colors |= metal::MTLColorWriteMask::Red;
    }
    if color_components.contains(gpu::ColorComponents::GREEN) {
        colors |= metal::MTLColorWriteMask::Green;
    }
    if color_components.contains(gpu::ColorComponents::BLUE) {
        colors |= metal::MTLColorWriteMask::Blue;
    }
    if color_components.contains(gpu::ColorComponents::ALPHA) {
        colors |= metal::MTLColorWriteMask::Alpha;
    }
    colors
}

pub struct MTLGraphicsPipeline {
    pipeline: metal::RenderPipelineState,
    primitive_type: metal::MTLPrimitiveType,
    resource_map: Arc<PipelineResourceMap>,
}

impl MTLGraphicsPipeline {
    pub(crate) fn new(device: &metal::DeviceRef, info: &gpu::GraphicsPipelineInfo<MTLBackend>, renderpass_info: &gpu::RenderPassInfo, subpass: u32, name: Option<&str>) -> Self {
        let subpass = &renderpass_info.subpasses[subpass as usize];

        let descriptor = metal::RenderPipelineDescriptor::new();
        println!("VS name: {:?}", info.vs.name);
        for name in info.vs.handle().function_names() {
            println!("function: {:?}", name);
        }
        let vertex_function = info.vs.handle().get_function(SHADER_ENTRY_POINT_NAME, None);
        if vertex_function.is_err() {
            for name in info.vs.handle().function_names() {
                println!("function: {:?}", name);
            }
            panic!("ERROR {:?} shader: {:?}", vertex_function.err().unwrap(), info.vs.name.as_ref());
        }
        let vertex_function = vertex_function.unwrap();
        descriptor.set_vertex_function(Some(&vertex_function));
        let fragment_function = info.fs.map(|fs| fs.handle().get_function(SHADER_ENTRY_POINT_NAME, None).unwrap());
        descriptor.set_fragment_function(fragment_function.as_ref().map(|fs| &fs as &metal::FunctionRef));

        let vertex_descriptor = metal::VertexDescriptor::new().to_owned();
        for (idx, a) in info.vertex_layout.shader_inputs.iter().enumerate() {
            let adesc = metal::VertexAttributeDescriptor::new();
            adesc.set_offset(a.offset as u64);
            adesc.set_buffer_index(a.input_assembler_binding as u64);
            adesc.set_format(match a.format {
                gpu::Format::R32Float => metal::MTLVertexFormat::Float,
                gpu::Format::RG32Float => metal::MTLVertexFormat::Float2,
                gpu::Format::RGB32Float => metal::MTLVertexFormat::Float3,
                gpu::Format::RGBA32Float => metal::MTLVertexFormat::Float4,
                gpu::Format::RGBA8UNorm => metal::MTLVertexFormat::Char4Normalized,
                gpu::Format::RG16UInt => metal::MTLVertexFormat::UShort2,
                gpu::Format::RG16UNorm => metal::MTLVertexFormat::UShort2Normalized,
                _ => panic!("Unsupported format")
            });
            vertex_descriptor.attributes().set_object_at(idx as u64, Some(&adesc));
        }
        for a in info.vertex_layout.input_assembler {
            let adesc = metal::VertexBufferLayoutDescriptor::new();
            adesc.set_step_function(match a.input_rate {
                gpu::InputRate::PerVertex => metal::MTLVertexStepFunction::PerVertex,
                gpu::InputRate::PerInstance => metal::MTLVertexStepFunction::PerInstance,
            });
            adesc.set_stride(a.stride as u64);
            vertex_descriptor.layouts().set_object_at(a.binding as u64, Some(&adesc))
        }
        descriptor.set_vertex_descriptor(Some(&vertex_descriptor));

        for (idx, blend) in info.blend.attachments.iter().enumerate() {
            let attachment_desc = descriptor.color_attachments().object_at(idx as u64).unwrap();
            attachment_desc.set_blending_enabled(blend.blend_enabled);
            attachment_desc.set_rgb_blend_operation(blend_op_to_mtl(blend.color_blend_op));
            attachment_desc.set_alpha_blend_operation(blend_op_to_mtl(blend.alpha_blend_op));
            attachment_desc.set_source_rgb_blend_factor(blend_factor_to_mtl(blend.src_color_blend_factor));
            attachment_desc.set_destination_rgb_blend_factor(blend_factor_to_mtl(blend.dst_color_blend_factor));
            attachment_desc.set_source_alpha_blend_factor(blend_factor_to_mtl(blend.src_alpha_blend_factor));
            attachment_desc.set_destination_alpha_blend_factor(blend_factor_to_mtl(blend.dst_alpha_blend_factor));
            attachment_desc.set_write_mask(color_components_to_mtl(blend.write_mask));
        }
        descriptor.set_alpha_to_coverage_enabled(info.blend.alpha_to_coverage_enabled);


        for (idx, attachment_ref) in subpass.output_color_attachments.iter().enumerate() {
            let attachment_desc = descriptor.color_attachments().object_at(idx as u64).unwrap();
            let attachment = &renderpass_info.attachments[attachment_ref.index as usize];
            descriptor.set_raster_sample_count(samples_to_mtl(attachment.samples));
            attachment_desc.set_pixel_format(format_to_mtl(attachment.format));
        }

        if let Some(attachment_ref) = subpass.depth_stencil_attachment.as_ref() {
            let attachment = &renderpass_info.attachments[attachment_ref.index as usize];
            descriptor.set_depth_attachment_pixel_format(format_to_mtl(attachment.format));
            descriptor.set_raster_sample_count(samples_to_mtl(attachment.samples));
            if attachment.format.is_stencil() {
                descriptor.set_stencil_attachment_pixel_format(format_to_mtl(attachment.format));
            }
        }

        descriptor.set_rasterization_enabled(true);
        descriptor.set_input_primitive_topology(match info.primitive_type {
            gpu::PrimitiveType::Triangles | gpu::PrimitiveType::TriangleStrip => metal::MTLPrimitiveTopologyClass::Triangle,
            gpu::PrimitiveType::Lines | gpu::PrimitiveType::LineStrip => metal::MTLPrimitiveTopologyClass::Line,
            gpu::PrimitiveType::Points => metal::MTLPrimitiveTopologyClass::Point,
        });

        let primitive_type = match info.primitive_type {
            gpu::PrimitiveType::Triangles => metal::MTLPrimitiveType::Triangle,
            gpu::PrimitiveType::TriangleStrip => metal::MTLPrimitiveType::TriangleStrip,
            gpu::PrimitiveType::Lines => metal::MTLPrimitiveType::Line,
            gpu::PrimitiveType::LineStrip => metal::MTLPrimitiveType::LineStrip,
            gpu::PrimitiveType::Points => metal::MTLPrimitiveType::Point,
        };

        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let pipeline = device.new_render_pipeline_state(&descriptor).unwrap();

        let mut resource_map = PipelineResourceMap {
            resources: HashMap::new(),
            push_constants: HashMap::new(),
            bindless_argument_buffer_binding: HashMap::new()
        };
        for ((set, binding), msl_binding) in &info.vs.resource_map.resources {
            resource_map.resources.insert((gpu::ShaderType::VertexShader, *set, *binding), msl_binding.clone());
        }
        if let Some(push_constants) = info.vs.resource_map.push_constants.as_ref() {
            resource_map.push_constants.insert(gpu::ShaderType::VertexShader, push_constants.clone());
        }
        if let Some(bindless_binding) = info.vs.resource_map.bindless_argument_buffer_binding {
            resource_map.bindless_argument_buffer_binding.insert(gpu::ShaderType::VertexShader, bindless_binding);
        }
        if let Some(fs) = info.fs.as_ref() {
            for ((set, binding), msl_binding) in &fs.resource_map.resources {
                resource_map.resources.insert((gpu::ShaderType::FragmentShader, *set, *binding), msl_binding.clone());
            }
            if let Some(push_constants) = fs.resource_map.push_constants.as_ref() {
                resource_map.push_constants.insert(gpu::ShaderType::FragmentShader, push_constants.clone());
            }
            if let Some(bindless_binding) = fs.resource_map.bindless_argument_buffer_binding {
                resource_map.bindless_argument_buffer_binding.insert(gpu::ShaderType::FragmentShader, bindless_binding);
            }
        }

        Self {
            pipeline,
            primitive_type,
            resource_map: Arc::new(resource_map)
        }
    }

    pub(crate) fn handle(&self) -> &metal::RenderPipelineStateRef {
        &self.pipeline
    }

    pub(crate) fn primitive_type(&self) -> metal::MTLPrimitiveType {
        self.primitive_type
    }

    pub(crate) fn resource_map(&self) -> &Arc<PipelineResourceMap> {
        &self.resource_map
    }
}

pub struct MTLComputePipeline {
    pipeline: metal::ComputePipelineState,
    resource_map: Arc<PipelineResourceMap>
}

impl MTLComputePipeline {
    pub(crate) fn new(device: &metal::DeviceRef, shader: &MTLShader, name: Option<&str>) -> Self {
        println!("shader name: {:?}", shader.name.as_ref());
        let function = shader.handle().get_function(SHADER_ENTRY_POINT_NAME, None).unwrap();
        let pipeline = device.new_compute_pipeline_state_with_function(&function).unwrap();
        let mut resource_map = PipelineResourceMap {
            resources: HashMap::new(),
            push_constants: HashMap::new(),
            bindless_argument_buffer_binding: HashMap::new()
        };
        for ((set, binding), msl_binding) in &shader.resource_map.resources {
            resource_map.resources.insert((gpu::ShaderType::ComputeShader, *set, *binding), msl_binding.clone());
        }
        if let Some(push_constants) = shader.resource_map.push_constants.as_ref() {
            resource_map.push_constants.insert(gpu::ShaderType::ComputeShader, push_constants.clone());
        }
        if let Some(bindless_binding) = shader.resource_map.bindless_argument_buffer_binding {
            resource_map.bindless_argument_buffer_binding.insert(gpu::ShaderType::ComputeShader, bindless_binding);
        }
        Self {
            pipeline,
            resource_map: Arc::new(resource_map)
        }
    }

    pub(crate) fn handle(&self) -> &metal::ComputePipelineStateRef {
        &self.pipeline
    }

    pub(crate) fn resource_map(&self) -> &Arc<PipelineResourceMap> {
        &self.resource_map
    }
}

impl gpu::ComputePipeline for MTLComputePipeline {
    fn binding_info(&self, set: gpu::BindingFrequency, slot: u32) -> Option<gpu::BindingInfo> {
        todo!()
    }
}
