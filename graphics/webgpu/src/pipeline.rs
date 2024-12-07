use js_sys::{wasm_bindgen::JsValue, Array};
use log::warn;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, BindingFrequency, Format, GraphicsPipelineInfo, Shader, ShaderType, StencilInfo};
use web_sys::{Gpu, GpuBlendComponent, GpuBlendFactor, GpuBlendOperation, GpuBlendState, GpuColorTargetState, GpuCompareFunction, GpuComputePipeline, GpuComputePipelineDescriptor, GpuCullMode, GpuDepthStencilState, GpuDevice, GpuFragmentState, GpuFrontFace, GpuMultisampleState, GpuPrimitiveState, GpuPrimitiveTopology, GpuProgrammableStage, GpuRenderPipeline, GpuRenderPipelineDescriptor, GpuShaderModule, GpuShaderModuleDescriptor, GpuStencilFaceState, GpuStencilOperation, GpuVertexAttribute, GpuVertexBufferLayout, GpuVertexFormat, GpuVertexState, GpuVertexStepMode};
use std::hash::Hash;

use crate::{texture::format_to_webgpu, WebGPUBackend};

pub struct WebGPUShader {
    module: GpuShaderModule,
    shader_type: ShaderType,
    resources: [Box<[gpu::Resource]>; 4],
}

impl PartialEq for WebGPUShader {
    fn eq(&self, other: &Self) -> bool {
        self.module == other.module
    }
}

impl Eq for WebGPUShader {}

impl Hash for WebGPUShader {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let ptr_val: usize = unsafe { std::mem::transmute(self.module.as_ref() as *const GpuShaderModule) };
        ptr_val.hash(state);
    }
}

unsafe impl Send for WebGPUShader {}
unsafe impl Sync for WebGPUShader {}

impl WebGPUShader {
    pub fn new(device: &GpuDevice, shader: gpu::PackedShader, name: Option<&str>) -> Self {
        let descriptor = GpuShaderModuleDescriptor::new(&shader.shader_wgsl);
        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let module = device.create_shader_module(&descriptor);
        Self {
            module,
            shader_type: shader.shader_type,
            resources: shader.resources.clone()
        }
    }

    pub(crate) fn module(&self) -> &GpuShaderModule {
        &self.module
    }

    pub(crate) fn resources(&self) -> &[Box<[gpu::Resource]>; 4] {
        &self.resources
    }
}

impl Shader for WebGPUShader {
    fn shader_type(&self) -> gpu::ShaderType {
        self.shader_type
    }
}

pub struct WebGPUGraphicsPipeline {
    pipeline: GpuRenderPipeline
}

unsafe impl Send for WebGPUGraphicsPipeline {}
unsafe impl Sync for WebGPUGraphicsPipeline {}

fn format_to_vertex_format(format: Format) -> GpuVertexFormat {
    match format {
        Format::Unknown => GpuVertexFormat::__Invalid,
        Format::R32UNorm => panic!("Unsupported vertex format"),
        Format::R16UNorm => GpuVertexFormat::Unorm16,
        Format::R8Unorm => GpuVertexFormat::Unorm8,
        Format::RGBA8UNorm => GpuVertexFormat::Unorm8x4,
        Format::RGBA8Srgb => panic!("Unsupported vertex format"),
        Format::BGR8UNorm => panic!("Unsupported vertex format"),
        Format::BGRA8UNorm => GpuVertexFormat::Unorm8x4Bgra,
        Format::BC1 => panic!("Unsupported vertex format"),
        Format::BC1Alpha => panic!("Unsupported vertex format"),
        Format::BC2 => panic!("Unsupported vertex format"),
        Format::BC3 => panic!("Unsupported vertex format"),
        Format::R16Float => GpuVertexFormat::Float16,
        Format::R32Float => GpuVertexFormat::Float32,
        Format::RG32Float => GpuVertexFormat::Float32x2,
        Format::RG16Float => GpuVertexFormat::Float16x2,
        Format::RGB32Float => GpuVertexFormat::Float32x3,
        Format::RGBA32Float => GpuVertexFormat::Float32x4,
        Format::RG16UNorm => GpuVertexFormat::Unorm16x2,
        Format::RG8UNorm => GpuVertexFormat::Unorm8x2,
        Format::R32UInt => GpuVertexFormat::Uint32,
        Format::RGBA16Float => GpuVertexFormat::Float16x4,
        Format::R11G11B10Float => panic!("Unsupported vertex format"),
        Format::RG16UInt => GpuVertexFormat::Uint16x2,
        Format::RG16SInt => GpuVertexFormat::Sint16x2,
        Format::R16UInt => GpuVertexFormat::Uint16,
        Format::R16SNorm => GpuVertexFormat::Snorm16,
        Format::R16SInt => GpuVertexFormat::Sint16,
        Format::D16 => panic!("Unsupported vertex format"),
        Format::D16S8 => panic!("Unsupported vertex format"),
        Format::D32 => panic!("Unsupported vertex format"),
        Format::D32S8 => panic!("Unsupported vertex format"),
        Format::D24S8 => panic!("Unsupported vertex format"),
    }
}

