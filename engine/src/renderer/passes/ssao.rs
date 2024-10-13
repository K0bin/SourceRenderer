use std::cell::Ref;
use std::sync::Arc;

use rand::random;
use crate::graphics::*;
use sourcerenderer_core::{
    Platform,
    Vec2UI,
    Vec4,
};

use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::{
    ComputePipelineHandle,
    ShaderManager,
};

pub struct SsaoPass<P: Platform> {
    pipeline: ComputePipelineHandle,
    kernel: Arc<BufferSlice<P::GPUBackend>>,
    blur_pipeline: ComputePipelineHandle,
}

fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + f * (b - a)
}

impl<P: Platform> SsaoPass<P> {
    const SSAO_INTERNAL_TEXTURE_NAME: &'static str = "SSAO";
    pub const SSAO_TEXTURE_NAME: &'static str = "SSAOBlurred";

    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
        resolution: Vec2UI,
        resources: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
        visibility_buffer: bool,
    ) -> Self {
        resources.create_texture(
            Self::SSAO_INTERNAL_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::R16Float,
                width: resolution.x / 2,
                height: resolution.y / 2,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            false,
        );

        resources.create_texture(
            Self::SSAO_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::R16Float,
                width: resolution.x / 2,
                height: resolution.y / 2,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                supports_srgb: false,
            },
            true,
        );

        let pipeline = shader_manager.request_compute_pipeline("shaders/ssao.comp.json");

        // TODO: Clear history texture

        let kernel = Self::create_hemisphere(device, 64);

        let blur_pipeline = shader_manager.request_compute_pipeline(if !visibility_buffer {
            "shaders/ssao_blur.comp.json"
        } else {
            "shaders/ssao_blur_vis_buf.comp.json"
        });

        Self {
            pipeline,
            kernel,
            blur_pipeline,
        }
    }

    fn create_hemisphere(
        device: &Arc<Device<P::GPUBackend>>,
        samples: u32,
    ) -> Arc<BufferSlice<P::GPUBackend>> {
        let mut ssao_kernel = Vec::<Vec4>::with_capacity(samples as usize);
        const BIAS: f32 = 0.15f32;
        for i in 0..samples {
            let mut sample = Vec4::new(
                (random::<f32>() - BIAS) * 2.0f32 - (1.0f32 - BIAS),
                (random::<f32>() - BIAS) * 2.0f32 - (1.0f32 - BIAS),
                random::<f32>(),
                0.0f32,
            );
            sample = sample.normalize();
            sample *= random::<f32>();
            let mut scale = (i as f32) / (samples as f32);
            scale = lerp(0.1f32, 1.0f32, scale * scale);
            sample *= scale;
            ssao_kernel.push(sample);
        }

        let buffer = device.create_buffer(
            &BufferInfo {
                size: std::mem::size_of_val(&ssao_kernel[..]) as u64,
                usage: BufferUsage::INITIAL_COPY | BufferUsage::CONSTANT,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            Some("SSAOKernel"),
        ).unwrap();

        device.init_buffer(&ssao_kernel[..], &buffer, 0).unwrap();
        buffer
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>,
        depth_name: &str,
        motion_name: Option<&str>,
        camera: &Arc<BufferSlice<P::GPUBackend>>,
        blue_noise_view: &Arc<TextureView<P::GPUBackend>>,
        blue_noise_sampler: &Arc<Sampler<P::GPUBackend>>,
        visibility_buffer: bool,
    ) {
        let ssao_uav = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_INTERNAL_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
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

        let mut motion_srv =
            Option::<Ref<Arc<TextureView<P::GPUBackend>>>>::None;
        let mut id_view =
            Option::<Ref<Arc<TextureView<P::GPUBackend>>>>::None;
        let mut barycentrics_view =
            Option::<Ref<Arc<TextureView<P::GPUBackend>>>>::None;
        if !visibility_buffer {
            motion_srv = Some(pass_params.resources.access_view(
                cmd_buffer,
                motion_name.unwrap(),
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::SAMPLING_READ,
                TextureLayout::Sampled,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            ));
        } else {
            id_view = Some(pass_params.resources.access_view(
                cmd_buffer,
                super::modern::VisibilityBufferPass::PRIMITIVE_ID_TEXTURE_NAME,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_READ,
                TextureLayout::Storage,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            ));
            barycentrics_view = Some(pass_params.resources.access_view(
                cmd_buffer,
                super::modern::VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_READ,
                TextureLayout::Storage,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            ));
        }

        cmd_buffer.begin_label("SSAO pass");
        let pipeline = pass_params.shader_manager.get_compute_pipeline(self.pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.flush_barriers();
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            0,
            BufferRef::Regular(&self.kernel),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1,
            blue_noise_view,
            blue_noise_sampler,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            2,
            &*depth_srv,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.bind_uniform_buffer(BindingFrequency::VeryFrequent, 3, BufferRef::Regular(camera), 0, WHOLE_BUFFER);
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 4, &*ssao_uav);
        cmd_buffer.finish_binding();
        let ssao_info = ssao_uav.texture().unwrap().info();
        cmd_buffer.dispatch(
            (ssao_info.width + 7) / 8,
            (ssao_info.height + 7) / 8,
            ssao_info.depth,
        );

        std::mem::drop(ssao_uav);
        let ssao_srv = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_INTERNAL_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let blurred_uav = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let blurred_srv_b = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Past,
        );

        let blur_pipeline = pass_params.shader_manager.get_compute_pipeline(self.blur_pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&blur_pipeline));
        cmd_buffer.flush_barriers();
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, &*blurred_uav);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1,
            &*ssao_srv,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            2,
            &*blurred_srv_b,
            pass_params.resources.linear_sampler(),
        );
        if !visibility_buffer {
            cmd_buffer.bind_sampling_view_and_sampler(
                BindingFrequency::VeryFrequent,
                3,
                &motion_srv.unwrap(),
                pass_params.resources.nearest_sampler(),
            );
        } else {
            cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 3, &id_view.unwrap());
            cmd_buffer.bind_storage_texture(
                BindingFrequency::VeryFrequent,
                4,
                &barycentrics_view.unwrap(),
            );
        }
        cmd_buffer.finish_binding();
        let blur_info = blurred_uav.texture().unwrap().info();
        cmd_buffer.dispatch(
            (blur_info.width + 7) / 8,
            (blur_info.height + 7) / 8,
            blur_info.depth,
        );
        cmd_buffer.end_label();
    }
}
