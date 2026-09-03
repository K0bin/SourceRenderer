use crate::asset::asset_manager::AssetFile;
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager, TextureData,
};
use bytemuck::{BoxBytes, box_bytes_of, cast_slice_mut, cast_vec};
use futures_lite::AsyncReadExt;
use half::f16;
use smallvec::SmallVec;
use sourcerenderer_core::Vec3;
use sourcerenderer_core::gpu::{Format, SampleCount, TextureDimension, TextureInfo, TextureUsage};
use std::sync::Arc;

pub struct RawVolumeLoaderTexture {}

impl RawVolumeLoaderTexture {
    pub fn new() -> Self {
        Self {}
    }
}

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
        progress.inc_expected(5);

        // Parse metadata

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
                    let mut word = word_opt.ok_or(())?;
                    width = word.parse().map_err(|_| ())?;

                    word_opt = words.next();
                    word = word_opt.ok_or(())?;
                    height = word.parse().map_err(|_| ())?;

                    word_opt = words.next();
                    word = word_opt.ok_or(())?;
                    depth = word.parse().map_err(|_| ())?;
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

        // Load actual data

        let mut src_data = Vec::<u16>::with_capacity(values_count);
        unsafe { src_data.set_len(values_count) };
        let src_data_bytes: &mut [u8] = cast_slice_mut(&mut src_data[..]);
        data_file.read_exact(src_data_bytes).await.map_err(|_| ())?;
        progress.inc_finished(1);

        log::info!(
            "Loading density data. Resolution: {}x{}x{}, {} voxels, spacing: {:?}",
            width,
            height,
            depth,
            (width as usize) * (height as usize) * (depth as usize),
            spacing,
        );

        let mut data = SmallVec::<[Vec<f16>; 1]>::new();
        let mut data_min = SmallVec::<[Vec<f16>; 1]>::new();
        let mut data_max = SmallVec::<[Vec<f16>; 1]>::new();

        // Data is stored as unsigned shorts, we need halves.
        for val in &mut src_data {
            // numeric cast unsigned int -> float, then store the bits in-place.
            let val_f16: f16 = f16::from_f32(*val as f32);
            *val = val_f16.to_bits();
        }
        // Reinterpret as f16s. Works because we just converted and stored the bits.
        let src_data_f16 = cast_vec(src_data);
        data.push(src_data_f16);

        // Build lower mips
        let mip_count = width.ilog2().min(height.ilog2()).min(depth.ilog2());
        progress.inc_expected(mip_count - 1);
        for mip in 1..mip_count {
            let source_image_width = width >> (mip - 1);
            let source_image_height = height >> (mip - 1);
            let source_image_depth = depth >> (mip - 1);

            let mip_width = width >> mip;
            let mip_height = height >> mip;
            let mip_depth = depth >> mip;
            let mip_capacity = (mip_width * mip_height * mip_depth) as usize;
            let mut mip_data = Vec::<f16>::with_capacity(mip_capacity);
            let mut mip_data_max = Vec::<f16>::with_capacity(mip_capacity);
            let mut mip_data_min = Vec::<f16>::with_capacity(mip_capacity);

            // The way this is implemented is pretty slow and doesn't reuse the downscaled values from previous mip levels...
            for z_base in (0usize..((source_image_depth - 1) as usize)).step_by(2) {
                for y_base in (0usize..((source_image_height - 1) as usize)).step_by(2) {
                    for x_base in (0usize..((source_image_width - 1) as usize)).step_by(2) {
                        let mut value = 0f32;
                        let mut value_min = f32::MAX;
                        let mut value_max = f32::MIN;
                        for z in 0usize..2 {
                            for y in 0usize..2 {
                                for x in 0usize..2 {
                                    let i = (z_base + z)
                                        * (source_image_width as usize)
                                        * (source_image_height as usize)
                                        + (y_base + y) * (source_image_width as usize)
                                        + (x_base + x);

                                    let last_mip_data = data.last().unwrap();
                                    let val = last_mip_data[i].to_f32();
                                    value += val / 8.0f32;

                                    let val_max =
                                        data_max.last().unwrap_or(last_mip_data)[i].to_f32();
                                    value_max = value_max.max(val_max);
                                    let val_min =
                                        data_min.last().unwrap_or(last_mip_data)[i].to_f32();
                                    value_min = value_min.min(val_min);
                                }
                            }
                        }
                        mip_data.push(f16::from_f32(value));
                        mip_data_max.push(f16::from_f32(value_max));
                        mip_data_min.push(f16::from_f32(value_min));
                    }
                }
            }

            mip_data.shrink_to_fit();
            data.push(mip_data);
            mip_data_max.shrink_to_fit();
            data_max.push(mip_data_max);
            mip_data_min.shrink_to_fit();
            data_min.push(mip_data_min);

            progress.inc_finished(1);
        }

        // Normalize all values

        if !has_min_value {
            min_value = f32::MAX;
            for &val in data_min.last().unwrap() {
                min_value = min_value.min(val.to_f32());
            }
        }
        if !has_max_value {
            max_value = f32::MIN;
            for &val in data_max.last().unwrap() {
                max_value = max_value.max(val.to_f32());
            }
        }

        for mip_values in &mut data {
            for val in mip_values {
                let val32 = val.to_f32();
                *val = f16::from_f32((val32 - min_value) / (max_value - min_value));
            }
        }
        for mip_values in &mut data_min {
            for val in mip_values {
                let val32 = val.to_f32();
                *val = f16::from_f32((val32 - min_value) / (max_value - min_value));
            }
        }
        for mip_values in &mut data_max {
            for val in mip_values {
                let val32 = val.to_f32();
                *val = f16::from_f32((val32 - min_value) / (max_value - min_value));
            }
        }
        progress.inc_finished(1);

        log::info!(
            "Loaded density data. Min density: {:?}, max density: {:?}, mip maps: {:?}",
            min_value,
            max_value,
            mip_count
        );

        // Done, push the data back to the asset manager

        let mut mips_boxed = SmallVec::<[BoxBytes; 4]>::new();
        let mut mips_boxed_min = SmallVec::<[BoxBytes; 4]>::new();
        let mut mips_boxed_max = SmallVec::<[BoxBytes; 4]>::new();
        for mip_values in data {
            let mip_box = box_bytes_of(mip_values.into_boxed_slice());
            mips_boxed.push(mip_box);
        }
        for mip_values in data_min {
            let mip_box = box_bytes_of(mip_values.into_boxed_slice());
            mips_boxed_min.push(mip_box);
        }
        for mip_values in data_max {
            let mip_box = box_bytes_of(mip_values.into_boxed_slice());
            mips_boxed_max.push(mip_box);
        }
        progress.inc_finished(1);

        manager.add_asset_data_with_progress(
            file.path(),
            AssetData::Texture(TextureData {
                info: TextureInfo {
                    dimension: TextureDimension::Dim3D,
                    format: Format::R16Float,
                    width,
                    height,
                    depth,
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

        let min_max_width = width >> (mip_count - (mips_boxed_max.len() as u32));
        let min_max_height = height >> (mip_count - (mips_boxed_max.len() as u32));
        let min_max_depth = depth >> (mip_count - (mips_boxed_max.len() as u32));
        manager.add_asset_data_with_progress(
            &(file.path().to_string() + "_max"),
            AssetData::Texture(TextureData {
                info: TextureInfo {
                    dimension: TextureDimension::Dim3D,
                    format: Format::R16Float,
                    width: min_max_width,
                    height: min_max_height,
                    depth: min_max_depth,
                    mip_levels: mips_boxed_max.len() as u32,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::STORAGE
                        | TextureUsage::SAMPLED
                        | TextureUsage::COPY_DST
                        | TextureUsage::INITIAL_COPY,
                    supports_srgb: false,
                },
                data: mips_boxed_max,
            }),
            Some(progress),
            priority,
        );

        manager.add_asset_data_with_progress(
            &(file.path().to_string() + "_min"),
            AssetData::Texture(TextureData {
                info: TextureInfo {
                    dimension: TextureDimension::Dim3D,
                    format: Format::R16Float,
                    width: min_max_width,
                    height: min_max_height,
                    depth: min_max_depth,
                    mip_levels: mips_boxed_min.len() as u32,
                    array_length: 1,
                    samples: SampleCount::Samples1,
                    usage: TextureUsage::STORAGE
                        | TextureUsage::SAMPLED
                        | TextureUsage::COPY_DST
                        | TextureUsage::INITIAL_COPY,
                    supports_srgb: false,
                },
                data: mips_boxed_min,
            }),
            Some(progress),
            priority,
        );

        Ok(())
    }
}