pub(crate) fn compare_func_to_webgpu(compare_func: gpu::CompareFunc) -> GpuCompareFunction {
    match compare_func {
        gpu::CompareFunc::Never => GpuCompareFunction::Never,
        gpu::CompareFunc::Less => GpuCompareFunction::Less,
        gpu::CompareFunc::LessEqual => GpuCompareFunction::LessEqual,
        gpu::CompareFunc::Equal => GpuCompareFunction::Equal,
        gpu::CompareFunc::NotEqual => GpuCompareFunction::NotEqual,
        gpu::CompareFunc::GreaterEqual => GpuCompareFunction::GreaterEqual,
        gpu::CompareFunc::Greater => GpuCompareFunction::Greater,
        gpu::CompareFunc::Always => GpuCompareFunction::Always,
    }
}

fn blend_factor_to_webgpu(blend_factor: gpu::BlendFactor) -> GpuBlendFactor {
    match blend_factor {
        gpu::BlendFactor::Zero => GpuBlendFactor::Zero,
        gpu::BlendFactor::One => GpuBlendFactor::One,
        gpu::BlendFactor::SrcColor => GpuBlendFactor::Src,
        gpu::BlendFactor::OneMinusSrcColor => GpuBlendFactor::OneMinusSrc,
        gpu::BlendFactor::DstColor => GpuBlendFactor::Dst,
        gpu::BlendFactor::OneMinusDstColor => GpuBlendFactor::OneMinusDst,
        gpu::BlendFactor::SrcAlpha => GpuBlendFactor::SrcAlpha,
        gpu::BlendFactor::OneMinusSrcAlpha => GpuBlendFactor::OneMinusSrcAlpha,
        gpu::BlendFactor::DstAlpha => GpuBlendFactor::DstAlpha,
        gpu::BlendFactor::OneMinusDstAlpha => GpuBlendFactor::OneMinusDstAlpha,
        gpu::BlendFactor::ConstantColor => GpuBlendFactor::Constant,
        gpu::BlendFactor::OneMinusConstantColor => GpuBlendFactor::OneMinusConstant,
        gpu::BlendFactor::SrcAlphaSaturate => GpuBlendFactor::SrcAlphaSaturated,
        gpu::BlendFactor::Src1Color => GpuBlendFactor::Src1,
        gpu::BlendFactor::OneMinusSrc1Color => GpuBlendFactor::OneMinusSrc1,
        gpu::BlendFactor::Src1Alpha => GpuBlendFactor::Src1Alpha,
        gpu::BlendFactor::OneMinusSrc1Alpha => GpuBlendFactor::OneMinusSrc1Alpha,
    }
}

fn blend_op_to_webgpu(blend_op: gpu::BlendOp) -> GpuBlendOperation {
    match blend_op {
        gpu::BlendOp::Add => GpuBlendOperation::Add,
        gpu::BlendOp::Subtract => GpuBlendOperation::Subtract,
        gpu::BlendOp::ReverseSubtract => GpuBlendOperation::ReverseSubtract,
        gpu::BlendOp::Min => GpuBlendOperation::Min,
        gpu::BlendOp::Max => GpuBlendOperation::Max,
    }
}

fn blend_attachment_to_webgpu(blend_attachment: &gpu::AttachmentBlendInfo, color: bool) -> GpuBlendComponent {
    let blend_component = GpuBlendComponent::new();
    if !blend_attachment.blend_enabled {
        blend_component.set_operation(GpuBlendOperation::Add);
        blend_component.set_src_factor(GpuBlendFactor::One);
        blend_component.set_dst_factor(GpuBlendFactor::Zero);
    } else {
        blend_component.set_dst_factor(blend_factor_to_webgpu(if color { blend_attachment.dst_color_blend_factor } else { blend_attachment.dst_alpha_blend_factor }));
        blend_component.set_src_factor(blend_factor_to_webgpu(if color { blend_attachment.src_color_blend_factor } else { blend_attachment.src_alpha_blend_factor }));
        blend_component.set_operation(blend_op_to_webgpu(if color { blend_attachment.color_blend_op } else { blend_attachment.alpha_blend_op }));
    }
    blend_component
}

