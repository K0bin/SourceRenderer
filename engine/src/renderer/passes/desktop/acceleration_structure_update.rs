use std::sync::Arc;

use nalgebra::Vector3;
use sourcerenderer_core::{graphics::{Backend, CommandBuffer, AccelerationStructureInstance, Device, TopLevelAccelerationStructureInfo, BufferInfo, BufferUsage, MemoryUsage, BottomLevelAccelerationStructureInfo, AccelerationStructureMeshRange, IndexFormat, Format, Barrier, BarrierSync, BarrierAccess, FrontFace}, Matrix4};

use crate::renderer::renderer_scene::RendererScene;

pub struct AccelerationStructureUpdatePass<B: Backend> {
  device: Arc<B::Device>,
  acceleration_structure: Arc<B::AccelerationStructure>
}

impl<B: Backend> AccelerationStructureUpdatePass<B> {
  pub fn new(device: &Arc<B::Device>, init_cmd_buffer: &mut B::CommandBuffer) -> Self {
    let instances_buffer = init_cmd_buffer.upload_top_level_instances(&[]);
    let info = TopLevelAccelerationStructureInfo {
      instances_buffer: &instances_buffer,
      instances: &[]
    };
    let sizes = device.get_top_level_acceleration_structure_size(&info);
    let scratch_buffer = init_cmd_buffer.create_temporary_buffer(&BufferInfo {
      size: sizes.build_scratch_size as usize,
      usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
    }, MemoryUsage::GpuOnly);
    let buffer = device.create_buffer(&BufferInfo {
      size: sizes.size as usize,
      usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
    }, MemoryUsage::GpuOnly, Some("AccelerationStructure"));
    let acceleration_structure = init_cmd_buffer.create_top_level_acceleration_structure(&info, sizes.size as usize, &buffer, &scratch_buffer);

    Self {
      device: device.clone(),
      acceleration_structure
    }
  }

  pub fn execute(
    &mut self,
    cmd_buffer: &mut B::CommandBuffer,
    scene: &RendererScene<B>,
    camera_buffer: &Arc<B::Buffer>
  ) {
    let static_drawables = scene.static_drawables();

    let mut created_blas = false;
    let bl_acceleration_structures: Vec<Arc<B::AccelerationStructure>> = static_drawables
      .iter()
      .map(|drawable| {
        let blas = drawable.model.acceleration_structure().clone();
        blas.unwrap_or_else(|| {
          let blas = {
            let mesh = drawable.model.mesh();
            let parts: Vec<AccelerationStructureMeshRange> = mesh.parts.iter().map(|p| {
              debug_assert_eq!(p.start % 3, 0);
              debug_assert_eq!(p.count % 3, 0);
              AccelerationStructureMeshRange {
                primitive_start: p.start / 3,
                primitive_count: p.count / 3
              }
            }).collect();

            debug_assert_ne!(mesh.vertex_count, 0);
            let info = BottomLevelAccelerationStructureInfo {
              vertex_buffer: &mesh.vertices,
              index_buffer: mesh.indices.as_ref().unwrap(),
              index_format: IndexFormat::U32,
              vertex_position_offset: 0,
              vertex_format: Format::RGB32Float,
              vertex_stride: 44,
              mesh_parts: &parts,
              opaque: true,
              max_vertex: mesh.vertex_count - 1
            };
            let sizes = self.device.get_bottom_level_acceleration_structure_size(&info);

            let scratch_buffer = cmd_buffer.create_temporary_buffer(&BufferInfo {
              size: sizes.build_scratch_size as usize,
              usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
            }, MemoryUsage::GpuOnly);
            let buffer = self.device.create_buffer(&BufferInfo {
              size: sizes.size as usize,
              usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
            }, MemoryUsage::GpuOnly, Some("AccelerationStructure"));
          cmd_buffer.create_bottom_level_acceleration_structure(&info, sizes.size as usize, &buffer, &scratch_buffer)
        };
        drawable.model.set_acceleration_structure(&blas);
        created_blas = true;
        blas
      })
    }).collect();

    if created_blas {
      cmd_buffer.barrier(&[Barrier::GlobalBarrier {
        old_sync: BarrierSync::COMPUTE_SHADER | BarrierSync::ACCELERATION_STRUCTURE_BUILD,
        new_sync: BarrierSync::COMPUTE_SHADER | BarrierSync::ACCELERATION_STRUCTURE_BUILD,
        old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE | BarrierAccess::SHADER_WRITE,
        new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ | BarrierAccess::ACCELERATION_STRUCTURE_WRITE | BarrierAccess::SHADER_READ,
      }]);
      cmd_buffer.flush_barriers();
    }

    let mut instances = Vec::<AccelerationStructureInstance<B>>::with_capacity(static_drawables.len());
    for (bl, drawable) in bl_acceleration_structures.iter().zip(static_drawables.iter()) {
      instances.push(
        AccelerationStructureInstance::<B> {
          acceleration_structure: bl,
          transform: drawable.transform,
          front_face: FrontFace::CounterClockwise
        }
      );
    }

    let tl_instances_buffer = cmd_buffer.upload_top_level_instances(&instances[..]);

    let tl_info = TopLevelAccelerationStructureInfo {
      instances_buffer: &tl_instances_buffer,
      instances: &instances[..]
    };

    let sizes = self.device.get_top_level_acceleration_structure_size(&tl_info);
    let scratch_buffer = cmd_buffer.create_temporary_buffer(&BufferInfo {
      size: sizes.build_scratch_size as usize,
      usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
    }, MemoryUsage::GpuOnly);
    let buffer = self.device.create_buffer(&BufferInfo {
      size: sizes.size as usize,
      usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
    }, MemoryUsage::GpuOnly, Some("AccelerationStructure"));

    self.acceleration_structure = cmd_buffer.create_top_level_acceleration_structure(&tl_info, sizes.size as usize, &buffer, &scratch_buffer);

    cmd_buffer.barrier(&[Barrier::GlobalBarrier {
      old_sync: BarrierSync::COMPUTE_SHADER | BarrierSync::ACCELERATION_STRUCTURE_BUILD,
      new_sync: BarrierSync::COMPUTE_SHADER | BarrierSync::RAY_TRACING,
      old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE | BarrierAccess::SHADER_WRITE,
      new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ | BarrierAccess::SHADER_READ,
    }]);
  }

  pub fn acceleration_structure(&self) -> &Arc<B::AccelerationStructure> {
    &self.acceleration_structure
  }
}