use std::cell::Ref;
use std::sync::Arc;

use rand::random;
use sourcerenderer_core::{Vec2UI, Vec4};

use crate::graphics::*;
use crate::renderer::asset::*;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};

pub struct SsaoPass {
    pipeline: ComputePipelineHandle,
    kernel: Arc<BufferSlice>,
    blur_pipeline: ComputePipelineHandle,
    noise_texture_view: Arc<TextureView>,
    noise_sampler: Arc<Sampler>,
}

fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + f * (b - a)
}

impl SsaoPass {
    const SSAO_INTERNAL_TEXTURE_NAME: &'static str = "SSAO";
    pub const SSAO_TEXTURE_NAME: &'static str = "SSAOBlurred";
    pub const SSAO_NOISE_TEXTURE_NAME: &'static str = "SSAONoise";

    #[allow(unused)]
    pub fn new(
        device: &Arc<Device>,
        resolution: Vec2UI,
        resources: &mut RendererResources,
        assets: &RendererAssets,
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

        let noise_texture_view = Self::create_noise_texture(device, 4u32);

        let noise_sampler = device.create_sampler(&SamplerInfo {
            min_filter: Filter::Nearest,
            mag_filter: Filter::Nearest,
            mip_filter: Filter::Nearest,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mip_bias: 0.0f32,
            max_anisotropy: 0.0f32,
            compare_op: None,
            min_lod: 0.0f32,
            max_lod: None,
        });

        let pipeline = assets.request_compute_pipeline("shaders/ssao.comp.json");

        // TODO: Clear history texture

        let kernel = Self::create_hemisphere(device, 64u32);

        let blur_pipeline = assets.request_compute_pipeline(if !visibility_buffer {
            "shaders/ssao_blur.comp.json"
        } else {
            "shaders/ssao_blur_vis_buf.comp.json"
        });

        Self {
            pipeline,
            kernel,
            blur_pipeline,
            noise_texture_view,
            noise_sampler: Arc::new(noise_sampler),
        }
    }

    fn create_hemisphere(device: &Arc<Device>, samples: u32) -> Arc<BufferSlice> {
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

        let buffer = device
            .create_buffer(
                &BufferInfo {
                    size: std::mem::size_of_val(&ssao_kernel[..]) as u64,
                    usage: BufferUsage::INITIAL_COPY | BufferUsage::CONSTANT,
                    sharing_mode: QueueSharingMode::Exclusive,
                },
                MemoryUsage::GPUMemory,
                Some("SSAOKernel"),
            )
            .unwrap();

        device.init_buffer(&ssao_kernel[..], &buffer, 0).unwrap();
        buffer
    }

    fn create_noise_texture(device: &Arc<Device>, size: u32) -> Arc<TextureView> {
        let mut noise = Vec::<Vec4>::with_capacity((size * size) as usize);

        for i in 0..size * size {
            noise.push(Vec4::new(
                random::<f32>() * 2.0f32 - 1.0f32,
                random::<f32>() * 2.0f32 - 1.0f32,
                random::<f32>() * 2.0f32 - 1.0f32,
                random::<f32>() * 2.0f32 - 1.0f32,
            ));
        }

        noise.shrink_to_fit();
        let boxed_data = noise.into_boxed_slice();

        let noise_texture = device
            .create_texture(
                &TextureInfo {
                    dimension: TextureDimension::Dim2D,
                    format: Format::RGBA16Float,
                    width: size,
                    height: size,
                    depth: 1,
                    mip_levels: 1,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::STORAGE
                        | TextureUsage::SAMPLED
                        | TextureUsage::INITIAL_COPY,
                    supports_srgb: false,
                },
                Some("SSAONoise"),
            )
            .unwrap();

        device
            .init_texture_box(boxed_data, &noise_texture, 0u32, 0u32)
            .unwrap();

        let noise_texture_view = device.create_texture_view(
            &noise_texture,
            &TextureViewInfo {
                base_mip_level: 0u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                mip_level_length: 1u32,
                format: None,
            },
            Some("SSAONoiseView"),
        );

        noise_texture_view
    }

    #[inline(always)]
    pub(super) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
            && assets.get_compute_pipeline(self.blur_pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters<'_>,
        depth_name: &str,
        camera: &TransientBufferSlice,
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

        cmd_buffer.begin_label("SSAO pass");
        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.pipeline)
            .unwrap();
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
            &self.noise_texture_view,
            &self.noise_sampler,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            2,
            &*depth_srv,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            3,
            BufferRef::Transient(camera),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 4, &*ssao_uav);
        cmd_buffer.finish_binding();
        let ssao_info = ssao_uav.texture().unwrap().info();
        cmd_buffer.dispatch(
            (ssao_info.width + 7) / 8,
            (ssao_info.height + 7) / 8,
            ssao_info.depth,
        );

        std::mem::drop(ssao_uav);

        cmd_buffer.end_label();

        // Blurring

        let ssao_sampling_view = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_INTERNAL_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let smoothed_ssao_view = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );
        let smoothed_ssao_history_view = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Past,
        );
        cmd_buffer.begin_label("SSAO smoothing pass");
        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.blur_pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.flush_barriers();
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0u32, &smoothed_ssao_view);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1u32,
            &ssao_sampling_view,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            2u32,
            &smoothed_ssao_history_view,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.finish_binding();
        let smoothed_info = smoothed_ssao_view.texture().unwrap().info();
        cmd_buffer.dispatch(
            (smoothed_info.width + 7u32) / 8u32,
            (smoothed_info.height + 7u32) / 8u32,
            smoothed_info.depth,
        );
        cmd_buffer.end_label();
    }
}
