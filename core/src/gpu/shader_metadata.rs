use serde::{Serialize, Deserialize};

use super::ShaderType;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ResourceType {
    UniformBuffer,
    StorageBuffer,
    SubpassInput,
    SampledTexture,
    StorageTexture,
    Sampler,
    CombinedTextureSampler,
    AccelerationStructure
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Resource {
    pub name: String,
    pub set: u32,
    pub binding: u32,
    pub array_size: u32,
    pub resource_type: ResourceType,
    pub writable: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PackedShader {
    pub push_constant_size: u32,
    pub resources: [Box<[Resource]>; 4],
    pub shader_type: ShaderType,
    pub uses_bindless_texture_set: bool,
    pub shader_spirv: Box<[u8]>,
    pub shader_air: Box<[u8]>,
    pub shader_dxil: Box<[u8]>,
}
