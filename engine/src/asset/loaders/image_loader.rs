use std::io::BufReader;
use std::sync::Arc;

use image::{
    ImageReader,
    GenericImageView,
    ImageFormat,
};

use bevy_tasks::futures_lite::AsyncReadExt;

use sourcerenderer_core::Platform;

use crate::graphics::*;

use crate::asset::asset_manager::{AssetFile, AssetLoaderAsync};
use crate::asset::{
    Asset, AssetLoadPriority, AssetLoaderProgress, AssetManager, DirectlyLoadedAsset, Texture
};

pub struct ImageLoader {}

impl ImageLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl<P: Platform> AssetLoaderAsync<P> for ImageLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path.ends_with(".png") || file.path.ends_with(".jpg") || file.path.ends_with(".jpeg")
    }

    async fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<DirectlyLoadedAsset, ()> {
        let is_png = file.path.ends_with(".png");

        let path = file.path.clone();
        let mut data = Vec::<u8>::new();
        let _bytes_read = file.read_to_end(&mut data).await.map_err(|_| ())?;

        let buf_read = BufReader::new(file);
        let image_reader = ImageReader::with_format(
            buf_read,
            if is_png {
                ImageFormat::Png
            } else {
                ImageFormat::Jpeg
            },
        );
        let img = image_reader.decode().map_err(|_e| ())?;
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

        manager.add_asset_with_progress(
            &path,
            Asset::Texture(Texture {
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

        Ok(DirectlyLoadedAsset::None)
    }
}
