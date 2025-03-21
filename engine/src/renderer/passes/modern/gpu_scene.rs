use std::collections::HashMap;
use smallvec::SmallVec;
use sourcerenderer_core::{
    Matrix4,
    Vec3,
    Vec4,
};
use bitflags::bitflags;

use crate::graphics::*;
use crate::asset::{MaterialHandle, MeshHandle, ModelHandle};
use crate::renderer::asset::{RendererAssetsReadOnly, RendererMaterial, RendererMaterialValue};
use crate::renderer::renderer_scene::RendererScene;

pub const DRAWABLE_CAPACITY: u32 = 4096;
pub const PART_CAPACITY: u32 = 4096;
pub const DRAW_CAPACITY: u32 = 4096;
#[allow(unused)]
pub const MATERIAL_CAPACITY: u32 = 4096;
#[allow(unused)]
pub const MESH_CAPACITY: u32 = 4096;
#[allow(unused)]
pub const LIGHT_CAPACITY: u32 = 64;

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUScene {
    drawable_count: u32,
    draw_count: u32,
    light_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUDraw {
    drawable_index: u16,
    part_index: u16,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUMeshPart {
    material_index: u32,
    mesh_first_index: u32,
    mesh_index_count: u32,
    mesh_vertex_offset: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUMaterial {
    albedo: Vec4,
    roughness_factor: f32,
    metalness_factor: f32,
    albedo_texture_index: u32,
    _padding: u32,
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct GPUDrawableFlags : u32 {
        const STATIC = 0b1;
        const CASTS_SHADOW = 0b10;
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUDrawable {
    transform: Matrix4,
    old_transform: Matrix4,
    mesh_index: u32,
    flags: GPUDrawableFlags,
    part_start: u32,
    part_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUBoundingBox {
    min: Vec4,
    max: Vec4,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUBoundingSphere {
    center: Vec3,
    radius: f32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUMesh {
    aabb: GPUBoundingBox,
    sphere: GPUBoundingSphere,
}

#[allow(unused)]
#[repr(u32)]
#[derive(Debug, Clone)]
enum GPULightType {
    PointLight,
    DirectionalLight,
    SpotLight
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPULight {
    position: Vec3,
    light_type: GPULightType,
    direction: Vec3,
    intensity: f32,
    color: Vec3,
    _padding: u32
}

struct ModelEntry {
    mesh_index: u32,
    part_start: u32,
    part_count: u32,
}

pub struct BufferBinding {
    pub offset: u64,
    pub length: u64
}

pub struct SceneBuffers {
    pub buffer: TransientBufferSlice,
    pub scene_buffer: BufferBinding,
    pub draws_buffer: BufferBinding,
    pub meshes_buffer: BufferBinding,
    pub drawables_buffer: BufferBinding,
    pub parts_buffer: BufferBinding,
    pub materials_buffer: BufferBinding,
    pub lights_buffer: BufferBinding
}

#[profiling::function]
pub fn upload(
    cmd_buffer: &mut CommandBuffer,
    scene: &RendererScene,
    zero_view_index: u32,
    assets: &RendererAssetsReadOnly<'_>,
) -> SceneBuffers {
    let mut local = GPUScene {
        drawable_count: 0,
        draw_count: 0,
        light_count: 0,
    };

    let mut material_map = HashMap::<MaterialHandle, u32>::new();
    let mut mesh_map = HashMap::<MeshHandle, u32>::new();
    let mut model_map = HashMap::<ModelHandle, ModelEntry>::new();

    let mut draws = Vec::<GPUDraw>::with_capacity(scene.static_drawables().len());
    let mut drawables = SmallVec::<[GPUDrawable; 16]>::with_capacity(scene.static_drawables().len());
    let mut parts = SmallVec::<[GPUMeshPart; 16]>::with_capacity(scene.static_drawables().len());
    let mut materials = SmallVec::<[GPUMaterial; 16]>::new();
    let mut meshes = SmallVec::<[GPUMesh; 16]>::new();
    let mut lights = SmallVec::<[GPULight; 16]>::new();

    {
        profiling::scope!("CollectingSceneData");
        for drawable in scene.static_drawables() {
            let model_entry = if let Some(model_entry) = model_map.get(&drawable.model) {
                model_entry
            } else {
                let model = assets.get_model(drawable.model);
                if model.is_none() {
                    log::info!("Skipping draw because of missing model");
                    continue;
                }
                let model = model.unwrap();
                let mesh = assets.get_mesh(model.mesh_handle());
                if mesh.is_none() {
                    log::info!("Skipping draw because of missing mesh");
                    continue;
                }
                let mesh = mesh.unwrap();
                let model_materials: SmallVec<[&RendererMaterial; 8]> = model
                    .material_handles()
                    .iter()
                    .map(|handle| assets.get_material(*handle))
                    .collect();

                let model_part_start = parts.len() as u32;
                for (index, part) in mesh.parts.iter().enumerate() {
                    let material_handle = model.material_handles()[index];
                    let material_index = if let Some(material_index) =
                        material_map.get(&material_handle)
                    {
                        *material_index
                    } else {
                        let material = model_materials[index];
                        let material_index = materials.len() as u32;
                        let mut gpu_material = GPUMaterial {
                            albedo: Vec4::new(1f32, 1f32, 1f32, 1f32),
                            roughness_factor: 1f32,
                            metalness_factor: 0f32,
                            albedo_texture_index: zero_view_index,
                            _padding: 0,
                        };

                        let albedo_value = material.get("albedo").unwrap();
                        match albedo_value {
                            RendererMaterialValue::Texture(handle) => {
                                let texture = assets.get_texture(*handle);
                                gpu_material.albedo_texture_index = texture.bindless_index.as_ref().map(|b| b.slot()).unwrap_or(zero_view_index)
                            }
                            RendererMaterialValue::Vec4(val) => gpu_material.albedo = *val,
                            RendererMaterialValue::Float(_) => unimplemented!(),
                        }
                        let roughness_value = material.get("roughness");
                        match roughness_value {
                            Some(RendererMaterialValue::Texture(_texture)) => {
                                unimplemented!()
                            }
                            Some(RendererMaterialValue::Vec4(_)) => unimplemented!(),
                            Some(RendererMaterialValue::Float(val)) => {
                                gpu_material.roughness_factor = *val;
                            }
                            None => {}
                        }
                        let metalness_value = material.get("metalness");
                        match metalness_value {
                            Some(RendererMaterialValue::Texture(_texture)) => {
                                unimplemented!()
                            }
                            Some(RendererMaterialValue::Vec4(_)) => unimplemented!(),
                            Some(RendererMaterialValue::Float(val)) => {
                                gpu_material.metalness_factor = *val;
                            }
                            None => {}
                        }
                        materials.push(gpu_material);
                        material_map.insert(material_handle, material_index);
                        material_index
                    };

                    let indices = mesh
                        .indices
                        .as_ref()
                        .expect("Non indexed drawing is not supported");
                    let vertices = &mesh.vertices;
                    assert_eq!(indices.offset() % (std::mem::size_of::<u32>() as u32), 0);
                    assert_eq!(
                        vertices.offset() % (std::mem::size_of::<crate::renderer::Vertex>() as u32),
                        0
                    );
                    let gpu_part = GPUMeshPart {
                        material_index: material_index,
                        mesh_first_index: part.start + indices.offset() / std::mem::size_of::<u32>() as u32,
                        mesh_index_count: part.count,
                        mesh_vertex_offset: vertices.offset() / (std::mem::size_of::<crate::renderer::Vertex>() as u32), // TODO: hardcoded vertex size
                    };
                    parts.push(gpu_part);
                }

                let mesh_index = if let Some(index) = mesh_map.get(&model.mesh_handle()) {
                    *index
                } else {
                    let index = meshes.len() as u32;
                    let mesh = mesh
                        .bounding_box
                        .as_ref()
                        .map(|bb| GPUMesh {
                            aabb: GPUBoundingBox {
                                min: Vec4::new(bb.min.x, bb.min.y, bb.min.z, 1f32),
                                max: Vec4::new(bb.max.x, bb.max.y, bb.max.z, 1f32),
                            },
                            sphere: GPUBoundingSphere {
                                center: bb.min + (bb.max - bb.min) * 0.5f32,
                                radius: (bb.max - (bb.min + (bb.max - bb.min) * 0.5f32)).length(),
                            },
                        })
                        .unwrap_or_else(|| GPUMesh {
                            aabb: GPUBoundingBox {
                                min: Vec4::new(f32::MIN, f32::MIN, f32::MIN, 1f32),
                                max: Vec4::new(f32::MAX, f32::MAX, f32::MAX, 1f32),
                            },
                            sphere: GPUBoundingSphere {
                                center: Vec3::new(0f32, 0f32, 0f32),
                                radius: f32::MAX,
                            },
                        });
                    meshes.push(mesh);
                    mesh_map.insert(model.mesh_handle(), index);
                    index
                };
                model_map.insert(
                    drawable.model,
                    ModelEntry {
                        mesh_index,
                        part_start: model_part_start,
                        part_count: parts.len() as u32 - model_part_start,
                    },
                );
                model_map.get(&drawable.model).unwrap()
            };

            let drawable_index = drawables.len() as u16;
            {
                let mut gpu_drawable = GPUDrawable {
                    transform: Matrix4::from(drawable.transform),
                    old_transform: Matrix4::from(drawable.old_transform),
                    mesh_index: model_entry.mesh_index,
                    flags: GPUDrawableFlags::empty(),
                    part_start: model_entry.part_start,
                    part_count: model_entry.part_count,
                };

                if !drawable.can_move {
                    gpu_drawable.flags |= GPUDrawableFlags::STATIC;
                }
                if drawable.cast_shadows {
                    gpu_drawable.flags |= GPUDrawableFlags::CASTS_SHADOW;
                }
                drawables.push(gpu_drawable);
            }

            for part_index in
            model_entry.part_start..(model_entry.part_start + model_entry.part_count)
            {
                let gpu_draw = GPUDraw {
                    drawable_index,
                    part_index: part_index as u16
                };
                draws.push(gpu_draw);
            }
        }

        local.light_count = 0;
        for light in scene.directional_lights() {
            let gpu_light = GPULight {
                light_type: GPULightType::DirectionalLight,
                position: Vec3::new(0f32,
                                    0f32,
                                    0f32),
                direction: light.direction,
                intensity: light.intensity,
                color: Vec3::new(1f32,
                                 1f32,
                                 1f32),
                _padding: 0,
            };
            lights.push(gpu_light);
        }
        for light in scene.point_lights() {
            let gpu_light = GPULight {
                light_type: GPULightType::PointLight,
                position: light.position,
                direction: Vec3::new(0f32,
                                     0f32,
                                     0f32),
                intensity: light.intensity,
                color: Vec3::new(1f32,
                                 1f32,
                                 1f32),
                _padding: 0,
            };
            lights.push(gpu_light);
        }

        local.light_count = lights.len() as u32;
        local.drawable_count = drawables.len() as u32;
        local.draw_count = draws.len() as u32;
    }


    let scene_size = std::mem::size_of::<GPUScene>() as u64;
    let drawables_size = (drawables.len() * std::mem::size_of::<GPUDrawable>()) as u64;
    let draws_size = (draws.len() * std::mem::size_of::<GPUDraw>()) as u64;
    let meshes_size = (meshes.len() * std::mem::size_of::<GPUMesh>()) as u64;
    let parts_size = (parts.len() * std::mem::size_of::<GPUMeshPart>()) as u64;
    let materials_size = (materials.len() * std::mem::size_of::<GPUMaterial>()) as u64;
    let lights_size = (lights.len() * std::mem::size_of::<GPULight>()) as u64;

    let scene_offset = 0u64;
    let drawables_offset = (align_up(scene_offset + scene_size, 256)) as u64;
    let draws_offset = (align_up(drawables_offset + drawables_size, 256)) as u64;
    let meshes_offset = (align_up(draws_offset + draws_size, 256)) as u64;
    let parts_offset = (align_up(meshes_offset + meshes_size, 256)) as u64;
    let materials_offset = (align_up(parts_offset + parts_size, 256)) as u64;
    let lights_offset = (align_up(materials_offset + materials_size, 256)) as u64;
    let buffer_size = lights_offset + lights_size.max(std::mem::size_of::<GPULight>() as u64);

    let scene_buffer = cmd_buffer.create_temporary_buffer(
        &BufferInfo {
            size: buffer_size as u64,
            usage: BufferUsage::STORAGE,
            sharing_mode: QueueSharingMode::Concurrent
        },
        MemoryUsage::MappableGPUMemory,
    ).unwrap();
    unsafe {
        profiling::scope!("Copying scene data to VRAM");

        let base_ptr = scene_buffer.map(false).unwrap();

        let mut ptr = base_ptr.add(scene_offset as usize);
        ptr.copy_from(std::mem::transmute(&local), scene_size as usize);

        ptr = base_ptr.add(drawables_offset as usize);
        ptr.copy_from(std::mem::transmute(drawables.as_ptr()), drawables_size as usize);

        ptr = base_ptr.add(draws_offset as usize);
        ptr.copy_from(std::mem::transmute(draws.as_ptr()), draws_size as usize);

        ptr = base_ptr.add(meshes_offset as usize);
        ptr.copy_from(std::mem::transmute(meshes.as_ptr()), meshes_size as usize);

        ptr = base_ptr.add(parts_offset as usize);
        ptr.copy_from(std::mem::transmute(parts.as_ptr()), parts_size as usize);

        ptr = base_ptr.add(materials_offset as usize);
        ptr.copy_from(std::mem::transmute(materials.as_ptr()), materials_size as usize);

        ptr = base_ptr.add(lights_offset as usize);
        ptr.copy_from(std::mem::transmute(lights.as_ptr()), lights_size as usize);

        scene_buffer.unmap(true);
    }

    SceneBuffers {
        buffer: scene_buffer,
        scene_buffer: BufferBinding {
            offset: scene_offset,
            length: scene_size.max(16)
        },
        draws_buffer: BufferBinding {
            offset: draws_offset,
            length: draws_size.max(16)
        },
        meshes_buffer: BufferBinding {
            offset: meshes_offset,
            length: meshes_size.max(16)
        },
        drawables_buffer: BufferBinding {
            offset: drawables_offset,
            length: drawables_size.max(16)
        },
        parts_buffer: BufferBinding {
            offset: parts_offset,
            length: parts_size.max(16)
        },
        materials_buffer: BufferBinding {
            offset: materials_offset,
            length: materials_size.max(16)
        },
        lights_buffer: BufferBinding {
            offset: lights_offset,
            length: lights_size.max(16)
        },
    }
}

#[allow(unused)]
fn align_up_to_cache_line(value: u64) -> u64 {
    const CACHE_LINE: u64 = 64;
    if value == 0 {
        return 0;
    }
    (value + CACHE_LINE - 1) & !(CACHE_LINE - 1)
}

fn align_up(value: u64, alignment: u64) -> u64 {
    if value == 0 {
        return 0;
    }
    (value + alignment - 1) & !(alignment - 1)
}
