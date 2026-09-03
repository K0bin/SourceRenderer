use crate::asset::TextureHandle;
use crate::graphics::*;
use crate::renderer::asset::{
    ComputePipelineHandle, PathPipelineShaderStage, RendererAssets, RendererAssetsReadOnly,
};
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use bytemuck::{Pod, Zeroable};
use smallvec::SmallVec;
use sourcerenderer_core::Vec3UI;
use sourcerenderer_core::gpu::SpecConstValue;
use std::cell::Ref;
use std::collections::HashMap;
use std::sync::Arc;

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct MarchingCubesConfig {
    pub extent: Vec3UI,
    pub lod: u32,
    pub min: Vec3UI,
    pub thresholds_count: u32,
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct MarchingCubesKey {
    texture_handle: TextureHandle,
    lod: u32,
    min_threshold: u32,
    max_threshold: u32,
}

impl MarchingCubesKey {
    pub fn new(
        texture_handle: TextureHandle,
        lod: u32,
        min_threshold: f32,
        max_threshold: f32,
    ) -> Self {
        Self {
            texture_handle,
            lod,
            min_threshold: (min_threshold * 10000.0) as u32,
            max_threshold: (max_threshold * 10000.0) as u32,
        }
    }

    fn buffer_name(&self) -> String {
        format!(
            "MarchingCubes IBO for {:?}_{}_{}_{}",
            self.texture_handle, self.lod, self.min_threshold, self.max_threshold,
        )
    }
}

pub struct MarchingCubesInfo {
    pub buffer_name: String,
    pub indirect_buffer_offset: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct MarchingCubesIndirectCall {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    vertex_offset: i32,
    first_instance: u32,
    vertex_count: u32,
}

pub struct MarchingCubesPass {
    pipelines: [ComputePipelineHandle; 4],
    edges_buffer: Arc<BufferSlice>,
    tris_buffer: Arc<BufferSlice>,
    executed_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct MarchingCubesThresholds {
    min_threshold: f32,
    max_threshold: f32,
}

impl MarchingCubesPass {
    pub const ATOMICS_BUFFER_NAME: &'static str = "marchingcubes_atomics";

    #[allow(unused)]
    pub fn new(
        device: &Device,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        // Compile optimized pipelines for 1-3 thresholds
        let mut spec_consts = HashMap::<u32, SpecConstValue>::with_capacity(1);
        let mut pipelines = SmallVec::<[ComputePipelineHandle; 4]>::with_capacity(4);
        for i in 0..4 {
            spec_consts.insert(0u32, SpecConstValue::UInt(i));
            pipelines.push(assets.request_compute_pipeline(PathPipelineShaderStage {
                shader_path: "shaders/marching_cubes.comp.json",
                spec_consts: Some(&spec_consts),
            }));
        }

        // Allocate enough room for 16 draws
        resources.create_buffer(
            Self::ATOMICS_BUFFER_NAME,
            &BufferInfo {
                size: std::mem::size_of::<MarchingCubesIndirectCall>() as u64 * 16u64,
                usage: BufferUsage::STORAGE | BufferUsage::CONSTANT | BufferUsage::INDIRECT,
                sharing_mode: QueueSharingMode::Exclusive,
            },
            MemoryUsage::GPUMemory,
            false,
        );

        const EDGE_TABLE: [u32; 256] = [
            0x0, 0x109, 0x203, 0x30a, 0x406, 0x50f, 0x605, 0x70c, 0x80c, 0x905, 0xa0f, 0xb06,
            0xc0a, 0xd03, 0xe09, 0xf00, 0x190, 0x99, 0x393, 0x29a, 0x596, 0x49f, 0x795, 0x69c,
            0x99c, 0x895, 0xb9f, 0xa96, 0xd9a, 0xc93, 0xf99, 0xe90, 0x230, 0x339, 0x33, 0x13a,
            0x636, 0x73f, 0x435, 0x53c, 0xa3c, 0xb35, 0x83f, 0x936, 0xe3a, 0xf33, 0xc39, 0xd30,
            0x3a0, 0x2a9, 0x1a3, 0xaa, 0x7a6, 0x6af, 0x5a5, 0x4ac, 0xbac, 0xaa5, 0x9af, 0x8a6,
            0xfaa, 0xea3, 0xda9, 0xca0, 0x460, 0x569, 0x663, 0x76a, 0x66, 0x16f, 0x265, 0x36c,
            0xc6c, 0xd65, 0xe6f, 0xf66, 0x86a, 0x963, 0xa69, 0xb60, 0x5f0, 0x4f9, 0x7f3, 0x6fa,
            0x1f6, 0xff, 0x3f5, 0x2fc, 0xdfc, 0xcf5, 0xfff, 0xef6, 0x9fa, 0x8f3, 0xbf9, 0xaf0,
            0x650, 0x759, 0x453, 0x55a, 0x256, 0x35f, 0x55, 0x15c, 0xe5c, 0xf55, 0xc5f, 0xd56,
            0xa5a, 0xb53, 0x859, 0x950, 0x7c0, 0x6c9, 0x5c3, 0x4ca, 0x3c6, 0x2cf, 0x1c5, 0xcc,
            0xfcc, 0xec5, 0xdcf, 0xcc6, 0xbca, 0xac3, 0x9c9, 0x8c0, 0x8c0, 0x9c9, 0xac3, 0xbca,
            0xcc6, 0xdcf, 0xec5, 0xfcc, 0xcc, 0x1c5, 0x2cf, 0x3c6, 0x4ca, 0x5c3, 0x6c9, 0x7c0,
            0x950, 0x859, 0xb53, 0xa5a, 0xd56, 0xc5f, 0xf55, 0xe5c, 0x15c, 0x55, 0x35f, 0x256,
            0x55a, 0x453, 0x759, 0x650, 0xaf0, 0xbf9, 0x8f3, 0x9fa, 0xef6, 0xfff, 0xcf5, 0xdfc,
            0x2fc, 0x3f5, 0xff, 0x1f6, 0x6fa, 0x7f3, 0x4f9, 0x5f0, 0xb60, 0xa69, 0x963, 0x86a,
            0xf66, 0xe6f, 0xd65, 0xc6c, 0x36c, 0x265, 0x16f, 0x66, 0x76a, 0x663, 0x569, 0x460,
            0xca0, 0xda9, 0xea3, 0xfaa, 0x8a6, 0x9af, 0xaa5, 0xbac, 0x4ac, 0x5a5, 0x6af, 0x7a6,
            0xaa, 0x1a3, 0x2a9, 0x3a0, 0xd30, 0xc39, 0xf33, 0xe3a, 0x936, 0x83f, 0xb35, 0xa3c,
            0x53c, 0x435, 0x73f, 0x636, 0x13a, 0x33, 0x339, 0x230, 0xe90, 0xf99, 0xc93, 0xd9a,
            0xa96, 0xb9f, 0x895, 0x99c, 0x69c, 0x795, 0x49f, 0x596, 0x29a, 0x393, 0x99, 0x190,
            0xf00, 0xe09, 0xd03, 0xc0a, 0xb06, 0xa0f, 0x905, 0x80c, 0x70c, 0x605, 0x50f, 0x406,
            0x30a, 0x203, 0x109, 0x0,
        ];

        // Modified: First index is the amount of indices
        // Original lookup table only had 16 entries per line

        const TRI_TABLE: [[i32; 17]; 256] = [
            [
                0, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                3, 0, 8, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                3, 0, 1, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 1, 8, 3, 9, 8, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [
                3, 1, 2, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 0, 8, 3, 1, 2, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [6, 9, 2, 10, 0, 2, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 2, 8, 3, 2, 10, 8, 10, 9, 8, -1, -1, -1, -1, -1, -1, -1],
            [
                3, 3, 11, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                6, 0, 11, 2, 8, 11, 0, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 1, 9, 0, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 1, 11, 2, 1, 9, 11, 9, 8, 11, -1, -1, -1, -1, -1, -1, -1],
            [
                6, 3, 10, 1, 11, 10, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 0, 10, 1, 0, 8, 10, 8, 11, 10, -1, -1, -1, -1, -1, -1, -1],
            [9, 3, 9, 0, 3, 11, 9, 11, 10, 9, -1, -1, -1, -1, -1, -1, -1],
            [
                6, 9, 8, 10, 10, 8, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                3, 4, 7, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 4, 3, 0, 7, 3, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [6, 0, 1, 9, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 4, 1, 9, 4, 7, 1, 7, 3, 1, -1, -1, -1, -1, -1, -1, -1],
            [6, 1, 2, 10, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 3, 4, 7, 3, 0, 4, 1, 2, 10, -1, -1, -1, -1, -1, -1, -1],
            [9, 9, 2, 10, 9, 0, 2, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1],
            [12, 2, 10, 9, 2, 9, 7, 2, 7, 3, 7, 9, 4, -1, -1, -1, -1],
            [6, 8, 4, 7, 3, 11, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 11, 4, 7, 11, 2, 4, 2, 0, 4, -1, -1, -1, -1, -1, -1, -1],
            [9, 9, 0, 1, 8, 4, 7, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1],
            [12, 4, 7, 11, 9, 4, 11, 9, 11, 2, 9, 2, 1, -1, -1, -1, -1],
            [9, 3, 10, 1, 3, 11, 10, 7, 8, 4, -1, -1, -1, -1, -1, -1, -1],
            [12, 1, 11, 10, 1, 4, 11, 1, 0, 4, 7, 11, 4, -1, -1, -1, -1],
            [12, 4, 7, 8, 9, 0, 11, 9, 11, 10, 11, 0, 3, -1, -1, -1, -1],
            [9, 4, 7, 11, 4, 11, 9, 9, 11, 10, -1, -1, -1, -1, -1, -1, -1],
            [
                3, 9, 5, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 9, 5, 4, 0, 8, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [6, 0, 5, 4, 1, 5, 0, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 8, 5, 4, 8, 3, 5, 3, 1, 5, -1, -1, -1, -1, -1, -1, -1],
            [6, 1, 2, 10, 9, 5, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 3, 0, 8, 1, 2, 10, 4, 9, 5, -1, -1, -1, -1, -1, -1, -1],
            [9, 5, 2, 10, 5, 4, 2, 4, 0, 2, -1, -1, -1, -1, -1, -1, -1],
            [12, 2, 10, 5, 3, 2, 5, 3, 5, 4, 3, 4, 8, -1, -1, -1, -1],
            [6, 9, 5, 4, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 0, 11, 2, 0, 8, 11, 4, 9, 5, -1, -1, -1, -1, -1, -1, -1],
            [9, 0, 5, 4, 0, 1, 5, 2, 3, 11, -1, -1, -1, -1, -1, -1, -1],
            [12, 2, 1, 5, 2, 5, 8, 2, 8, 11, 4, 8, 5, -1, -1, -1, -1],
            [9, 10, 3, 11, 10, 1, 3, 9, 5, 4, -1, -1, -1, -1, -1, -1, -1],
            [12, 4, 9, 5, 0, 8, 1, 8, 10, 1, 8, 11, 10, -1, -1, -1, -1],
            [12, 5, 4, 0, 5, 0, 11, 5, 11, 10, 11, 0, 3, -1, -1, -1, -1],
            [9, 5, 4, 8, 5, 8, 10, 10, 8, 11, -1, -1, -1, -1, -1, -1, -1],
            [6, 9, 7, 8, 5, 7, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 9, 3, 0, 9, 5, 3, 5, 7, 3, -1, -1, -1, -1, -1, -1, -1],
            [9, 0, 7, 8, 0, 1, 7, 1, 5, 7, -1, -1, -1, -1, -1, -1, -1],
            [6, 1, 5, 3, 3, 5, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 9, 7, 8, 9, 5, 7, 10, 1, 2, -1, -1, -1, -1, -1, -1, -1],
            [12, 10, 1, 2, 9, 5, 0, 5, 3, 0, 5, 7, 3, -1, -1, -1, -1],
            [12, 8, 0, 2, 8, 2, 5, 8, 5, 7, 10, 5, 2, -1, -1, -1, -1],
            [9, 2, 10, 5, 2, 5, 3, 3, 5, 7, -1, -1, -1, -1, -1, -1, -1],
            [9, 7, 9, 5, 7, 8, 9, 3, 11, 2, -1, -1, -1, -1, -1, -1, -1],
            [12, 9, 5, 7, 9, 7, 2, 9, 2, 0, 2, 7, 11, -1, -1, -1, -1],
            [12, 2, 3, 11, 0, 1, 8, 1, 7, 8, 1, 5, 7, -1, -1, -1, -1],
            [9, 11, 2, 1, 11, 1, 7, 7, 1, 5, -1, -1, -1, -1, -1, -1, -1],
            [12, 9, 5, 8, 8, 5, 7, 10, 1, 3, 10, 3, 11, -1, -1, -1, -1],
            [15, 5, 7, 0, 5, 0, 9, 7, 11, 0, 1, 0, 10, 11, 10, 0, -1],
            [15, 11, 10, 0, 11, 0, 3, 10, 5, 0, 8, 0, 7, 5, 7, 0, -1],
            [
                6, 11, 10, 5, 7, 11, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                3, 10, 6, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 0, 8, 3, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [6, 9, 0, 1, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 1, 8, 3, 1, 9, 8, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1],
            [6, 1, 6, 5, 2, 6, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 1, 6, 5, 1, 2, 6, 3, 0, 8, -1, -1, -1, -1, -1, -1, -1],
            [9, 9, 6, 5, 9, 0, 6, 0, 2, 6, -1, -1, -1, -1, -1, -1, -1],
            [12, 5, 9, 8, 5, 8, 2, 5, 2, 6, 3, 2, 8, -1, -1, -1, -1],
            [
                6, 2, 3, 11, 10, 6, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 11, 0, 8, 11, 2, 0, 10, 6, 5, -1, -1, -1, -1, -1, -1, -1],
            [9, 0, 1, 9, 2, 3, 11, 5, 10, 6, -1, -1, -1, -1, -1, -1, -1],
            [12, 5, 10, 6, 1, 9, 2, 9, 11, 2, 9, 8, 11, -1, -1, -1, -1],
            [9, 6, 3, 11, 6, 5, 3, 5, 1, 3, -1, -1, -1, -1, -1, -1, -1],
            [12, 0, 8, 11, 0, 11, 5, 0, 5, 1, 5, 11, 6, -1, -1, -1, -1],
            [12, 3, 11, 6, 0, 3, 6, 0, 6, 5, 0, 5, 9, -1, -1, -1, -1],
            [9, 6, 5, 9, 6, 9, 11, 11, 9, 8, -1, -1, -1, -1, -1, -1, -1],
            [6, 5, 10, 6, 4, 7, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 4, 3, 0, 4, 7, 3, 6, 5, 10, -1, -1, -1, -1, -1, -1, -1],
            [9, 1, 9, 0, 5, 10, 6, 8, 4, 7, -1, -1, -1, -1, -1, -1, -1],
            [12, 10, 6, 5, 1, 9, 7, 1, 7, 3, 7, 9, 4, -1, -1, -1, -1],
            [9, 6, 1, 2, 6, 5, 1, 4, 7, 8, -1, -1, -1, -1, -1, -1, -1],
            [12, 1, 2, 5, 5, 2, 6, 3, 0, 4, 3, 4, 7, -1, -1, -1, -1],
            [12, 8, 4, 7, 9, 0, 5, 0, 6, 5, 0, 2, 6, -1, -1, -1, -1],
            [15, 7, 3, 9, 7, 9, 4, 3, 2, 9, 5, 9, 6, 2, 6, 9, -1],
            [9, 3, 11, 2, 7, 8, 4, 10, 6, 5, -1, -1, -1, -1, -1, -1, -1],
            [12, 5, 10, 6, 4, 7, 2, 4, 2, 0, 2, 7, 11, -1, -1, -1, -1],
            [12, 0, 1, 9, 4, 7, 8, 2, 3, 11, 5, 10, 6, -1, -1, -1, -1],
            [15, 9, 2, 1, 9, 11, 2, 9, 4, 11, 7, 11, 4, 5, 10, 6, -1],
            [12, 8, 4, 7, 3, 11, 5, 3, 5, 1, 5, 11, 6, -1, -1, -1, -1],
            [15, 5, 1, 11, 5, 11, 6, 1, 0, 11, 7, 11, 4, 0, 4, 11, -1],
            [15, 0, 5, 9, 0, 6, 5, 0, 3, 6, 11, 6, 3, 8, 4, 7, -1],
            [12, 6, 5, 9, 6, 9, 11, 4, 7, 9, 7, 11, 9, -1, -1, -1, -1],
            [
                6, 10, 4, 9, 6, 4, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 4, 10, 6, 4, 9, 10, 0, 8, 3, -1, -1, -1, -1, -1, -1, -1],
            [9, 10, 0, 1, 10, 6, 0, 6, 4, 0, -1, -1, -1, -1, -1, -1, -1],
            [12, 8, 3, 1, 8, 1, 6, 8, 6, 4, 6, 1, 10, -1, -1, -1, -1],
            [9, 1, 4, 9, 1, 2, 4, 2, 6, 4, -1, -1, -1, -1, -1, -1, -1],
            [12, 3, 0, 8, 1, 2, 9, 2, 4, 9, 2, 6, 4, -1, -1, -1, -1],
            [6, 0, 2, 4, 4, 2, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 8, 3, 2, 8, 2, 4, 4, 2, 6, -1, -1, -1, -1, -1, -1, -1],
            [9, 10, 4, 9, 10, 6, 4, 11, 2, 3, -1, -1, -1, -1, -1, -1, -1],
            [12, 0, 8, 2, 2, 8, 11, 4, 9, 10, 4, 10, 6, -1, -1, -1, -1],
            [12, 3, 11, 2, 0, 1, 6, 0, 6, 4, 6, 1, 10, -1, -1, -1, -1],
            [15, 6, 4, 1, 6, 1, 10, 4, 8, 1, 2, 1, 11, 8, 11, 1, -1],
            [12, 9, 6, 4, 9, 3, 6, 9, 1, 3, 11, 6, 3, -1, -1, -1, -1],
            [15, 8, 11, 1, 8, 1, 0, 11, 6, 1, 9, 1, 4, 6, 4, 1, -1],
            [9, 3, 11, 6, 3, 6, 0, 0, 6, 4, -1, -1, -1, -1, -1, -1, -1],
            [6, 6, 4, 8, 11, 6, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 7, 10, 6, 7, 8, 10, 8, 9, 10, -1, -1, -1, -1, -1, -1, -1],
            [12, 0, 7, 3, 0, 10, 7, 0, 9, 10, 6, 7, 10, -1, -1, -1, -1],
            [12, 10, 6, 7, 1, 10, 7, 1, 7, 8, 1, 8, 0, -1, -1, -1, -1],
            [9, 10, 6, 7, 10, 7, 1, 1, 7, 3, -1, -1, -1, -1, -1, -1, -1],
            [12, 1, 2, 6, 1, 6, 8, 1, 8, 9, 8, 6, 7, -1, -1, -1, -1],
            [15, 2, 6, 9, 2, 9, 1, 6, 7, 9, 0, 9, 3, 7, 3, 9, -1],
            [9, 7, 8, 0, 7, 0, 6, 6, 0, 2, -1, -1, -1, -1, -1, -1, -1],
            [6, 7, 3, 2, 6, 7, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [12, 2, 3, 11, 10, 6, 8, 10, 8, 9, 8, 6, 7, -1, -1, -1, -1],
            [15, 2, 0, 7, 2, 7, 11, 0, 9, 7, 6, 7, 10, 9, 10, 7, -1],
            [15, 1, 8, 0, 1, 7, 8, 1, 10, 7, 6, 7, 10, 2, 3, 11, -1],
            [12, 11, 2, 1, 11, 1, 7, 10, 6, 1, 6, 7, 1, -1, -1, -1, -1],
            [15, 8, 9, 6, 8, 6, 7, 9, 1, 6, 11, 6, 3, 1, 3, 6, -1],
            [6, 0, 9, 1, 11, 6, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [12, 7, 8, 0, 7, 0, 6, 3, 11, 0, 11, 6, 0, -1, -1, -1, -1],
            [
                3, 7, 11, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                3, 7, 6, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 3, 0, 8, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [6, 0, 1, 9, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 8, 1, 9, 8, 3, 1, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1],
            [
                6, 10, 1, 2, 6, 11, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 1, 2, 10, 3, 0, 8, 6, 11, 7, -1, -1, -1, -1, -1, -1, -1],
            [9, 2, 9, 0, 2, 10, 9, 6, 11, 7, -1, -1, -1, -1, -1, -1, -1],
            [12, 6, 11, 7, 2, 10, 3, 10, 8, 3, 10, 9, 8, -1, -1, -1, -1],
            [6, 7, 2, 3, 6, 2, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 7, 0, 8, 7, 6, 0, 6, 2, 0, -1, -1, -1, -1, -1, -1, -1],
            [9, 2, 7, 6, 2, 3, 7, 0, 1, 9, -1, -1, -1, -1, -1, -1, -1],
            [12, 1, 6, 2, 1, 8, 6, 1, 9, 8, 8, 7, 6, -1, -1, -1, -1],
            [9, 10, 7, 6, 10, 1, 7, 1, 3, 7, -1, -1, -1, -1, -1, -1, -1],
            [12, 10, 7, 6, 1, 7, 10, 1, 8, 7, 1, 0, 8, -1, -1, -1, -1],
            [12, 0, 3, 7, 0, 7, 10, 0, 10, 9, 6, 10, 7, -1, -1, -1, -1],
            [9, 7, 6, 10, 7, 10, 8, 8, 10, 9, -1, -1, -1, -1, -1, -1, -1],
            [6, 6, 8, 4, 11, 8, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 3, 6, 11, 3, 0, 6, 0, 4, 6, -1, -1, -1, -1, -1, -1, -1],
            [9, 8, 6, 11, 8, 4, 6, 9, 0, 1, -1, -1, -1, -1, -1, -1, -1],
            [12, 9, 4, 6, 9, 6, 3, 9, 3, 1, 11, 3, 6, -1, -1, -1, -1],
            [9, 6, 8, 4, 6, 11, 8, 2, 10, 1, -1, -1, -1, -1, -1, -1, -1],
            [12, 1, 2, 10, 3, 0, 11, 0, 6, 11, 0, 4, 6, -1, -1, -1, -1],
            [12, 4, 11, 8, 4, 6, 11, 0, 2, 9, 2, 10, 9, -1, -1, -1, -1],
            [15, 10, 9, 3, 10, 3, 2, 9, 4, 3, 11, 3, 6, 4, 6, 3, -1],
            [9, 8, 2, 3, 8, 4, 2, 4, 6, 2, -1, -1, -1, -1, -1, -1, -1],
            [6, 0, 4, 2, 4, 6, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [12, 1, 9, 0, 2, 3, 4, 2, 4, 6, 4, 3, 8, -1, -1, -1, -1],
            [9, 1, 9, 4, 1, 4, 2, 2, 4, 6, -1, -1, -1, -1, -1, -1, -1],
            [9, 8, 1, 3, 8, 6, 1, 8, 4, 6, 6, 10, 1, -1, -1, -1, -1],
            [9, 10, 1, 0, 10, 0, 6, 6, 0, 4, -1, -1, -1, -1, -1, -1, -1],
            [15, 4, 6, 3, 4, 3, 8, 6, 10, 3, 0, 3, 9, 10, 9, 3, -1],
            [
                6, 10, 9, 4, 6, 10, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 4, 9, 5, 7, 6, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 0, 8, 3, 4, 9, 5, 11, 7, 6, -1, -1, -1, -1, -1, -1, -1],
            [9, 5, 0, 1, 5, 4, 0, 7, 6, 11, -1, -1, -1, -1, -1, -1, -1],
            [12, 11, 7, 6, 8, 3, 4, 3, 5, 4, 3, 1, 5, -1, -1, -1, -1],
            [9, 9, 5, 4, 10, 1, 2, 7, 6, 11, -1, -1, -1, -1, -1, -1, -1],
            [12, 6, 11, 7, 1, 2, 10, 0, 8, 3, 4, 9, 5, -1, -1, -1, -1],
            [12, 7, 6, 11, 5, 4, 10, 4, 2, 10, 4, 0, 2, -1, -1, -1, -1],
            [15, 3, 4, 8, 3, 5, 4, 3, 2, 5, 10, 5, 2, 11, 7, 6, -1],
            [9, 7, 2, 3, 7, 6, 2, 5, 4, 9, -1, -1, -1, -1, -1, -1, -1],
            [12, 9, 5, 4, 0, 8, 6, 0, 6, 2, 6, 8, 7, -1, -1, -1, -1],
            [12, 3, 6, 2, 3, 7, 6, 1, 5, 0, 5, 4, 0, -1, -1, -1, -1],
            [15, 6, 2, 8, 6, 8, 7, 2, 1, 8, 4, 8, 5, 1, 5, 8, -1],
            [12, 9, 5, 4, 10, 1, 6, 1, 7, 6, 1, 3, 7, -1, -1, -1, -1],
            [15, 1, 6, 10, 1, 7, 6, 1, 0, 7, 8, 7, 0, 9, 5, 4, -1],
            [15, 4, 0, 10, 4, 10, 5, 0, 3, 10, 6, 10, 7, 3, 7, 10, -1],
            [12, 7, 6, 10, 7, 10, 8, 5, 4, 10, 4, 8, 10, -1, -1, -1, -1],
            [9, 6, 9, 5, 6, 11, 9, 11, 8, 9, -1, -1, -1, -1, -1, -1, -1],
            [12, 3, 6, 11, 0, 6, 3, 0, 5, 6, 0, 9, 5, -1, -1, -1, -1],
            [12, 0, 11, 8, 0, 5, 11, 0, 1, 5, 5, 6, 11, -1, -1, -1, -1],
            [9, 6, 11, 3, 6, 3, 5, 5, 3, 1, -1, -1, -1, -1, -1, -1, -1],
            [12, 1, 2, 10, 9, 5, 11, 9, 11, 8, 11, 5, 6, -1, -1, -1, -1],
            [15, 0, 11, 3, 0, 6, 11, 0, 9, 6, 5, 6, 9, 1, 2, 10, -1],
            [15, 11, 8, 5, 11, 5, 6, 8, 0, 5, 10, 5, 2, 0, 2, 5, -1],
            [12, 6, 11, 3, 6, 3, 5, 2, 10, 3, 10, 5, 3, -1, -1, -1, -1],
            [12, 5, 8, 9, 5, 2, 8, 5, 6, 2, 3, 8, 2, -1, -1, -1, -1],
            [9, 9, 5, 6, 9, 6, 0, 0, 6, 2, -1, -1, -1, -1, -1, -1, -1],
            [15, 1, 5, 8, 1, 8, 0, 5, 6, 8, 3, 8, 2, 6, 2, 8, -1],
            [6, 1, 5, 6, 2, 1, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [15, 1, 3, 6, 1, 6, 10, 3, 8, 6, 5, 6, 9, 8, 9, 6, -1],
            [12, 10, 1, 0, 10, 0, 6, 9, 5, 0, 5, 6, 0, -1, -1, -1, -1],
            [6, 0, 3, 8, 5, 6, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [
                3, 10, 5, 6, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                6, 11, 5, 10, 7, 5, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 11, 5, 10, 11, 7, 5, 8, 3, 0, -1, -1, -1, -1, -1, -1, -1],
            [9, 5, 11, 7, 5, 10, 11, 1, 9, 0, -1, -1, -1, -1, -1, -1, -1],
            [12, 10, 7, 5, 10, 11, 7, 9, 8, 1, 8, 3, 1, -1, -1, -1, -1],
            [9, 11, 1, 2, 11, 7, 1, 7, 5, 1, -1, -1, -1, -1, -1, -1, -1],
            [12, 0, 8, 3, 1, 2, 7, 1, 7, 5, 7, 2, 11, -1, -1, -1, -1],
            [12, 9, 7, 5, 9, 2, 7, 9, 0, 2, 2, 11, 7, -1, -1, -1, -1],
            [15, 7, 5, 2, 7, 2, 11, 5, 9, 2, 3, 2, 8, 9, 8, 2, -1],
            [9, 2, 5, 10, 2, 3, 5, 3, 7, 5, -1, -1, -1, -1, -1, -1, -1],
            [12, 8, 2, 0, 8, 5, 2, 8, 7, 5, 10, 2, 5, -1, -1, -1, -1],
            [12, 9, 0, 1, 5, 10, 3, 5, 3, 7, 3, 10, 2, -1, -1, -1, -1],
            [15, 9, 8, 2, 9, 2, 1, 8, 7, 2, 10, 2, 5, 7, 5, 2, -1],
            [6, 1, 3, 5, 3, 7, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 0, 8, 7, 0, 7, 1, 1, 7, 5, -1, -1, -1, -1, -1, -1, -1],
            [9, 9, 0, 3, 9, 3, 5, 5, 3, 7, -1, -1, -1, -1, -1, -1, -1],
            [6, 9, 8, 7, 5, 9, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 5, 8, 4, 5, 10, 8, 10, 11, 8, -1, -1, -1, -1, -1, -1, -1],
            [12, 5, 0, 4, 5, 11, 0, 5, 10, 11, 11, 3, 0, -1, -1, -1, -1],
            [12, 0, 1, 9, 8, 4, 10, 8, 10, 11, 10, 4, 5, -1, -1, -1, -1],
            [15, 10, 11, 4, 10, 4, 5, 11, 3, 4, 9, 4, 1, 3, 1, 4, -1],
            [12, 2, 5, 1, 2, 8, 5, 2, 11, 8, 4, 5, 8, -1, -1, -1, -1],
            [15, 0, 4, 11, 0, 11, 3, 4, 5, 11, 2, 11, 1, 5, 1, 11, -1],
            [15, 0, 2, 5, 0, 5, 9, 2, 11, 5, 4, 5, 8, 11, 8, 5, -1],
            [6, 9, 4, 5, 2, 11, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [12, 2, 5, 10, 3, 5, 2, 3, 4, 5, 3, 8, 4, -1, -1, -1, -1],
            [9, 5, 10, 2, 5, 2, 4, 4, 2, 0, -1, -1, -1, -1, -1, -1, -1],
            [15, 3, 10, 2, 3, 5, 10, 3, 8, 5, 4, 5, 8, 0, 1, 9, -1],
            [12, 5, 10, 2, 5, 2, 4, 1, 9, 2, 9, 4, 2, -1, -1, -1, -1],
            [9, 8, 4, 5, 8, 5, 3, 3, 5, 1, -1, -1, -1, -1, -1, -1, -1],
            [6, 0, 4, 5, 1, 0, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [12, 8, 4, 5, 8, 5, 3, 9, 0, 5, 0, 3, 5, -1, -1, -1, -1],
            [
                3, 9, 4, 5, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 4, 11, 7, 4, 9, 11, 9, 10, 11, -1, -1, -1, -1, -1, -1, -1],
            [12, 0, 8, 3, 4, 9, 7, 9, 11, 7, 9, 10, 11, -1, -1, -1, -1],
            [12, 1, 10, 11, 1, 11, 4, 1, 4, 0, 7, 4, 11, -1, -1, -1, -1],
            [15, 3, 1, 4, 3, 4, 8, 1, 10, 4, 7, 4, 11, 10, 11, 4, -1],
            [12, 4, 11, 7, 9, 11, 4, 9, 2, 11, 9, 1, 2, -1, -1, -1, -1],
            [15, 9, 7, 4, 9, 11, 7, 9, 1, 11, 2, 11, 1, 0, 8, 3, -1],
            [9, 11, 7, 4, 11, 4, 2, 2, 4, 0, -1, -1, -1, -1, -1, -1, -1],
            [12, 11, 7, 4, 11, 4, 2, 8, 3, 4, 3, 2, 4, -1, -1, -1, -1],
            [12, 2, 9, 10, 2, 7, 9, 2, 3, 7, 7, 4, 9, -1, -1, -1, -1],
            [15, 9, 10, 7, 9, 7, 4, 10, 2, 7, 8, 7, 0, 2, 0, 7, -1],
            [15, 3, 7, 10, 3, 10, 2, 7, 4, 10, 1, 10, 0, 4, 0, 10, -1],
            [6, 1, 10, 2, 8, 7, 4, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [9, 4, 9, 1, 4, 1, 7, 7, 1, 3, -1, -1, -1, -1, -1, -1, -1],
            [12, 4, 9, 1, 4, 1, 7, 0, 8, 1, 8, 7, 1, -1, -1, -1, -1],
            [6, 4, 0, 3, 7, 4, 3, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [
                3, 4, 8, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                6, 9, 10, 8, 10, 11, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 3, 0, 9, 3, 9, 11, 11, 9, 10, -1, -1, -1, -1, -1, -1, -1],
            [9, 0, 1, 10, 0, 10, 8, 8, 10, 11, -1, -1, -1, -1, -1, -1, -1],
            [
                6, 3, 1, 10, 11, 3, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 1, 2, 11, 1, 11, 9, 9, 11, 8, -1, -1, -1, -1, -1, -1, -1],
            [12, 3, 0, 9, 3, 9, 11, 1, 2, 9, 2, 11, 9, -1, -1, -1, -1],
            [
                6, 0, 2, 11, 8, 0, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                3, 3, 2, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [9, 2, 3, 8, 2, 8, 10, 10, 8, 9, -1, -1, -1, -1, -1, -1, -1],
            [6, 9, 10, 2, 0, 9, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [12, 2, 3, 8, 2, 8, 10, 0, 1, 8, 1, 10, 8, -1, -1, -1, -1],
            [
                3, 1, 10, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [6, 1, 3, 8, 9, 1, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1],
            [
                3, 0, 9, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                3, 0, 3, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
            [
                0, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            ],
        ];

        let edges_buffer = device
            .create_buffer(
                &BufferInfo {
                    size: std::mem::size_of_val(&EDGE_TABLE) as u64,
                    usage: BufferUsage::STORAGE | BufferUsage::CONSTANT | BufferUsage::INITIAL_COPY,
                    sharing_mode: QueueSharingMode::Exclusive,
                },
                MemoryUsage::GPUMemory,
                Some("MachingCubesEdges"),
            )
            .unwrap();

        let tris_buffer = device
            .create_buffer(
                &BufferInfo {
                    size: std::mem::size_of_val(&TRI_TABLE) as u64,
                    usage: BufferUsage::STORAGE | BufferUsage::CONSTANT | BufferUsage::INITIAL_COPY,
                    sharing_mode: QueueSharingMode::Exclusive,
                },
                MemoryUsage::GPUMemory,
                Some("MachingCubesTris"),
            )
            .unwrap();

        device
            .init_buffer(&EDGE_TABLE, &edges_buffer, 0u64)
            .unwrap();
        device.init_buffer(&TRI_TABLE, &tris_buffer, 0u64).unwrap();

        device.flush_transfers();

        Self {
            pipelines: pipelines.as_array().unwrap().clone(),
            edges_buffer,
            tris_buffer,
            executed_count: 0u32,
        }
    }

    fn buffer_key(
        texture_handle: TextureHandle,
        lod: u32,
        min_threshold: f32,
        max_threshold: f32,
    ) -> String {
        format!(
            "{:?}_{}_{:.3}_{:.3}",
            texture_handle, lod, min_threshold, max_threshold,
        )
    }

    fn create_buffers(resources: &mut RendererResources, name: &str) {
        // The texture might not be loaded yet, it might not be loaded yet.
        let mut resolution_multiplied = 512 * 512 * 512;

        // Assume there's a lot of empty space/fully filled space and voxels with fewer triangles
        // to avoid huge buffers.
        // In case of the manix, the theoretical space is 148x larger than the actually necessary one.
        resolution_multiplied /= 100;

        // The theoretical maximum is that every voxel adds 5 triangles, so 15 indices.
        resources.create_buffer(
            name,
            &BufferInfo {
                size: (std::mem::size_of::<u32>() * 15 * resolution_multiplied) as u64,
                usage: BufferUsage::STORAGE | BufferUsage::INDEX,
                sharing_mode: QueueSharingMode::Exclusive,
            },
            MemoryUsage::GPUMemory,
            false,
        );
    }

    #[inline(always)]
    pub(in crate::renderer::passes) fn is_ready(
        &self,
        assets: &RendererAssetsReadOnly<'_>,
    ) -> bool {
        for &pipeline in &self.pipelines {
            if !assets.get_compute_pipeline(pipeline).is_some() {
                return false;
            }
        }
        true
    }

    pub fn execute(
        &mut self,
        command_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
    ) -> HashMap<MarchingCubesKey, MarchingCubesInfo> {
        let mut map = HashMap::<MarchingCubesKey, MarchingCubesInfo>::new();
        let mut textures_with_lod: SmallVec<[(TextureHandle, u32); 2]> = SmallVec::new();

        let mut indirect_buffer_offset = 0;
        for d in pass_params.scene.scene.volume_mesh_instances() {
            if !textures_with_lod
                .iter()
                .any(|&e| e == (d.volume_texture, d.texture_lod))
            {
                textures_with_lod.push((d.volume_texture, d.texture_lod));
            }

            let key = MarchingCubesKey::new(
                d.volume_texture,
                d.texture_lod,
                d.min_threshold,
                d.max_threshold,
            );
            let buffer_name = key.buffer_name();
            Self::create_buffers(pass_params.resources, &buffer_name);
            map.insert(
                key,
                MarchingCubesInfo {
                    buffer_name,
                    indirect_buffer_offset,
                },
            );
            indirect_buffer_offset += std::mem::size_of::<MarchingCubesIndirectCall>();
        }

        let resources = &pass_params.resources;
        if self.executed_count > 0u32 {
            //return;
        }
        self.executed_count += 1u32;

        command_buffer.begin_label("Marching Cubes pass");

        // Access all of them here to batch barriers
        let mut buffer_slices: SmallVec<[Ref<Arc<BufferSlice>>; 2]> =
            SmallVec::with_capacity(pass_params.scene.scene.volume_mesh_instances().len());
        for d in pass_params.scene.scene.volume_mesh_instances() {
            let key = MarchingCubesKey::new(
                d.volume_texture,
                d.texture_lod,
                d.min_threshold,
                d.max_threshold,
            );
            let entry = map.get(&key).unwrap();

            buffer_slices.push(pass_params.resources.access_buffer(
                command_buffer,
                &entry.buffer_name,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_WRITE,
                HistoryResourceEntry::Current,
            ));
        }

        let atomics_slice = pass_params.resources.access_buffer(
            command_buffer,
            Self::ATOMICS_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            HistoryResourceEntry::Current,
        );
        command_buffer.flush_barriers();
        command_buffer.clear_storage_buffer(
            BufferRef::Regular(&atomics_slice),
            0u64,
            pass_params
                .resources
                .buffer_range(Self::ATOMICS_BUFFER_NAME)
                .length
                / 4,
            0u32,
        );

        for slice in buffer_slices.iter() {
            command_buffer.clear_storage_buffer(
                BufferRef::Regular(&slice),
                0u64,
                slice.length() / 4,
                0u32,
            );
        }
        buffer_slices.clear();

        let atomics_slice = pass_params.resources.access_buffer(
            command_buffer,
            Self::ATOMICS_BUFFER_NAME,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE | BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );
        for d in pass_params.scene.scene.volume_mesh_instances() {
            let key = MarchingCubesKey::new(
                d.volume_texture,
                d.texture_lod,
                d.min_threshold,
                d.max_threshold,
            );
            let entry = map.get(&key).unwrap();

            buffer_slices.push(pass_params.resources.access_buffer(
                command_buffer,
                &entry.buffer_name,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_WRITE,
                HistoryResourceEntry::Current,
            ));
        }

        command_buffer.flush_barriers();
        std::mem::drop(buffer_slices);

        for (texture, lod) in textures_with_lod {
            let mut buffer_slices = SmallVec::<[Ref<Arc<BufferSlice>>; 4]>::new();
            let mut thresholds = SmallVec::<[MarchingCubesThresholds; 4]>::new();
            let mut min_atomics_offset = usize::MAX;
            for (index, d) in pass_params
                .scene
                .scene
                .volume_mesh_instances()
                .iter()
                .filter(|d| d.volume_texture == texture && d.texture_lod == lod)
                .enumerate()
            {
                let key = MarchingCubesKey::new(texture, lod, d.min_threshold, d.max_threshold);
                let map_entry = map.get(&key).unwrap();
                assert_eq!(
                    index * std::mem::size_of::<MarchingCubesIndirectCall>(),
                    map_entry.indirect_buffer_offset
                );
                min_atomics_offset = min_atomics_offset.min(map_entry.indirect_buffer_offset);

                let slice = pass_params.resources.access_buffer(
                    command_buffer,
                    &map_entry.buffer_name,
                    BarrierSync::COMPUTE_SHADER,
                    BarrierAccess::STORAGE_WRITE,
                    HistoryResourceEntry::Current,
                );
                buffer_slices.push(slice);
                thresholds.push(MarchingCubesThresholds {
                    min_threshold: d.min_threshold,
                    max_threshold: d.max_threshold,
                });
            }
            assert_ne!(min_atomics_offset, usize::MAX);
            assert!(buffer_slices.len() <= 16);
            assert_ne!(buffer_slices.len(), 0);

            command_buffer.begin_label(&format!(
                "Marching Cube Texture: {:?} with lod: {}",
                texture, lod
            ));

            let entries: SmallVec<[BufferArrayEntry; 2]> = buffer_slices
                .iter()
                .map(|slice| BufferArrayEntry {
                    buffer: BufferRef::Regular(slice),
                    offset: 0,
                    length: WHOLE_BUFFER,
                })
                .collect();

            let pipeline_index = if entries.len() < self.pipelines.len() {
                entries.len()
            } else {
                0 // Spec const value 0 makes it use the dynamic value
            };
            let pipeline = pass_params
                .assets
                .get_compute_pipeline(self.pipelines[pipeline_index])
                .unwrap();
            command_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));
            command_buffer.bind_storage_buffer(
                BindingFrequency::Frequent,
                0,
                BufferRef::Regular(&self.edges_buffer),
                0,
                WHOLE_BUFFER,
            );
            command_buffer.bind_storage_buffer(
                BindingFrequency::Frequent,
                1,
                BufferRef::Regular(&self.tris_buffer),
                0,
                WHOLE_BUFFER,
            );

            command_buffer.bind_sampler(BindingFrequency::Frequent, 6, resources.linear_sampler());
            command_buffer.bind_sampler(BindingFrequency::Frequent, 7, resources.nearest_sampler());

            command_buffer.bind_storage_buffer_array(BindingFrequency::Frequent, 3, &entries);

            let volume_texture = pass_params.assets.get_texture(texture);
            command_buffer.bind_sampling_view(BindingFrequency::Frequent, 2, &volume_texture.view);

            let mut extent = Vec3UI::new(512, 512, 512);
            let texture_view = &volume_texture.view;
            if let Some(texture) = texture_view.texture().as_ref() {
                let texture_info = texture.info();
                extent.x = texture_info.width >> lod;
                extent.y = texture_info.height >> lod;
                extent.z = texture_info.depth >> lod;
            }

            let thresholds_buffer = command_buffer
                .upload_dynamic_data(&thresholds, BufferUsage::CONSTANT)
                .unwrap();

            command_buffer.bind_uniform_buffer(
                BindingFrequency::Frequent,
                4,
                BufferRef::Transient(&thresholds_buffer),
                0,
                WHOLE_BUFFER,
            );

            command_buffer.set_push_constant_data(
                &[MarchingCubesConfig {
                    lod,
                    min: Vec3UI::new(0, 0, 0),
                    extent,
                    thresholds_count: thresholds.len() as u32,
                }],
                ShaderType::ComputeShader,
            );

            command_buffer.bind_storage_buffer(
                BindingFrequency::Frequent,
                5,
                BufferRef::Regular(&atomics_slice),
                min_atomics_offset as u64,
                WHOLE_BUFFER,
            );

            command_buffer.finish_binding();

            if extent.x > 0 && extent.y > 0 && extent.z > 0 {
                command_buffer.dispatch(
                    (extent.x + 3u32) / 4u32,
                    (extent.y + 3u32) / 4u32,
                    (extent.z + 3u32) / 4u32,
                );
            }

            command_buffer.end_label();
        }

        command_buffer.end_label();
        map
    }
}
