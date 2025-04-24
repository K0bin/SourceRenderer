use serde::{Deserialize, Serialize};

use crate::Vec3UI;

use super::{texture::TextureDimension, Format, ShaderType, NON_BINDLESS_SET_COUNT};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ResourceType {
    UniformBuffer,
    StorageBuffer,
    SubpassInput,
    SampledTexture,
    StorageTexture,
    Sampler,
    CombinedTextureSampler,
    AccelerationStructure,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum SamplingType {
    Float,
    Depth,
    SInt,
    UInt,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Resource {
    pub name: String,
    pub set: u32,
    pub binding: u32,
    pub array_size: u32,
    pub resource_type: ResourceType,
    pub writable: bool,
    pub texture_dimension: TextureDimension,
    pub is_multisampled: bool,
    pub sampling_type: SamplingType,
    pub storage_format: Format,
    pub struct_size: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackedShader {
    pub push_constant_size: u32,
    pub stage_input_count: u32,
    pub max_stage_input: u32,
    pub resources: [Box<[Resource]>; NON_BINDLESS_SET_COUNT as usize],
    pub shader_type: ShaderType,
    pub workgroup_size: Vec3UI,
    pub uses_bindless_texture_set: bool,
    pub shader_spirv: Box<[u8]>,
    pub shader_air: Box<[u8]>,
    pub shader_dxil: Box<[u8]>,
    pub shader_wgsl: String,
}
