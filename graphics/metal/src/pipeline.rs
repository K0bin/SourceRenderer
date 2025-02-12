use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::sync::Arc;

use metal;
use metal::foreign_types::ForeignType;

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
    library: metal::Library,
    function: metal::Function,
    resource_map: ShaderResourceMap,
}

const METAL_DEBUGGER_WORKAROUND: bool = true;

impl MTLShader {
    pub(crate) fn new(device: &metal::DeviceRef, shader: &gpu::PackedShader, name: Option<&str>) -> Self {
        assert_ne!(shader.shader_air.len(), 0);
        let data = shader.shader_air.clone(); // Need to keep this alive because of a bug in metal-rs

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
            device.new_library_with_file(&temp_path).unwrap()
        } else {
            device.new_library_with_data(&data).unwrap()
        };

        if let Some(name) = name {
            library.set_label(name);
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

        let function = library.get_function(SHADER_ENTRY_POINT_NAME, None).unwrap();

        Self {
            shader_type: shader.shader_type,
            library,
            resource_map,
            function
        }
    }

    pub(crate) fn function_handle(&self) -> &metal::FunctionRef {
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

fn stencil_info_to_mtl(stencil: &gpu::StencilInfo, stencil_enabled: bool, read_mask: u8, write_mask: u8) -> metal::StencilDescriptor {
    let descriptor = metal::StencilDescriptor::new();
    descriptor.set_stencil_compare_function(if stencil_enabled { compare_func_to_mtl(stencil.func)
    }else {
        metal::MTLCompareFunction::Always
    });
    descriptor.set_depth_stencil_pass_operation(stencil_op_to_mtl(stencil.pass_op));
    descriptor.set_depth_failure_operation(stencil_op_to_mtl(stencil.fail_op));
    descriptor.set_read_mask(read_mask as u32);
    descriptor.set_write_mask(write_mask as u32);
    descriptor
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub(crate) struct MTLRasterizerInfo {
  pub(crate) fill_mode: metal::MTLTriangleFillMode,
  pub(crate) cull_mode: metal::MTLCullMode,
  pub(crate) front_face: metal::MTLWinding,
}

pub struct MTLGraphicsPipeline {
    pipeline: metal::RenderPipelineState,
    primitive_type: metal::MTLPrimitiveType,
    resource_map: Arc<PipelineResourceMap>,
    rasterizer_state: MTLRasterizerInfo,
    depth_stencil_state: metal::DepthStencilState,
}

impl MTLGraphicsPipeline {
    pub(crate) fn new(device: &metal::DeviceRef, info: &gpu::GraphicsPipelineInfo<MTLBackend>, name: Option<&str>) -> Self {
        let descriptor = metal::RenderPipelineDescriptor::new();

        if let Some(name) = name {
            descriptor.set_label(name);
        }

        descriptor.set_vertex_function(Some(info.vs.function_handle()));
        descriptor.set_fragment_function(info.fs.map(|fs| fs.function_handle()));

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
                gpu::Format::R32UInt => metal::MTLVertexFormat::UInt,
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


        for (idx, &format) in info.render_target_formats.iter().enumerate() {
            let attachment_desc = descriptor.color_attachments().object_at(idx as u64).unwrap();
            descriptor.set_raster_sample_count(samples_to_mtl(info.rasterizer.sample_count));
            attachment_desc.set_pixel_format(format_to_mtl(format));
            if info.depth_stencil_format.is_stencil() {
                descriptor.set_stencil_attachment_pixel_format(format_to_mtl(info.depth_stencil_format));
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

        let mut resource_map = PipelineResourceMap {
            resources: HashMap::new(),
            push_constants: HashMap::new(),
            bindless_argument_buffer_binding: HashMap::new()
        };
        for ((set, binding), msl_binding) in &info.vs.resource_map.resources {
            if let Some(buffer_binding) = msl_binding.buffer_binding {
                descriptor.vertex_buffers().unwrap().object_at(buffer_binding as u64).as_ref().unwrap().set_mutability(if msl_binding.writable {
                    metal::MTLMutability::Mutable
                } else {
                    metal::MTLMutability::Immutable
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
                    descriptor.fragment_buffers().unwrap().object_at(buffer_binding as u64).as_ref().unwrap().set_mutability(if msl_binding.writable {
                        metal::MTLMutability::Mutable
                    } else {
                        metal::MTLMutability::Immutable
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

        let pipeline = device.new_render_pipeline_state(&descriptor).unwrap();

        let rasterizer_state = MTLRasterizerInfo {
            front_face: match info.rasterizer.front_face {
                gpu::FrontFace::CounterClockwise => metal::MTLWinding::CounterClockwise,
                gpu::FrontFace::Clockwise => metal::MTLWinding::Clockwise,
            },
            fill_mode: match info.rasterizer.fill_mode {
                gpu::FillMode::Fill => metal::MTLTriangleFillMode::Fill,
                gpu::FillMode::Line => metal::MTLTriangleFillMode::Lines,
            },
            cull_mode: match info.rasterizer.cull_mode {
                gpu::CullMode::None => metal::MTLCullMode::None,
                gpu::CullMode::Front => metal::MTLCullMode::Front,
                gpu::CullMode::Back => metal::MTLCullMode::Back,
            },
        };

        let depth_stencil_state_descriptor = metal::DepthStencilDescriptor::new();
        depth_stencil_state_descriptor.set_depth_compare_function(if !info.depth_stencil.depth_test_enabled {
            metal::MTLCompareFunction::Always
        } else {
            compare_func_to_mtl(info.depth_stencil.depth_func)
        });
        depth_stencil_state_descriptor.set_depth_write_enabled(info.depth_stencil.depth_write_enabled);
        depth_stencil_state_descriptor.set_front_face_stencil(
            Some(&stencil_info_to_mtl(&info.depth_stencil.stencil_front,
                info.depth_stencil.stencil_enable,
                info.depth_stencil.stencil_read_mask,
                info.depth_stencil.stencil_write_mask
        )));
        depth_stencil_state_descriptor.set_back_face_stencil(
            Some(&stencil_info_to_mtl(&info.depth_stencil.stencil_back,
                info.depth_stencil.stencil_enable,
                info.depth_stencil.stencil_read_mask,
                info.depth_stencil.stencil_write_mask
        )));
        let depth_stencil_state = device.new_depth_stencil_state(&depth_stencil_state_descriptor);

        Self {
            pipeline,
            primitive_type,
            resource_map: Arc::new(resource_map),
            rasterizer_state,
            depth_stencil_state
        }
    }

    pub(crate) fn handle(&self) -> &metal::RenderPipelineStateRef {
        &self.pipeline
    }

    pub(crate) fn primitive_type(&self) -> metal::MTLPrimitiveType {
        self.primitive_type
    }

    pub(crate) fn rasterizer_state(&self) -> &MTLRasterizerInfo {
        &self.rasterizer_state
    }

    pub(crate) fn depth_stencil_state(&self) -> &metal::DepthStencilState {
        &self.depth_stencil_state
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
        let descriptor = metal::ComputePipelineDescriptor::new();
        if let Some(name) = name {
            descriptor.set_label(name);
        }

        descriptor.set_compute_function(Some(shader.function_handle()));

        let pipeline = device.new_compute_pipeline_state(&descriptor).unwrap();
        let mut resource_map = PipelineResourceMap {
            resources: HashMap::new(),
            push_constants: HashMap::new(),
            bindless_argument_buffer_binding: HashMap::new()
        };
        for ((set, binding), msl_binding) in &shader.resource_map.resources {
            if let Some(buffer_binding) = msl_binding.buffer_binding {
                descriptor.buffers().unwrap().object_at(buffer_binding as u64).as_ref().unwrap().set_mutability(if msl_binding.writable {
                    metal::MTLMutability::Mutable
                } else {
                    metal::MTLMutability::Immutable
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

    pub(crate) fn handle(&self) -> &metal::ComputePipelineStateRef {
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
