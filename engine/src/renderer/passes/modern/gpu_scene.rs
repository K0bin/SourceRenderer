use std::{collections::HashMap, sync::Arc};

use sourcerenderer_core::{Vec4, Vec3, Matrix4, graphics::{Backend, CommandBuffer, BufferInfo, BufferUsage, MemoryUsage, Buffer}};

use crate::renderer::{renderer_scene::RendererScene, renderer_assets::{RendererMesh, RendererMaterial, RendererMaterialValue}};

use super::draw_prep::{PART_CAPACITY, DRAWABLE_CAPACITY};

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUScene {
  part_count: u32,
  material_count: u32,
  drawable_count: u32,
  aabb_count: u32,
  parts: [GPUDrawableRange; PART_CAPACITY as usize],
  materials: [GPUMaterial; 1024],
  drawables: [GPUDrawable; DRAWABLE_CAPACITY as usize],
  aabbs: [GPUBoundingBox; 1024]
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUDrawableRange {
  material_index: u32,
  drawable_index: u32,
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
  aabb_index: u32,
  part_start: u32,
  part_count: u32,
  _padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
struct GPUBoundingBox {
  min: Vec4,
  max: Vec4
}

pub(crate) fn upload<B: Backend>(cmd_buffer: &mut B::CommandBuffer, scene: &RendererScene<B>, zero_view_index: u32) -> Arc<B::Buffer> {
  let buffer = cmd_buffer.create_temporary_buffer(&BufferInfo {
    size: std::mem::size_of::<GPUScene>(),
    usage: BufferUsage::STORAGE,
  }, MemoryUsage::CpuToGpu);
  let mut map = buffer.map_mut::<GPUScene>().unwrap();
  debug_assert!(scene.static_drawables().len() < map.parts.len()); // pls compile that to a const instead of somehow reading uncached memory
  map.drawable_count = scene.static_drawables().len() as u32;

  let mut mesh_map = HashMap::<u64, u32>::new();
  let mut material_map = HashMap::<u64, u32>::new();
  let mut aabb_count: u32 = 0;
  let mut material_count: u32 = 0;
  let mut drawable_count: u32 = 0;
  let mut part_count: u32 = 0;
  for drawable in scene.static_drawables() {
    let mesh = drawable.model.mesh();
    let mesh_ptr = mesh.as_ref() as *const RendererMesh<B> as u64;

    let aabb_index = if let Some(aabb_index) = mesh_map.get(&mesh_ptr) {
      *aabb_index
    } else {
      let aabb_index = aabb_count;
      debug_assert!(aabb_count < map.aabbs.len() as u32);
      let aabb = mesh.bounding_box.as_ref().map(|bb| GPUBoundingBox {
        min: Vec4::new(bb.min.x, bb.min.y, bb.min.z, 1f32),
        max: Vec4::new(bb.max.x, bb.max.y, bb.max.z, 1f32)
      }).unwrap_or_else(|| GPUBoundingBox {
        min: Vec4::new(f32::MIN, f32::MIN, f32::MIN, 1f32),
        max: Vec4::new(f32::MAX, f32::MAX, f32::MAX, 1f32)
      });
      debug_assert!(aabb_index < map.aabbs.len() as u32);
      map.aabbs[aabb_index as usize] = aabb;
      mesh_map.insert(mesh_ptr, aabb_index);
      aabb_count += 1;
      aabb_index
    };

    let drawable_index = drawable_count;
    debug_assert!(drawable_index < map.drawables.len() as u32);
    {
      let gpu_drawable = &mut map.drawables[drawable_index as usize];
      gpu_drawable.transform = drawable.transform;
      gpu_drawable.old_transform = drawable.old_transform;
      gpu_drawable.aabb_index = aabb_index;
      gpu_drawable.part_start = part_count;
      gpu_drawable.part_count = drawable.model.mesh().parts.len() as u32;
      drawable_count += 1;
    }

    let materials = drawable.model.materials();
    for (index, part) in drawable.model.mesh().parts.iter().enumerate() {
      let material = &materials[index];
      let material_ptr = material.as_ref() as *const RendererMaterial<B> as u64;
      let material_index = if let Some(material_index) = material_map.get(&material_ptr) {
        *material_index
      } else {
        let material_index = material_count;
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
        map.materials[material_index as usize] = gpu_material;
        debug_assert!(material_count < map.materials.len() as u32);
        material_map.insert(material_ptr, material_index);
        material_count += 1;
        material_index
      };

      let part_index = part_count;
      debug_assert!(part_index < map.parts.len() as u32);
      let indices = mesh.indices.as_ref().expect("Non indexed drawing is not supported");
      let vertices = &mesh.vertices;
      assert_eq!(indices.offset() % (std::mem::size_of::<u32>() as u32), 0);
      assert_eq!(vertices.offset() % 44, 0);
      map.parts[part_index as usize].material_index = material_index;
      map.parts[part_index as usize].drawable_index = drawable_index;
      map.parts[part_index as usize].mesh_first_index = part.start + indices.offset() / std::mem::size_of::<u32>() as u32;
      map.parts[part_index as usize].mesh_index_count = part.count;
      map.parts[part_index as usize].mesh_vertex_offset = vertices.offset() / 44; // TODO: hardcoded vertex size
      part_count += 1;
    }
  }

  map.aabb_count = aabb_count;
  map.material_count = material_count;
  map.part_count = part_count;
  std::mem::drop(map);
  buffer
}
