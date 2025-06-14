use std::io::BufReader;
use std::sync::Arc;

use sourcerenderer_core::Platform;
use sourcerenderer_vtf::{
    ImageFormat as VTFTextureFormat,
    VtfTexture,
};

use crate::asset::asset_manager::{
    AssetFile,
    AssetLoadPriority,
    AssetLoaderProgress,
    DirectlyLoadedAsset,
    Texture,
};
use crate::asset::{
    Asset,
    AssetLoader,
    AssetManager,
};
use crate::graphics::*;

pub struct VTFTextureLoader {}

impl VTFTextureLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl AssetLoader for VTFTextureLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        if !file.path.ends_with(".vtf") {
            return false;
        }
        VtfTexture::<AssetFile>::check_file(file).unwrap_or(false)
    }

    fn load(
        &self,
        file: AssetFile,
        manager: &Arc<AssetManager>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        let path = file.path.clone();
        let mut vtf_texture = VtfTexture::new(BufReader::new(file)).unwrap();
        let mut data = Vec::<Box<[u8]>>::new();
        for i in 0..vtf_texture.header().mipmap_count {
            let reversed_mip = vtf_texture.header().mipmap_count - 1 - i;
            let mipmap = &vtf_texture.read_mip_map(reversed_mip as u32).unwrap();
            data.push(mipmap.frames[0].faces[0].slices[0].data.clone());
        }
        let mipmap = &vtf_texture
            .read_mip_map(vtf_texture.header().mipmap_count as u32 - 1)
            .unwrap();
        let texture = Texture {
            info: TextureInfo {
                dimension: TextureDimension::Dim2D,
                format: convert_vtf_texture_format(mipmap.format),
                width: mipmap.width,
                height: mipmap.height,
                depth: 1,
                mip_levels: vtf_texture.header().mipmap_count as u32,
                array_length: 1,
                samples: SampleCount::Samples1,
                usage: TextureUsage::SAMPLED | TextureUsage::BLIT_DST,
                supports_srgb: false,
            },
            data: data.into_boxed_slice(),
        };

        manager.add_asset_with_progress(&path, Asset::Texture(texture), Some(progress), priority);

        Ok(())
    }
}

fn convert_vtf_texture_format(texture_format: VTFTextureFormat) -> Format {
    match texture_format {
        VTFTextureFormat::DXT1 => Format::BC1,
        VTFTextureFormat::DXT1OneBitAlpha => Format::BC1Alpha,
        VTFTextureFormat::DXT3 => Format::BC2,
        VTFTextureFormat::DXT5 => Format::BC3,
        VTFTextureFormat::RGBA8888 => Format::RGBA8UNorm,
        _ => panic!("VTF format {:?} is not supported", texture_format),
    }
}
