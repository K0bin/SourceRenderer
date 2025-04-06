use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSString, NSUInteger, NSURL};
use objc2_metal::{self, MTLDevice as _, MTLLibrary as _};

use sourcerenderer_core::gpu;

use super::*;

#[derive(Clone, Default, Debug)]
pub(crate) struct MSLBinding {
    pub(crate) buffer_binding: Option<u32>,
    pub(crate) texture_binding: Option<u32>,
    pub(crate) sampler_binding: Option<u32>,
    pub(crate) array_count: u32,
    pub(crate) writable: bool
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
    library: Retained<ProtocolObject<dyn objc2_metal::MTLLibrary>>,
    function: Retained<ProtocolObject<dyn objc2_metal::MTLFunction>>,
    resource_map: ShaderResourceMap,
}

unsafe impl Send for MTLShader {}
unsafe impl Sync for MTLShader {}

const METAL_DEBUGGER_WORKAROUND: bool = true;

impl MTLShader {
    pub(crate) unsafe fn new(device: &ProtocolObject<dyn objc2_metal::MTLDevice>, shader: &gpu::PackedShader, name: Option<&str>) -> Self {
        assert_ne!(shader.shader_air.len(), 0);
        let data = &shader.shader_air;

        let library = if METAL_DEBUGGER_WORKAROUND {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let hash = hasher.finish();

            let temp_dir = std::env::temp_dir();
            let temp_path = temp_dir.join(format!("{}.metallib", hash));
            let mut file = std::fs::File::create(&temp_path).unwrap();
            file.write_all(&data).unwrap();
            file.flush().unwrap();
            std::mem::drop(file);
            device.newLibraryWithURL_error(&NSURL::URLWithString(&NSString::from_str(temp_path.to_str().unwrap())).unwrap()).unwrap()
        } else {
            MTLDevice::metal_library_from_data(device, data).unwrap()
        };

        if let Some(name) = name {
            library.setLabel(Some(&NSString::from_str(name)));
        }

        let mut resource_map = ShaderResourceMap {
            resources: HashMap::new(),
            push_constants: None,
            bindless_argument_buffer_binding: None
        };
        let mut buffer_count: u32 = if shader.shader_type == gpu::ShaderType::VertexShader { shader.max_stage_input + 1 } else { 0 };
        if shader.uses_bindless_texture_set { buffer_count += 1; }
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
                        binding.writable = resource.writable;
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
        }
        if shader.uses_bindless_texture_set {
            resource_map.bindless_argument_buffer_binding = Some(shader.max_stage_input + 1);
        }

        let function = library.newFunctionWithName(&NSString::from_str(SHADER_ENTRY_POINT_NAME)).unwrap();

        Self {
            shader_type: shader.shader_type,
            library,
            resource_map,
            function
        }
    }

    pub(crate) fn function_handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLFunction> {
        &self.function
    }
}

impl gpu::Shader for MTLShader {
    fn shader_type(&self) -> gpu::ShaderType {
        self.shader_type
    }
}

impl PartialEq<MTLShader> for MTLShader {
    fn eq(&self, other: &MTLShader) -> bool {
        self.library == other.library
    }
}

impl Eq for MTLShader {}

impl Hash for MTLShader {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.library.hash(state);
    }
}

const SHADER_ENTRY_POINT_NAME: &str = "main0";

pub(crate) fn samples_to_mtl(samples: gpu::SampleCount) -> usize {
    match samples {
        gpu::SampleCount::Samples1 => 1,
        gpu::SampleCount::Samples2 => 2,
        gpu::SampleCount::Samples4 => 4,
        gpu::SampleCount::Samples8 => 8,
    }
}

