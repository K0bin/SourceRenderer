use crate::asset::asset_manager::{AssetFile, AssetLoader};
use crate::asset::{AssetData, AssetLoadPriority, AssetLoaderProgress, AssetManager, TextureData};
use crate::graphics::*;
use bytemuck::box_bytes_of;
use image::{GenericImageView, ImageFormat, ImageReader};
use smallvec::smallvec;
use std::sync::Arc;
use half::f16;

pub struct ImageLoader {}

impl ImageLoader {
    pub fn new() -> Self {
        Self {}
    }
}

const REDUCE_32_BIT_PRECISION: bool = true;

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
            image::DynamicImage::ImageRgba8(data) => {
                let bytes = box_bytes_of(data.into_raw().into_boxed_slice());
                (Format::RGBA8UNorm, bytes)
            }
            image::DynamicImage::ImageRgba16(data) => {
                let bytes = box_bytes_of(data.into_raw().into_boxed_slice());
                (Format::RGBA16UNorm, bytes)
            }
            image::DynamicImage::ImageRgba32F(data) => {
                if REDUCE_32_BIT_PRECISION {
                    let mut reduced_precision = Vec::<f16>::with_capacity((data.width() * data.height() * 4) as usize * std::mem::size_of::<f16>());
                    for pixel in data.into_raw() {
                        reduced_precision.push(f16::from_f32(pixel));
                    }
                    let bytes = box_bytes_of(reduced_precision.into_boxed_slice());
                    (Format::RGBA16Float, bytes)
                } else {
                    let bytes = box_bytes_of(data.into_raw().into_boxed_slice());
                    (Format::RGBA32Float, bytes)
                }
            }
            image::DynamicImage::ImageRgb32F(data) => {
                if REDUCE_32_BIT_PRECISION {
                    let mut reduced_precision = Vec::<f16>::with_capacity((data.width() * data.height() * 4 * 4) as usize * std::mem::size_of::<f16>());
                    for pixel in data.into_raw() {
                        reduced_precision.push(f16::from_f32(pixel));
                        if reduced_precision.len() % 4 == 3 {
                            reduced_precision.push(f16::from_f32(1.0f32));
                        }
                    }
                    let bytes = box_bytes_of(reduced_precision.into_boxed_slice());
                    (Format::RGBA16Float, bytes)
                } else {
                    let img = image::DynamicImage::ImageRgb32F(data);
                    let bytes = box_bytes_of(img.into_rgba32f().into_raw().into_boxed_slice());
                    (Format::RGBA32Float, bytes)
                }
            }
            _ => {
                log::warn!("Unsupported image format, using RGBA8UNorm");
                let bytes = box_bytes_of(img.into_rgb8().into_raw().into_boxed_slice());
                (Format::RGBA8UNorm, bytes)
            },
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
                data: smallvec![data],
            }),
            Some(progress),
            priority,
        );

        Ok(())
    }
}
