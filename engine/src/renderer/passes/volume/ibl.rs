use crate::asset::{AssetHandle, AssetLoadPriority, AssetType, TextureHandle};
use crate::graphics::{CommandBuffer, PipelineBinding, RenderPassBeginInfo, RenderTarget, StoreOp};
use crate::renderer::asset::{ComputePipelineHandle, RendererAssets, RendererAssetsReadOnly};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use sourcerenderer_core::gpu::{
    BarrierAccess, BarrierSync, BarrierTextureRange, BindingFrequency, ClearColor, Format,
    LoadOpColor, SampleCount, ShaderType, TextureDimension, TextureInfo, TextureLayout,
    TexturePlane, TextureUsage, TextureViewInfo,
};
use std::sync::Arc;

pub struct ImageBasedLightingPreparation {
    handle: TextureHandle,
    project_to_cube_pipeline: ComputePipelineHandle,
    prefilter_diffuse_pipeline: ComputePipelineHandle,
    prefilter_specular_pipeline: ComputePipelineHandle,
    preintegrate_pipeline: ComputePipelineHandle,
    prepared: bool,
}

impl ImageBasedLightingPreparation {
    pub const ENVIRONMENT_MAP_TEXTURE_NAME: &'static str = "EnvironmentMap";
    pub const FILTERED_DIFFUSE_ENVIRONMENT_MAP_TEXTURE_NAME: &'static str =
        "FilteredDiffuseEnvironmentMap";
    pub const FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME: &'static str =
        "FilteredSpecularEnvironmentMap";
    pub const PREINTEGRATION_MAP_TEXTURE_NAME: &'static str = "PreintegrationMap";
    pub(crate) fn new(
        _device: &Arc<crate::graphics::Device>,
        assets: &RendererAssets,
        _init_cmd_buffer: &mut crate::graphics::CommandBuffer,
        resources: &mut RendererResources,
    ) -> Self {
        let (ibl_map_handle, _) = assets.asset_manager().request_asset(
            //"assets/little_paris_eiffel_tower_4k.hdr",
            "assets/BlaubeurenNight1k.hdr",
            AssetType::Texture,
            AssetLoadPriority::Normal,
        );

        resources.create_texture(
            Self::PREINTEGRATION_MAP_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RG16UNorm,
                width: 512,
                height: 512,
                depth: 1u32,
                mip_levels: 1u32,
                array_length: 1u32,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
                supports_srgb: false,
            },
            false,
        );

        let project_to_cube_pipeline =
            assets.request_compute_pipeline("shaders/project_equirectangular.comp.json");
        let prefilter_diffuse_pipeline =
            assets.request_compute_pipeline("shaders/prefilter_env_map_diffuse.comp.json");
        let prefilter_specular_pipeline =
            assets.request_compute_pipeline("shaders/prefilter_env_map_specular.comp.json");
        let preintegrate_pipeline =
            assets.request_compute_pipeline("shaders/preintegrate_brdf.comp.json");

        Self {
            handle: TextureHandle::from(ibl_map_handle),
            project_to_cube_pipeline,
            prefilter_diffuse_pipeline,
            prefilter_specular_pipeline,
            preintegrate_pipeline,
            prepared: false,
        }
    }

    fn calculate_preintegration_lut(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
    ) {
        cmd_buffer.begin_label("Calculate integration LUT");

        let lut = pass_params.resources.access_view(
            cmd_buffer,
            Self::PREINTEGRATION_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
                plane: TexturePlane::Primary,
            },
            HistoryResourceEntry::Current,
        );

        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.preintegrate_pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(pipeline));

        cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 0u32, &lut);
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((512 + 7) / 8, (512 + 7) / 8, 6);
        cmd_buffer.end_label();
    }

    pub fn is_ready(&self, assets: &RendererAssetsReadOnly) -> bool {
        assets
            .get_compute_pipeline(self.preintegrate_pipeline)
            .is_some()
            && assets
                .get_compute_pipeline(self.prefilter_diffuse_pipeline)
                .is_some()
            && assets
                .get_compute_pipeline(self.prefilter_specular_pipeline)
                .is_some()
            && assets
                .get_compute_pipeline(self.project_to_cube_pipeline)
                .is_some()
    }

    fn deproject_env_map(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
    ) {
        cmd_buffer.begin_label("Environment Map deprojecting");
        let texture = pass_params.assets.get_texture_opt(self.handle);
        let texture = texture.unwrap();
        let info = texture.view.texture().unwrap().info();
        let size = info.width.min(info.height);

        pass_params.resources.create_texture(
            Self::ENVIRONMENT_MAP_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Cube,
                format: Format::RGBA8UNorm,
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
            true,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
                plane: TexturePlane::Primary,
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
        cmd_buffer.end_label();
    }

    fn filter_env_map(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
    ) {
        cmd_buffer.begin_label("Environment Map prefiltering");
        let mut info = pass_params
            .resources
            .texture_info(Self::ENVIRONMENT_MAP_TEXTURE_NAME)
            .clone();

        pass_params.resources.create_texture(
            Self::FILTERED_DIFFUSE_ENVIRONMENT_MAP_TEXTURE_NAME,
            &info,
            false,
        );

        info.mip_levels = 32u32 - u32::leading_zeros(info.width);
        pass_params.resources.create_texture(
            Self::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
            &info,
            false,
        );

        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.prefilter_diffuse_pipeline)
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
                plane: TexturePlane::Primary,
            },
            HistoryResourceEntry::Current,
        );
        let filtered_cube = pass_params.resources.access_view(
            cmd_buffer,
            Self::FILTERED_DIFFUSE_ENVIRONMENT_MAP_TEXTURE_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            &TextureViewInfo {
                base_mip_level: 0u32,
                mip_level_length: 1u32,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
                format: None,
                plane: TexturePlane::Primary,
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

        let pipeline = pass_params
            .assets
            .get_compute_pipeline(self.prefilter_specular_pipeline)
            .unwrap();
        cmd_buffer.set_pipeline(PipelineBinding::Compute(pipeline));

        let texture_temp = pass_params.resources.access_texture(
            cmd_buffer,
            Self::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
            &BarrierTextureRange {
                base_mip_level: 0u32,
                mip_level_length: info.mip_levels,
                base_array_layer: 0u32,
                array_layer_length: 1u32,
            },
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            TextureLayout::Storage,
            true,
            HistoryResourceEntry::Current,
        );
        std::mem::drop(texture_temp);

        for i in 0..info.mip_levels {
            cmd_buffer.set_push_constant_data(
                &[(1.0f32 / ((info.mip_levels - 1u32) as f32)) * (i as f32)],
                ShaderType::ComputeShader,
            );
            let output_view = pass_params.resources.get_view(
                Self::FILTERED_SPECULAR_ENVIRONMENT_MAP_TEXTURE_NAME,
                &TextureViewInfo {
                    mip_level_length: 1,
                    array_layer_length: 1,
                    base_array_layer: 0,
                    base_mip_level: i,
                    format: None,
                    plane: TexturePlane::Primary,
                },
                HistoryResourceEntry::Current,
            );
            cmd_buffer.bind_storage_texture(BindingFrequency::VeryFrequent, 1u32, &output_view);
            cmd_buffer.finish_binding();
            cmd_buffer.dispatch(((info.width >> i) + 7) / 8, ((info.height >> i) + 7) / 8, 6);
        }

        cmd_buffer.end_label();
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
        self.calculate_preintegration_lut(cmd_buffer, pass_params);
        self.prepared = true;
    }
}
