use crate::asset::{AssetLoader, Asset, AssetType, AssetManager};
use crate::asset::asset_manager::{AssetLoaderResult, AssetFile, AssetFileData, AssetLoaderProgress, AssetLoadPriority};
use sourcerenderer_core::Platform;
use sourcerenderer_vmt::VMTMaterial;
use std::io::{BufReader, Seek, SeekFrom};
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
  fn matches(&self, file: &mut AssetFile<P>) -> bool {
    file.path.starts_with("materials/") && file.path.ends_with(".vmt")
  }

  fn load(&self, asset_file: AssetFile<P>, manager: &Arc<AssetManager<P>>, priority: AssetLoadPriority, progress: &Arc<AssetLoaderProgress>) -> Result<AssetLoaderResult, ()> {
    let path = asset_file.path.clone();
    let mut vmt_material = match asset_file.data {
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
    }.map_err(|_| ())?;

    if vmt_material.is_patch() {
      let base_path = vmt_material.get_patch_base().unwrap().replace('\\', "/").to_lowercase();
      let base_file = manager.load_file(&base_path);
      if base_file.is_none() {
        return Err(());
      }
      let base_file = base_file.unwrap();
      let mut base_material = match base_file.data {
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
      }.map_err(|_| ())?;
      base_material.apply_patch(&vmt_material);
      vmt_material = base_material
    }

    let albedo_opt = vmt_material.get_base_texture_name();
    if albedo_opt.is_none() {
      return Err(());
    }
    let albedo = albedo_opt.unwrap();
    let albedo_path = "materials/".to_string() + albedo.to_lowercase().replace('\\', "/").as_str() + ".vtf";
    let material = Material {
      albedo_texture_path: albedo_path.clone()
    };

    manager.request_asset_with_progress(&albedo_path, AssetType::Texture, priority, Some(progress));
    manager.add_asset_with_progress(&path, Asset::Material(material), Some(progress), priority);

    Ok(AssetLoaderResult {
      level: None
    })
  }
}