pub(crate) fn compare_func_to_mtl(compare_func: gpu::CompareFunc) -> objc2_metal::MTLCompareFunction {
    match compare_func {
        gpu::CompareFunc::Always => objc2_metal::MTLCompareFunction::Always,
        gpu::CompareFunc::NotEqual => objc2_metal::MTLCompareFunction::NotEqual,
        gpu::CompareFunc::Never => objc2_metal::MTLCompareFunction::Never,
        gpu::CompareFunc::Less => objc2_metal::MTLCompareFunction::Less,
        gpu::CompareFunc::LessEqual => objc2_metal::MTLCompareFunction::LessEqual,
        gpu::CompareFunc::Equal => objc2_metal::MTLCompareFunction::Equal,
        gpu::CompareFunc::GreaterEqual => objc2_metal::MTLCompareFunction::GreaterEqual,
        gpu::CompareFunc::Greater => objc2_metal::MTLCompareFunction::Greater,
    }
}

pub(crate) fn stencil_op_to_mtl(stencil_op: gpu::StencilOp) -> objc2_metal::MTLStencilOperation {
    match stencil_op {
        gpu::StencilOp::Decrease => objc2_metal::MTLStencilOperation::DecrementWrap,
        gpu::StencilOp::Increase => objc2_metal::MTLStencilOperation::IncrementWrap,
        gpu::StencilOp::DecreaseClamp => objc2_metal::MTLStencilOperation::DecrementClamp,
        gpu::StencilOp::IncreaseClamp => objc2_metal::MTLStencilOperation::IncrementClamp,
        gpu::StencilOp::Invert => objc2_metal::MTLStencilOperation::Invert,
        gpu::StencilOp::Keep => objc2_metal::MTLStencilOperation::Keep,
        gpu::StencilOp::Replace => objc2_metal::MTLStencilOperation::Replace,
        gpu::StencilOp::Zero => objc2_metal::MTLStencilOperation::Zero,
    }
}

pub(super) fn blend_factor_to_mtl(blend_factor: gpu::BlendFactor) -> objc2_metal::MTLBlendFactor {
    match blend_factor {
        gpu::BlendFactor::ConstantColor => objc2_metal::MTLBlendFactor::BlendColor,
        gpu::BlendFactor::DstAlpha => objc2_metal::MTLBlendFactor::DestinationAlpha,
        gpu::BlendFactor::DstColor => objc2_metal::MTLBlendFactor::DestinationColor,
        gpu::BlendFactor::One => objc2_metal::MTLBlendFactor::One,
        gpu::BlendFactor::OneMinusConstantColor => objc2_metal::MTLBlendFactor::OneMinusBlendColor,
        gpu::BlendFactor::OneMinusDstAlpha => objc2_metal::MTLBlendFactor::OneMinusDestinationAlpha,
        gpu::BlendFactor::OneMinusDstColor => objc2_metal::MTLBlendFactor::OneMinusDestinationColor,
        gpu::BlendFactor::OneMinusSrc1Alpha => objc2_metal::MTLBlendFactor::OneMinusSource1Alpha,
        gpu::BlendFactor::OneMinusSrc1Color => objc2_metal::MTLBlendFactor::OneMinusSource1Color,
        gpu::BlendFactor::OneMinusSrcColor => objc2_metal::MTLBlendFactor::OneMinusSourceColor,
        gpu::BlendFactor::Src1Alpha => objc2_metal::MTLBlendFactor::Source1Alpha,
        gpu::BlendFactor::Src1Color => objc2_metal::MTLBlendFactor::Source1Color,
        gpu::BlendFactor::SrcAlphaSaturate => objc2_metal::MTLBlendFactor::SourceAlphaSaturated,
        gpu::BlendFactor::SrcColor => objc2_metal::MTLBlendFactor::SourceColor,
        gpu::BlendFactor::Zero => objc2_metal::MTLBlendFactor::Zero,
        gpu::BlendFactor::SrcAlpha => objc2_metal::MTLBlendFactor::SourceAlpha,
        gpu::BlendFactor::OneMinusSrcAlpha => objc2_metal::MTLBlendFactor::OneMinusSourceAlpha,
    }
}