impl WebGPUGraphicsPipeline {
    pub fn new(device: &GpuDevice, info: &GraphicsPipelineInfo<WebGPUBackend>, name: Option<&str>) -> Result<Self, ()> {
        let vertex_buffers = Array::new_with_length(info.vertex_layout.input_assembler.len() as u32);
        for vb_info in info.vertex_layout.input_assembler {
            let mut attributes_count = 0;
            let attributes = Array::new();
            for shader_input in info.vertex_layout.shader_inputs {
                if shader_input.input_assembler_binding != vb_info.binding {
                    continue;
                }
                let shader_attr: GpuVertexAttribute = GpuVertexAttribute::new(format_to_vertex_format(shader_input.format), shader_input.offset as f64, shader_input.location_vk_mtl);
                attributes.set(attributes_count, JsValue::from(shader_attr));
                attributes_count += 1;
            }

            let vb_layout = GpuVertexBufferLayout::new(vb_info.stride as f64, &attributes);
            vb_layout.set_step_mode(match vb_info.input_rate {
                gpu::InputRate::PerVertex => GpuVertexStepMode::Vertex,
                gpu::InputRate::PerInstance => GpuVertexStepMode::Instance,
            });
            vertex_buffers.set(vb_info.binding as u32, JsValue::from(&vb_layout));
        }

        let vertex_state = GpuVertexState::new(info.vs.module());
        vertex_state.set_buffers(&JsValue::from(vertex_buffers));
        let descriptor = GpuRenderPipelineDescriptor::new(&JsValue::from_str("auto"), &vertex_state);

        let primitive = GpuPrimitiveState::new();
        primitive.set_cull_mode(match info.rasterizer.cull_mode {
            gpu::CullMode::None => GpuCullMode::None,
            gpu::CullMode::Front => GpuCullMode::Front,
            gpu::CullMode::Back => GpuCullMode::Back,
        });
        primitive.set_front_face(match info.rasterizer.front_face {
            gpu::FrontFace::CounterClockwise => GpuFrontFace::Ccw,
            gpu::FrontFace::Clockwise => GpuFrontFace::Cw,
        });
        primitive.set_topology(match info.primitive_type {
            gpu::PrimitiveType::Triangles => GpuPrimitiveTopology::TriangleList,
            gpu::PrimitiveType::TriangleStrip => GpuPrimitiveTopology::TriangleStrip,
            gpu::PrimitiveType::Lines => GpuPrimitiveTopology::LineList,
            gpu::PrimitiveType::LineStrip => GpuPrimitiveTopology::LineStrip,
            gpu::PrimitiveType::Points => GpuPrimitiveTopology::PointList,
        });
        if info.primitive_type == gpu::PrimitiveType::TriangleStrip || info.primitive_type == gpu::PrimitiveType::LineStrip {
            primitive.set_strip_index_format(web_sys::GpuIndexFormat::Uint32);
            warn!("WebGPU requires specifying the index format at pipeline creation time. Other apis only require this at draw time. Defaulting to Uint32.");
        }
        descriptor.set_primitive(&primitive);

        if info.depth_stencil_format != Format::Unknown {
            let depth_stencil = GpuDepthStencilState::new(format_to_webgpu(info.depth_stencil_format));
            depth_stencil.set_depth_write_enabled(info.depth_stencil.depth_write_enabled);
            depth_stencil.set_depth_compare(if !info.depth_stencil.depth_test_enabled {
                GpuCompareFunction::Always
            } else {
                compare_func_to_webgpu(info.depth_stencil.depth_func)
            });
            depth_stencil.set_stencil_write_mask(info.depth_stencil.stencil_write_mask as u32);
            depth_stencil.set_stencil_read_mask(info.depth_stencil.stencil_read_mask as u32);

            fn stencil_op_to_webgpu(stencil_op: gpu::StencilOp) -> GpuStencilOperation {
                match stencil_op {
                    gpu::StencilOp::Keep => GpuStencilOperation::Keep,
                    gpu::StencilOp::Zero => GpuStencilOperation::Zero,
                    gpu::StencilOp::Replace => GpuStencilOperation::Replace,
                    gpu::StencilOp::IncreaseClamp => GpuStencilOperation::IncrementClamp,
                    gpu::StencilOp::DecreaseClamp => GpuStencilOperation::DecrementClamp,
                    gpu::StencilOp::Invert => GpuStencilOperation::Invert,
                    gpu::StencilOp::Increase => GpuStencilOperation::IncrementWrap,
                    gpu::StencilOp::Decrease => GpuStencilOperation::DecrementWrap,
                }
            }
            fn stencil_to_webgpu(stencil: &gpu::StencilInfo) -> GpuStencilFaceState {
                let stencil_state = GpuStencilFaceState::new();
                stencil_state.set_compare(compare_func_to_webgpu(stencil.func));
                stencil_state.set_depth_fail_op(stencil_op_to_webgpu(stencil.depth_fail_op));
                stencil_state.set_fail_op(stencil_op_to_webgpu(stencil.fail_op));
                stencil_state.set_pass_op(stencil_op_to_webgpu(stencil.pass_op));
                stencil_state
            }
            depth_stencil.set_stencil_front(&stencil_to_webgpu(&info.depth_stencil.stencil_front));
            depth_stencil.set_stencil_back(&stencil_to_webgpu(&info.depth_stencil.stencil_back));
            descriptor.set_depth_stencil(&depth_stencil);
        }

        let any_blending_enabled = info.blend.attachments.iter().any(|a| a.blend_enabled);

        if let Some(fs) = info.fs.as_ref() {
            let targets = Array::new_with_length(info.render_target_formats.len() as u32);
            for i in 0..info.render_target_formats.len() {
                let format = info.render_target_formats[i];
                let blend_attachment = &info.blend.attachments[i];
                let target_state = GpuColorTargetState::new(format_to_webgpu(format));
                target_state.set_write_mask(blend_attachment.write_mask.bits() as u32);
                if any_blending_enabled {
                    let blend_state = GpuBlendState::new(&blend_attachment_to_webgpu(blend_attachment, false), &blend_attachment_to_webgpu(blend_attachment, true));
                    target_state.set_blend(&blend_state);
                }
                targets.set(i as u32, JsValue::from(&target_state));
            }
            let fragment_state = GpuFragmentState::new(fs.module(), &targets);
            descriptor.set_fragment(&fragment_state);
        }

        let multisample_state = GpuMultisampleState::new();
        multisample_state.set_alpha_to_coverage_enabled(info.blend.alpha_to_coverage_enabled);
        multisample_state.set_count(match info.rasterizer.sample_count {
            gpu::SampleCount::Samples1 => 1,
            gpu::SampleCount::Samples2 => 2,
            gpu::SampleCount::Samples4 => 4,
            gpu::SampleCount::Samples8 => 8,
        });
        descriptor.set_multisample(&multisample_state);

        if let Some(name) = name {
            descriptor.set_label(name);
        }

        let pipeline = device.create_render_pipeline(&descriptor).map_err(|_| ())?;

        Ok(Self {
            pipeline
        })
    }

