use std::sync::{Arc, Mutex, RwLock};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::NSString;
use objc2_metal::{MTLDevice as _, MTLLibrary as _};
use sourcerenderer_core::gpu;

use crate::{MTLBindlessArgumentBuffer, MTLDevice, MTLGraphicsPipeline, MTLShader};

pub(crate) struct MTLShared {
    pub(crate) device: Retained<ProtocolObject<dyn objc2_metal::MTLDevice>>,
    pub(crate) blit_pipeline: MTLGraphicsPipeline,
    pub(crate) mdi_pipeline: Retained<ProtocolObject<dyn objc2_metal::MTLComputePipelineState>>,
    pub(crate) linear_sampler: Retained<ProtocolObject<dyn objc2_metal::MTLSamplerState>>,
    pub(crate) bindless: MTLBindlessArgumentBuffer,
    pub(crate) acceleration_structure_list:
        Arc<Mutex<Vec<Retained<ProtocolObject<dyn objc2_metal::MTLAccelerationStructure>>>>>,
    pub(crate) heap_list: Arc<RwLock<Vec<Retained<ProtocolObject<dyn objc2_metal::MTLHeap>>>>>,
}

impl MTLShared {
    pub(crate) unsafe fn new(
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        bindless: MTLBindlessArgumentBuffer,
    ) -> Self {
        let fullscreen_vs_shader_bytes =
            include_bytes!("../meta_shaders/fullscreen_quad.vert.json");
        let fullscreen_vs_packed: gpu::PackedShader =
            serde_json::from_slice(fullscreen_vs_shader_bytes).unwrap();
        let blit_shader_bytes = include_bytes!("../meta_shaders/blit.frag.json");
        let blit_shader_packed: gpu::PackedShader =
            serde_json::from_slice(blit_shader_bytes).unwrap();
        let fullscreen_vs = MTLShader::new(device, &fullscreen_vs_packed, Some("Fullscreen"));
        let blit_fs = MTLShader::new(device, &blit_shader_packed, Some("Blit"));
        let blit_pipeline = MTLGraphicsPipeline::new(
            device,
            &gpu::GraphicsPipelineInfo {
                vs: &fullscreen_vs,
                fs: Some(&blit_fs),
                vertex_layout: gpu::VertexLayoutInfo {
                    shader_inputs: &[],
                    input_assembler: &[],
                },
                rasterizer: gpu::RasterizerInfo::default(),
                depth_stencil: gpu::DepthStencilInfo {
                    depth_test_enabled: false,
                    depth_write_enabled: false,
                    ..Default::default()
                },
                blend: gpu::BlendInfo {
                    alpha_to_coverage_enabled: false,
                    logic_op_enabled: false,
                    logic_op: gpu::LogicOp::Noop,
                    attachments: &[gpu::AttachmentBlendInfo::default()],
                    constants: [1f32, 1f32, 1f32, 1f32],
                },
                primitive_type: gpu::PrimitiveType::Triangles,
                render_target_formats: &[gpu::Format::BGRA8UNorm],
                depth_stencil_format: gpu::Format::Unknown,
            },
            Some("Blit Pipeline"),
        );

        let mdi_shader_bytes = include_bytes!("../meta_shaders/mdi.metallib");

        let mdi_lib_res = MTLDevice::metal_library_from_data(device, mdi_shader_bytes);
        let mdi_lib = mdi_lib_res.unwrap();

        let mdi_function = mdi_lib
            .newFunctionWithName(&NSString::from_str("writeMDICommands"))
            .unwrap();
        let mdi_pipeline = device
            .newComputePipelineStateWithFunction_error(&mdi_function)
            .unwrap();

        let sampler_descriptor = objc2_metal::MTLSamplerDescriptor::new();
        sampler_descriptor.setSAddressMode(objc2_metal::MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.setTAddressMode(objc2_metal::MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.setRAddressMode(objc2_metal::MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.setMagFilter(objc2_metal::MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.setMinFilter(objc2_metal::MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.setMipFilter(objc2_metal::MTLSamplerMipFilter::Linear);
        let linear_sampler = device
            .newSamplerStateWithDescriptor(&sampler_descriptor)
            .unwrap();

        Self {
            device: Retained::from(device),
            blit_pipeline,
            mdi_pipeline,
            linear_sampler,
            bindless,
            acceleration_structure_list: Arc::new(Mutex::new(Vec::new())),
            heap_list: Arc::new(RwLock::new(Vec::new())),
        }
    }
}
