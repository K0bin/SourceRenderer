use std::cell::Ref;
use std::sync::Arc;

use sourcerenderer_core::{
    Platform,
    Vec2,
    Vec2UI,
};

use crate::asset::AssetManager;
use crate::renderer::asset::{ComputePipelineHandle, RendererAssetsReadOnly};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::graphics::*;

pub(crate) fn scaled_halton_point(width: u32, height: u32, index: u32) -> Vec2 {
    let width_frac = 1.0f32 / (width as f32 * 0.5f32);
    let height_frac = 1.0f32 / (height as f32 * 0.5f32);
    let mut halton_point = halton_point(index);
    halton_point.x *= width_frac;
    halton_point.y *= height_frac;
    halton_point
}

pub(crate) fn halton_point(index: u32) -> Vec2 {
    Vec2::new(
        halton_sequence(index, 2) - 0.5f32,
        halton_sequence(index, 3) - 0.5f32,
    )
}

pub(crate) fn halton_sequence(mut index: u32, base: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;

    while index > 0 {
        f /= base as f32;
        r += f * (index as f32 % (base as f32));
        index = (index as f32 / (base as f32)).floor() as u32;
    }

    r
}

pub struct TAAPass {
    pipeline: ComputePipelineHandle,
}

impl TAAPass {
    pub const TAA_TEXTURE_NAME: &'static str = "TAAOuput";

    #[allow(unused)]
    pub fn new<P: Platform>(
        resolution: Vec2UI,
        resources: &mut RendererResources<P::GPUBackend>,
        asset_manager: &Arc<AssetManager<P>>,
        visibility_buffer: bool,
    ) -> Self {
        let pipeline = asset_manager.request_compute_pipeline(if !visibility_buffer {
            "shaders/taa.comp.json"
        } else {
            "shaders/taa_vis_buf.comp.json"
        });

        let texture_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::RGBA8UNorm,
            width: resolution.x,
            height: resolution.y,
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
            supports_srgb: false,
        };
        resources.create_texture(Self::TAA_TEXTURE_NAME, &texture_info, true);

        // TODO: Clear history texture

        Self { pipeline }
    }

    #[inline(always)]
    pub(super) fn is_ready<P: Platform>(&self, assets: &RendererAssetsReadOnly<'_, P>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    pub fn execute<P: Platform>(
        &mut self,
        cmd_buf: &mut CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>,
        input_name: &str,
        depth_name: &str,
        motion_name: Option<&str>,
        visibility_buffer: bool,
    ) {
        cmd_buf.begin_label("TAA pass");

        let output_srv = pass_params.resources.access_view(
            cmd_buf,
            input_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let taa_uav = pass_params.resources.access_view(
            cmd_buf,
            Self::TAA_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let taa_history_srv = pass_params.resources.access_view(
            cmd_buf,
            Self::TAA_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Past,
        );

        let mut motion_srv =
            Option::<Ref<Arc<TextureView<P::GPUBackend>>>>::None;
        let mut id_view =
            Option::<Ref<Arc<TextureView<P::GPUBackend>>>>::None;
        let mut barycentrics_view =
            Option::<Ref<Arc<TextureView<P::GPUBackend>>>>::None;
        if !visibility_buffer {
            motion_srv = Some(pass_params.resources.access_view(
                cmd_buf,
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
                cmd_buf,
                super::modern::VisibilityBufferPass::PRIMITIVE_ID_TEXTURE_NAME,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_READ,
                TextureLayout::Storage,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            ));
            barycentrics_view = Some(pass_params.resources.access_view(
                cmd_buf,
                super::modern::VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_READ,
                TextureLayout::Storage,
                false,
                &TextureViewInfo::default(),
                HistoryResourceEntry::Current,
            ));
        }

        let depth_srv = pass_params.resources.access_view(
            cmd_buf,
            depth_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let pipeline = pass_params.assets.get_compute_pipeline(self.pipeline).unwrap();
        cmd_buf.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buf.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            0,
            &*output_srv,
            pass_params.resources.linear_sampler(),
        );
        cmd_buf.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            1,
            &*taa_history_srv,
            pass_params.resources.linear_sampler(),
        );
        cmd_buf.bind_storage_texture(BindingFrequency::VeryFrequent, 2, &*taa_uav);
        cmd_buf.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            3,
            &*depth_srv,
            pass_params.resources.linear_sampler(),
        );
        if !visibility_buffer {
            cmd_buf.bind_sampling_view_and_sampler(
                BindingFrequency::VeryFrequent,
                4,
                &motion_srv.unwrap(),
                pass_params.resources.nearest_sampler(),
            );
        } else {
            cmd_buf.bind_storage_texture(BindingFrequency::VeryFrequent, 4, &id_view.unwrap());
            cmd_buf.bind_storage_texture(
                BindingFrequency::VeryFrequent,
                5,
                &barycentrics_view.unwrap(),
            );
        }
        cmd_buf.finish_binding();

        let info = taa_uav.texture().unwrap().info();
        cmd_buf.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 1);
        cmd_buf.end_label();
    }
}
