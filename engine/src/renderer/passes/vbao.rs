use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use crate::graphics::*;
use crate::renderer::asset::*;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use sourcerenderer_core::Vec2UI;

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
struct PushConsts {
    sample_count: u32,
    sample_radius: f32,
    slice_count: u32,
    hit_thickness: f32,
}

pub struct VbaoPass {
    pipeline: ComputePipelineHandle,
}

impl VbaoPass {
    pub const SSAO_TEXTURE_NAME: &'static str = "VBAO";

    #[allow(unused)]
    pub fn new(
        device: &Arc<Device>,
        resources: &mut RendererResources,
        assets: &RendererAssets,
        resolution: Vec2UI,
        visibility_buffer: bool,
    ) -> Self {
        resources.create_texture(
            Self::SSAO_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA16Float,
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

        let shader_path = crate::renderer::shader_path!("vbao.comp");
        let pipeline = assets.request_compute_pipeline(PathPipelineShaderStage::empty_spec_consts(&shader_path));

        // TODO: Clear history texture

        Self {
            pipeline,
        }
    }

    #[inline(always)]
    pub(in crate::renderer::passes) fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        assets.get_compute_pipeline(self.pipeline).is_some()
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters<'_>,
        color_name: &str,
        depth_name: &str,
    ) {
        cmd_buffer.clear_all_bindings(BindingFrequency::Frequent);
        cmd_buffer.clear_all_bindings(BindingFrequency::VeryFrequent);

        let color_srv = pass_params.resources.access_view(
            cmd_buffer,
            color_name,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo::default(),
            HistoryResourceEntry::Current,
        );

        let ssao_uav = pass_params.resources.access_view(
            cmd_buffer,
            Self::SSAO_TEXTURE_NAME,
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

        cmd_buffer.begin_label("SSAO pass (VBAO)");
        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
        cmd_buffer.flush_barriers();
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0, ssao_uav.view());
        cmd_buffer.bind_sampling_view(
            BindingFrequency::VeryFrequent,
            1,
            depth_srv.view(),
        );
        cmd_buffer.bind_sampling_view(
            BindingFrequency::VeryFrequent,
            2,
            color_srv.view(),
        );
        cmd_buffer.set_push_constant_data(&[PushConsts {
            sample_count: 4,
            sample_radius: 3.0,
            slice_count: 4,
            hit_thickness: 0.2,
        }]);
        cmd_buffer.finish_binding();
        let ssao_info = ssao_uav.texture().info();
        cmd_buffer.dispatch(
            (ssao_info.width + 7) / 8,
            (ssao_info.height + 7) / 8,
            ssao_info.depth,
        );

        std::mem::drop(ssao_uav);

        cmd_buffer.end_label();
    }
}