pub(crate) fn blend_op_to_mtl(blend_op: gpu::BlendOp) -> objc2_metal::MTLBlendOperation {
    match blend_op {
        gpu::BlendOp::Add => objc2_metal::MTLBlendOperation::Add,
        gpu::BlendOp::Subtract => objc2_metal::MTLBlendOperation::Subtract,
        gpu::BlendOp::ReverseSubtract => objc2_metal::MTLBlendOperation::ReverseSubtract,
        gpu::BlendOp::Min => objc2_metal::MTLBlendOperation::Min,
        gpu::BlendOp::Max => objc2_metal::MTLBlendOperation::Max,
    }
}

pub(super) fn color_components_to_mtl(color_components: gpu::ColorComponents) -> objc2_metal::MTLColorWriteMask {
    let mut colors = objc2_metal::MTLColorWriteMask::empty();
    if color_components.contains(gpu::ColorComponents::RED) {
        colors |= objc2_metal::MTLColorWriteMask::Red;
    }
    if color_components.contains(gpu::ColorComponents::GREEN) {
        colors |= objc2_metal::MTLColorWriteMask::Green;
    }
    if color_components.contains(gpu::ColorComponents::BLUE) {
        colors |= objc2_metal::MTLColorWriteMask::Blue;
    }
    if color_components.contains(gpu::ColorComponents::ALPHA) {
        colors |= objc2_metal::MTLColorWriteMask::Alpha;
    }
    colors
}

