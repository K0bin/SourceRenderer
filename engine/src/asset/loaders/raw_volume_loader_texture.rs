use crate::asset::asset_manager::AssetFile;
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager, TextureData,
};
use futures_lite::AsyncReadExt;
use half::f16;
use smallvec::SmallVec;
use sourcerenderer_core::Vec3;
use sourcerenderer_core::gpu::{Format, SampleCount, TextureDimension, TextureInfo, TextureUsage};
use std::slice;
use std::sync::Arc;

pub struct RawVolumeLoaderTexture {}

impl RawVolumeLoaderTexture {
    pub fn new() -> Self {
        Self {}
    }
}

pub const RESOLUTION_DOWNSCALE_FACTOR: usize = 1usize;
pub const LOAD_MIPS: bool = true;

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

        let mut src_data = Vec::<u8>::with_capacity(values_count);
        let file_size = data_file.read_to_end(&mut src_data).await.map_err(|_| ())?;
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
            (downsampled_width as usize)
                * (downsampled_height as usize)
                * (downsampled_depth as usize),
            spacing,
        );

        let mut data = SmallVec::<[Vec<f16>; 1]>::new();

        let mip_count = if LOAD_MIPS {
            downsampled_width
                .ilog2()
                .min(downsampled_height.ilog2())
                .min(downsampled_depth.ilog2())
        } else {
            1u32
        };

        for mip in 0..mip_count {
            let downscale_factor = if mip == 0 {
                RESOLUTION_DOWNSCALE_FACTOR
            } else {
                2usize
            };
            let mut source_image_width = width;
            let mut source_image_height = height;
            let mut source_image_depth = depth;
            if mip != 0 {
                let scale_factor = (RESOLUTION_DOWNSCALE_FACTOR as u32) * 2u32.pow(mip - 1);
                source_image_width = downsampled_width / scale_factor;
                source_image_height = downsampled_height / scale_factor;
                source_image_depth = downsampled_depth / scale_factor;
            }

            let mut mip_data = Vec::<f16>::with_capacity(
                (width as usize) / downscale_factor * (height as usize) / downscale_factor
                    * (depth as usize)
                    / downscale_factor,
            );

            // The way this is implemented is pretty slow and doesn't reuse the downscaled values from previous mip levels...
            for z_base in (0usize..(source_image_depth as usize)).step_by(downscale_factor) {
                if z_base + downscale_factor > source_image_depth as usize {
                    continue;
                }
                for y_base in (0usize..(source_image_height as usize)).step_by(downscale_factor) {
                    if y_base + downscale_factor > source_image_height as usize {
                        continue;
                    }
                    for x_base in (0usize..(source_image_width as usize)).step_by(downscale_factor)
                    {
                        if x_base + downscale_factor > source_image_width as usize {
                            continue;
                        }

                        let mut value = 0f32;
                        for z in 0usize..downscale_factor {
                            for y in 0usize..downscale_factor {
                                for x in 0usize..downscale_factor {
                                    let i = (z_base + z)
                                        * (source_image_width as usize)
                                        * (source_image_height as usize)
                                        + (y_base + y) * (source_image_width as usize)
                                        + (x_base + x);

                                    if mip == 0 {
                                        let val = if value_size == 1usize {
                                            src_data[i] as f32
                                        } else {
                                            ((src_data[i * 2usize] as u16)
                                                | ((src_data[i * 2usize + 1usize] as u16) << 8u16))
                                                as f32
                                        };
                                        if !has_min_value {
                                            min_value = min_value.min(val);
                                        }
                                        if !has_max_value {
                                            max_value = max_value.max(val);
                                        }
                                        value += val;
                                    } else {
                                        value += data.last().unwrap()[i].to_f32();
                                    }
                                }
                            }
                        }
                        let val = value
                            / ((downscale_factor * downscale_factor * downscale_factor) as f32);
                        mip_data.push(f16::from_f32(val));
                    }
                }
            }

            if mip == 0 {
                src_data.clear();
                src_data.shrink_to_fit();
            }

            mip_data.shrink_to_fit();
            data.push(mip_data);
        }
        log::info!(
            "Loaded density. Min density: {:?}, max density: {:?}, mip maps: {:?}",
            min_value,
            max_value,
            mip_count
        );

        for mip_values in &mut data {
            for val in mip_values {
                let val32 = val.to_f32();
                *val = f16::from_f32((val32 - min_value) / (max_value - min_value));
            }
        }

        let mut mips_boxed = SmallVec::<[Box<[u8]>; 4]>::new();
        for mip_values in data {
            let mip_box = unsafe {
                let values_box = mip_values.into_boxed_slice();
                let values_len = values_box.len();
                let values_raw = Box::into_raw(values_box);
                Box::from_raw(slice::from_raw_parts_mut(
                    values_raw as *mut u8,
                    values_len * std::mem::size_of::<f16>(),
                ))
            };
            mips_boxed.push(mip_box);
        }

        manager.add_asset_data_with_progress(
            file.path(),
            AssetData::Texture(TextureData {
                info: TextureInfo {
                    dimension: TextureDimension::Dim3D,
                    format: Format::R16Float,
                    width: downsampled_width,
                    height: downsampled_height,
                    depth: downsampled_depth,
                    mip_levels: mip_count,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::STORAGE
                        | TextureUsage::SAMPLED
                        | TextureUsage::COPY_DST
                        | TextureUsage::INITIAL_COPY,
                    supports_srgb: false,
                },
                data: mips_boxed,
            }),
            Some(progress),
            priority,
        );

        Ok(())
    }
}
