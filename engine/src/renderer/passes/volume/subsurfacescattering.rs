use bytemuck::{Pod, Zeroable};
use sourcerenderer_core::{Vec2, Vec2UI};
use std::sync::Arc;

use crate::graphics::*;
use crate::renderer::asset::*;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};

pub struct SSSPass {
    pipeline: ComputePipelineHandle,
    linear_sampler: Arc<Sampler>,
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

        let pipeline = assets.request_compute_pipeline(PathPipelineShaderStage::empty_spec_consts(
            "shaders/subsurface_scattering.comp.json",
        ));

        let sampler = device.create_sampler(&SamplerInfo {
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
        });

        Self {
            pipeline,
            linear_sampler: Arc::new(sampler),
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
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            0,
            &*color_view,
            &self.linear_sampler,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            4,
            &*sss_intensity_view,
            &self.linear_sampler,
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1, &*sss_temp_uav);
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            2,
            &*depth_srv,
            &self.linear_sampler,
        );
        cmd_buffer.bind_uniform_buffer(
            BindingFrequency::VeryFrequent,
            3,
            BufferRef::Transient(camera),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.finish_binding();

        cmd_buffer.begin_label("SSS horizonal pass");

        let sss_temp_info = sss_temp_uav.texture().unwrap().info();

        let params = SSSParams {
            dir: Vec2::new(1.0f32, 0.0f32),
            sss_width: sss_width,
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

        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            0,
            &*sss_temp_srv,
            &self.linear_sampler,
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
