use std::sync::Arc;

use image::{
    ImageReader,
    GenericImageView,
    ImageFormat,
};

use crate::graphics::*;

use crate::asset::asset_manager::{AssetFile, AssetLoader};
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoaderProgress, AssetManager, TextureData
};

pub struct ImageLoader {}

impl ImageLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl AssetLoader for ImageLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path().ends_with(".png") || file.path().ends_with(".jpg") || file.path().ends_with(".jpeg")
    }

    async fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        let path = file.path().to_string();
        let is_png = file.path().ends_with(".png");

        let cursor = file.into_memory_cursor().await.map_err(|_| ())?;

        let image_reader = ImageReader::with_format(
            cursor,
            if is_png {
                ImageFormat::Png
            } else {
                ImageFormat::Jpeg
            },
        );
        let img = image_reader.decode().map_err(|e| log::error!("Image decoding error: {:?}", e))?;
        let (width, height) = img.dimensions();

        let (format, data) = match img {
            image::DynamicImage::ImageRgba8(data) => (
                Format::RGBA8UNorm,
                data.as_raw().clone(),
            ),
            _ => (
                Format::RGBA8UNorm,
                img.into_rgba8().as_raw().clone(),
            ),
        };

        manager.add_asset_data_with_progress(
            &path,
            AssetData::Texture(TextureData {
                info: TextureInfo {
                    dimension: TextureDimension::Dim2D,
                    format,
                    width,
                    height,
                    depth: 1,
                    mip_levels: 1,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::SAMPLED | TextureUsage::INITIAL_COPY,
                    supports_srgb: false,
                },
                data: vec![data.into_boxed_slice()].into_boxed_slice(),
            }),
            Some(progress),
            priority,
        );

        Ok(())
    }
}
