use crate::graphics::*;
use crate::renderer::asset::*;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use bytemuck::{Pod, Zeroable};
use sourcerenderer_core::gpu::SpecConstValue;
use sourcerenderer_core::{Vec2, Vec2UI, Vec4};
use std::collections::HashMap;
use std::sync::Arc;

pub struct SSSPass {
    pipeline: ComputePipelineHandle,
    linear_sampler: Arc<Sampler>,
    kernel_buffer: Arc<BufferSlice>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Zeroable, Pod)]
struct SSSParams {
    dir: Vec2,
    sss_width: f32,
}

impl SSSPass {
    const SSS_INTERNAL_TEMP_TEXTURE_NAME: &'static str = "SSS";

    #[allow(unused)]
    pub fn new(
        device: &Arc<Device>,
        resources: &mut RendererResources,
        resolution: Vec2UI,
        assets: &RendererAssets,
    ) -> Self {
        Self::create_textures(resources, resolution);

        let sampler = device.create_sampler(
            &SamplerInfo {
                mag_filter: Filter::Linear,
                min_filter: Filter::Linear,
                mip_filter: Filter::Linear,
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                address_mode_w: AddressMode::ClampToEdge,
                mip_bias: 0.0f32,
                max_anisotropy: 1f32,
                compare_op: None,
                min_lod: 0.0f32,
                max_lod: None,
            },
            None,
        );

        const KERNEL_25: [Vec4; 25] = [
            Vec4::new(0.530605, 0.613514, 0.739601, 0.0),
            Vec4::new(0.000973794, 1.11862e-005, 9.43437e-007, -3.0),
            Vec4::new(0.00333804, 7.85443e-005, 1.2945e-005, -2.52083),
            Vec4::new(0.00500364, 0.00020094, 5.28848e-005, -2.08333),
            Vec4::new(0.00700976, 0.00049366, 0.000151938, -1.6875),
            Vec4::new(0.0094389, 0.00139119, 0.000416598, -1.33333),
            Vec4::new(0.0128496, 0.00356329, 0.00132016, -1.02083),
            Vec4::new(0.017924, 0.00711691, 0.00347194, -0.75),
            Vec4::new(0.0263642, 0.0119715, 0.00684598, -0.520833),
            Vec4::new(0.0410172, 0.0199899, 0.0118481, -0.333333),
            Vec4::new(0.0493588, 0.0367726, 0.0219485, -0.1875),
            Vec4::new(0.0402784, 0.0657244, 0.04631, -0.0833333),
            Vec4::new(0.0211412, 0.0459286, 0.0378196, -0.0208333),
            Vec4::new(0.0211412, 0.0459286, 0.0378196, 0.0208333),
            Vec4::new(0.0402784, 0.0657244, 0.04631, 0.0833333),
            Vec4::new(0.0493588, 0.0367726, 0.0219485, 0.1875),
            Vec4::new(0.0410172, 0.0199899, 0.0118481, 0.333333),
            Vec4::new(0.0263642, 0.0119715, 0.00684598, 0.520833),
            Vec4::new(0.017924, 0.00711691, 0.00347194, 0.75),
            Vec4::new(0.0128496, 0.00356329, 0.00132016, 1.02083),
            Vec4::new(0.0094389, 0.00139119, 0.000416598, 1.33333),
            Vec4::new(0.00700976, 0.00049366, 0.000151938, 1.6875),
            Vec4::new(0.00500364, 0.00020094, 5.28848e-005, 2.08333),
            Vec4::new(0.00333804, 7.85443e-005, 1.2945e-005, 2.52083),
            Vec4::new(0.000973794, 1.11862e-005, 9.43437e-007, 3.0),
        ];

        const KERNEL_17: [Vec4; 17] = [
            Vec4::new(0.536343, 0.624624, 0.748867, 0.0),
            Vec4::new(0.00317394, 0.000134823, 3.77269e-005, -2.0),
            Vec4::new(0.0100386, 0.000914679, 0.000275702, -1.53125),
            Vec4::new(0.0144609, 0.00317269, 0.00106399, -1.125),
            Vec4::new(0.0216301, 0.00794618, 0.00376991, -0.78125),
            Vec4::new(0.0347317, 0.0151085, 0.00871983, -0.5),
            Vec4::new(0.0571056, 0.0287432, 0.0172844, -0.28125),
            Vec4::new(0.0582416, 0.0659959, 0.0411329, -0.125),
            Vec4::new(0.0324462, 0.0656718, 0.0532821, -0.03125),
            Vec4::new(0.0324462, 0.0656718, 0.0532821, 0.03125),
            Vec4::new(0.0582416, 0.0659959, 0.0411329, 0.125),
            Vec4::new(0.0571056, 0.0287432, 0.0172844, 0.28125),
            Vec4::new(0.0347317, 0.0151085, 0.00871983, 0.5),
            Vec4::new(0.0216301, 0.00794618, 0.00376991, 0.78125),
            Vec4::new(0.0144609, 0.00317269, 0.00106399, 1.125),
            Vec4::new(0.0100386, 0.000914679, 0.000275702, 1.53125),
            Vec4::new(0.00317394, 0.000134823, 3.77269e-005, 2.0),
        ];

        const KERNEL_11: [Vec4; 11] = [
            Vec4::new(0.560479, 0.669086, 0.784728, 0.0),
            Vec4::new(0.00471691, 0.000184771, 5.07566e-005, -2.0),
            Vec4::new(0.0192831, 0.00282018, 0.00084214, -1.28),
            Vec4::new(0.03639, 0.0130999, 0.00643685, -0.72),
            Vec4::new(0.0821904, 0.0358608, 0.0209261, -0.32),
            Vec4::new(0.0771802, 0.113491, 0.0793803, -0.08),
            Vec4::new(0.0771802, 0.113491, 0.0793803, 0.08),
            Vec4::new(0.0821904, 0.0358608, 0.0209261, 0.32),
            Vec4::new(0.03639, 0.0130999, 0.00643685, 0.72),
            Vec4::new(0.0192831, 0.00282018, 0.00084214, 1.28),
            Vec4::new(0.00471691, 0.000184771, 5.07565e-005, 2.0),
        ];

        let picked_kernel = &KERNEL_17;

        let shader_path = crate::renderer::shader_path!("subsurface_scattering.comp");
        let mut spec_consts = HashMap::<u32, SpecConstValue>::new();
        spec_consts.insert(0, SpecConstValue::UInt(picked_kernel.len() as u32));
        let pipeline = assets.request_compute_pipeline(PathPipelineShaderStage {
            shader_path: &shader_path,
            spec_consts: Some(&spec_consts),
        });

        let kernel_buffer = device
            .create_buffer(
                &BufferInfo {
                    size: std::mem::size_of_val(picked_kernel) as u64,
                    usage: BufferUsage::STORAGE | BufferUsage::CONSTANT | BufferUsage::INITIAL_COPY,
                    sharing_mode: QueueSharingMode::Exclusive,
                },
                MemoryUsage::GPUMemory,
                Some("SSSKernel"),
            )
            .unwrap();

        device
            .init_buffer(picked_kernel, &kernel_buffer, 0u64)
            .unwrap();

        Self {
            pipeline,
            linear_sampler: Arc::new(sampler),
            kernel_buffer,
        }
    }

