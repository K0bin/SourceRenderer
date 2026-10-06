use bytemuck::BoxBytes;
use smallvec::SmallVec;
use sourcerenderer_core::Vec4;

use super::AssetType;
use super::loaded_level::LevelData;
use crate::graphics::{PackedShader, TextureInfo};
use crate::math::BoundingBox;

#[derive(Clone)]
pub struct MeshRange {
    pub start: u32,
    pub count: u32,
}

pub struct TextureData {
    pub info: TextureInfo,
    pub data: SmallVec<[BoxBytes; 4]>,
}

pub struct MeshData {
    pub indices: Option<BoxBytes>,
    pub vertices: BoxBytes,
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
pub enum MaterialData {
    SimplePBR {
        albedo_path: Option<String>,
        roughness_path: Option<String>,
        metalness_path: Option<String>,
        albedo_color: Vec4,
        roughness_factor: f32,
        metalness_factor: f32,
    },
}

impl MaterialData {
    pub fn new_pbr(albedo_texture_path: &str, roughness: f32, metalness: f32) -> Self {
        Self::SimplePBR {
            albedo_path: Some(albedo_texture_path.to_string()),
            roughness_path: None,
            metalness_path: None,
            albedo_color: Vec4::new(1.0f32, 1.0f32, 1.0f32, 1.0f32),
            roughness_factor: roughness,
            metalness_factor: metalness,
        }
    }

    pub fn new_pbr_color(albedo: Vec4, roughness: f32, metalness: f32) -> Self {
        Self::SimplePBR {
            albedo_path: None,
            roughness_path: None,
            metalness_path: None,
            albedo_color: albedo,
            roughness_factor: roughness,
            metalness_factor: metalness,
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
