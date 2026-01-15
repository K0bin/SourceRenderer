use crate::asset::asset_manager::AssetFile;
use crate::asset::{
    AssetData, AssetLoadPriority, AssetLoader, AssetLoaderProgress, AssetManager, MeshData,
    MeshRange,
};
use futures_lite::AsyncReadExt;
use sourcerenderer_core::{HalfVec3, Vec3};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::slice;
use std::sync::Arc;

pub struct RawVolumeLoader {}

impl RawVolumeLoader {
    pub fn new() -> Self {
        Self {}
    }
}

const RESOLUTION_DOWNSCALE_FACTOR: usize = 1usize;
const SIZE_SCALE_FACTOR: f32 = 0.01f32;
//const THRESHOLD: f32 = 0.08f32;
//const THRESHOLD: f32 = 0.0505f32;
const THRESHOLD: f32 = 0.0485f32;
//const THRESHOLD: f32 = 0.035f32;
//const THRESHOLD: f32 = 0.026f32;

impl AssetLoader for RawVolumeLoader {
    fn matches(&self, file: &mut AssetFile) -> bool {
        file.path().contains("raw.txt")
    }

    async fn load(
        &self,
        mut file: AssetFile,
        manager: &Arc<AssetManager>,
        priority: AssetLoadPriority,
        _progress: &Arc<AssetLoaderProgress>,
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

        let (vertices, indices) = marching_cubes(
            (downsampled_width, downsampled_height, downsampled_depth),
            |x, y, z| {
                values[(z as usize) * (downsampled_width as usize) * (downsampled_height as usize)
                    + (y as usize) * (downsampled_width as usize)
                    + (x as usize)]
            },
            THRESHOLD,
            spacing * SIZE_SCALE_FACTOR * (RESOLUTION_DOWNSCALE_FACTOR as f32),
            RESOLUTION_DOWNSCALE_FACTOR as u32,
        );
        let vertex_count = vertices.len() as u32;
        let vertices_box = vertices.into_boxed_slice();
        let size_old = std::mem::size_of_val(vertices_box.as_ref());
        let ptr = Box::into_raw(vertices_box);
        let data_ptr = unsafe {
            slice::from_raw_parts_mut(
                ptr as *mut u8,
                (vertex_count as usize) * std::mem::size_of::<HalfVec3>(),
            ) as *mut [u8]
        };
        let vertices_data = unsafe { Box::from_raw(data_ptr) };
        assert_eq!(size_old, std::mem::size_of_val(vertices_data.as_ref()));
        let index_count = indices.len() as u32;
        let indices_box = indices.into_boxed_slice();
        let indices_size_old = std::mem::size_of_val(indices_box.as_ref());
        let indices_ptr = Box::into_raw(indices_box);
        let indices_data_ptr = unsafe {
            slice::from_raw_parts_mut(
                indices_ptr as *mut u8,
                (index_count as usize) * std::mem::size_of::<u32>(),
            ) as *mut [u8]
        };
        let indices_data = unsafe { Box::from_raw(indices_data_ptr) };
        assert_eq!(
            indices_size_old,
            std::mem::size_of_val(indices_data.as_ref())
        );
        log::info!(
            "Generated {} vertices ({} MB) + {} indices ({} MB)",
            vertex_count,
            vertices_data.len() / 1024usize / 1024usize,
            index_count,
            indices_data.len() / 1024usize / 1024usize
        );

        let mesh = MeshData {
            indices: Some(indices_data),
            vertices: vertices_data,
            parts: Box::new([MeshRange {
                start: 0u32,
                count: vertex_count,
            }]),
            bounding_box: None,
            vertex_count,
        };
        manager.add_asset_data(file.path(), AssetData::Mesh(mesh), priority);

        Ok(())
    }
}

