use crate::asset::asset_manager::AssetFile;
use crate::asset::AssetData::{Material, Model};
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager, MaterialData,
    MaterialValue, MeshData, MeshRange, ModelData, TextureData,
};
use crate::renderer::asset::RendererMaterialValue;
use futures_lite::AsyncReadExt;
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

const RESOLUTION_DOWNSCALE_FACTOR: usize = 1usize;
//const THRESHOLD: f32 = 0.08f32;
const THRESHOLD: f32 = 0.0505f32;
//const THRESHOLD: f32 = 0.0485f32;
//const THRESHOLD: f32 = 0.046f32;
//const THRESHOLD: f32 = 0.035f32;
//const THRESHOLD: f32 = 0.026f32;

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

        let mut words = metadata.split(" ");
        let mut word = words.next();
        if word != Some("size:") {
            return Err(());
        }
        word = words.next();
        if word.is_none() {
            return Err(());
        }
        let width: u32 = word.unwrap().parse().map_err(|_| ())?;
        word = words.next();
        if word.is_none() {
            return Err(());
        }
        let height: u32 = word.unwrap().parse().map_err(|_| ())?;
        word = words.next();
        if word.is_none() {
            return Err(());
        }
        let depth: u32 = word.unwrap().parse().map_err(|_| ())?;

        let mut word = words.next();
        if word != Some("spacing:") {
            return Err(());
        }
        let mut spacing = Vec3::new(0.0f32, 0.0f32, 0.0f32);
        for i in 0..3 {
            word = words.next();
            if word.is_none() {
                return Err(());
            }
            spacing[i] = word.unwrap().parse().map_err(|_| ())?;
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
        let mut values = Vec::<f32>::with_capacity(values_count);

        let mut min_value = f32::MAX;
        let mut max_value = 0.0f32;
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
                                if value_size == 1usize {
                                    value += data[i] as f32;
                                } else {
                                    value += ((data[i * 2usize] as u16)
                                        | ((data[i * 2usize + 1usize] as u16) << 8u16))
                                        as f32;
                                }
                            }
                        }
                    }
                    let val = value
                        / ((RESOLUTION_DOWNSCALE_FACTOR
                            * RESOLUTION_DOWNSCALE_FACTOR
                            * RESOLUTION_DOWNSCALE_FACTOR) as f32);
                    values.push(val);
                    min_value = min_value.min(val);
                    max_value = max_value.max(val);
                }
            }
        }
        log::info!(
            "Loaded density. Min density: {:?}, max density: {:?}, threshold: {:?}",
            min_value,
            max_value,
            THRESHOLD
        );

        for val in &mut values {
            *val = (*val - min_value) / (max_value - min_value);
        }

        values.shrink_to_fit();

        log::info!(
            "Adding texture data for {:?} test {:?}",
            file.path(),
            values[300]
        );

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
                    format: Format::R32Float,
                    width: width / (RESOLUTION_DOWNSCALE_FACTOR as u32),
                    height: height / (RESOLUTION_DOWNSCALE_FACTOR as u32),
                    depth: depth / (RESOLUTION_DOWNSCALE_FACTOR as u32),
                    mip_levels: 1,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::STORAGE
                        | TextureUsage::SAMPLED
                        | TextureUsage::COPY_DST
                        | TextureUsage::INITIAL_COPY,
                    supports_srgb: false,
                },
                data: Box::new([data]),
            }),
            Some(progress),
            priority,
        );

        Ok(())
    }
}
