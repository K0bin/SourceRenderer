use std::sync::{Arc, Mutex, RwLock};

use sourcerenderer_core::gpu::{self, Format};

use crate::{MTLBindlessArgumentBuffer, MTLGraphicsPipeline, MTLShader};

pub(crate) struct MTLShared {
    pub(crate) device: metal::Device,
    pub(crate) blit_pipeline: MTLGraphicsPipeline,
    pub(crate) mdi_pipeline: metal::ComputePipelineState,
    pub(crate) linear_sampler: metal::SamplerState,
    pub(crate) bindless: MTLBindlessArgumentBuffer,
    pub(crate) acceleration_structure_list: Arc<Mutex<Vec<metal::AccelerationStructure>>>,
    pub(crate) heap_list: Arc<RwLock<Vec<metal::Heap>>>
}

impl MTLShared {
    pub(crate) fn new(device: &metal::DeviceRef, bindless: MTLBindlessArgumentBuffer) -> Self {
        let fullscreen_vs_shader_bytes = include_bytes!("../meta_shaders/fullscreen_quad.vert.json");
        let fullscreen_vs_packed: gpu::PackedShader = serde_json::from_slice(fullscreen_vs_shader_bytes).unwrap();
        let blit_shader_bytes = include_bytes!("../meta_shaders/blit.frag.json");
        let blit_shader_packed: gpu::PackedShader = serde_json::from_slice(blit_shader_bytes).unwrap();
        let fullscreen_vs = MTLShader::new(
            device,
            fullscreen_vs_packed,
            Some("Fullscreen"),
        );
        let blit_fs = MTLShader::new(
            device,
            blit_shader_packed,
            Some("Blit")
        );
        let blit_pipeline = MTLGraphicsPipeline::new(
            device,
            &gpu::GraphicsPipelineInfo {
                vs: &fullscreen_vs,
                fs: Some(&blit_fs),
                vertex_layout: gpu::VertexLayoutInfo {
                    shader_inputs: &[],
                    input_assembler: &[]
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
                depth_stencil_format: Format::Unknown
            }, Some("Blit Pipeline")
        );

        let mdi_shader_bytes = include_bytes!("../meta_shaders/mdi.metallib");
        let mdi_lib = device.new_library_with_data(mdi_shader_bytes).unwrap();
        let mdi_function = mdi_lib.get_function("writeMDICommands", None).unwrap();
        let mdi_pipeline = device.new_compute_pipeline_state_with_function(&mdi_function).unwrap();

        let sampler_descriptor = metal::SamplerDescriptor::new();
        sampler_descriptor.set_address_mode_r(metal::MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.set_address_mode_s(metal::MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.set_address_mode_t(metal::MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.set_mag_filter(metal::MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.set_min_filter(metal::MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.set_mip_filter(metal::MTLSamplerMipFilter::Linear);
        let linear_sampler = device.new_sampler(&sampler_descriptor);

        Self {
            device: device.to_owned(),
            blit_pipeline,
            mdi_pipeline,
            linear_sampler,
            bindless,
            acceleration_structure_list: Arc::new(Mutex::new(Vec::new())),
            heap_list: Arc::new(RwLock::new(Vec::new()))
        }
    }
}