// https://people.eecs.berkeley.edu/~jrs/meshpapers/LorensenCline.pdf
// https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Chernyaev96.pdf
// https://paulbourke.net/geometry/polygonise/
// https://paulbourke.net/geometry/polygonise/marchingsource.cpp
// https://paulbourke.net/geometry/polygonise/table2.txt
fn marching_cubes<F: Fn(u32, u32, u32) -> f32>(
    size: (u32, u32, u32),
    value_lookup: F,
    threshold: f32,
    scale: Vec3,
    downscale_factor: u32,
) -> (Vec<HalfVec3>, Vec<u32>) {
    const EDGE_TABLE: [u32; 256] = [
        0x0, 0x109, 0x203, 0x30a, 0x406, 0x50f, 0x605, 0x70c, 0x80c, 0x905, 0xa0f, 0xb06, 0xc0a,
        0xd03, 0xe09, 0xf00, 0x190, 0x99, 0x393, 0x29a, 0x596, 0x49f, 0x795, 0x69c, 0x99c, 0x895,
        0xb9f, 0xa96, 0xd9a, 0xc93, 0xf99, 0xe90, 0x230, 0x339, 0x33, 0x13a, 0x636, 0x73f, 0x435,
        0x53c, 0xa3c, 0xb35, 0x83f, 0x936, 0xe3a, 0xf33, 0xc39, 0xd30, 0x3a0, 0x2a9, 0x1a3, 0xaa,
        0x7a6, 0x6af, 0x5a5, 0x4ac, 0xbac, 0xaa5, 0x9af, 0x8a6, 0xfaa, 0xea3, 0xda9, 0xca0, 0x460,
        0x569, 0x663, 0x76a, 0x66, 0x16f, 0x265, 0x36c, 0xc6c, 0xd65, 0xe6f, 0xf66, 0x86a, 0x963,
        0xa69, 0xb60, 0x5f0, 0x4f9, 0x7f3, 0x6fa, 0x1f6, 0xff, 0x3f5, 0x2fc, 0xdfc, 0xcf5, 0xfff,
        0xef6, 0x9fa, 0x8f3, 0xbf9, 0xaf0, 0x650, 0x759, 0x453, 0x55a, 0x256, 0x35f, 0x55, 0x15c,
        0xe5c, 0xf55, 0xc5f, 0xd56, 0xa5a, 0xb53, 0x859, 0x950, 0x7c0, 0x6c9, 0x5c3, 0x4ca, 0x3c6,
        0x2cf, 0x1c5, 0xcc, 0xfcc, 0xec5, 0xdcf, 0xcc6, 0xbca, 0xac3, 0x9c9, 0x8c0, 0x8c0, 0x9c9,
        0xac3, 0xbca, 0xcc6, 0xdcf, 0xec5, 0xfcc, 0xcc, 0x1c5, 0x2cf, 0x3c6, 0x4ca, 0x5c3, 0x6c9,
        0x7c0, 0x950, 0x859, 0xb53, 0xa5a, 0xd56, 0xc5f, 0xf55, 0xe5c, 0x15c, 0x55, 0x35f, 0x256,
        0x55a, 0x453, 0x759, 0x650, 0xaf0, 0xbf9, 0x8f3, 0x9fa, 0xef6, 0xfff, 0xcf5, 0xdfc, 0x2fc,
        0x3f5, 0xff, 0x1f6, 0x6fa, 0x7f3, 0x4f9, 0x5f0, 0xb60, 0xa69, 0x963, 0x86a, 0xf66, 0xe6f,
        0xd65, 0xc6c, 0x36c, 0x265, 0x16f, 0x66, 0x76a, 0x663, 0x569, 0x460, 0xca0, 0xda9, 0xea3,
        0xfaa, 0x8a6, 0x9af, 0xaa5, 0xbac, 0x4ac, 0x5a5, 0x6af, 0x7a6, 0xaa, 0x1a3, 0x2a9, 0x3a0,
        0xd30, 0xc39, 0xf33, 0xe3a, 0x936, 0x83f, 0xb35, 0xa3c, 0x53c, 0x435, 0x73f, 0x636, 0x13a,
        0x33, 0x339, 0x230, 0xe90, 0xf99, 0xc93, 0xd9a, 0xa96, 0xb9f, 0x895, 0x99c, 0x69c, 0x795,
        0x49f, 0x596, 0x29a, 0x393, 0x99, 0x190, 0xf00, 0xe09, 0xd03, 0xc0a, 0xb06, 0xa0f, 0x905,
        0x80c, 0x70c, 0x605, 0x50f, 0x406, 0x30a, 0x203, 0x109, 0x0,
    ];
    const TRI_TABLE: [[i32; 16]; 256] = [
        [
            -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        ],
        [0, 8, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 1, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 8, 3, 9, 8, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 3, 1, 2, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [9, 2, 10, 0, 2, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [2, 8, 3, 2, 10, 8, 10, 9, 8, -1, -1, -1, -1, -1, -1, -1],
        [3, 11, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1], //
        [0, 11, 2, 8, 11, 0, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 9, 0, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 11, 2, 1, 9, 11, 9, 8, 11, -1, -1, -1, -1, -1, -1, -1],
        [3, 10, 1, 11, 10, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 10, 1, 0, 8, 10, 8, 11, 10, -1, -1, -1, -1, -1, -1, -1],
        [3, 9, 0, 3, 11, 9, 11, 10, 9, -1, -1, -1, -1, -1, -1, -1],
        [9, 8, 10, 10, 8, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 7, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 3, 0, 7, 3, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 1, 9, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 1, 9, 4, 7, 1, 7, 3, 1, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 10, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [3, 4, 7, 3, 0, 4, 1, 2, 10, -1, -1, -1, -1, -1, -1, -1],
        [9, 2, 10, 9, 0, 2, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1],
        [2, 10, 9, 2, 9, 7, 2, 7, 3, 7, 9, 4, -1, -1, -1, -1],
        [8, 4, 7, 3, 11, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [11, 4, 7, 11, 2, 4, 2, 0, 4, -1, -1, -1, -1, -1, -1, -1],
        [9, 0, 1, 8, 4, 7, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1],
        [4, 7, 11, 9, 4, 11, 9, 11, 2, 9, 2, 1, -1, -1, -1, -1],
        [3, 10, 1, 3, 11, 10, 7, 8, 4, -1, -1, -1, -1, -1, -1, -1],
        [1, 11, 10, 1, 4, 11, 1, 0, 4, 7, 11, 4, -1, -1, -1, -1],
        [4, 7, 8, 9, 0, 11, 9, 11, 10, 11, 0, 3, -1, -1, -1, -1],
        [4, 7, 11, 4, 11, 9, 9, 11, 10, -1, -1, -1, -1, -1, -1, -1],
        [9, 5, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [9, 5, 4, 0, 8, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 5, 4, 1, 5, 0, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [8, 5, 4, 8, 3, 5, 3, 1, 5, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 10, 9, 5, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [3, 0, 8, 1, 2, 10, 4, 9, 5, -1, -1, -1, -1, -1, -1, -1],
        [5, 2, 10, 5, 4, 2, 4, 0, 2, -1, -1, -1, -1, -1, -1, -1],
        [2, 10, 5, 3, 2, 5, 3, 5, 4, 3, 4, 8, -1, -1, -1, -1],
        [9, 5, 4, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 11, 2, 0, 8, 11, 4, 9, 5, -1, -1, -1, -1, -1, -1, -1],
        [0, 5, 4, 0, 1, 5, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1],
        [2, 1, 5, 2, 5, 8, 2, 8, 11, 4, 8, 5, -1, -1, -1, -1],
        [10, 3, 11, 10, 1, 3, 9, 5, 4, -1, -1, -1, -1, -1, -1, -1],
        [4, 9, 5, 0, 8, 1, 8, 10, 1, 8, 11, 10, -1, -1, -1, -1],
        [5, 4, 0, 5, 0, 11, 5, 11, 10, 11, 0, 3, -1, -1, -1, -1],
        [5, 4, 8, 5, 8, 10, 10, 8, 11, -1, -1, -1, -1, -1, -1, -1],
        [9, 7, 8, 5, 7, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [9, 3, 0, 9, 5, 3, 5, 7, 3, -1, -1, -1, -1, -1, -1, -1],
        [0, 7, 8, 0, 1, 7, 1, 5, 7, -1, -1, -1, -1, -1, -1, -1],
        [1, 5, 3, 3, 5, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [9, 7, 8, 9, 5, 7, 10, 1, 2, -1, -1, -1, -1, -1, -1, -1],
        [10, 1, 2, 9, 5, 0, 5, 3, 0, 5, 7, 3, -1, -1, -1, -1],
        [8, 0, 2, 8, 2, 5, 8, 5, 7, 10, 5, 2, -1, -1, -1, -1],
        [2, 10, 5, 2, 5, 3, 3, 5, 7, -1, -1, -1, -1, -1, -1, -1],
        [7, 9, 5, 7, 8, 9, 3, 11, 2, -1, -1, -1, -1, -1, -1, -1],
        [9, 5, 7, 9, 7, 2, 9, 2, 0, 2, 7, 11, -1, -1, -1, -1],
        [2, 3, 11, 0, 1, 8, 1, 7, 8, 1, 5, 7, -1, -1, -1, -1],
        [11, 2, 1, 11, 1, 7, 7, 1, 5, -1, -1, -1, -1, -1, -1, -1],
        [9, 5, 8, 8, 5, 7, 10, 1, 3, 10, 3, 11, -1, -1, -1, -1],
        [5, 7, 0, 5, 0, 9, 7, 11, 0, 1, 0, 10, 11, 10, 0, -1],
        [11, 10, 0, 11, 0, 3, 10, 5, 0, 8, 0, 7, 5, 7, 0, -1],
        [11, 10, 5, 7, 11, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [10, 6, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 3, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [9, 0, 1, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 8, 3, 1, 9, 8, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1],
        [1, 6, 5, 2, 6, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 6, 5, 1, 2, 6, 3, 0, 8, -1, -1, -1, -1, -1, -1, -1],
        [9, 6, 5, 9, 0, 6, 0, 2, 6, -1, -1, -1, -1, -1, -1, -1],
        [5, 9, 8, 5, 8, 2, 5, 2, 6, 3, 2, 8, -1, -1, -1, -1],
        [2, 3, 11, 10, 6, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [11, 0, 8, 11, 2, 0, 10, 6, 5, -1, -1, -1, -1, -1, -1, -1],
        [0, 1, 9, 2, 3, 11, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1],
        [5, 10, 6, 1, 9, 2, 9, 11, 2, 9, 8, 11, -1, -1, -1, -1],
        [6, 3, 11, 6, 5, 3, 5, 1, 3, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 11, 0, 11, 5, 0, 5, 1, 5, 11, 6, -1, -1, -1, -1],
        [3, 11, 6, 0, 3, 6, 0, 6, 5, 0, 5, 9, -1, -1, -1, -1],
        [6, 5, 9, 6, 9, 11, 11, 9, 8, -1, -1, -1, -1, -1, -1, -1],
        [5, 10, 6, 4, 7, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 3, 0, 4, 7, 3, 6, 5, 10, -1, -1, -1, -1, -1, -1, -1],
        [1, 9, 0, 5, 10, 6, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1],
        [10, 6, 5, 1, 9, 7, 1, 7, 3, 7, 9, 4, -1, -1, -1, -1],
        [6, 1, 2, 6, 5, 1, 4, 7, 8, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 5, 5, 2, 6, 3, 0, 4, 3, 4, 7, -1, -1, -1, -1],
        [8, 4, 7, 9, 0, 5, 0, 6, 5, 0, 2, 6, -1, -1, -1, -1],
        [7, 3, 9, 7, 9, 4, 3, 2, 9, 5, 9, 6, 2, 6, 9, -1],
        [3, 11, 2, 7, 8, 4, 10, 6, 5, -1, -1, -1, -1, -1, -1, -1],
        [5, 10, 6, 4, 7, 2, 4, 2, 0, 2, 7, 11, -1, -1, -1, -1],
        [0, 1, 9, 4, 7, 8, 2, 3, 11, 5, 10, 6, -1, -1, -1, -1],
        [9, 2, 1, 9, 11, 2, 9, 4, 11, 7, 11, 4, 5, 10, 6, -1],
        [8, 4, 7, 3, 11, 5, 3, 5, 1, 5, 11, 6, -1, -1, -1, -1],
        [5, 1, 11, 5, 11, 6, 1, 0, 11, 7, 11, 4, 0, 4, 11, -1],
        [0, 5, 9, 0, 6, 5, 0, 3, 6, 11, 6, 3, 8, 4, 7, -1],
        [6, 5, 9, 6, 9, 11, 4, 7, 9, 7, 11, 9, -1, -1, -1, -1],
        [10, 4, 9, 6, 4, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 10, 6, 4, 9, 10, 0, 8, 3, -1, -1, -1, -1, -1, -1, -1],
        [10, 0, 1, 10, 6, 0, 6, 4, 0, -1, -1, -1, -1, -1, -1, -1],
        [8, 3, 1, 8, 1, 6, 8, 6, 4, 6, 1, 10, -1, -1, -1, -1],
        [1, 4, 9, 1, 2, 4, 2, 6, 4, -1, -1, -1, -1, -1, -1, -1],
        [3, 0, 8, 1, 2, 9, 2, 4, 9, 2, 6, 4, -1, -1, -1, -1],
        [0, 2, 4, 4, 2, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [8, 3, 2, 8, 2, 4, 4, 2, 6, -1, -1, -1, -1, -1, -1, -1],
        [10, 4, 9, 10, 6, 4, 11, 2, 3, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 2, 2, 8, 11, 4, 9, 10, 4, 10, 6, -1, -1, -1, -1],
        [3, 11, 2, 0, 1, 6, 0, 6, 4, 6, 1, 10, -1, -1, -1, -1],
        [6, 4, 1, 6, 1, 10, 4, 8, 1, 2, 1, 11, 8, 11, 1, -1],
        [9, 6, 4, 9, 3, 6, 9, 1, 3, 11, 6, 3, -1, -1, -1, -1],
        [8, 11, 1, 8, 1, 0, 11, 6, 1, 9, 1, 4, 6, 4, 1, -1],
        [3, 11, 6, 3, 6, 0, 0, 6, 4, -1, -1, -1, -1, -1, -1, -1],
        [6, 4, 8, 11, 6, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [7, 10, 6, 7, 8, 10, 8, 9, 10, -1, -1, -1, -1, -1, -1, -1],
        [0, 7, 3, 0, 10, 7, 0, 9, 10, 6, 7, 10, -1, -1, -1, -1],
        [10, 6, 7, 1, 10, 7, 1, 7, 8, 1, 8, 0, -1, -1, -1, -1],
        [10, 6, 7, 10, 7, 1, 1, 7, 3, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 6, 1, 6, 8, 1, 8, 9, 8, 6, 7, -1, -1, -1, -1],
        [2, 6, 9, 2, 9, 1, 6, 7, 9, 0, 9, 3, 7, 3, 9, -1],
        [7, 8, 0, 7, 0, 6, 6, 0, 2, -1, -1, -1, -1, -1, -1, -1],
        [7, 3, 2, 6, 7, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [2, 3, 11, 10, 6, 8, 10, 8, 9, 8, 6, 7, -1, -1, -1, -1],
        [2, 0, 7, 2, 7, 11, 0, 9, 7, 6, 7, 10, 9, 10, 7, -1],
        [1, 8, 0, 1, 7, 8, 1, 10, 7, 6, 7, 10, 2, 3, 11, -1],
        [11, 2, 1, 11, 1, 7, 10, 6, 1, 6, 7, 1, -1, -1, -1, -1],
        [8, 9, 6, 8, 6, 7, 9, 1, 6, 11, 6, 3, 1, 3, 6, -1],
        [0, 9, 1, 11, 6, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [7, 8, 0, 7, 0, 6, 3, 11, 0, 11, 6, 0, -1, -1, -1, -1],
        [7, 11, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [7, 6, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [3, 0, 8, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 1, 9, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [8, 1, 9, 8, 3, 1, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1],
        [10, 1, 2, 6, 11, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 10, 3, 0, 8, 6, 11, 7, -1, -1, -1, -1, -1, -1, -1],
        [2, 9, 0, 2, 10, 9, 6, 11, 7, -1, -1, -1, -1, -1, -1, -1],
        [6, 11, 7, 2, 10, 3, 10, 8, 3, 10, 9, 8, -1, -1, -1, -1],
        [7, 2, 3, 6, 2, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [7, 0, 8, 7, 6, 0, 6, 2, 0, -1, -1, -1, -1, -1, -1, -1],
        [2, 7, 6, 2, 3, 7, 0, 1, 9, -1, -1, -1, -1, -1, -1, -1],
        [1, 6, 2, 1, 8, 6, 1, 9, 8, 8, 7, 6, -1, -1, -1, -1],
        [10, 7, 6, 10, 1, 7, 1, 3, 7, -1, -1, -1, -1, -1, -1, -1],
        [10, 7, 6, 1, 7, 10, 1, 8, 7, 1, 0, 8, -1, -1, -1, -1],
        [0, 3, 7, 0, 7, 10, 0, 10, 9, 6, 10, 7, -1, -1, -1, -1],
        [7, 6, 10, 7, 10, 8, 8, 10, 9, -1, -1, -1, -1, -1, -1, -1],
        [6, 8, 4, 11, 8, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [3, 6, 11, 3, 0, 6, 0, 4, 6, -1, -1, -1, -1, -1, -1, -1],
        [8, 6, 11, 8, 4, 6, 9, 0, 1, -1, -1, -1, -1, -1, -1, -1],
        [9, 4, 6, 9, 6, 3, 9, 3, 1, 11, 3, 6, -1, -1, -1, -1],
        [6, 8, 4, 6, 11, 8, 2, 10, 1, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 10, 3, 0, 11, 0, 6, 11, 0, 4, 6, -1, -1, -1, -1],
        [4, 11, 8, 4, 6, 11, 0, 2, 9, 2, 10, 9, -1, -1, -1, -1],
        [10, 9, 3, 10, 3, 2, 9, 4, 3, 11, 3, 6, 4, 6, 3, -1],
        [8, 2, 3, 8, 4, 2, 4, 6, 2, -1, -1, -1, -1, -1, -1, -1],
        [0, 4, 2, 4, 6, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 9, 0, 2, 3, 4, 2, 4, 6, 4, 3, 8, -1, -1, -1, -1],
        [1, 9, 4, 1, 4, 2, 2, 4, 6, -1, -1, -1, -1, -1, -1, -1],
        [8, 1, 3, 8, 6, 1, 8, 4, 6, 6, 10, 1, -1, -1, -1, -1],
        [10, 1, 0, 10, 0, 6, 6, 0, 4, -1, -1, -1, -1, -1, -1, -1],
        [4, 6, 3, 4, 3, 8, 6, 10, 3, 0, 3, 9, 10, 9, 3, -1],
        [10, 9, 4, 6, 10, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 9, 5, 7, 6, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 3, 4, 9, 5, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1],
        [5, 0, 1, 5, 4, 0, 7, 6, 11, -1, -1, -1, -1, -1, -1, -1],
        [11, 7, 6, 8, 3, 4, 3, 5, 4, 3, 1, 5, -1, -1, -1, -1],
        [9, 5, 4, 10, 1, 2, 7, 6, 11, -1, -1, -1, -1, -1, -1, -1],
        [6, 11, 7, 1, 2, 10, 0, 8, 3, 4, 9, 5, -1, -1, -1, -1],
        [7, 6, 11, 5, 4, 10, 4, 2, 10, 4, 0, 2, -1, -1, -1, -1],
        [3, 4, 8, 3, 5, 4, 3, 2, 5, 10, 5, 2, 11, 7, 6, -1],
        [7, 2, 3, 7, 6, 2, 5, 4, 9, -1, -1, -1, -1, -1, -1, -1],
        [9, 5, 4, 0, 8, 6, 0, 6, 2, 6, 8, 7, -1, -1, -1, -1],
        [3, 6, 2, 3, 7, 6, 1, 5, 0, 5, 4, 0, -1, -1, -1, -1],
        [6, 2, 8, 6, 8, 7, 2, 1, 8, 4, 8, 5, 1, 5, 8, -1],
        [9, 5, 4, 10, 1, 6, 1, 7, 6, 1, 3, 7, -1, -1, -1, -1],
        [1, 6, 10, 1, 7, 6, 1, 0, 7, 8, 7, 0, 9, 5, 4, -1],
        [4, 0, 10, 4, 10, 5, 0, 3, 10, 6, 10, 7, 3, 7, 10, -1],
        [7, 6, 10, 7, 10, 8, 5, 4, 10, 4, 8, 10, -1, -1, -1, -1],
        [6, 9, 5, 6, 11, 9, 11, 8, 9, -1, -1, -1, -1, -1, -1, -1],
        [3, 6, 11, 0, 6, 3, 0, 5, 6, 0, 9, 5, -1, -1, -1, -1],
        [0, 11, 8, 0, 5, 11, 0, 1, 5, 5, 6, 11, -1, -1, -1, -1],
        [6, 11, 3, 6, 3, 5, 5, 3, 1, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 10, 9, 5, 11, 9, 11, 8, 11, 5, 6, -1, -1, -1, -1],
        [0, 11, 3, 0, 6, 11, 0, 9, 6, 5, 6, 9, 1, 2, 10, -1],
        [11, 8, 5, 11, 5, 6, 8, 0, 5, 10, 5, 2, 0, 2, 5, -1],
        [6, 11, 3, 6, 3, 5, 2, 10, 3, 10, 5, 3, -1, -1, -1, -1],
        [5, 8, 9, 5, 2, 8, 5, 6, 2, 3, 8, 2, -1, -1, -1, -1],
        [9, 5, 6, 9, 6, 0, 0, 6, 2, -1, -1, -1, -1, -1, -1, -1],
        [1, 5, 8, 1, 8, 0, 5, 6, 8, 3, 8, 2, 6, 2, 8, -1],
        [1, 5, 6, 2, 1, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 3, 6, 1, 6, 10, 3, 8, 6, 5, 6, 9, 8, 9, 6, -1],
        [10, 1, 0, 10, 0, 6, 9, 5, 0, 5, 6, 0, -1, -1, -1, -1],
        [0, 3, 8, 5, 6, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [10, 5, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [11, 5, 10, 7, 5, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [11, 5, 10, 11, 7, 5, 8, 3, 0, -1, -1, -1, -1, -1, -1, -1],
        [5, 11, 7, 5, 10, 11, 1, 9, 0, -1, -1, -1, -1, -1, -1, -1],
        [10, 7, 5, 10, 11, 7, 9, 8, 1, 8, 3, 1, -1, -1, -1, -1],
        [11, 1, 2, 11, 7, 1, 7, 5, 1, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 3, 1, 2, 7, 1, 7, 5, 7, 2, 11, -1, -1, -1, -1],
        [9, 7, 5, 9, 2, 7, 9, 0, 2, 2, 11, 7, -1, -1, -1, -1],
        [7, 5, 2, 7, 2, 11, 5, 9, 2, 3, 2, 8, 9, 8, 2, -1],
        [2, 5, 10, 2, 3, 5, 3, 7, 5, -1, -1, -1, -1, -1, -1, -1],
        [8, 2, 0, 8, 5, 2, 8, 7, 5, 10, 2, 5, -1, -1, -1, -1],
        [9, 0, 1, 5, 10, 3, 5, 3, 7, 3, 10, 2, -1, -1, -1, -1],
        [9, 8, 2, 9, 2, 1, 8, 7, 2, 10, 2, 5, 7, 5, 2, -1],
        [1, 3, 5, 3, 7, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 7, 0, 7, 1, 1, 7, 5, -1, -1, -1, -1, -1, -1, -1],
        [9, 0, 3, 9, 3, 5, 5, 3, 7, -1, -1, -1, -1, -1, -1, -1],
        [9, 8, 7, 5, 9, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [5, 8, 4, 5, 10, 8, 10, 11, 8, -1, -1, -1, -1, -1, -1, -1],
        [5, 0, 4, 5, 11, 0, 5, 10, 11, 11, 3, 0, -1, -1, -1, -1],
        [0, 1, 9, 8, 4, 10, 8, 10, 11, 10, 4, 5, -1, -1, -1, -1],
        [10, 11, 4, 10, 4, 5, 11, 3, 4, 9, 4, 1, 3, 1, 4, -1],
        [2, 5, 1, 2, 8, 5, 2, 11, 8, 4, 5, 8, -1, -1, -1, -1],
        [0, 4, 11, 0, 11, 3, 4, 5, 11, 2, 11, 1, 5, 1, 11, -1],
        [0, 2, 5, 0, 5, 9, 2, 11, 5, 4, 5, 8, 11, 8, 5, -1],
        [9, 4, 5, 2, 11, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [2, 5, 10, 3, 5, 2, 3, 4, 5, 3, 8, 4, -1, -1, -1, -1],
        [5, 10, 2, 5, 2, 4, 4, 2, 0, -1, -1, -1, -1, -1, -1, -1],
        [3, 10, 2, 3, 5, 10, 3, 8, 5, 4, 5, 8, 0, 1, 9, -1],
        [5, 10, 2, 5, 2, 4, 1, 9, 2, 9, 4, 2, -1, -1, -1, -1],
        [8, 4, 5, 8, 5, 3, 3, 5, 1, -1, -1, -1, -1, -1, -1, -1],
        [0, 4, 5, 1, 0, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [8, 4, 5, 8, 5, 3, 9, 0, 5, 0, 3, 5, -1, -1, -1, -1],
        [9, 4, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 11, 7, 4, 9, 11, 9, 10, 11, -1, -1, -1, -1, -1, -1, -1],
        [0, 8, 3, 4, 9, 7, 9, 11, 7, 9, 10, 11, -1, -1, -1, -1],
        [1, 10, 11, 1, 11, 4, 1, 4, 0, 7, 4, 11, -1, -1, -1, -1],
        [3, 1, 4, 3, 4, 8, 1, 10, 4, 7, 4, 11, 10, 11, 4, -1],
        [4, 11, 7, 9, 11, 4, 9, 2, 11, 9, 1, 2, -1, -1, -1, -1],
        [9, 7, 4, 9, 11, 7, 9, 1, 11, 2, 11, 1, 0, 8, 3, -1],
        [11, 7, 4, 11, 4, 2, 2, 4, 0, -1, -1, -1, -1, -1, -1, -1],
        [11, 7, 4, 11, 4, 2, 8, 3, 4, 3, 2, 4, -1, -1, -1, -1],
        [2, 9, 10, 2, 7, 9, 2, 3, 7, 7, 4, 9, -1, -1, -1, -1],
        [9, 10, 7, 9, 7, 4, 10, 2, 7, 8, 7, 0, 2, 0, 7, -1],
        [3, 7, 10, 3, 10, 2, 7, 4, 10, 1, 10, 0, 4, 0, 10, -1],
        [1, 10, 2, 8, 7, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 9, 1, 4, 1, 7, 7, 1, 3, -1, -1, -1, -1, -1, -1, -1],
        [4, 9, 1, 4, 1, 7, 0, 8, 1, 8, 7, 1, -1, -1, -1, -1],
        [4, 0, 3, 7, 4, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [4, 8, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [9, 10, 8, 10, 11, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [3, 0, 9, 3, 9, 11, 11, 9, 10, -1, -1, -1, -1, -1, -1, -1],
        [0, 1, 10, 0, 10, 8, 8, 10, 11, -1, -1, -1, -1, -1, -1, -1],
        [3, 1, 10, 11, 3, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 2, 11, 1, 11, 9, 9, 11, 8, -1, -1, -1, -1, -1, -1, -1],
        [3, 0, 9, 3, 9, 11, 1, 2, 9, 2, 11, 9, -1, -1, -1, -1],
        [0, 2, 11, 8, 0, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [3, 2, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [2, 3, 8, 2, 8, 10, 10, 8, 9, -1, -1, -1, -1, -1, -1, -1],
        [9, 10, 2, 0, 9, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [2, 3, 8, 2, 8, 10, 0, 1, 8, 1, 10, 8, -1, -1, -1, -1],
        [1, 10, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [1, 3, 8, 9, 1, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 9, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [0, 3, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
        [
            -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        ],
    ];

    /*let test_vertices = test(
        0b1000,
        &value_lookup,
        threshold,
        scale,
        &TRI_TABLE,
        downscale_factor,
    );
    export_debug(&test_vertices);
    return test_vertices;*/

    let mut indices_map = HashMap::<u32, u32>::new();
    let mut vertices =
        Vec::<HalfVec3>::with_capacity((size.0 as usize) * (size.1 as usize) * (size.2 as usize));
    let mut indices = Vec::<u32>::new();
    let mut hits = 0u64;
    let mut total = 0u64;

    let mut cube_vertex_indices: [u32; 12usize] = [0u32; 12usize];
    for z_base in 0..size.2 - 1 {
        for y_base in 0..size.1 - 1 {
            for x_base in 0..size.0 - 1 {
                let mut key = 0u8;

                for z in 0..2 {
                    for y in 0..2 {
                        for x in 0..2 {
                            // The indexing convention is weird.
                            let index = ((x + z) & 1) + z * 2 + y * 4;
                            let pos = (x_base + x, y_base + y, z_base + z);
                            let value = value_lookup(pos.0, pos.1, pos.2);

                            if value < threshold {
                                continue;
                            }

                            key |= 1u8 << index;
                        }
                    }
                }

                if key == 0u8 {
                    continue;
                }

                let index_to_pos = |idx: u32| {
                    (
                        x_base + (((!(idx >> 1) & (idx & 1)) | ((idx >> 1) & !(idx & 1))) & 1),
                        y_base + ((idx >> 2) & 1),
                        z_base + ((idx >> 1) & 1),
                    )
                };

                let pos_to_cache_key = |mut pos: (u32, u32, u32), pos2: (u32, u32, u32)| {
                    let x_diff = pos2.0.max(pos.0) - pos.0.min(pos2.0);
                    let y_diff = pos2.1.max(pos.1) - pos.1.min(pos2.1);
                    let z_diff = pos2.2.max(pos.2) - pos.2.min(pos2.2);
                    assert_eq!(x_diff + y_diff + z_diff, 1u32);

                    let mut dir = 0u32;
                    if y_diff == 1u32 {
                        dir = 1u32;
                    } else if z_diff == 1u32 {
                        dir = 2u32;
                    }

                    pos = pos.min(pos2);

                    (pos.2 * size.0 * size.1 + pos.1 * size.0 + pos.0) * 3u32 + dir
                };

                for i in 0usize..4usize {
                    if (EDGE_TABLE[key as usize] & (1 << i)) != 0 {
                        total += 1;
                        let index0 = pos_to_cache_key(
                            index_to_pos(i as u32),
                            index_to_pos((i as u32 + 1u32) % 4u32),
                        );
                        let existing_index0 = indices_map.get(&index0).cloned();
                        if let Some(index) = existing_index0 {
                            cube_vertex_indices[i] = index;
                            hits += 1;
                        } else {
                            let vertex = interpolate_vertices(
                                index_to_pos(i as u32),
                                index_to_pos((i as u32 + 1u32) % 4u32),
                                &value_lookup,
                                threshold,
                            ) * scale;
                            let index = vertices.len() as u32;
                            vertices.push(vertex);
                            cube_vertex_indices[i] = index;
                            indices_map.insert(index0, index);
                        }
                    }

                    if (EDGE_TABLE[key as usize] & (16 << i)) != 0 {
                        total += 1;
                        let index1 = pos_to_cache_key(
                            index_to_pos(i as u32 + 4u32),
                            index_to_pos((i as u32 + 1u32) % 4u32 + 4u32),
                        );
                        let existing_index1 = indices_map.get(&index1).cloned();
                        if let Some(index) = existing_index1 {
                            cube_vertex_indices[i + 4usize] = index;
                            hits += 1;
                        } else {
                            let vertex = interpolate_vertices(
                                index_to_pos(i as u32 + 4u32),
                                index_to_pos((i as u32 + 1u32) % 4u32 + 4u32),
                                &value_lookup,
                                threshold,
                            ) * scale;
                            let index = vertices.len() as u32;
                            vertices.push(vertex);
                            cube_vertex_indices[i + 4usize] = index;
                            indices_map.insert(index1, index);
                        }
                    }

                    if (EDGE_TABLE[key as usize] & (256 << i)) != 0 {
                        total += 1;
                        let index2 =
                            pos_to_cache_key(index_to_pos(i as u32), index_to_pos(i as u32 + 4u32));
                        let existing_index2 = indices_map.get(&index2).cloned();
                        if let Some(index) = existing_index2 {
                            cube_vertex_indices[i + 8usize] = index;
                            hits += 1;
                        } else {
                            let vertex = interpolate_vertices(
                                index_to_pos(i as u32),
                                index_to_pos(i as u32 + 4u32),
                                &value_lookup,
                                threshold,
                            ) * scale;
                            let index = vertices.len() as u32;
                            vertices.push(vertex);
                            cube_vertex_indices[i + 8usize] = index;
                            indices_map.insert(index2, index);
                        }
                    }
                }

                let mut i = 0usize;
                while TRI_TABLE[key as usize][i] != -1 {
                    indices.push(cube_vertex_indices[TRI_TABLE[key as usize][i] as usize]);
                    indices.push(cube_vertex_indices[TRI_TABLE[key as usize][i + 1usize] as usize]);
                    indices.push(cube_vertex_indices[TRI_TABLE[key as usize][i + 2usize] as usize]);

                    /*log::info!(
                        "Triangle: {:?}\n{:?}\n{:?}",
                        cube_vertices[TRI_TABLE[key as usize][i] as usize],
                        cube_vertices[TRI_TABLE[key as usize][i + 1usize] as usize],
                        cube_vertices[TRI_TABLE[key as usize][i + 2usize] as usize]
                    );*/

                    i += 3usize;
                }

                let total_iterations = size.0 * size.1 * size.2;
                let current_iterations = z_base * size.0 * size.1 + y_base * size.0 + x_base;
                if (current_iterations % 10000) == 0 {
                    log::trace!(
                        "Marching cube progress: {}/{} ({} %)",
                        current_iterations,
                        total_iterations,
                        ((current_iterations as f64) / (total_iterations as f64)) * 100.0f64
                    );
                }
            }
        }
    }

    log::info!(
        "HIT RATE: {:?}, vertices: {:?}",
        ((hits as f64) / (total as f64)) * 100.0f64,
        vertices.len()
    );

    export_debug(&vertices, &indices);

    return (vertices, indices);
}

fn interpolate_vertices<F: Fn(u32, u32, u32) -> f32>(
    pos1: (u32, u32, u32),
    pos2: (u32, u32, u32),
    value_lookup: &F,
    threshold: f32,
) -> HalfVec3 {
    let pos1f = HalfVec3::new_from_f32(pos1.0 as f32, pos1.1 as f32, pos1.2 as f32);
    let pos2f = HalfVec3::new_from_f32(pos2.0 as f32, pos2.1 as f32, pos2.2 as f32);

    // Simple interpolation:
    // return pos1f + (pos2f - pos1f) * 0.5f32;

    let value1 = value_lookup(pos1.0, pos1.1, pos1.2);
    let value2 = value_lookup(pos2.0, pos2.1, pos2.2);
    if (value1 - threshold).abs() < 0.00001f32 || (value1 - value2).abs() < 0.00001f32 {
        return pos1f;
    }
    if (value2 - threshold).abs() < 0.00001f32 {
        return pos2f;
    }
    let a = (threshold - value1) / (value2 - value1);
    pos1f + a * (pos2f - pos1f)
}

fn export_debug(vertices: &[HalfVec3], indices: &[u32]) {
    let path = Path::new("geometry.obj");
    let _ = std::fs::remove_file(path);
    let file_res = File::create(path);
    if file_res.is_err() {
        log::error!("export_debug: Failed to create: {:?}: {:?}", path, file_res);
        return;
    }
    let mut writer = BufWriter::new(file_res.unwrap());
    for vertex in vertices {
        let f32_vertex = Vec3::new(vertex.x.to_f32(), vertex.y.to_f32(), vertex.z.to_f32());
        let res = writer.write_fmt(format_args!(
            "v {:?} {:?} {:?}\n",
            f32_vertex.x, f32_vertex.y, f32_vertex.z
        ));
        if res.is_err() {
            log::error!(
                "export_debug: Failed to write vertex: {:?}: {:?}",
                vertex,
                res
            );
        }
    }
    for i in (0..indices.len()).step_by(3) {
        let res = writer.write_fmt(format_args!(
            "f {} {} {}\n",
            indices[i] + 1,
            indices[i + 1] + 1,
            indices[i + 2] + 1
        ));
        if res.is_err() {
            log::error!(
                "export_debug: Failed to write face: {:?} {:?} {:?}: {:?}",
                i,
                i + 1,
                i + 2,
                res
            );
        }
    }
    log::info!("export_debug: Exported geometry to {:?}", path);
}

fn test<F: Fn(u32, u32, u32) -> f32>(
    key: u8,
    value_lookup: &F,
    threshold: f32,
    scale: f32,
    tri_table: &[[i32; 16]; 256],
    downscale_factor: u32,
) -> Vec<HalfVec3> {
    let index_to_pos = |idx: u32| {
        (
            ((!(idx >> 1) & (idx & 1)) | ((idx >> 1) & !(idx & 1))) & 1,
            (idx >> 2) & 1,
            (idx >> 1) & 1,
        )
    };

    for i in 0..12 {
        let pos = index_to_pos(i as u32);
        log::info!("IDX: {:?}, POS: ({:?}, {:?}, {:?})", i, pos.0, pos.1, pos.2);
    }

    let downscale_factor_f = downscale_factor as f32;
    let base_pos = HalfVec3::new_from_f32(12.0f32, 23.0f32, 34.0f32);

    let mut cube_vertices: [HalfVec3; 12usize] =
        [HalfVec3::new_from_f32(0.0f32, 0.0f32, 0.0f32); 12usize];
    for i in 0usize..4usize {
        cube_vertices[i] = (interpolate_vertices(
            index_to_pos(i as u32),
            index_to_pos((i as u32 + 1u32) % 4u32),
            &value_lookup,
            threshold,
        ) + base_pos)
            * scale;

        cube_vertices[i + 4usize] = (interpolate_vertices(
            index_to_pos(i as u32 + 4u32),
            index_to_pos((i as u32 + 1u32) % 4u32 + 4u32),
            &value_lookup,
            threshold,
        ) + base_pos)
            * scale;

        cube_vertices[i + 8usize] = (interpolate_vertices(
            index_to_pos(i as u32),
            index_to_pos(i as u32 + 4u32),
            &value_lookup,
            threshold,
        ) + base_pos)
            * scale;
    }

    let mut vertices = Vec::<HalfVec3>::new();
    let mut i = 0usize;
    while tri_table[key as usize][i] != -1 {
        vertices.push(cube_vertices[tri_table[key as usize][i] as usize]);
        vertices.push(cube_vertices[tri_table[key as usize][i + 1usize] as usize]);
        vertices.push(cube_vertices[tri_table[key as usize][i + 2usize] as usize]);

        /*log::info!(
            "Triangle: {:?}\n{:?}\n{:?}",
            cube_vertices[TRI_TABLE[key as usize][i] as usize],
            cube_vertices[TRI_TABLE[key as usize][i + 1usize] as usize],
            cube_vertices[TRI_TABLE[key as usize][i + 2usize] as usize]
        );*/

        i += 3usize;
    }
    vertices
}
