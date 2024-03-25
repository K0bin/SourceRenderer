use std::hash::Hash;

use metal;
use metal::foreign_types::ForeignType;

use sourcerenderer_core::gpu;

use super::*;

pub struct MTLShader {
    shader_type: gpu::ShaderType,
    library: metal::Library
}

impl MTLShader {
    pub(crate) fn new(device: &metal::DeviceRef, shader_type: gpu::ShaderType, data: &[u8]) -> Self {
        let library = device.new_library_with_data(data).unwrap();
        Self {
            shader_type,
            library
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

const SHADER_ENTRY_POINT_NAME: &str = "main";

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
    let mut colors = 0u64;
    colors |= components_bits.rotate_left(
        (gpu::ColorComponents::RED.bits() as u64).trailing_zeros()
            - metal::MTLColorWriteMask::Red.bits().trailing_zeros()
    ) & metal::MTLColorWriteMask::Red.bits();
    colors |= components_bits.rotate_left(
        (gpu::ColorComponents::GREEN.bits() as u64).trailing_zeros()
            - metal::MTLColorWriteMask::Green.bits().trailing_zeros()
    ) & metal::MTLColorWriteMask::Green.bits();
    colors |= components_bits.rotate_left(
        (gpu::ColorComponents::BLUE.bits() as u64).trailing_zeros()
            - metal::MTLColorWriteMask::Blue.bits().trailing_zeros()
    ) & metal::MTLColorWriteMask::Blue.bits();
    colors |= components_bits.rotate_left(
        (gpu::ColorComponents::ALPHA.bits() as u64).trailing_zeros()
            - metal::MTLColorWriteMask::Alpha.bits().trailing_zeros()
    ) & metal::MTLColorWriteMask::Alpha.bits();
    metal::MTLColorWriteMask::from_bits(colors).unwrap()
}

pub struct MTLGraphicsPipeline {
    pipeline: metal::RenderPipelineState
}

impl MTLGraphicsPipeline {
    pub(crate) fn new(device: &metal::DeviceRef, info: &gpu::GraphicsPipelineInfo<MTLBackend>, renderpass_info: &gpu::RenderPassInfo, subpass: u32, name: Option<&str>) -> Self {
        let subpass = &renderpass_info.subpasses[subpass as usize];

        let descriptor = metal::RenderPipelineDescriptor::new();
        let function_descriptor = metal::FunctionDescriptor::new();
        function_descriptor.set_name(SHADER_ENTRY_POINT_NAME);
        descriptor.set_vertex_function(Some(&info.vs.handle().new_function_with_descriptor(&function_descriptor).unwrap()));
        descriptor.set_fragment_function(info.fs.map(|fs| &fs.handle().new_function_with_descriptor(&function_descriptor).unwrap() as &metal::FunctionRef));

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
            gpu::PrimitiveType::Triangles => metal::MTLPrimitiveTopologyClass::Triangle,
            gpu::PrimitiveType::TriangleStrip => panic!("Metal does not support triangle strips"),
            gpu::PrimitiveType::Lines => metal::MTLPrimitiveTopologyClass::Line,
            gpu::PrimitiveType::LineStrip => panic!("Metal does not support line strips"),
            gpu::PrimitiveType::Points => metal::MTLPrimitiveTopologyClass::Point,
        });

        let pipeline = device.new_render_pipeline_state(&descriptor).unwrap();
        Self {
            pipeline
        }
    }

    pub(crate) fn handle(&self) -> &metal::RenderPipelineStateRef {
        &self.pipeline
    }
}

