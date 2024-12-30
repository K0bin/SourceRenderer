use std::collections::HashMap;
use std::sync::Arc;

use crate::asset::*;
use crate::graphics::{BindlessSlot, TextureView};
use crate::math::BoundingBox;

use super::*;

use smallvec::SmallVec;
use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::Vec4;

pub struct RendererTexture<B: GPUBackend> {
    pub(super) view: Arc<TextureView<B>>,
    pub(super) bindless_index: Option<BindlessSlot<B>>,
}

impl<B: GPUBackend> PartialEq for RendererTexture<B> {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
    }
}
impl<B: GPUBackend> Eq for RendererTexture<B> {}

pub struct RendererMaterial {
    pub(super) properties: HashMap<String, RendererMaterialValue>,
    pub(super) shader_name: String, // TODO reference actual shader
}

impl Clone for RendererMaterial {
    fn clone(&self) -> Self {
        Self {
            properties: self.properties.clone(),
            shader_name: self.shader_name.clone(),
        }
    }
}

pub enum RendererMaterialValue {
    Float(f32),
    Vec4(Vec4),
    Texture(TextureHandle),
}

impl PartialEq for RendererMaterialValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Float(l0), Self::Float(r0)) => (l0 * 100f32) as u32 == (r0 * 100f32) as u32,
            (Self::Vec4(l0), Self::Vec4(r0)) => {
                (l0.x * 100f32) as u32 == (r0.x * 100f32) as u32
                    && (l0.y * 100f32) as u32 == (r0.y * 100f32) as u32
                    && (l0.z * 100f32) as u32 == (r0.z * 100f32) as u32
                    && (l0.w * 100f32) as u32 == (r0.w * 100f32) as u32
            }
            (Self::Texture(l0), Self::Texture(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl Eq for RendererMaterialValue {}

impl Clone for RendererMaterialValue {
    fn clone(&self) -> Self {
        match self {
            Self::Float(val) => Self::Float(*val),
            Self::Vec4(val) => Self::Vec4(*val),
            Self::Texture(tex) => Self::Texture(*tex),
        }
    }
}

impl PartialOrd for RendererMaterialValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RendererMaterialValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (RendererMaterialValue::Float(val1), RendererMaterialValue::Float(val2)) => {
                ((val1 * 100f32) as u32).cmp(&((val2 * 100f32) as u32))
            }
            (RendererMaterialValue::Float(_), RendererMaterialValue::Texture(_)) => {
                std::cmp::Ordering::Less
            }
            (RendererMaterialValue::Float(_), RendererMaterialValue::Vec4(_)) => {
                std::cmp::Ordering::Less
            }
            (RendererMaterialValue::Texture(_), RendererMaterialValue::Float(_)) => {
                std::cmp::Ordering::Greater
            }
            (RendererMaterialValue::Texture(_), RendererMaterialValue::Vec4(_)) => {
                std::cmp::Ordering::Greater
            }
            (RendererMaterialValue::Texture(tex1), RendererMaterialValue::Texture(tex2)) => {
                tex1.cmp(&tex2)
            }
            (RendererMaterialValue::Vec4(val1), RendererMaterialValue::Vec4(val2)) => {
                ((val1.x * 100f32) as u32)
                    .cmp(&((val2.x * 100f32) as u32))
                    .then(((val1.y * 100f32) as u32).cmp(&((val2.y * 100f32) as u32)))
                    .then(((val1.z * 100f32) as u32).cmp(&((val2.z * 100f32) as u32)))
                    .then(((val1.w * 100f32) as u32).cmp(&((val2.w * 100f32) as u32)))
            }
            (RendererMaterialValue::Vec4(_), RendererMaterialValue::Texture(_)) => {
                std::cmp::Ordering::Less
            }
            (RendererMaterialValue::Vec4(_), RendererMaterialValue::Float(_)) => {
                std::cmp::Ordering::Greater
            }
        }
    }
}

impl PartialEq for RendererMaterial {
    fn eq(&self, other: &Self) -> bool {
        if self.shader_name != other.shader_name {
            return false;
        }
        for (key, value) in self.properties.iter() {
            if other.properties.get(key) != Some(value) {
                return false;
            }
        }
        true
    }
}

impl RendererMaterial {
    pub fn new_pbr(albedo_texture: TextureHandle) -> Self {
        let mut props = HashMap::new();
        props.insert(
            "albedo".to_string(),
            RendererMaterialValue::Texture(albedo_texture),
        );
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }

    pub fn new_pbr_color(color: Vec4) -> Self {
        let mut props = HashMap::new();
        props.insert("albedo".to_string(), RendererMaterialValue::Vec4(color));
        Self {
            shader_name: "pbr".to_string(),
            properties: props,
        }
    }

    pub fn get(&self, key: &str) -> Option<&RendererMaterialValue> {
        self.properties.get(key)
    }
}

impl Eq for RendererMaterial {}

impl PartialOrd for RendererMaterial {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RendererMaterial {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let mut last_result = self
            .shader_name
            .cmp(&other.shader_name)
            .then(self.properties.len().cmp(&other.properties.len()));

        if last_result != std::cmp::Ordering::Equal {
            return last_result;
        }

        for (key, value) in &self.properties {
            let other_val = other.properties.get(key);
            if let Some(other_val) = other_val {
                last_result = value.cmp(other_val);
                if last_result != std::cmp::Ordering::Equal {
                    return last_result;
                }
            }
        }
        std::cmp::Ordering::Equal
    }
}

pub struct RendererModel {
    mesh: MeshHandle,
    materials: SmallVec<[MaterialHandle; 16]>,
}

impl RendererModel {
    pub fn new(mesh: MeshHandle, materials: SmallVec<[MaterialHandle; 16]>) -> Self {
        Self {
            mesh: mesh,
            materials,
        }
    }

    pub fn mesh_handle(&self) -> MeshHandle {
        self.mesh
    }

    pub fn material_handles(&self) -> &[MaterialHandle] {
        &self.materials
    }
}

pub type RendererShader<B: GPUBackend> = Arc<B::Shader>;

pub struct RendererMesh<B: GPUBackend> {
    pub vertices: AssetBufferSlice<B>,
    pub indices: Option<AssetBufferSlice<B>>,
    pub parts: Box<[MeshRange]>,
    pub bounding_box: Option<BoundingBox>,
    pub vertex_count: u32,
}
