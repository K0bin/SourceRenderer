use std::sync::Arc;

use crate::asset::asset_manager::{AssetFile, AssetLoader};
use crate::asset::{AssetData, AssetLoadPriority, AssetLoaderProgress, AssetManager, TextureData};
use crate::graphics::*;
use image::{EncodableLayout, GenericImageView, ImageFormat, ImageReader};
use smallvec::{smallvec, SmallVec};

pub struct ImageLoader {}

impl ImageLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl AssetLoader for ImageLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path().ends_with(".png")
            || file.path().ends_with(".jpg")
            || file.path().ends_with(".jpeg")
            || file.path().ends_with(".hdr")
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
        let is_jpeg = !is_png && (file.path().ends_with(".jpeg") || file.path().ends_with(".jpg"));
        let is_hdr = !is_png && !is_jpeg && file.path().ends_with(".hdr");

        let cursor = file.into_memory_cursor().await.map_err(|_| ())?;

        let image_reader = ImageReader::with_format(
            cursor,
            if is_png {
                ImageFormat::Png
            } else if is_jpeg {
                ImageFormat::Jpeg
            } else if is_hdr {
                ImageFormat::Hdr
            } else {
                panic!("Unsupported image format")
            },
        );
        let img = image_reader
            .decode()
            .map_err(|e| log::error!("Image decoding error: {:?}", e))?;
        let (width, height) = img.dimensions();

        let (format, data) = match img {
            image::DynamicImage::ImageRgba8(data) => (Format::RGBA8UNorm, data.as_raw().clone()),
            image::DynamicImage::ImageRgba16(data) => {
                (Format::RGBA16UNorm, Vec::<u8>::from(data.as_bytes()))
            }
            image::DynamicImage::ImageRgba32F(data) => {
                (Format::RGBA32Float, Vec::<u8>::from(data.as_bytes()))
            }
            _ => (Format::RGBA8UNorm, img.into_rgba8().into_raw()),
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
                data: smallvec![data.into_boxed_slice()],
            }),
            Some(progress),
            priority,
        );

        Ok(())
    }
}
