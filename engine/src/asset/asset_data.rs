use std::collections::HashMap;

use sourcerenderer_core::Vec4;

use super::loaded_level::LevelData;
use super::AssetType;
use crate::graphics::{
    PackedShader,
    TextureInfo,
};
use crate::math::BoundingBox;

#[derive(Clone)]
pub struct MeshRange {
    pub start: u32,
    pub count: u32,
}

pub struct TextureData {
    pub info: TextureInfo,
    pub data: Box<[Box<[u8]>]>,
}

pub struct MeshData {
    pub indices: Option<Box<[u8]>>,
    pub vertices: Box<[u8]>,
    pub parts: Box<[MeshRange]>,
    pub bounding_box: Option<BoundingBox>,
    pub vertex_count: u32,
}

#[derive(Clone)]
pub struct ModelData {
    pub mesh_path: String,
    pub material_paths: Vec<String>,
}

#[derive(Clone)]
pub struct MaterialData {
    pub shader_name: String,
    pub properties: HashMap<String, MaterialValue>,
}

impl MaterialData {
    pub fn new_pbr(albedo_texture_path: &str, roughness: f32, metalness: f32) -> Self {
        let mut props = HashMap::new();
        props.insert(
            "albedo".to_string(),
            MaterialValue::Texture(albedo_texture_path.to_string()),
        );
        props.insert("roughness".to_string(), MaterialValue::Float(roughness));
        props.insert("metalness".to_string(), MaterialValue::Float(metalness));
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }

    pub fn new_pbr_color(albedo: Vec4, roughness: f32, metalness: f32) -> Self {
        let mut props = HashMap::new();
        props.insert("albedo".to_string(), MaterialValue::Vec4(albedo));
        props.insert("roughness".to_string(), MaterialValue::Float(roughness));
        props.insert("metalness".to_string(), MaterialValue::Float(metalness));
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }
}

#[derive(Clone)]
pub enum MaterialValue {
    Texture(String),
    Float(f32),
    Vec4(Vec4),
}

pub type ShaderData = PackedShader;

pub type SoundData = ();

pub enum AssetData {
    Texture(TextureData),
    Mesh(MeshData),
    Model(ModelData),
    Sound(SoundData),
    Material(MaterialData),
    Shader(ShaderData),
    Level(LevelData),
}

impl AssetData {
    pub fn asset_type(&self) -> AssetType {
        match self {
            AssetData::Texture(_) => AssetType::Texture,
            AssetData::Mesh(_) => AssetType::Mesh,
            AssetData::Model(_) => AssetType::Model,
            AssetData::Sound(_) => AssetType::Sound,
            AssetData::Material(_) => AssetType::Material,
            AssetData::Shader(_) => AssetType::Shader,
            AssetData::Level(_) => AssetType::Level,
        }
    }
}
