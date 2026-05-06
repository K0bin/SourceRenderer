use crate::asset::{AssetHandle, AssetLoadPriority, AssetType, TextureHandle};
use crate::graphics::{CommandBuffer, PipelineBinding, RenderPassBeginInfo, RenderTarget, StoreOp};
use crate::renderer::asset::{ComputePipelineHandle, RendererAssets, RendererAssetsReadOnly};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use sourcerenderer_core::gpu::{
    BarrierAccess, BarrierSync, BindingFrequency, ClearColor, Format, LoadOpColor, SampleCount,
    TextureDimension, TextureInfo, TextureLayout, TextureUsage, TextureViewInfo,
};
use std::sync::Arc;

pub struct ImageBasedLightingPreparation {
    handle: TextureHandle,
    project_to_cube_pipeline: ComputePipelineHandle,
    prefilter_pipeline: ComputePipelineHandle,
    prepared: bool,
}

impl ImageBasedLightingPreparation {
    pub const ENVIRONMENT_MAP_TEXTURE_NAME: &'static str = "EnvironmentMap";
    pub(crate) fn new(
        device: &Arc<crate::graphics::Device>,
        assets: &RendererAssets,
        _init_cmd_buffer: &mut crate::graphics::CommandBuffer,
        resources: &mut RendererResources,
    ) -> Self {
        let (ibl_map_handle, _) = assets.asset_manager().request_asset(
            "assets/little_paris_eiffel_tower_4k.hdr",
            AssetType::Texture,
            AssetLoadPriority::Normal,
        );

        let project_to_cube_pipeline =
            assets.request_compute_pipeline("shaders/project_equirectangular.comp.json");
        let prefilter_pipeline =
            assets.request_compute_pipeline("shaders/prefilter_env_map.comp.json");

        Self {
            handle: TextureHandle::from(ibl_map_handle),
            project_to_cube_pipeline,
            prefilter_pipeline,
            prepared: false,
        }
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
    ) {
        let texture = pass_params.assets.get_texture_opt(self.handle);
        if texture.is_none() || self.prepared {
            return;
        }
        let texture = texture.unwrap();
        let info = texture.view.texture().unwrap().info();
        let size = info.width.min(info.height);
        let mips = 32u32 - u32::leading_zeros(size);

        pass_params.resources.create_texture(
            Self::ENVIRONMENT_MAP_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Cube,
                format: Format::RGBA16Float,
                width: size,
                height: size,
                depth: 1u32,
                mip_levels: mips,
                array_length: 1u32,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                supports_srgb: false,
            },
            false,
        );

        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.project_to_cube_pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(pipeline));

        let cube = pass_params.resources.access_view(
            cmd_buffer,
            Self::ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 6u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            0u32,
            &texture.view,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1u32, &cube);

        for mip in 0..1 {
            cmd_buffer.finish_binding();
            cmd_buffer.dispatch((size + 7) / 8, (size + 7) / 8, 6);

            /*cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
                render_targets: &[RenderTarget {
                    view: &rt,
                    load_op: LoadOpColor::Clear(ClearColor::from_u32([0, 0, 0, 0])),
                    store_op: StoreOp::Store,
                }],
                depth_stencil: None,
                query_range: None,
            });

            cmd_buffer.bind_sampling_view_and_sampler(
                BindingFrequency::VeryFrequent,
                0,
                &texture.view,
                resources.linear_sampler(),
            );
            cmd_buffer.finish_binding();
            cmd_buffer.draw(3, 1, 0, 0);

            cmd_buffer.end_render_pass();*/
        }
        //self.prepared = true;
    }
}
