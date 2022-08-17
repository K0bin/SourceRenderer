use crate::asset::{AssetLoader, Asset, AssetManager};
use sourcerenderer_core::Platform;
use crate::asset::asset_manager::{AssetFile, AssetLoaderResult, AssetFileData, AssetLoaderProgress, AssetLoadPriority, Texture};
use std::io::{Cursor, BufReader};
use sourcerenderer_vtf::{VtfTexture, ImageFormat as VTFTextureFormat};
use std::fs::File;
use sourcerenderer_core::graphics::{SampleCount, TextureInfo, TextureUsage, TextureDimension};
use sourcerenderer_core::graphics::Format;
use std::sync::Arc;

pub struct VTFTextureLoader {

}

impl VTFTextureLoader {
  pub fn new() -> Self {
    Self {}
  }
}

impl<P: Platform> AssetLoader<P> for VTFTextureLoader {
  fn matches(&self, file: &mut AssetFile<P>) -> bool {
    if !file.path.ends_with(".vtf") {
      return false;
    }

    match &mut file.data {
      AssetFileData::File(file) => {
        VtfTexture::<File>::check_file(file).unwrap_or(false)
      }
      AssetFileData::Memory(memory) => {
        VtfTexture::<Cursor<Box<[u8]>>>::check_file(memory).unwrap_or(false)
      }
    }
  }

  fn load(&self, file: AssetFile<P>, manager: &Arc<AssetManager<P>>, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()> {
    let path = file.path.clone();
    let texture = match file.data {
      AssetFileData::File(file) => {
        let mut texture = VtfTexture::new(BufReader::new(file)).unwrap();
        let mut data = Vec::<Box<[u8]>>::new();
        for i in 0..texture.header().mipmap_count {
          let reversed_mip = texture.header().mipmap_count - 1 - i;
          let mipmap = &texture.read_mip_map(reversed_mip as u32).unwrap();
          data.push(mipmap.frames[0].faces[0].slices[0].data.clone());
        }
        let mipmap = &texture.read_mip_map(texture.header().mipmap_count as u32 - 1).unwrap();
        Texture {
          info: TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: convert_vtf_texture_format(mipmap.format),
            width: mipmap.width,
            height: mipmap.height,
            depth: 1,
            mip_levels: texture.header().mipmap_count as u32,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::SAMPLED | TextureUsage::BLIT_DST
          },
          data: data.into_boxed_slice(),
        }
      }
      AssetFileData::Memory(cursor) => {
        let mut texture = VtfTexture::new(BufReader::new(cursor)).unwrap();
        let mipmap = texture.read_mip_map(texture.header().mipmap_count as u32 - 1).unwrap();
        let mut data = Vec::<Box<[u8]>>::new();
        for i in 0..texture.header().mipmap_count {
          let reversed_mip = texture.header().mipmap_count - 1 - i;
          let mipmap = &texture.read_mip_map(reversed_mip as u32).unwrap();
          data.push(mipmap.frames[0].faces[0].slices[0].data.clone());
        }
        Texture {
          info: TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: convert_vtf_texture_format(mipmap.format),
            width: mipmap.width,
            height: mipmap.height,
            depth: 1,
            mip_levels: texture.header().mipmap_count as u32,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::SAMPLED | TextureUsage::BLIT_DST
          },
          data: data.into_boxed_slice(),
        }
      }
    };

    manager.add_asset_with_progress(&path, Asset::Texture(texture), Some(progress), priority);

    Ok(AssetLoaderResult {
      level: None
    })
  }
}

fn convert_vtf_texture_format(texture_format: VTFTextureFormat) -> Format {
  match texture_format {
    VTFTextureFormat::DXT1 => Format::DXT1,
    VTFTextureFormat::DXT1OneBitAlpha => Format::DXT1Alpha,
    VTFTextureFormat::DXT3 => Format::DXT3,
    VTFTextureFormat::DXT5 => Format::DXT5,
    VTFTextureFormat::RGBA8888 => Format::RGBA8,
    _ => panic!("VTF format {:?} is not supported", texture_format)
  }
}