use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::Vec4;

use super::*;
use crate::asset::*;
use crate::graphics::{BindlessSlot, Shader, Texture, TextureView};
use crate::math::BoundingBox;

pub struct RendererTexture {
    pub(crate) view: Arc<TextureView>,
    pub(crate) bindless_index: Option<BindlessSlot>,
}

impl PartialEq for RendererTexture {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
    }
}
impl Eq for RendererTexture {}

impl RendererTexture {
    pub(crate) fn texture(&self) -> &Texture {
        self.view.texture().unwrap().as_ref()
    }
}


pub enum RendererMaterial {
    SimplePBR {
        albedo: Option<TextureHandle>,
        roughness: Option<TextureHandle>,
        metalness: Option<TextureHandle>,
        albedo_color: Vec4,
        roughness_factor: f32,
        metalness_factor: f32,
    },
}

impl RendererMaterial {
    pub fn new_pbr(albedo_texture: TextureHandle) -> Self {
        Self::SimplePBR {
            albedo: Some(albedo_texture),
            roughness: None,
            metalness: None,
            albedo_color: Vec4::new(1.0f32, 1.0f32, 1.0f32, 1.0f32),
            roughness_factor: 1.0f32,
            metalness_factor: 1.0f32,
        }
    }

    pub fn new_pbr_color(color: Vec4) -> Self {
        Self::SimplePBR {
            albedo: None,
            roughness: None,
            metalness: None,
            albedo_color: color,
            roughness_factor: 1.0f32,
            metalness_factor: 1.0f32,
        }
    }

    pub fn sorting_key(&self) -> u64 {
        let mut sort_index = 0u64;
        let enum_index;
        match self {
            RendererMaterial::SimplePBR {
                albedo, roughness, metalness,
                ..
            } => {
                enum_index = 0;

                #[inline(always)]
                fn get_tex_index(texture: &Option<TextureHandle>) -> u64 {
                    texture.map(|t| {
                        let handle: AssetHandle = t.into();
                        handle.index()
                    }).unwrap_or(0)
                }

                // 6 bits for the texture, packed right to left
                let mut bit_pos = 64;
                const BITS_PER_TEXTURE: u64 = 6;

                bit_pos -= BITS_PER_TEXTURE;
                let albedo_tex_index = get_tex_index(albedo);
                sort_index |= (albedo_tex_index % ((BITS_PER_TEXTURE << 6) - 1)) << bit_pos;
                bit_pos -= BITS_PER_TEXTURE;
                let roughness_tex_index = get_tex_index(roughness);
                sort_index |= (roughness_tex_index % ((BITS_PER_TEXTURE << 6) - 1)) << bit_pos;
                bit_pos -= BITS_PER_TEXTURE;
                let metalness_tex_index = get_tex_index(metalness);
                sort_index |= (metalness_tex_index % ((BITS_PER_TEXTURE << 6) - 1)) << bit_pos
            },
        }
        sort_index >>= 4;
        sort_index |= (enum_index % 0b111111) << 58; // 6 bits for enum index
        sort_index
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

    #[inline(always)]
    pub fn mesh_handle(&self) -> MeshHandle {
        self.mesh
    }

    #[inline(always)]
    pub fn material_handles(&self) -> &[MaterialHandle] {
        &self.materials
    }
}

pub type RendererShader = Arc<Shader>;
pub type RendererGraphicsPipeline = CompiledPipeline<GraphicsCompileTask>;
pub type RendererMeshGraphicsPipeline = CompiledPipeline<MeshGraphicsCompileTask>;
pub type RendererComputePipeline = CompiledPipeline<ComputeCompileTask>;
pub type RendererRayTracingPipeline = CompiledPipeline<RayTracingCompileTask>;

pub struct RendererMesh {
    pub vertices: AssetBufferSlice,
    pub indices: Option<AssetBufferSlice>,
    pub parts: Box<[MeshRange]>,
    pub bounding_box: Option<BoundingBox>,
    pub vertex_count: u32,
}

impl RendererMesh {
    pub fn sorting_key(&self) -> u64 {
        self as *const RendererMesh as u64
    }
}
