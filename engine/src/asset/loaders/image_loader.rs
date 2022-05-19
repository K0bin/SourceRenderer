use std::{sync::Arc, io::BufReader};

use sourcerenderer_core::{Platform, graphics::{SampleCount, TextureUsage, TextureInfo}};

use crate::asset::{AssetLoader, asset_manager::{AssetFile, AssetLoaderResult}, AssetManager, AssetLoadPriority, AssetLoaderProgress, Texture};

use image::{io::Reader as ImageReader, ImageFormat, GenericImageView};

pub struct ImageLoader {}

impl ImageLoader {
  pub fn new() -> Self {
    Self {}
  }
}

impl<P: Platform> AssetLoader<P> for ImageLoader {
  fn matches(&self, file: &mut AssetFile<P>) -> bool {
    file.path.ends_with(".png") || file.path.ends_with(".jpg") || file.path.ends_with(".jpeg")
  }

  fn load(&self, file: AssetFile<P>, manager: &Arc<AssetManager<P>>, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()> {
    let is_png = file.path.ends_with(".png");

    let path = file.path.clone();
    let buf_read = BufReader::new(file);
    let image_reader = ImageReader::with_format(buf_read, if is_png { ImageFormat::Png } else { ImageFormat::Jpeg });
    let img = image_reader.decode().map_err(|_e| ())?;
    let (width, height) = img.dimensions();

    let (format, data) = match img {
      image::DynamicImage::ImageRgba8(data) => (sourcerenderer_core::graphics::Format::RGBA8, data.as_raw().clone()),
      image::DynamicImage::ImageBgra8(data) => (sourcerenderer_core::graphics::Format::BGRA8UNorm, data.as_raw().clone()),
      _ => (sourcerenderer_core::graphics::Format::RGBA8, img.into_rgba8().as_raw().clone()),
    };

    manager.add_texture(&path, &TextureInfo {
      format,
      width,
      height,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1,
      usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
    }, data.into_boxed_slice());
    Ok(AssetLoaderResult {
      level: None
    })
  }
}
