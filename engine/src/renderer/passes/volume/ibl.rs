use crate::asset::{AssetHandle, AssetLoadPriority, AssetType, TextureHandle};
use crate::graphics::{CommandBuffer, RenderPassBeginInfo, RenderTarget, StoreOp};
use crate::renderer::asset::{RendererAssets, RendererAssetsReadOnly};
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use sourcerenderer_core::gpu::{
    BindingFrequency, ClearColor, Format, LoadOpColor, SampleCount, TextureDimension, TextureInfo,
    TextureUsage, TextureViewInfo,
};
use std::sync::Arc;

pub struct ImageBasedLightingPreparation {
    handle: TextureHandle,
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

        Self {
            handle: TextureHandle::from(ibl_map_handle),
            prepared: false,
        }
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBuffer,
        assets: &RendererAssetsReadOnly<'_>,
        resources: &mut RendererResources,
    ) {
        let texture = assets.get_texture_opt(self.handle);
        if texture.is_none() || self.prepared {
            return;
        }
        let texture = texture.unwrap();
        let info = texture.view.texture().unwrap().info();
        let mips = 32u32 - u32::leading_zeros(info.width.max(info.height));

        resources.create_texture(
            Self::ENVIRONMENT_MAP_TEXTURE_NAME,
            &TextureInfo {
                dimension: TextureDimension::Dim2DArray,
                format: Format::RGBA16Float,
                width: info.width,
                height: info.height,
                depth: 1u32,
                mip_levels: 32u32 - u32::leading_zeros(info.width.max(info.height)),
                array_length: 6u32,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET,
                supports_srgb: false,
            },
            false,
        );

        for face in 0..6 {
            for mip in 0..mips {
                let rt = resources.get_view(
                    Self::ENVIRONMENT_MAP_TEXTURE_NAME,
                    &TextureViewInfo {
                        base_mip_level: mip,
                        mip_level_length: 1u32,
                        base_array_layer: face,
                        array_layer_length: 0,
                        format: None,
                    },
                    HistoryResourceEntry::Current,
                );

                cmd_buffer.begin_render_pass(&RenderPassBeginInfo {
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

                cmd_buffer.end_render_pass();
            }
        }
        self.prepared = true;
    }
}
