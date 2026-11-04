use crate::asset::{AssetHandle, TextureHandle};
use crate::graphics::*;
use crate::renderer::asset::{ComputePipelinePermutations, PathPipelineShaderStage, RendererAssets, RendererAssetsReadOnly};
use crate::renderer::drawable::RendererVolumeDrawable;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{BufferAccessor, HistoryResourceEntry, RendererResources};
use bevy_ecs::entity::Entity;
use bytemuck::{Pod, Zeroable};
use smallvec::{smallvec, SmallVec};
use sourcerenderer_core::{Vec3UI, Vec4UI};
use sourcerenderer_core::gpu::SpecConstValue;
use std::collections::HashMap;
use std::sync::Arc;
use bevy_math::UVec3;
use crate::renderer::VolumeRendererOptions;

// TODO:
//  - Move culling to separate file

pub const MAX_SIZE: u32 = 512;

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct MarchingCubesConfig {
    pub extent: Vec3UI,
    pub lod: u32,
    pub thresholds_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct MarchingCubesCullingConfig {
    pub extent: Vec3UI,
    pub lod: u32,
    pub thresholds_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct IndirectCommand {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct MarchingCubesIndirectCall {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    vertex_offset: i32,
    first_instance: u32,
    _pad_to_16_0: u32,
    _pad_to_16_1: u32,
    _pad_to_16_2: u32,
}

const WORK_GROUP_SIZE: UVec3 = UVec3::new(4, 4, 4);
const MESH_WORK_GROUP_SIZE: UVec3 = UVec3::new(4, 4, 2);

pub struct MarchingCubesPass {
    pipelines: ComputePipelinePermutations,
    culling_pipelines: ComputePipelinePermutations,
    edges_buffer: Arc<BufferSlice>,
    tris_buffer: Arc<BufferSlice>,
}

#[derive(PartialEq)]
struct Dispatch {
    handle: TextureHandle,
    lod: u32,
    as_cubes: bool
}

struct Batch<'a> {
    dispatch: Dispatch,
    drawables: SmallVec<[&'a RendererVolumeDrawable; 2]>,
}

impl MarchingCubesPass {
    #[allow(unused)]
    pub fn new(
        device: &Device,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        let shader_path = crate::renderer::shader_path!("marching_cubes.comp");

        // Compile optimized pipelines for 1-3 thresholds
        let pipelines = ComputePipelinePermutations::request(assets, &PathPipelineShaderStage {
            shader_path,
            spec_consts: &[]
        }, &[
            [SpecConstValue::UInt(0), SpecConstValue::Bool(false), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(1), SpecConstValue::Bool(false), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(2), SpecConstValue::Bool(false), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(3), SpecConstValue::Bool(false), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(0), SpecConstValue::Bool(true),  SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(1), SpecConstValue::Bool(true),  SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(2), SpecConstValue::Bool(true),  SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(3), SpecConstValue::Bool(true),  SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(0), SpecConstValue::Bool(true),  SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(1), SpecConstValue::Bool(true),  SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(2), SpecConstValue::Bool(true),  SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(3), SpecConstValue::Bool(true),  SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(0), SpecConstValue::Bool(false), SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(1), SpecConstValue::Bool(false), SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(2), SpecConstValue::Bool(false), SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(3), SpecConstValue::Bool(false), SpecConstValue::Bool(true),  SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
        ]);

        let culling_shader_path = crate::renderer::shader_path!("marching_cubes_precull.comp");
        let culling_pipelines = ComputePipelinePermutations::request(assets, &PathPipelineShaderStage {
            shader_path: culling_shader_path,
            spec_consts: &[]
        }, &[
            [SpecConstValue::UInt(0), SpecConstValue::UInt(WORK_GROUP_SIZE.x), SpecConstValue::UInt(WORK_GROUP_SIZE.y), SpecConstValue::UInt(WORK_GROUP_SIZE.z), SpecConstValue::Bool(true), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(1), SpecConstValue::UInt(WORK_GROUP_SIZE.x), SpecConstValue::UInt(WORK_GROUP_SIZE.y), SpecConstValue::UInt(WORK_GROUP_SIZE.z), SpecConstValue::Bool(true), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(2), SpecConstValue::UInt(WORK_GROUP_SIZE.x), SpecConstValue::UInt(WORK_GROUP_SIZE.y), SpecConstValue::UInt(WORK_GROUP_SIZE.z), SpecConstValue::Bool(true), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(3), SpecConstValue::UInt(WORK_GROUP_SIZE.x), SpecConstValue::UInt(WORK_GROUP_SIZE.y), SpecConstValue::UInt(WORK_GROUP_SIZE.z), SpecConstValue::Bool(true), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(0), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.x), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.y), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.z), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(1), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.x), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.y), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.z), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(2), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.x), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.y), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.z), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
            [SpecConstValue::UInt(3), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.x), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.y), SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.z), SpecConstValue::Bool(false), SpecConstValue::None, SpecConstValue::None, SpecConstValue::None],
        ]);

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

        device.flush();

        Self {
            pipelines,
            culling_pipelines,
            edges_buffer,
            tris_buffer,
        }
    }

    fn buffer_name_prefix() -> &'static str {
        "marching_cubes"
    }

    pub fn index_buffer_name(entity: Entity) -> String {
        format!(
            "{}_{}",
            Self::buffer_name_prefix(),
            entity,
        )
    }

    pub fn culling_buffer_name(entity: Entity) -> String {
        format!(
            "{}_{}_culling",
            Self::buffer_name_prefix(),
            entity,
        )
    }

    pub fn culling_dispatch_buffer_name(handle: TextureHandle, lod: u32, as_cubes: bool) -> String {
        let asset_handle: AssetHandle = handle.into();
        format!(
            "{}_d_t{:?}_{:?}_{:?}_culling",
            Self::buffer_name_prefix(),
            asset_handle.index(),
            lod,
            as_cubes
        )
    }

    #[inline(always)]
    pub(in crate::renderer::passes) fn is_ready(
        &self,
        assets: &RendererAssetsReadOnly<'_>,
    ) -> bool {
        self.pipelines.all_ready(assets)
            && self.culling_pipelines.all_ready(assets)
    }

    #[inline(always)]
    pub(super) fn tris_table(&self) -> &Arc<BufferSlice> {
        &self.tris_buffer
    }

    fn build_batches<'a>(&self,
                     pass_params: &'a RenderPassParameters)
    ->  SmallVec<[Batch<'a>; 2]> {
        let mut batches:
            SmallVec<[Batch; 2]> = SmallVec::new();
        for d in pass_params.scene.scene.volume_mesh_instances() {
            let dispatch = Dispatch {
                handle: d.volume_texture,
                lod: d.texture_lod,
                as_cubes: d.render_as_cubes,
            };
            let mut appended = false;
            for batch in &mut batches {
                if &dispatch == &batch.dispatch {
                    batch.drawables.push(d);
                    appended = true;
                    break;
                }
            }
            if !appended {
                batches.push(Batch { dispatch, drawables: smallvec![d] });
            }
        }
        batches
    }

    /// Creates required buffers for every active volume mesh instance.
    /// Also destroys the ones that are no longer needed.
    fn manage_buffers(&mut self,
                      cmd_buffer: &mut CommandBuffer,
                      pass_params: &mut RenderPassParameters,
                      use_mesh_shaders: bool,
    ) {
        for RendererVolumeDrawable { entity, volume_texture, texture_lod, render_as_cubes, .. } in pass_params
            .scene
            .scene
            .volume_mesh_instances() {
            if use_mesh_shaders {
                // Mesh shaders need 1 culling buffer per entity
                let workgroup_count: usize = (((512 + MESH_WORK_GROUP_SIZE.x - 1) / MESH_WORK_GROUP_SIZE.x)
                    * (512 + MESH_WORK_GROUP_SIZE.y - 1) / MESH_WORK_GROUP_SIZE.y
                    * (512 + MESH_WORK_GROUP_SIZE.z - 1) / MESH_WORK_GROUP_SIZE.z) as usize;

                let culling_buffer = cmd_buffer.create_temporary_buffer(
                    &BufferInfo {
                    size: (std::mem::size_of::<IndirectCommand>() + std::mem::size_of::<u32>() + std::mem::size_of::<Vec4UI>() * workgroup_count) as u64,
                    usage: BufferUsage::CONSTANT | BufferUsage::STORAGE | BufferUsage::INDIRECT | BufferUsage::COPY_DST,
                    sharing_mode: QueueSharingMode::Exclusive,
                    },
                    MemoryUsage::GPUMemory
                ).unwrap();
                let name = Self::culling_buffer_name(*entity);
                pass_params.resources.track_transient_buffer(&name, culling_buffer);
            } else {
                // Legacy graphics needs 1 index buffer per entity 1 culling buffer per dispatch (grouped by texture + lod + cube?

                let dispatch_buffer_name = Self::culling_dispatch_buffer_name(*volume_texture, *texture_lod, *render_as_cubes);
                if !pass_params.resources.has_buffer(&dispatch_buffer_name) {
                    let workgroup_count: usize = (((512 + WORK_GROUP_SIZE.x - 1) / WORK_GROUP_SIZE.x)
                        * (512 + WORK_GROUP_SIZE.y - 1) / WORK_GROUP_SIZE.y
                        * (512 + WORK_GROUP_SIZE.z - 1) / WORK_GROUP_SIZE.z) as usize;

                    let batch_culling_buffer = cmd_buffer.create_temporary_buffer(
                        &BufferInfo {
                            size: (std::mem::size_of::<IndirectCommand>() + std::mem::size_of::<u32>() + std::mem::size_of::<Vec4UI>() * workgroup_count) as u64,
                            usage: BufferUsage::CONSTANT | BufferUsage::STORAGE | BufferUsage::INDIRECT | BufferUsage::COPY_DST,
                            sharing_mode: QueueSharingMode::Exclusive,
                        },
                        MemoryUsage::GPUMemory
                    ).unwrap();
                    pass_params.resources.track_transient_buffer(&dispatch_buffer_name, batch_culling_buffer);
                }

                // The texture might not be loaded yet, it might not be loaded yet.
                let mut resolution_multiplied = 512 * 512 * 512;

                // Assume there's a lot of empty space/fully filled space and voxels with fewer triangles
                // to avoid huge buffers.
                // Besides, the theoretical worst-case space we need is ~7.7 GB per model. That's not feasible!
                resolution_multiplied /= 50;
                let index_buffer = cmd_buffer.create_temporary_buffer(
                    &BufferInfo {
                        size: (std::mem::size_of::<MarchingCubesIndirectCall>() + std::mem::size_of::<u32>() * 15 * resolution_multiplied) as u64,
                        usage: BufferUsage::STORAGE | BufferUsage::INDEX | BufferUsage::INDIRECT | BufferUsage::COPY_DST,
                        sharing_mode: QueueSharingMode::Exclusive,
                    },
                    MemoryUsage::GPUMemory
                ).unwrap();
                let name = Self::index_buffer_name(*entity);
                pass_params.resources.track_transient_buffer(&name, index_buffer);
            }
        }
    }


    /// Clears all buffers required to render the active volume mesh instances
    fn clear_buffers(&mut self,
                     command_buffer: &mut CommandBuffer,
                     pass_params: &RenderPassParameters)
    {
        command_buffer.begin_label("Clearing");

        // Access all of them here to batch barriers
        let mut buffer_slices = HashMap::<String, BufferAccessor>::with_capacity(pass_params.scene.scene.volume_mesh_instances().len());
        for RendererVolumeDrawable { entity, volume_texture, texture_lod, render_as_cubes, .. } in pass_params.scene.scene.volume_mesh_instances() {
            let buffer_name = Self::index_buffer_name(*entity);
            let culling_name = Self::culling_buffer_name(*entity);
            let dispatch_buffer_name = Self::culling_dispatch_buffer_name(*volume_texture, *texture_lod, *render_as_cubes);

            if pass_params.resources.has_buffer(&buffer_name) {
                let slice = pass_params.resources.access_buffer(
                    command_buffer,
                    &buffer_name,
                    BarrierSync::CLEAR,
                    BarrierAccess::CLEAR_WRITE,
                    HistoryResourceEntry::Current,
                );
                buffer_slices.insert(buffer_name, slice);
            }

            if pass_params.resources.has_buffer(&culling_name) {
                let culling_slice = pass_params.resources.access_buffer(
                    command_buffer,
                    &culling_name,
                    BarrierSync::CLEAR,
                    BarrierAccess::CLEAR_WRITE,
                    HistoryResourceEntry::Current,
                );
                buffer_slices.insert(culling_name, culling_slice);
            }

            if pass_params.resources.has_buffer(&dispatch_buffer_name) {
                let exists = buffer_slices.iter().any(|(name, _)| name == &dispatch_buffer_name);
                if !exists {
                    let culling_buffer_slice = pass_params.resources.access_buffer(
                        command_buffer,
                        &dispatch_buffer_name,
                        BarrierSync::CLEAR,
                        BarrierAccess::CLEAR_WRITE,
                        HistoryResourceEntry::Current,
                    );
                    buffer_slices.insert(dispatch_buffer_name, culling_buffer_slice);
                }
            }
        }

        command_buffer.flush_barriers();

        for (_, slice) in buffer_slices.iter() {
            command_buffer.copy_clear_buffer(
                slice.buffer_ref(),
                0u64,
                slice.length() / 4,
                0u32,
            );
        }

        command_buffer.end_label();
    }

    pub fn use_culling(lod_extent: UVec3) -> bool {
        lod_extent.x >= 128 && lod_extent.y >= 128 && lod_extent.z >= 128
    }

    pub fn execute(
        &mut self,
        command_buffer: &mut CommandBuffer,
        pass_params: &mut RenderPassParameters<'_>,
        options: &VolumeRendererOptions,
    ) {
        command_buffer.clear_all_bindings(BindingFrequency::Frequent);
        command_buffer.clear_all_bindings(BindingFrequency::VeryFrequent);

        let use_mesh_shader = pass_params.device.supports_mesh_shader() && options.use_mesh_shader;

        command_buffer.begin_label("Marching Cubes pass");

        self.manage_buffers(command_buffer, pass_params, use_mesh_shader);
        let batches = self.build_batches(pass_params);
        self.clear_buffers(command_buffer, pass_params);

        // Culling pass

        // Access all of them here to batch barriers
        let mut buffer_slices: SmallVec<[(String, BufferAccessor); 2]> =
            SmallVec::with_capacity(pass_params.scene.scene.volume_mesh_instances().len());

        if options.culling {
            command_buffer.begin_label("Culling");

            for RendererVolumeDrawable { entity, texture_lod, volume_texture, render_as_cubes, .. } in pass_params.scene.scene.volume_mesh_instances() {
                let culling_buffer_name = Self::culling_buffer_name(*entity);
                if pass_params.resources.has_buffer(&culling_buffer_name) {
                    let culling_buffer_slice = pass_params.resources.access_buffer(
                        command_buffer,
                        &culling_buffer_name,
                        BarrierSync::COMPUTE_SHADER,
                        BarrierAccess::STORAGE_WRITE | BarrierAccess::STORAGE_READ,
                        HistoryResourceEntry::Current,
                    );
                    buffer_slices.push((culling_buffer_name, culling_buffer_slice));
                }

                let dispatch_buffer_name = Self::culling_dispatch_buffer_name(*volume_texture, *texture_lod, *render_as_cubes);
                if pass_params.resources.has_buffer(&dispatch_buffer_name) {
                    let exists = buffer_slices.iter().any(|(name, _)| name == &dispatch_buffer_name);
                    if !exists {
                        let culling_buffer_slice = pass_params.resources.access_buffer(
                            command_buffer,
                            &dispatch_buffer_name,
                            BarrierSync::COMPUTE_SHADER,
                            BarrierAccess::STORAGE_WRITE | BarrierAccess::STORAGE_READ,
                            HistoryResourceEntry::Current,
                        );
                        buffer_slices.push((dispatch_buffer_name, culling_buffer_slice));
                    }
                }
            }

            command_buffer.flush_barriers();

            for batch in &batches {
                self.dispatch_culling_batch(command_buffer, pass_params, &buffer_slices, batch, use_mesh_shader);
            }

            command_buffer.end_label();
        }

        if use_mesh_shader {
            command_buffer.end_label();
            return;
        }

        // Marching cubes pass

        command_buffer.begin_label("Marching cubes");

        buffer_slices.clear();
        for RendererVolumeDrawable { entity, volume_texture, texture_lod, render_as_cubes, .. } in pass_params.scene.scene.volume_mesh_instances() {
            let buffer_name = Self::index_buffer_name(*entity);
            let buffer_slice = pass_params.resources.access_buffer(
                command_buffer,
                &buffer_name,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_WRITE,
                HistoryResourceEntry::Current,
            );
            buffer_slices.push((buffer_name, buffer_slice));

            let culling_buffer_name = Self::culling_buffer_name(*entity);
            if pass_params.resources.has_buffer(&culling_buffer_name) {
                let culling_buffer_slice = pass_params.resources.access_buffer(
                    command_buffer,
                    &culling_buffer_name,
                    BarrierSync::COMPUTE_SHADER | BarrierSync::INDIRECT,
                    BarrierAccess::INDIRECT_READ | BarrierAccess::STORAGE_READ,
                    HistoryResourceEntry::Current,
                );
                buffer_slices.push((culling_buffer_name, culling_buffer_slice));
            }

            let dispatch_buffer_name = Self::culling_dispatch_buffer_name(*volume_texture, *texture_lod, *render_as_cubes);
            if pass_params.resources.has_buffer(&dispatch_buffer_name) {
                let exists = buffer_slices.iter().any(|(name, _)| name == &dispatch_buffer_name);
                if !exists {
                    let culling_buffer_slice = pass_params.resources.access_buffer(
                        command_buffer,
                        &dispatch_buffer_name,
                        BarrierSync::COMPUTE_SHADER | BarrierSync::INDIRECT,
                        BarrierAccess::INDIRECT_READ | BarrierAccess::STORAGE_READ,
                        HistoryResourceEntry::Current,
                    );
                    buffer_slices.push((dispatch_buffer_name, culling_buffer_slice));
                }
            }
        }

        command_buffer.flush_barriers();

        for batch in &batches {
            self.dispatch_batch(command_buffer, pass_params, &buffer_slices, batch, options.culling);
        }

        command_buffer.end_label();
        command_buffer.end_label();
    }

    fn dispatch_culling_batch(&self,
                      command_buffer: &mut CommandBuffer,
                      pass_params: &RenderPassParameters,
                      buffer_slices: &[(String, BufferAccessor)],
                      batch: &Batch,
                      use_mesh_shader: bool,)
    {
        assert!(!batch.drawables.is_empty());

        let mut batch_buffer_slices = SmallVec::<[&BufferAccessor; 4]>::new();
        let mut thresholds = SmallVec::<[f32; 4]>::new();

        if !use_mesh_shader {
            // Find the buffer for the entire dispatch
            let culling_buffer_name = Self::culling_dispatch_buffer_name(batch.dispatch.handle, batch.dispatch.lod, batch.dispatch.as_cubes);
            for (name, slice) in buffer_slices {
                if name == &culling_buffer_name {
                    batch_buffer_slices.push(slice);
                    break;
                }
            }
        }
        for d in &batch.drawables {
            thresholds.push(d.min_threshold);

            if !use_mesh_shader {
                continue;
            }

            // Find the buffer for the entities
            // Quadratic but fine due to tiny counts
            let culling_buffer_name = Self::culling_buffer_name(d.entity);
            for (name, slice) in buffer_slices {
                if name == &culling_buffer_name {
                    batch_buffer_slices.push(slice);
                }
            }
        }
        assert!(batch_buffer_slices.len() <= 16);
        assert!(!batch_buffer_slices.is_empty());
        assert!(!thresholds.is_empty());
        if !use_mesh_shader {
            assert_eq!(batch_buffer_slices.len(), 1);
        } else {
            assert_eq!(batch_buffer_slices.len(), thresholds.len());
        }

        let volume_texture = pass_params.assets.get_texture(batch.dispatch.handle);
        let volume_texture_min = pass_params
            .assets
            .get_texture(batch.drawables.first().unwrap().volume_texture_min);
        let volume_texture_max = pass_params
            .assets
            .get_texture(batch.drawables.first().unwrap().volume_texture_max);

        let target_work_group_size = if use_mesh_shader {
            MESH_WORK_GROUP_SIZE
        } else {
            WORK_GROUP_SIZE
        };
        let max_group_size_dimension = target_work_group_size.x
            .max(target_work_group_size.y)
            .max(target_work_group_size.z);
        let reduction = 2u32.max(max_group_size_dimension.ilog2());

        let mut extent = Vec3UI::new(512, 512, 512);
        let texture_view = &volume_texture.view;
        if let Some(texture) = texture_view.texture().as_ref() {
            let texture_info = texture.info();
            extent.x = (texture_info.width >> batch.dispatch.lod).max(1);
            extent.y = (texture_info.height >> batch.dispatch.lod).max(1);
            extent.z = (texture_info.depth >> batch.dispatch.lod).max(1);
        }

        if !Self::use_culling(extent) {
            return;
        }

        let texture_name = pass_params.assets.get_path(batch.dispatch.handle.into())
            .unwrap_or_else(|| format!("{:?}", batch.dispatch.handle));
        command_buffer.begin_label(&format!(
            "Marching Cube Texture: {} with lod: {}",
            texture_name, batch.dispatch.lod
        ));

        let entries: SmallVec<[BufferArrayEntry; 2]> = batch_buffer_slices
            .iter()
            .map(|slice| BufferArrayEntry {
                buffer: slice.buffer_ref(),
                offset: 0,
                length: WHOLE_BUFFER,
            })
            .collect();

        let mut spec_consts = [SpecConstValue::None; 8];
        spec_consts[0] = SpecConstValue::UInt(thresholds.len() as u32);
        if !use_mesh_shader {
            spec_consts[1] = SpecConstValue::UInt(WORK_GROUP_SIZE.x);
            spec_consts[2] = SpecConstValue::UInt(WORK_GROUP_SIZE.y);
            spec_consts[3] = SpecConstValue::UInt(WORK_GROUP_SIZE.z);
            spec_consts[4] = SpecConstValue::Bool(true);
        } else {
            spec_consts[1] = SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.x);
            spec_consts[2] = SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.y);
            spec_consts[3] = SpecConstValue::UInt(MESH_WORK_GROUP_SIZE.z);
            spec_consts[4] = SpecConstValue::Bool(false);
        }
        let mut pipeline_opt = self.culling_pipelines.get_pipeline(pass_params.assets, &spec_consts);
        if pipeline_opt.is_none() {
            spec_consts[0] = SpecConstValue::UInt(0); // Spec const value 0 makes it use the dynamic value
            pipeline_opt = self.culling_pipelines.get_pipeline(pass_params.assets, &spec_consts);
        }
        let pipeline = pipeline_opt.unwrap();
        command_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));

        command_buffer.bind_storage_buffer_array(BindingFrequency::Frequent, 3, &entries);

        command_buffer.bind_sampling_view(BindingFrequency::Frequent, 0, &volume_texture.view);
        command_buffer.bind_sampling_view(
            BindingFrequency::Frequent,
            1,
            &volume_texture_min.view,
        );
        command_buffer.bind_sampling_view(
            BindingFrequency::Frequent,
            2,
            &volume_texture_max.view,
        );

        command_buffer.bind_storage_buffer_array(
            BindingFrequency::Frequent,
            3,
            &entries,
        );

        let thresholds_buffer = command_buffer
            .upload_dynamic_data(pass_params.device, &thresholds, BufferUsage::CONSTANT)
            .unwrap();

        command_buffer.bind_uniform_buffer(
            BindingFrequency::Frequent,
            4,
            BufferRef::Transient(&thresholds_buffer),
            0,
            WHOLE_BUFFER,
        );

        command_buffer.set_push_constant_data(
            &[MarchingCubesCullingConfig {
                lod: batch.dispatch.lod,
                extent,
                thresholds_count: thresholds.len() as u32,
            }],
        );

        command_buffer.finish_binding();

        let culling_extent = Vec3UI::new(
            (extent.x >> reduction).max(1),
            (extent.y >> reduction).max(1),
            (extent.z >> reduction).max(1)
        );
        command_buffer.dispatch((culling_extent.x + 3) / 4,
                                (culling_extent.y + 3) / 4,
                                (culling_extent.z + 3) / 4);

        command_buffer.end_label();
    }

    fn dispatch_batch(&self,
        command_buffer: &mut CommandBuffer,
        pass_params: &RenderPassParameters,
        buffer_slices: &[(String, BufferAccessor)],
        batch: &Batch,
        culling_enabled: bool,)
    {
        assert!(!batch.drawables.is_empty());

        let mut batch_buffer_slices = SmallVec::<[&BufferAccessor; 4]>::new();
        let mut thresholds = SmallVec::<[f32; 4]>::new();

        let culling_name = Self::culling_dispatch_buffer_name(batch.dispatch.handle, batch.dispatch.lod, batch.dispatch.as_cubes);
        let culling_indirect_slice = buffer_slices.iter().find_map(|(name, slice)| if name == &culling_name { Some(slice) } else { None }).unwrap();

        for (index, d) in batch.drawables.iter().enumerate() {
            thresholds.push(d.min_threshold);

            // Quadratic but fine due to tiny counts
            let drawable_buffer_name = Self::index_buffer_name(d.entity);
            for (name, slice) in buffer_slices {
                if name == &drawable_buffer_name {
                    batch_buffer_slices.push(slice);
                }
            }
        }

        assert!(batch_buffer_slices.len() <= 16);
        assert!(!batch_buffer_slices.is_empty());
        assert!(!thresholds.is_empty());
        assert_eq!(batch_buffer_slices.len(), thresholds.len());

        let texture_name = pass_params.assets.get_path(batch.dispatch.handle.into())
            .unwrap_or_else(|| format!("{:?}", batch.dispatch.handle));
        command_buffer.begin_label(&format!(
            "Marching Cube Texture: {} with lod: {}",
            texture_name, batch.dispatch.lod
        ));

        let entries: SmallVec<[BufferArrayEntry; 2]> = batch_buffer_slices
            .iter()
            .map(|slice| BufferArrayEntry {
                buffer: slice.buffer_ref(),
                offset: 0,
                length: WHOLE_BUFFER,
            })
            .collect();

        let volume_texture = pass_params.assets.get_texture(batch.dispatch.handle);
        let volume_texture_min = pass_params
            .assets
            .get_texture(batch.drawables.first().unwrap().volume_texture_min);
        let volume_texture_max = pass_params
            .assets
            .get_texture(batch.drawables.first().unwrap().volume_texture_max);
        command_buffer.bind_sampling_view(BindingFrequency::Frequent, 2, &volume_texture.view);
        let mut extent = Vec3UI::new(512, 512, 512);
        let texture_view = &volume_texture.view;
        if let Some(texture) = texture_view.texture().as_ref() {
            let texture_info = texture.info();
            extent.x = texture_info.width >> batch.dispatch.lod;
            extent.y = texture_info.height >> batch.dispatch.lod;
            extent.z = texture_info.depth >> batch.dispatch.lod;
        }
        let culling = culling_enabled && Self::use_culling(extent);

        let mut spec_consts = [SpecConstValue::None; 8];
        spec_consts[0] = SpecConstValue::UInt(thresholds.len() as u32);
        spec_consts[1] = SpecConstValue::Bool(batch.dispatch.as_cubes);
        spec_consts[2] = SpecConstValue::Bool(culling);

        let mut pipeline_opt = self.pipelines.get_pipeline(pass_params.assets, &spec_consts);
        if pipeline_opt.is_none() {
            spec_consts[0] = SpecConstValue::UInt(0); // Spec const value 0 makes it use the dynamic value
            pipeline_opt = self.pipelines.get_pipeline(pass_params.assets, &spec_consts);
        }
        let pipeline = pipeline_opt.unwrap();
        command_buffer.set_pipeline(PipelineBinding::Compute(&pipeline));

        command_buffer.bind_uniform_buffer(
            BindingFrequency::Frequent,
            0,
            BufferRef::Regular(&self.edges_buffer),
            0,
            WHOLE_BUFFER,
        );
        command_buffer.bind_uniform_buffer(
            BindingFrequency::Frequent,
            1,
            BufferRef::Regular(&self.tris_buffer),
            0,
            WHOLE_BUFFER,
        );

        command_buffer.bind_storage_buffer_array(BindingFrequency::Frequent, 3, &entries);
        command_buffer.bind_sampling_view(
            BindingFrequency::Frequent,
            8,
            &volume_texture_min.view,
        );
        command_buffer.bind_sampling_view(
            BindingFrequency::Frequent,
            9,
            &volume_texture_max.view,
        );

        let thresholds_buffer = command_buffer
            .upload_dynamic_data(pass_params.device, &thresholds, BufferUsage::CONSTANT)
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
                lod: batch.dispatch.lod,
                extent,
                thresholds_count: thresholds.len() as u32,
            }],
        );

        command_buffer.bind_storage_buffer(
            BindingFrequency::Frequent,
            6,
            culling_indirect_slice.buffer_ref(),
            0u64,
            WHOLE_BUFFER,
        );

        command_buffer.finish_binding();

        if extent.x > 0 && extent.y > 0 && extent.z > 0 {
            if culling {
                command_buffer.dispatch_indirect(culling_indirect_slice.buffer_ref(), 0u64);
            } else {
                command_buffer.dispatch((extent.x + 3) / 4, (extent.y + 3) / 4, (extent.z + 3) / 4);
            }
        } else {
            log::error!("Marching cubes texture mip has an extent of 0");
        }

        command_buffer.end_label();
    }
}
