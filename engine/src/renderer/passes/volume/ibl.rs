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
    pub const FILTERED_ENVIRONMENT_MAP_TEXTURE_NAME: &'static str = "FilteredEnvironmentMap";
    pub(crate) fn new(
        _device: &Arc<crate::graphics::Device>,
        assets: &RendererAssets,
        _init_cmd_buffer: &mut crate::graphics::CommandBuffer,
        _resources: &mut RendererResources,
    ) -> Self {
        let (ibl_map_handle, _) = assets.asset_manager().request_asset(
            //"assets/little_paris_eiffel_tower_4k.hdr",
            "assets/BlaubeurenNight1k.hdr",
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

    fn deproject_env_map(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
    ) {
        let texture = pass_params.assets.get_texture_opt(self.handle);
        let texture = texture.unwrap();
        let info = texture.view.texture().unwrap().info();
        let size = info.width.min(info.height);

        pass_params.resources.create_texture(
            Self::ENVIRONMENT_MAP_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Cube,
                format: Format::RGBA16Float,
                width: size,
                height: size,
                depth: 1u32,
                mip_levels: 1u32,
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
                array_layer_length: 1u32,
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
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((size + 7) / 8, (size + 7) / 8, 6);
    }

    fn filter_env_map(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
    ) {
        let mut info = pass_params
            .resources
            .texture_info(Self::ENVIRONMENT_MAP_TEXTURE_NAME)
            .clone();
        info.mip_levels = 32u32 - u32::leading_zeros(info.width);

        pass_params.resources.create_texture(
            Self::FILTERED_ENVIRONMENT_MAP_TEXTURE_NAME,
            &info,
            false,
        );

        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.prefilter_pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(pipeline));

        let cube = pass_params.resources.access_view(
            cmd_buffer,
            Self::ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );
        let filtered_cube = pass_params.resources.access_view(
            cmd_buffer,
            Self::FILTERED_ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            false,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
            },
            HistoryResourceEntry::Current,
        );
        cmd_buffer.bind_sampling_view_and_sampler(
            BindingFrequency::VeryFrequent,
            0u32,
            &cube,
            pass_params.resources.linear_sampler(),
        );
        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1u32, &filtered_cube);
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((info.width + 7) / 8, (info.height + 7) / 8, 6);
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
        self.deproject_env_map(cmd_buffer, pass_params);
        self.filter_env_map(cmd_buffer, pass_params);
        //self.prepared = true;
    }
}