unsafe fn stencil_info_to_mtl(stencil: &gpu::StencilInfo, stencil_enabled: bool, read_mask: u8, write_mask: u8) -> Retained<objc2_metal::MTLStencilDescriptor> {
    let descriptor = objc2_metal::MTLStencilDescriptor::new();
    descriptor.setStencilCompareFunction(if stencil_enabled { compare_func_to_mtl(stencil.func)
    }else {
        objc2_metal::MTLCompareFunction::Always
    });
    descriptor.setDepthStencilPassOperation(stencil_op_to_mtl(stencil.pass_op));
    descriptor.setDepthFailureOperation(stencil_op_to_mtl(stencil.fail_op));
    descriptor.setReadMask(read_mask as u32);
    descriptor.setWriteMask(write_mask as u32);
    descriptor
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub(crate) struct MTLRasterizerInfo {
  pub(crate) fill_mode: objc2_metal::MTLTriangleFillMode,
  pub(crate) cull_mode: objc2_metal::MTLCullMode,
  pub(crate) front_face: objc2_metal::MTLWinding,
}

pub struct MTLGraphicsPipeline {
    pipeline: Retained<ProtocolObject<dyn objc2_metal::MTLRenderPipelineState>>,
    primitive_type: objc2_metal::MTLPrimitiveType,
    resource_map: Arc<PipelineResourceMap>,
    rasterizer_state: MTLRasterizerInfo,
    depth_stencil_state: Retained<ProtocolObject<dyn objc2_metal::MTLDepthStencilState>>,
}

unsafe impl Send for MTLGraphicsPipeline {}
unsafe impl Sync for MTLGraphicsPipeline {}

impl MTLGraphicsPipeline {
    pub(crate) unsafe fn new(device: &ProtocolObject<dyn objc2_metal::MTLDevice>, info: &gpu::GraphicsPipelineInfo<MTLBackend>, name: Option<&str>) -> Self {
        let descriptor = objc2_metal::MTLRenderPipelineDescriptor::new();

        if let Some(name) = name {
            descriptor.setLabel(Some(&NSString::from_str(name)));
        }

        descriptor.setVertexFunction(Some(info.vs.function_handle()));
        descriptor.setFragmentFunction(info.fs.map(|fs| fs.function_handle()));

        let vertex_descriptor = objc2_metal::MTLVertexDescriptor::new();
        for (idx, a) in info.vertex_layout.shader_inputs.iter().enumerate() {
            let adesc = objc2_metal::MTLVertexAttributeDescriptor::new();
            adesc.setOffset(a.offset as NSUInteger);
            adesc.setBufferIndex(a.input_assembler_binding as NSUInteger);
            adesc.setFormat(match a.format {
                gpu::Format::R32Float => objc2_metal::MTLVertexFormat::Float,
                gpu::Format::RG32Float => objc2_metal::MTLVertexFormat::Float2,
                gpu::Format::RGB32Float => objc2_metal::MTLVertexFormat::Float3,
                gpu::Format::RGBA32Float => objc2_metal::MTLVertexFormat::Float4,
                gpu::Format::RGBA8UNorm => objc2_metal::MTLVertexFormat::Char4Normalized,
                gpu::Format::RG16UInt => objc2_metal::MTLVertexFormat::UShort2,
                gpu::Format::RG16UNorm => objc2_metal::MTLVertexFormat::UShort2Normalized,
                gpu::Format::R32UInt => objc2_metal::MTLVertexFormat::UInt,
                _ => panic!("Unsupported format")
            });
            vertex_descriptor.attributes().setObject_atIndexedSubscript(Some(&adesc), idx as NSUInteger);
        }
        for a in info.vertex_layout.input_assembler {
            let adesc = objc2_metal::MTLVertexBufferLayoutDescriptor::new();
            adesc.setStepFunction(match a.input_rate {
                gpu::InputRate::PerVertex => objc2_metal::MTLVertexStepFunction::PerVertex,
                gpu::InputRate::PerInstance => objc2_metal::MTLVertexStepFunction::PerInstance,
            });
            adesc.setStride(a.stride as NSUInteger);
            vertex_descriptor.layouts().setObject_atIndexedSubscript(Some(&adesc), a.binding as NSUInteger)
        }
        descriptor.setVertexDescriptor(Some(&vertex_descriptor));

        for (idx, blend) in info.blend.attachments.iter().enumerate() {
            let attachment_desc = descriptor.colorAttachments().objectAtIndexedSubscript(idx as NSUInteger);
            attachment_desc.setBlendingEnabled(blend.blend_enabled);
            attachment_desc.setRgbBlendOperation(blend_op_to_mtl(blend.color_blend_op));
            attachment_desc.setAlphaBlendOperation(blend_op_to_mtl(blend.alpha_blend_op));
            attachment_desc.setSourceRGBBlendFactor(blend_factor_to_mtl(blend.src_color_blend_factor));
            attachment_desc.setDestinationRGBBlendFactor(blend_factor_to_mtl(blend.dst_color_blend_factor));
            attachment_desc.setSourceAlphaBlendFactor(blend_factor_to_mtl(blend.src_alpha_blend_factor));
            attachment_desc.setDestinationAlphaBlendFactor(blend_factor_to_mtl(blend.dst_alpha_blend_factor));
            attachment_desc.setWriteMask(color_components_to_mtl(blend.write_mask));
        }
        descriptor.setAlphaToCoverageEnabled(info.blend.alpha_to_coverage_enabled);


        for (idx, &format) in info.render_target_formats.iter().enumerate() {
            let attachment_desc = descriptor.colorAttachments().objectAtIndexedSubscript(idx as NSUInteger);
            descriptor.setRasterSampleCount(samples_to_mtl(info.rasterizer.sample_count));
            attachment_desc.setPixelFormat(format_to_mtl(format));
        }

        descriptor.setRasterizationEnabled(true);
        descriptor.setDepthAttachmentPixelFormat(format_to_mtl(info.depth_stencil_format));
        descriptor.setInputPrimitiveTopology(match info.primitive_type {
            gpu::PrimitiveType::Triangles | gpu::PrimitiveType::TriangleStrip => objc2_metal::MTLPrimitiveTopologyClass::Triangle,
            gpu::PrimitiveType::Lines | gpu::PrimitiveType::LineStrip => objc2_metal::MTLPrimitiveTopologyClass::Line,
            gpu::PrimitiveType::Points => objc2_metal::MTLPrimitiveTopologyClass::Point,
        });

        let primitive_type = match info.primitive_type {
            gpu::PrimitiveType::Triangles => objc2_metal::MTLPrimitiveType::Triangle,
            gpu::PrimitiveType::TriangleStrip => objc2_metal::MTLPrimitiveType::TriangleStrip,
            gpu::PrimitiveType::Lines => objc2_metal::MTLPrimitiveType::Line,
            gpu::PrimitiveType::LineStrip => objc2_metal::MTLPrimitiveType::LineStrip,
            gpu::PrimitiveType::Points => objc2_metal::MTLPrimitiveType::Point,
        };

        if let Some(name) = name {
            descriptor.setLabel(Some(&NSString::from_str(name)));
        }

        let mut resource_map = PipelineResourceMap {
            resources: HashMap::new(),
            push_constants: HashMap::new(),
            bindless_argument_buffer_binding: HashMap::new()
        };
        for ((set, binding), msl_binding) in &info.vs.resource_map.resources {
            if let Some(buffer_binding) = msl_binding.buffer_binding {
                descriptor.vertexBuffers().objectAtIndexedSubscript(buffer_binding as NSUInteger).setMutability(if msl_binding.writable {
                    objc2_metal::MTLMutability::Mutable
                } else {
                    objc2_metal::MTLMutability::Immutable
                });
            }
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
                if let Some(buffer_binding) = msl_binding.buffer_binding {
                    descriptor.fragmentBuffers().objectAtIndexedSubscript(buffer_binding as NSUInteger).setMutability(if msl_binding.writable {
                        objc2_metal::MTLMutability::Mutable
                    } else {
                        objc2_metal::MTLMutability::Immutable
                    });
                }
                resource_map.resources.insert((gpu::ShaderType::FragmentShader, *set, *binding), msl_binding.clone());
            }
            if let Some(push_constants) = fs.resource_map.push_constants.as_ref() {
                resource_map.push_constants.insert(gpu::ShaderType::FragmentShader, push_constants.clone());
            }
            if let Some(bindless_binding) = fs.resource_map.bindless_argument_buffer_binding {
                resource_map.bindless_argument_buffer_binding.insert(gpu::ShaderType::FragmentShader, bindless_binding);
            }
        }

        let pipeline = device.newRenderPipelineStateWithDescriptor_error(&descriptor).unwrap();

        let rasterizer_state = MTLRasterizerInfo {
            front_face: match info.rasterizer.front_face {
                gpu::FrontFace::CounterClockwise => objc2_metal::MTLWinding::CounterClockwise,
                gpu::FrontFace::Clockwise => objc2_metal::MTLWinding::Clockwise,
            },
            fill_mode: match info.rasterizer.fill_mode {
                gpu::FillMode::Fill => objc2_metal::MTLTriangleFillMode::Fill,
                gpu::FillMode::Line => objc2_metal::MTLTriangleFillMode::Lines,
            },
            cull_mode: match info.rasterizer.cull_mode {
                gpu::CullMode::None => objc2_metal::MTLCullMode::None,
                gpu::CullMode::Front => objc2_metal::MTLCullMode::Front,
                gpu::CullMode::Back => objc2_metal::MTLCullMode::Back,
            },
        };

        let depth_stencil_state_descriptor = objc2_metal::MTLDepthStencilDescriptor::new();
        depth_stencil_state_descriptor.setDepthCompareFunction(if !info.depth_stencil.depth_test_enabled {
            objc2_metal::MTLCompareFunction::Always
        } else {
            compare_func_to_mtl(info.depth_stencil.depth_func)
        });
        depth_stencil_state_descriptor.setDepthWriteEnabled(info.depth_stencil.depth_write_enabled);
        depth_stencil_state_descriptor.setFrontFaceStencil(
            Some(&stencil_info_to_mtl(&info.depth_stencil.stencil_front,
                info.depth_stencil.stencil_enable,
                info.depth_stencil.stencil_read_mask,
                info.depth_stencil.stencil_write_mask
        )));
        depth_stencil_state_descriptor.setBackFaceStencil(
            Some(&stencil_info_to_mtl(&info.depth_stencil.stencil_back,
                info.depth_stencil.stencil_enable,
                info.depth_stencil.stencil_read_mask,
                info.depth_stencil.stencil_write_mask
        )));
        let depth_stencil_state = device.newDepthStencilStateWithDescriptor(&depth_stencil_state_descriptor).unwrap();

        Self {
            pipeline,
            primitive_type,
            resource_map: Arc::new(resource_map),
            rasterizer_state,
            depth_stencil_state
        }
    }


    pub(crate) unsafe fn new_mesh(device: &ProtocolObject<dyn objc2_metal::MTLDevice>, info: &gpu::MeshGraphicsPipelineInfo<MTLBackend>, name: Option<&str>) -> Self {
        let descriptor = objc2_metal::MTLMeshRenderPipelineDescriptor::new();

        if let Some(name) = name {
            descriptor.setLabel(Some(&NSString::from_str(name)));
        }

        descriptor.setMeshFunction(Some(info.ms.function_handle()));
        descriptor.setObjectFunction(info.ts.map(|ts| ts.function_handle()));
        descriptor.setFragmentFunction(info.fs.map(|fs| fs.function_handle()));

        for (idx, blend) in info.blend.attachments.iter().enumerate() {
            let attachment_desc = descriptor.colorAttachments().objectAtIndexedSubscript(idx as NSUInteger);
            attachment_desc.setBlendingEnabled(blend.blend_enabled);
            attachment_desc.setRgbBlendOperation(blend_op_to_mtl(blend.color_blend_op));
            attachment_desc.setAlphaBlendOperation(blend_op_to_mtl(blend.alpha_blend_op));
            attachment_desc.setSourceRGBBlendFactor(blend_factor_to_mtl(blend.src_color_blend_factor));
            attachment_desc.setDestinationRGBBlendFactor(blend_factor_to_mtl(blend.dst_color_blend_factor));
            attachment_desc.setSourceAlphaBlendFactor(blend_factor_to_mtl(blend.src_alpha_blend_factor));
            attachment_desc.setDestinationAlphaBlendFactor(blend_factor_to_mtl(blend.dst_alpha_blend_factor));
            attachment_desc.setWriteMask(color_components_to_mtl(blend.write_mask));
        }
        descriptor.setAlphaToCoverageEnabled(info.blend.alpha_to_coverage_enabled);


        for (idx, &format) in info.render_target_formats.iter().enumerate() {
            let attachment_desc = descriptor.colorAttachments().objectAtIndexedSubscript(idx as NSUInteger);
            descriptor.setRasterSampleCount(samples_to_mtl(info.rasterizer.sample_count));
            attachment_desc.setPixelFormat(format_to_mtl(format));
        }

        descriptor.setRasterizationEnabled(true);
        descriptor.setDepthAttachmentPixelFormat(format_to_mtl(info.depth_stencil_format));

        if let Some(name) = name {
            descriptor.setLabel(Some(&NSString::from_str(name)));
        }

        let mut resource_map = PipelineResourceMap {
            resources: HashMap::new(),
            push_constants: HashMap::new(),
            bindless_argument_buffer_binding: HashMap::new()
        };
        for ((set, binding), msl_binding) in &info.ms.resource_map.resources {
            if let Some(buffer_binding) = msl_binding.buffer_binding {
                descriptor.meshBuffers().objectAtIndexedSubscript(buffer_binding as NSUInteger).setMutability(if msl_binding.writable {
                    objc2_metal::MTLMutability::Mutable
                } else {
                    objc2_metal::MTLMutability::Immutable
                });
            }
            resource_map.resources.insert((gpu::ShaderType::MeshShader, *set, *binding), msl_binding.clone());
        }
        if let Some(bindless_binding) = info.ms.resource_map.bindless_argument_buffer_binding {
            resource_map.bindless_argument_buffer_binding.insert(gpu::ShaderType::MeshShader, bindless_binding);
        }

        if let Some(ts) = info.ts.as_ref() {
            for ((set, binding), msl_binding) in &ts.resource_map.resources {
                if let Some(buffer_binding) = msl_binding.buffer_binding {
                    descriptor.objectBuffers().objectAtIndexedSubscript(buffer_binding as NSUInteger).setMutability(if msl_binding.writable {
                        objc2_metal::MTLMutability::Mutable
                    } else {
                        objc2_metal::MTLMutability::Immutable
                    });
                }
                resource_map.resources.insert((gpu::ShaderType::TaskShader, *set, *binding), msl_binding.clone());
            }
            if let Some(push_constants) = ts.resource_map.push_constants.as_ref() {
                resource_map.push_constants.insert(gpu::ShaderType::TaskShader, push_constants.clone());
            }
            if let Some(bindless_binding) = ts.resource_map.bindless_argument_buffer_binding {
                resource_map.bindless_argument_buffer_binding.insert(gpu::ShaderType::TaskShader, bindless_binding);
            }
        }

        if let Some(fs) = info.fs.as_ref() {
            for ((set, binding), msl_binding) in &fs.resource_map.resources {
                if let Some(buffer_binding) = msl_binding.buffer_binding {
                    descriptor.fragmentBuffers().objectAtIndexedSubscript(buffer_binding as NSUInteger).setMutability(if msl_binding.writable {
                        objc2_metal::MTLMutability::Mutable
                    } else {
                        objc2_metal::MTLMutability::Immutable
                    });
                }
                resource_map.resources.insert((gpu::ShaderType::FragmentShader, *set, *binding), msl_binding.clone());
            }
            if let Some(push_constants) = fs.resource_map.push_constants.as_ref() {
                resource_map.push_constants.insert(gpu::ShaderType::FragmentShader, push_constants.clone());
            }
            if let Some(bindless_binding) = fs.resource_map.bindless_argument_buffer_binding {
                resource_map.bindless_argument_buffer_binding.insert(gpu::ShaderType::FragmentShader, bindless_binding);
            }
        }

        let pipeline = device.newRenderPipelineStateWithMeshDescriptor_options_reflection_error(
            &descriptor,
            objc2_metal::MTLPipelineOption::None,
            None
        ).unwrap();

        let rasterizer_state = MTLRasterizerInfo {
            front_face: match info.rasterizer.front_face {
                gpu::FrontFace::CounterClockwise => objc2_metal::MTLWinding::CounterClockwise,
                gpu::FrontFace::Clockwise => objc2_metal::MTLWinding::Clockwise,
            },
            fill_mode: match info.rasterizer.fill_mode {
                gpu::FillMode::Fill => objc2_metal::MTLTriangleFillMode::Fill,
                gpu::FillMode::Line => objc2_metal::MTLTriangleFillMode::Lines,
            },
            cull_mode: match info.rasterizer.cull_mode {
                gpu::CullMode::None => objc2_metal::MTLCullMode::None,
                gpu::CullMode::Front => objc2_metal::MTLCullMode::Front,
                gpu::CullMode::Back => objc2_metal::MTLCullMode::Back,
            },
        };

        let depth_stencil_state_descriptor = objc2_metal::MTLDepthStencilDescriptor::new();
        depth_stencil_state_descriptor.setDepthCompareFunction(if !info.depth_stencil.depth_test_enabled {
            objc2_metal::MTLCompareFunction::Always
        } else {
            compare_func_to_mtl(info.depth_stencil.depth_func)
        });
        depth_stencil_state_descriptor.setDepthWriteEnabled(info.depth_stencil.depth_write_enabled);
        depth_stencil_state_descriptor.setFrontFaceStencil(
            Some(&stencil_info_to_mtl(&info.depth_stencil.stencil_front,
                info.depth_stencil.stencil_enable,
                info.depth_stencil.stencil_read_mask,
                info.depth_stencil.stencil_write_mask
        )));
        depth_stencil_state_descriptor.setBackFaceStencil(
            Some(&stencil_info_to_mtl(&info.depth_stencil.stencil_back,
                info.depth_stencil.stencil_enable,
                info.depth_stencil.stencil_read_mask,
                info.depth_stencil.stencil_write_mask
        )));
        let depth_stencil_state = device.newDepthStencilStateWithDescriptor(&depth_stencil_state_descriptor).unwrap();

        Self {
            pipeline,
            primitive_type: objc2_metal::MTLPrimitiveType::Triangle, // Not part of the pipeline with mesh shaders
            resource_map: Arc::new(resource_map),
            rasterizer_state,
            depth_stencil_state
        }
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLRenderPipelineState> {
        &self.pipeline
    }

    pub(crate) fn primitive_type(&self) -> objc2_metal::MTLPrimitiveType {
        self.primitive_type
    }

    pub(crate) fn rasterizer_state(&self) -> &MTLRasterizerInfo {
        &self.rasterizer_state
    }

    pub(crate) fn depth_stencil_state(&self) -> &ProtocolObject<dyn objc2_metal::MTLDepthStencilState> {
        &self.depth_stencil_state
    }

    pub(crate) fn resource_map(&self) -> &Arc<PipelineResourceMap> {
        &self.resource_map
    }
}

pub struct MTLComputePipeline {
    pipeline: Retained<ProtocolObject<dyn objc2_metal::MTLComputePipelineState>>,
    resource_map: Arc<PipelineResourceMap>
}

unsafe impl Send for MTLComputePipeline {}
unsafe impl Sync for MTLComputePipeline {}

impl MTLComputePipeline {
    pub(crate) unsafe fn new(device: &ProtocolObject<dyn objc2_metal::MTLDevice>, shader: &MTLShader, name: Option<&str>) -> Self {
        let descriptor = objc2_metal::MTLComputePipelineDescriptor::new();
        if let Some(name) = name {
            descriptor.setLabel(Some(&NSString::from_str(name)));
        }

        descriptor.setComputeFunction(Some(shader.function_handle()));

        let pipeline = device.newComputePipelineStateWithDescriptor_options_reflection_error(&descriptor, objc2_metal::MTLPipelineOption::None, None).unwrap();
        let mut resource_map = PipelineResourceMap {
            resources: HashMap::new(),
            push_constants: HashMap::new(),
            bindless_argument_buffer_binding: HashMap::new()
        };
        for ((set, binding), msl_binding) in &shader.resource_map.resources {
            if let Some(buffer_binding) = msl_binding.buffer_binding {
                descriptor.buffers().objectAtIndexedSubscript(buffer_binding as NSUInteger).setMutability(if msl_binding.writable {
                    objc2_metal::MTLMutability::Mutable
                } else {
                    objc2_metal::MTLMutability::Immutable
                });
            }
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

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLComputePipelineState> {
        &self.pipeline
    }

    pub(crate) fn resource_map(&self) -> &Arc<PipelineResourceMap> {
        &self.resource_map
    }
}

impl gpu::ComputePipeline for MTLComputePipeline {
    fn binding_info(&self, _set: gpu::BindingFrequency, _slot: u32) -> Option<gpu::BindingInfo> {
        todo!()
    }
}
