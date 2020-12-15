use crate::asset::{AssetLoader, Asset, AssetType};
use crate::asset::asset_manager::{AssetLoaderContext, AssetLoaderResult, AssetFile, AssetFileData, LoadedAsset, AssetContainer, AssociatedAssetLoadRequest};
use sourcerenderer_core::Platform;
use sourcerenderer_vmt::VMTMaterial;
use std::io::{BufReader, Seek};
use async_std::io::SeekFrom;
use crate::asset::Material;
use std::sync::Arc;

pub struct VMTMaterialLoader {

}

impl VMTMaterialLoader {
  pub fn new() -> Self {
    Self {}
  }
}

impl<P: Platform> AssetLoader<P> for VMTMaterialLoader {
  fn matches(&self, file: &mut AssetFile) -> bool {
    file.path.starts_with("materials/") && file.path.ends_with(".vmt")
  }

  fn load(&self, asset_file: AssetFile, context: &AssetLoaderContext<P>) -> Result<AssetLoaderResult<P>, ()> {
    let path = asset_file.path.clone();
    let vmt_material_opt = match asset_file.data {
      AssetFileData::File(file) => {
        let mut bufreader = BufReader::new(file);
        let current = bufreader.seek(SeekFrom::Current(0)).unwrap();
        let len = bufreader.seek(SeekFrom::End(0)).unwrap();
        bufreader.seek(SeekFrom::Start(current)).unwrap();
        VMTMaterial::new(&mut bufreader, len as u32)
      }
      AssetFileData::Memory(mut cursor) => {
        let current = cursor.seek(SeekFrom::Current(0)).unwrap();
        let len = cursor.seek(SeekFrom::End(0)).unwrap();
        cursor.seek(SeekFrom::Start(current)).unwrap();
        VMTMaterial::new(&mut cursor, len as u32)
      }
    };
    if vmt_material_opt.is_err() {
      return Err(());
    }
    let vmt_material = vmt_material_opt.unwrap();
    let albedo_opt = vmt_material.get_base_texture_name();
    if albedo_opt.is_none() {
      return Err(());
    }
    let albedo = albedo_opt.unwrap();
    let albedo_path = "materials/".to_string() + albedo.to_lowercase().replace('\\', "/").as_str() + ".vtf";
    let material = Arc::new(Material {
      albedo_texture_path: albedo_path.clone()
    });

    Ok(AssetLoaderResult {
      requests: vec![AssociatedAssetLoadRequest {
        path: albedo_path,
        asset_type: AssetType::Texture
      }],
      assets: vec![
        LoadedAsset {
          path,
          asset: Asset::Material(material)
        }
      ]
    })
  }
}
