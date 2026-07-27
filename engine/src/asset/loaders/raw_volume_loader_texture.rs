use crate::asset::asset_manager::AssetFile;
use crate::asset::AssetData::{Material, Model};
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager, MaterialData,
    MaterialValue, MeshData, MeshRange, ModelData, TextureData,
};
use crate::renderer::asset::RendererMaterialValue;
use futures_lite::AsyncReadExt;
use half::f16;
use smallvec::smallvec;
use sourcerenderer_core::gpu::{Format, SampleCount, TextureDimension, TextureInfo, TextureUsage};
use sourcerenderer_core::{HalfVec3, Vec3, Vec4};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::slice;
use std::sync::Arc;

pub struct RawVolumeLoaderTexture {}

impl RawVolumeLoaderTexture {
    pub fn new() -> Self {
        Self {}
    }
}

pub const RESOLUTION_DOWNSCALE_FACTOR: usize = 1usize;

impl AssetLoader for RawVolumeLoaderTexture {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path().contains("raw.txt")
    }

    async fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager>,
        priority: AssetLoadPriority,
        progress: &Arc<AssetLoaderProgress>,
    ) -> Result<(), ()> {
        let metadata_path_str = file.path().to_string();
        let data_file_path = &metadata_path_str[..(metadata_path_str.len() - ".txt".len())];
        let mut data_file = manager.load_file(data_file_path).await.ok_or(())?;

        let mut metadata = String::new();
        file.read_to_string(&mut metadata).await.map_err(|_| ())?;

        let mut words = metadata.split(&[' ', '\r', '\n']);

        let mut width = 0u32;
        let mut height = 0u32;
        let mut depth = 0u32;
        let mut spacing = Vec3::new(0.0f32, 0.0f32, 0.0f32);
        let mut min_value = f32::MAX;
        let mut max_value = 0.0f32;
        let mut has_min_value = false;
        let mut has_max_value = false;

        let mut word_opt = words.next();
        while word_opt.is_some() {
            let word = word_opt.unwrap();
            match word {
                "size:" => {
                    word_opt = words.next();
                    if word_opt.is_none() {
                        return Err(());
                    }
                    width = word_opt.unwrap().parse().map_err(|_| ())?;
                    word_opt = words.next();
                    if word_opt.is_none() {
                        return Err(());
                    }
                    height = word_opt.unwrap().parse().map_err(|_| ())?;
                    word_opt = words.next();
                    if word_opt.is_none() {
                        return Err(());
                    }
                    depth = word_opt.unwrap().parse().map_err(|_| ())?;
                }
                "spacing:" => {
                    for i in 0..3 {
                        word_opt = words.next();
                        if word_opt.is_none() {
                            return Err(());
                        }
                        spacing[i] = word_opt.unwrap().parse().map_err(|_| ())?;
                    }
                }
                "min_level:" => {
                    word_opt = words.next();
                    if word_opt.is_none() {
                        return Err(());
                    }
                    min_value = word_opt.unwrap().parse().map_err(|_| ())?;
                    has_min_value = true;
                }
                "relevant_max_level:" => {
                    word_opt = words.next();
                    if word_opt.is_none() {
                        return Err(());
                    }
                    max_value = word_opt.unwrap().parse().map_err(|_| ())?;
                    has_max_value = true;
                }
                _ => {}
            }
            word_opt = words.next();
        }

        let values_count = (width as usize) * (height as usize) * (depth as usize);

        let mut data = Vec::<u8>::with_capacity(values_count);
        let file_size = data_file.read_to_end(&mut data).await.map_err(|_| ())?;
        let value_size = file_size / values_count;

        let downsampled_width = width / (RESOLUTION_DOWNSCALE_FACTOR as u32);
        let downsampled_height = height / (RESOLUTION_DOWNSCALE_FACTOR as u32);
        let downsampled_depth = depth / (RESOLUTION_DOWNSCALE_FACTOR as u32);
        log::info!(
            "Loading volume. Original resolution: {}x{}x{}, {} voxels, downscaled to {}x{}x{}, {} voxels,\nspacing: {:?}",
            width,
            height,
            depth,
            (width as usize) * (height as usize) * (depth as usize),
            downsampled_width,
            downsampled_height,
            downsampled_depth,
            (downsampled_width as usize) * (downsampled_height as usize) * (downsampled_depth as usize),
            spacing,
        );
        let mut values = Vec::<f16>::with_capacity(values_count);

        for z_base in (0usize..(depth as usize)).step_by(RESOLUTION_DOWNSCALE_FACTOR) {
            if z_base + RESOLUTION_DOWNSCALE_FACTOR > depth as usize {
                continue;
            }
            for y_base in (0usize..(height as usize)).step_by(RESOLUTION_DOWNSCALE_FACTOR) {
                if y_base + RESOLUTION_DOWNSCALE_FACTOR > height as usize {
                    continue;
                }
                for x_base in (0usize..(width as usize)).step_by(RESOLUTION_DOWNSCALE_FACTOR) {
                    if x_base + RESOLUTION_DOWNSCALE_FACTOR > width as usize {
                        continue;
                    }

                    let mut value = 0f32;
                    for z in 0usize..RESOLUTION_DOWNSCALE_FACTOR {
                        for y in 0usize..RESOLUTION_DOWNSCALE_FACTOR {
                            for x in 0usize..RESOLUTION_DOWNSCALE_FACTOR {
                                let i = (z_base + z) * (width as usize) * (height as usize)
                                    + (y_base + y) * (width as usize)
                                    + (x_base + x);
                                let val = if value_size == 1usize {
                                    data[i] as f32
                                } else {
                                    ((data[i * 2usize] as u16)
                                        | ((data[i * 2usize + 1usize] as u16) << 8u16))
                                        as f32
                                };
                                if !has_min_value {
                                    min_value = min_value.min(val);
                                }
                                if !has_max_value {
                                    max_value = max_value.max(val);
                                }
                                value += val;
                            }
                        }
                    }
                    let val = value
                        / ((RESOLUTION_DOWNSCALE_FACTOR
                            * RESOLUTION_DOWNSCALE_FACTOR
                            * RESOLUTION_DOWNSCALE_FACTOR) as f32);
                    values.push(f16::from_f32(val));
                }
            }
        }
        log::info!(
            "Loaded density. Min density: {:?}, max density: {:?}",
            min_value,
            max_value
        );

        for val in &mut values {
            let val32 = val.to_f32();
            *val = f16::from_f32((val32 - min_value) / (max_value - min_value));
        }

        values.shrink_to_fit();

        let data = unsafe {
            let values_box = values.into_boxed_slice();
            let values_len = values_box.len();
            let values_raw = Box::into_raw(values_box);
            Box::from_raw(slice::from_raw_parts_mut(
                values_raw as *mut u8,
                values_len * std::mem::size_of::<f32>(),
            ))
        };

        manager.add_asset_data_with_progress(
            file.path(),
            AssetData::Texture(TextureData {
                info: TextureInfo {
                    dimension: TextureDimension::Dim3D,
                    format: Format::R16Float,
                    width: downsampled_width,
                    height: downsampled_height,
                    depth: downsampled_depth,
                    mip_levels: 1,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::STORAGE
                        | TextureUsage::SAMPLED
                        | TextureUsage::COPY_DST
                        | TextureUsage::INITIAL_COPY,
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