    pub fn handle(&self) -> &GpuRenderPipeline {
        &self.pipeline
    }
}

pub struct WebGPUComputePipeline {
    pipeline: GpuComputePipeline,
    resources: [Box<[gpu::Resource]>; 4]
}

unsafe impl Send for WebGPUComputePipeline {}
unsafe impl Sync for WebGPUComputePipeline {}

impl WebGPUComputePipeline {
    pub fn new(device: &GpuDevice, shader: &WebGPUShader, name: Option<&str>) -> Result<Self, ()> {
        let stage = GpuProgrammableStage::new(shader.module());
        let descriptor = GpuComputePipelineDescriptor::new(&JsValue::from_str("auto"), &stage);
        if let Some(name) = name {
            descriptor.set_label(name);
        }
        let pipeline = device.create_compute_pipeline(&descriptor);

        Ok(Self {
            pipeline,
            resources: shader.resources().clone()
        })
    }

    pub fn handle(&self) -> &GpuComputePipeline {
        &self.pipeline
    }
}

impl gpu::ComputePipeline for WebGPUComputePipeline {
    fn binding_info(&self, set: gpu::BindingFrequency, slot: u32) -> Option<gpu::BindingInfo> {
        let bind_group = &self.resources[set as usize];
        for resource in bind_group {
            if resource.binding == slot {
                return Some(gpu::BindingInfo {
                    name: &resource.name,
                    binding_type: match resource.resource_type {
                        gpu::ResourceType::UniformBuffer => gpu::BindingType::ConstantBuffer,
                        gpu::ResourceType::StorageBuffer => gpu::BindingType::StorageBuffer,
                        gpu::ResourceType::SubpassInput => panic!("Deprecated resource type"),
                        gpu::ResourceType::SampledTexture => gpu::BindingType::SampledTexture,
                        gpu::ResourceType::StorageTexture => gpu::BindingType::StorageTexture,
                        gpu::ResourceType::Sampler => gpu::BindingType::Sampler,
                        gpu::ResourceType::CombinedTextureSampler => gpu::BindingType::TextureAndSampler,
                        gpu::ResourceType::AccelerationStructure => unimplemented!(),
                    }
                })
            }
        }
        None
    }
}
