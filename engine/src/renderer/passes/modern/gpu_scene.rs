use std::{collections::HashMap, sync::Arc, mem::MaybeUninit};

use field_offset::offset_of;
use sourcerenderer_core::{Vec4, Matrix4, graphics::{Backend, CommandBuffer, BufferInfo, BufferUsage, MemoryUsage, Buffer}};

use crate::renderer::{renderer_scene::RendererScene, renderer_assets::{RendererMaterial, RendererMaterialValue, RendererModel}};

pub const DRAWABLE_CAPACITY: u32 = 4096;
pub const PART_CAPACITY: u32 = 4096;
pub const DRAW_CAPACITY: u32 = 4096;
pub const MATERIAL_CAPACITY: u32 = 4096;
pub const MESH_CAPACITY: u32 = 4096;

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUScene {
  part_count: u32,
  material_count: u32,
  drawable_count: u32,
  mesh_count: u32,
  draw_count: u32,
  _padding: u32,
  _padding1: u32,
  _padding2: u32,
  draws: [GPUDraw; DRAW_CAPACITY as usize],
  parts: [GPUMeshPart; PART_CAPACITY as usize],
  materials: [GPUMaterial; MATERIAL_CAPACITY as usize],
  drawables: [GPUDrawable; DRAWABLE_CAPACITY as usize],
  meshes: [GPUMesh; MESH_CAPACITY as usize],
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUDraw {
  drawable_index: u32,
  part_index: u32,
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

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUDrawable {
  transform: Matrix4,
  old_transform: Matrix4,
  mesh_index: u32,
  _padding: u32,
  _padding1: u32,
  _padding2: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUBoundingBox {
  min: Vec4,
  max: Vec4
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUMesh {
  aabb: GPUBoundingBox,
}

#[profiling::function]
pub(crate) fn upload<B: Backend>(cmd_buffer: &mut B::CommandBuffer, scene: &RendererScene<B>, zero_view_index: u32) -> Arc<B::Buffer> {
  let mut scene_box = Box::new(MaybeUninit::<GPUScene>::uninit()); // TODO reuse the same allocation
  let local = unsafe { scene_box.assume_init_mut() };
  {
    profiling::scope!("CollectingSceneData");

    struct ModelEntry {
      mesh_index: u32,
      part_start: u32,
      part_count: u32
    }
    let mut model_map = HashMap::<u64, ModelEntry>::new();
    let mut material_map = HashMap::<u64, u32>::new();
    local.mesh_count = 0;
    local.draw_count = 0;
    local.material_count = 0;
    local.drawable_count = 0;
    local.part_count = 0;
    for drawable in scene.static_drawables() {
      let model = &drawable.model;
      let mesh = drawable.model.mesh();
      let model_ptr = model.as_ref() as *const RendererModel<B> as u64;

      let model_entry = if let Some(model_entry) = model_map.get(&model_ptr) {
        model_entry
      } else {
        let materials = drawable.model.materials();
        let model_part_start = local.part_count;
        for (index, part) in mesh.parts.iter().enumerate() {
          let material = &materials[index];
          let material_ptr = material.as_ref() as *const RendererMaterial<B> as u64;
          let material_index = if let Some(material_index) = material_map.get(&material_ptr) {
            *material_index
          } else {
            let material_index = local.material_count;
            let mut gpu_material = GPUMaterial {
              albedo: Vec4::new(1f32, 1f32, 1f32, 1f32),
              roughness_factor: 1f32,
              metalness_factor: 0f32,
              albedo_texture_index: zero_view_index,
              _padding: 0
            };

            let albedo_value = material.get("albedo").unwrap();
            match albedo_value {
              RendererMaterialValue::Texture(texture) => {
                let albedo_view = &texture.view;
                cmd_buffer.track_texture_view(albedo_view);
                gpu_material.albedo_texture_index = texture.bindless_index.unwrap();
              },
              RendererMaterialValue::Vec4(val) => {
                gpu_material.albedo = *val
              },
              RendererMaterialValue::Float(_) => unimplemented!()
            }
            let roughness_value = material.get("roughness");
            match roughness_value {
              Some(RendererMaterialValue::Texture(_texture)) => {
                unimplemented!()
              }
              Some(RendererMaterialValue::Vec4(_)) => unimplemented!(),
              Some(RendererMaterialValue::Float(val)) => {
                gpu_material.roughness_factor = *val;
              },
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
              },
              None => {}
            }
            local.materials[material_index as usize] = gpu_material;
            debug_assert!(local.material_count < local.materials.len() as u32);
            material_map.insert(material_ptr, material_index);
            local.material_count += 1;
            material_index
          };

          let part_index = local.part_count;
          debug_assert!(part_index < local.parts.len() as u32);
          let indices = mesh.indices.as_ref().expect("Non indexed drawing is not supported");
          let vertices = &mesh.vertices;
          assert_eq!(indices.offset() % (std::mem::size_of::<u32>() as u32), 0);
          assert_eq!(vertices.offset() % 44, 0);
          let gpu_part = &mut local.parts[part_index as usize];
          gpu_part.material_index = material_index;
          gpu_part.mesh_first_index = part.start + indices.offset() / std::mem::size_of::<u32>() as u32;
          gpu_part.mesh_index_count = part.count;
          gpu_part.mesh_vertex_offset = vertices.offset() / 44; // TODO: hardcoded vertex size
          local.part_count += 1;
        }

        let mesh_index = local.mesh_count;
        debug_assert!(local.mesh_count < local.meshes.len() as u32);
        let mesh = mesh.bounding_box.as_ref().map(|bb| GPUMesh { aabb: GPUBoundingBox {
          min: Vec4::new(bb.min.x, bb.min.y, bb.min.z, 1f32),
          max: Vec4::new(bb.max.x, bb.max.y, bb.max.z, 1f32)
        }}).unwrap_or_else(|| GPUMesh { aabb: GPUBoundingBox {
          min: Vec4::new(f32::MIN, f32::MIN, f32::MIN, 1f32),
          max: Vec4::new(f32::MAX, f32::MAX, f32::MAX, 1f32)
        }});
        debug_assert!(mesh_index < local.meshes.len() as u32);
        local.meshes[mesh_index as usize] = mesh;
        model_map.insert(model_ptr, ModelEntry {
          mesh_index,
          part_start: model_part_start,
          part_count: local.part_count - model_part_start
        });
        local.mesh_count += 1;
        model_map.get(&model_ptr).unwrap()
      };

      let drawable_index = local.drawable_count;
      debug_assert!(drawable_index < local.drawables.len() as u32);
      {
        let gpu_drawable = &mut local.drawables[drawable_index as usize];
        gpu_drawable.transform = drawable.transform;
        gpu_drawable.old_transform = drawable.old_transform;
        gpu_drawable.mesh_index = model_entry.mesh_index;
        local.drawable_count += 1;
      }

      for part_index in model_entry.part_start .. (model_entry.part_start + model_entry.part_count) {
        let draw_index = local.draw_count;
        let gpu_draw = &mut local.draws[draw_index as usize];
        gpu_draw.drawable_index = drawable_index;
        gpu_draw.part_index = part_index;
        local.draw_count += 1;
      }
    }
    debug_assert!(scene.static_drawables().len() < local.parts.len());
  }

  let buffer_size = align_up_to_cache_line(std::mem::size_of::<GPUScene>());
  let buffer = cmd_buffer.create_temporary_buffer(&BufferInfo {
    size: buffer_size,
    usage: BufferUsage::STORAGE,
  }, MemoryUsage::MappableVRAM);
  unsafe {
    profiling::scope!("Copying scene data to VRAM");
    let dst_base = buffer.map_unsafe(false).unwrap();
    let src_base = local as *const GPUScene as *const u8;
    let draws_offset = offset_of!(GPUScene => draws).get_byte_offset();
    let parts_offset = offset_of!(GPUScene => parts).get_byte_offset();
    let materials_offset = offset_of!(GPUScene => materials).get_byte_offset();
    let drawables_offset = offset_of!(GPUScene => drawables).get_byte_offset();
    let meshes_offset = offset_of!(GPUScene => meshes).get_byte_offset();

    std::ptr::copy(src_base, dst_base, align_up_to_cache_line(draws_offset + local.draw_count as usize * std::mem::size_of::<GPUDraw>()));
    std::ptr::copy(src_base.add(parts_offset), dst_base.add(parts_offset), align_up_to_cache_line(local.part_count as usize * std::mem::size_of::<GPUMeshPart>()));
    std::ptr::copy(src_base.add(materials_offset), dst_base.add(materials_offset), align_up_to_cache_line(local.material_count as usize * std::mem::size_of::<GPUMaterial>()));
    std::ptr::copy(src_base.add(drawables_offset), dst_base.add(drawables_offset), align_up_to_cache_line(local.drawable_count as usize * std::mem::size_of::<GPUDrawable>()));
    std::ptr::copy(src_base.add(meshes_offset), dst_base.add(meshes_offset), align_up_to_cache_line(local.mesh_count as usize * std::mem::size_of::<GPUMesh>()));
    buffer.unmap_unsafe(true);
  }
  buffer
}

fn align_up_to_cache_line(value: usize) -> usize {
  const CACHE_LINE: usize = 64;
  if value == 0 {
    return 0
  }
  (value + CACHE_LINE - 1) & !(CACHE_LINE - 1)
}
