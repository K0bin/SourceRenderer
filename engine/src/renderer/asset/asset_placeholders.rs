use sourcerenderer_core::{gpu::{Format, SampleCount, TextureDimension, TextureInfo, TextureUsage, TextureViewInfo}, Platform, Vec4};

use super::*;

pub struct AssetPlaceholders<P: Platform> {
    texture_white: RendererTexture<P::GPUBackend>,
    texture_black: RendererTexture<P::GPUBackend>,
    material: RendererMaterial
}

impl<P: Platform> AssetPlaceholders<P> {
    pub fn new(device: &crate::graphics::Device<P::GPUBackend>) -> Self {
        let mut zero_data = Vec::<u8>::with_capacity(64 * 64 * 4);
        zero_data.resize(zero_data.capacity(), 255u8);
        let zero_texture = device.create_texture(
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: 64,
                height: 64,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::INITIAL_COPY,
                supports_srgb: false,
            },
            Some("AssetManagerZeroTexture"),
        ).unwrap();
        device.init_texture(&zero_data, &zero_texture, 0, 0).unwrap();
        let zero_view = device.create_texture_view(
            &zero_texture,
            &TextureViewInfo::default(),
            Some("AssetManagerZeroTextureView"),
        );
        let zero_index = if device.supports_bindless() {
            device.insert_texture_into_bindless_heap(&zero_view)
        } else {
            None
        };
        let zero_rtexture = RendererTexture {
            view: zero_view,
            bindless_index: zero_index,
        };

        zero_data.fill(0u8);
        let zero_texture_black = device.create_texture(
            &TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: Format::RGBA8UNorm,
                width: 64,
                height: 64,
                depth: 1,
                mip_levels: 1,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
                supports_srgb: false,
            },
            Some("AssetManagerZeroTextureBlack"),
        ).unwrap();
        device.init_texture(&zero_data, &zero_texture_black, 0, 0).unwrap();
        let zero_view_black = device.create_texture_view(
            &zero_texture_black,
            &TextureViewInfo::default(),
            Some("AssetManagerZeroTextureBlackView"),
        );
        let zero_black_index = if device.supports_bindless() {
            device.insert_texture_into_bindless_heap(&zero_view_black)
        } else {
            None
        };
        let zero_rtexture_black = RendererTexture {
            view: zero_view_black,
            bindless_index: zero_black_index,
        };
        let placeholder_material =
            RendererMaterial::new_pbr_color(Vec4::new(1f32, 1f32, 1f32, 1f32));

        Self {
            texture_black: zero_rtexture_black,
            texture_white: zero_rtexture,
            material: placeholder_material
        }
    }

    pub fn texture_black(&self) -> &RendererTexture<P::GPUBackend> {
        &self.texture_black
    }

    pub fn texture_white(&self) -> &RendererTexture<P::GPUBackend> {
        &self.texture_white
    }

    pub fn material(&self) -> &RendererMaterial {
        &self.material
    }
}