    pub fn create_textures(resources: &mut RendererResources, resolution: Vec2UI) {
        resources.create_texture(
            Self::SSS_INTERNAL_TEMP_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA16UNorm,
                width: resolution.x,
                height: resolution.y,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            false,
        );
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters<'_>,
        color_name: &str,
        sss_intensity_name: &str,
        depth_name: &str,
        camera: &TransientBufferSlice,
        sss_width: f32,
    ) {
        cmd_buffer.clear_all_bindings(BindingFrequency::Frequent);
        cmd_buffer.clear_all_bindings(BindingFrequency::VeryFrequent);

        // Horizonal pass

        let sss_temp_uav = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSS_INTERNAL_TEMP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let color_view = pass_params.resources.access_view(
            cmd_buffer,
            color_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let sss_intensity_view = pass_params.resources.access_view(
            cmd_buffer,
            sss_intensity_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let depth_srv = pass_params.resources.access_view(
            cmd_buffer,
            depth_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.flush_barriers();
        cmd_buffer.bind_sampling_view(
            BindingFrequency::VeryFrequent,
            0,
            &*color_view,
        );
        cmd_buffer.bind_sampling_view(
            BindingFrequency::VeryFrequent,
            4,
            &*sss_intensity_view,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &*sss_temp_uav);
        cmd_buffer.bind_sampling_view(
            BindingFrequency::VeryFrequent,
            2,
            &*depth_srv,
        );
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            3,
            BufferRef::Regular(&self.kernel_buffer),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.finish_binding();

        cmd_buffer.begin_label("SSS horizonal pass");

        let sss_temp_info = sss_temp_uav.texture().unwrap().info();

        let params = SSSParams {
            dir: Vec2::new(1.0f32, 0.0f32),
            sss_width,
        };
        cmd_buffer.set_push_constant_data(&[params], ShaderType::ComputeShader);

        cmd_buffer.dispatch(
            (sss_temp_info.width + 7) / 8,
            (sss_temp_info.height + 7) / 8,
            sss_temp_info.depth,
        );
        std::mem::drop(sss_temp_uav);
        std::mem::drop(color_view);

        cmd_buffer.end_label();

        // Vertical pass

        cmd_buffer.begin_label("SSS vertical pass");

        let sss_uav = pass_params.resources.access_view(
            cmd_buffer,
            color_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let sss_temp_srv = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSS_INTERNAL_TEMP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        cmd_buffer.bind_sampling_view(
            BindingFrequency::VeryFrequent,
            0,
            &*sss_temp_srv,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &*sss_uav);
        cmd_buffer.finish_binding();
        let sss_info = sss_uav.texture().unwrap().info();

        let params = SSSParams {
            dir: Vec2::new(0.0f32, 1.0f32),
            sss_width,
        };
        cmd_buffer.set_push_constant_data(&[params], ShaderType::ComputeShader);

        cmd_buffer.dispatch(
            (sss_info.width + 7) / 8,
            (sss_info.height + 7) / 8,
            sss_info.depth,
        );

        std::mem::drop(sss_uav);

        cmd_buffer.end_label();
    }
}
