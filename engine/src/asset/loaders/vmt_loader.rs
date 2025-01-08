use std::io::{
    Seek,
    SeekFrom,
};
use std::sync::Arc;

use log::warn;
use sourcerenderer_core::{
    Platform,
    Vec4,
};
use sourcerenderer_vmt::VMTMaterial;

use crate::asset::asset_manager::{
    AssetFile,
    AssetLoadPriority,
    AssetLoaderProgress,
    DirectlyLoadedAsset,
};
use crate::asset::{
    Asset,
    AssetLoader,
    AssetManager,
    AssetType,
    Material,
};

pub struct VMTMaterialLoader {}

impl VMTMaterialLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl<P: Platform> AssetLoader<P> for VMTMaterialLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path.starts_with("materials/") && file.path.ends_with(".vmt")
    }

    fn load(
        &self,
        mut asset_file: AssetFile,
        manager: &Arc<AssetManager<P>>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        let path = asset_file.path.clone();
        let mut vmt_material = {
            let current = asset_file.seek(SeekFrom::Current(0)).unwrap();
            let len = asset_file.seek(SeekFrom::End(0)).unwrap();
            asset_file.seek(SeekFrom::Start(current)).unwrap();
            VMTMaterial::new(&mut asset_file, len as u32)
        }
        .map_err(|_| ())?;

        if vmt_material.is_patch() {
            let base_path = vmt_material
                .get_patch_base()
                .unwrap()
                .replace('\\', "/")
                .to_lowercase();
            let base_file = manager.load_file(&base_path);
            if base_file.is_none() {
                return Err(());
            }
            let mut base_file = base_file.unwrap();
            let mut base_material = {
                let current = base_file.seek(SeekFrom::Current(0)).unwrap();
                let len = base_file.seek(SeekFrom::End(0)).unwrap();
                base_file.seek(SeekFrom::Start(current)).unwrap();
                VMTMaterial::new(&mut base_file, len as u32)
            }
            .map_err(|_| ())?;
            base_material.apply_patch(&vmt_material);
            vmt_material = base_material
        }

        let albedo_opt = vmt_material.get_base_texture_name();
        if let Some(albedo) = albedo_opt {
            let albedo_path = "materials/".to_string()
                + albedo
                    .to_lowercase()
                    .replace('\\', "/")
                    .as_str()
                    .trim_matches('/')
                    .trim_end_matches(".vtf")
                + ".vtf";
            let material = Material::new_pbr(&albedo_path, 0f32, 0f32);

            manager.request_asset_with_progress(
                &albedo_path,
                AssetType::Texture,
                priority,
                progress,
            );
            manager.add_asset_with_progress(
                &path,
                Asset::Material(material),
                Some(progress),
                priority,
            );
        } else {
            if vmt_material.get_shader() != sourcerenderer_vmt::SHADER_WATER {
                warn!("Unsupported material shader: {}", vmt_material.get_shader());
                return Err(());
            }

            let material = Material::new_pbr_color(Vec4::new(0f32, 0f32, 0f32, 1f32), 0f32, 1f32);
            manager.add_asset_with_progress(
                &path,
                Asset::Material(material),
                Some(progress),
                priority,
            );
        }

        Ok(())
    }
}
