use std::hash::Hash;
use std::sync::Arc;

use crate::command::{index_format_to_vk, VkCommandBuffer};
use crate::format::format_to_vk;
use crate::VkBackend;
use crate::{raw::RawVkDevice, buffer::VkBufferSlice};

use ash::vk::{self, AccelerationStructureReferenceKHR, DeviceOrHostAddressConstKHR};
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{BottomLevelAccelerationStructureInfo, AccelerationStructure, AccelerationStructureSizes, TopLevelAccelerationStructureInfo, FrontFace, BufferUsage, AccelerationStructureInstance};

pub struct VkAccelerationStructure {
  device: Arc<RawVkDevice>,
  buffer: Arc<VkBufferSlice>,
  acceleration_structure: vk::AccelerationStructureKHR,
  va: vk::DeviceAddress,

  // Bottom level
  vertex_buffer: Option<Arc<VkBufferSlice>>,
  index_buffer: Option<Arc<VkBufferSlice>>,

  // Top level
  bottom_level_structures: Vec<Arc<VkAccelerationStructure>>,
}

impl VkAccelerationStructure {
  pub fn upload_top_level_instances(command_buffer: &mut VkCommandBuffer, instances: &[AccelerationStructureInstance<VkBackend>]) -> Arc<VkBufferSlice> {
    let instances: Vec<vk::AccelerationStructureInstanceKHR> = instances.iter().map(|instance| {
      let mut transform_data = [0f32; 12];
      transform_data.copy_from_slice(&instance.transform.transpose().data.as_slice()[0 .. 12]);

      vk::AccelerationStructureInstanceKHR {
        transform: vk::TransformMatrixKHR {
          matrix: transform_data
        },
        instance_custom_index_and_mask: vk::Packed24_8::new(0, 1),
        instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, if instance.front_face == FrontFace::CounterClockwise { vk::GeometryInstanceFlagsKHR::TRIANGLE_FRONT_COUNTERCLOCKWISE.as_raw() as u8 } else { 0 }),
        acceleration_structure_reference: AccelerationStructureReferenceKHR {
          device_handle: instance.acceleration_structure.va()
        },
      }
    }).collect();

    command_buffer.upload_dynamic_data(&instances[..], BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE)
  }

  pub fn top_level_size(device: &Arc<RawVkDevice>, info: &TopLevelAccelerationStructureInfo<VkBackend>) -> AccelerationStructureSizes {
    let rt = device.rt.as_ref().unwrap();

    let instances_data = vk::AccelerationStructureGeometryInstancesDataKHR {
      array_of_pointers: vk::FALSE,
      data: DeviceOrHostAddressConstKHR {
        device_address: info.instances_buffer.va().unwrap(),
      },
      ..Default::default()
    };
    let geometry = vk::AccelerationStructureGeometryKHR {
      geometry_type: vk::GeometryTypeKHR::INSTANCES,
      geometry: vk::AccelerationStructureGeometryDataKHR {
        instances: instances_data
      },
      flags: vk::GeometryFlagsKHR::empty(),
      ..Default::default()
    };

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
      ty: vk::AccelerationStructureTypeKHR::TOP_LEVEL,
      flags: vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
      mode: vk::BuildAccelerationStructureModeKHR::BUILD,
      src_acceleration_structure: vk::AccelerationStructureKHR::null(),
      dst_acceleration_structure: vk::AccelerationStructureKHR::null(),
      geometry_count: 1,
      p_geometries: &geometry as *const vk::AccelerationStructureGeometryKHR,
      pp_geometries: std::ptr::null(),
      scratch_data: vk::DeviceOrHostAddressKHR {
        host_address: std::ptr::null_mut()
      },
      ..Default::default()
    };

    let size_info = unsafe {
      rt.acceleration_structure.get_acceleration_structure_build_sizes(
        vk::AccelerationStructureBuildTypeKHR::DEVICE,
        &build_info,
        &[info.instances.len() as u32]
      )
    };
    AccelerationStructureSizes {
      build_scratch_size: size_info.build_scratch_size,
      update_scratch_size: size_info.update_scratch_size,
      size: size_info.acceleration_structure_size
    }
  }

  pub fn new_top_level(device: &Arc<RawVkDevice>, info: &TopLevelAccelerationStructureInfo<VkBackend>, size: usize, target_buffer: &Arc<VkBufferSlice>, scratch_buffer: &Arc<VkBufferSlice>, cmd_buffer: &vk::CommandBuffer) -> Self {
    let rt = device.rt.as_ref().unwrap();

    let acceleration_structure = unsafe {
      rt.acceleration_structure.create_acceleration_structure(&vk::AccelerationStructureCreateInfoKHR {
        create_flags: vk::AccelerationStructureCreateFlagsKHR::empty(),
        buffer: *target_buffer.get_buffer().get_handle(),
        offset: target_buffer.get_offset() as vk::DeviceSize,
        size: size as vk::DeviceSize,
        ty: vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        device_address: 0,
        ..Default::default()
      }, None)
    }.unwrap();

    let va = unsafe {
      rt.acceleration_structure.get_acceleration_structure_device_address(&vk::AccelerationStructureDeviceAddressInfoKHR {
        acceleration_structure,
        ..Default::default()
      })
    };

    let instances_data = vk::AccelerationStructureGeometryInstancesDataKHR {
      array_of_pointers: vk::FALSE,
      data: DeviceOrHostAddressConstKHR {
        device_address: info.instances_buffer.va().unwrap(),
      },
      ..Default::default()
    };
    let geometry = vk::AccelerationStructureGeometryKHR {
      geometry_type: vk::GeometryTypeKHR::INSTANCES,
      geometry: vk::AccelerationStructureGeometryDataKHR {
        instances: instances_data
      },
      flags: vk::GeometryFlagsKHR::empty(),
      ..Default::default()
    };

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
      ty: vk::AccelerationStructureTypeKHR::TOP_LEVEL,
      flags: vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
      mode: vk::BuildAccelerationStructureModeKHR::BUILD,
      src_acceleration_structure: vk::AccelerationStructureKHR::null(),
      dst_acceleration_structure: acceleration_structure,
      geometry_count: 1,
      p_geometries: &geometry as *const vk::AccelerationStructureGeometryKHR,
      pp_geometries: std::ptr::null(),
      scratch_data: vk::DeviceOrHostAddressKHR {
        device_address: scratch_buffer.va().unwrap()
      },
      ..Default::default()
    };

    unsafe {
      rt.acceleration_structure.cmd_build_acceleration_structures(*cmd_buffer, &[build_info], &[&[
        vk::AccelerationStructureBuildRangeInfoKHR {
          primitive_count: info.instances.len() as u32,
          primitive_offset: 0,
          first_vertex: 0,
          transform_offset: 0,
        }
      ]]);
    }

    let bottom_level_structures: Vec<Arc<VkAccelerationStructure>> = info.instances.iter().map(|i| i.acceleration_structure).cloned().collect();
    Self {
      buffer: target_buffer.clone(),
      device: device.clone(),
      acceleration_structure: acceleration_structure,
      va,
      bottom_level_structures,
      vertex_buffer: None,
      index_buffer: None,
    }
  }

  pub fn bottom_level_size(device: &Arc<RawVkDevice>, info: &BottomLevelAccelerationStructureInfo<VkBackend>) -> AccelerationStructureSizes {
    let rt = device.rt.as_ref().unwrap();

    let geometry_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
      vertex_format: format_to_vk(info.vertex_format),
      vertex_data: vk::DeviceOrHostAddressConstKHR {
        device_address: info.vertex_buffer.va().unwrap() + info.vertex_position_offset as vk::DeviceSize
      },
      vertex_stride: info.vertex_stride as vk::DeviceSize,
      max_vertex: info.max_vertex,
      index_type: index_format_to_vk(info.index_format),
      index_data: vk::DeviceOrHostAddressConstKHR {
        device_address: info.index_buffer.va().unwrap()
      },
      transform_data: vk::DeviceOrHostAddressConstKHR {
        host_address: std::ptr::null()
      },
      ..Default::default()
    };

    let geometry = vk::AccelerationStructureGeometryKHR {
      geometry_type: vk::GeometryTypeKHR::TRIANGLES,
      geometry: vk::AccelerationStructureGeometryDataKHR {
        triangles: geometry_data
      },
      flags: if info.opaque { vk::GeometryFlagsKHR::OPAQUE } else { vk::GeometryFlagsKHR::empty() },
      ..Default::default()
    };

    let mut geometries = SmallVec::<[*const vk::AccelerationStructureGeometryKHR; 16]>::with_capacity(info.mesh_parts.len());
    let mut range_infos = SmallVec::<[vk::AccelerationStructureBuildRangeInfoKHR; 16]>::with_capacity(info.mesh_parts.len());
    let mut max_primitive_counts = SmallVec::<[u32; 16]>::with_capacity(info.mesh_parts.len());
    for part in info.mesh_parts.iter() {
      geometries.push(&geometry as *const vk::AccelerationStructureGeometryKHR);
      range_infos.push(vk::AccelerationStructureBuildRangeInfoKHR {
        primitive_count: part.primitive_count,
        primitive_offset: part.primitive_start,
        first_vertex: 0,
        transform_offset: 0,
      });
      max_primitive_counts.push(part.primitive_count);
    }

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
      ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
      flags: vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
      mode: vk::BuildAccelerationStructureModeKHR::BUILD,
      src_acceleration_structure: vk::AccelerationStructureKHR::null(),
      dst_acceleration_structure: vk::AccelerationStructureKHR::null(),
      geometry_count: geometries.len() as u32,
      p_geometries: std::ptr::null(),
      pp_geometries: geometries.as_ptr(),
      scratch_data: vk::DeviceOrHostAddressKHR {
        host_address: std::ptr::null_mut()
      },
      ..Default::default()
    };

    let size_info = unsafe {
      rt.acceleration_structure.get_acceleration_structure_build_sizes(
        vk::AccelerationStructureBuildTypeKHR::DEVICE,
        &build_info,
        &max_primitive_counts[..]
      )
    };
    AccelerationStructureSizes {
      build_scratch_size: size_info.build_scratch_size,
      update_scratch_size: size_info.update_scratch_size,
      size: size_info.acceleration_structure_size
    }
  }

  pub fn new_bottom_level(device: &Arc<RawVkDevice>, info: &BottomLevelAccelerationStructureInfo<VkBackend>, size: usize, target_buffer: &Arc<VkBufferSlice>, scratch_buffer: &Arc<VkBufferSlice>, cmd_buffer: &vk::CommandBuffer) -> Self {
    let rt = device.rt.as_ref().unwrap();

    let acceleration_structure = unsafe {
      rt.acceleration_structure.create_acceleration_structure(&vk::AccelerationStructureCreateInfoKHR {
        create_flags: vk::AccelerationStructureCreateFlagsKHR::empty(),
        buffer: *target_buffer.get_buffer().get_handle(),
        offset: target_buffer.get_offset() as vk::DeviceSize,
        size: size as vk::DeviceSize,
        ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
        device_address: 0,
        ..Default::default()
      }, None)
    }.unwrap();


    let va = unsafe {
      rt.acceleration_structure.get_acceleration_structure_device_address(&vk::AccelerationStructureDeviceAddressInfoKHR {
        acceleration_structure,
        ..Default::default()
      })
    };

    let geometry_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
      vertex_format: format_to_vk(info.vertex_format),
      vertex_data: vk::DeviceOrHostAddressConstKHR {
        device_address: info.vertex_buffer.va().unwrap() + info.vertex_position_offset as vk::DeviceSize
      },
      vertex_stride: info.vertex_stride as vk::DeviceSize,
      max_vertex: info.vertex_buffer.get_length() as u32 / info.vertex_stride,
      index_type: index_format_to_vk(info.index_format),
      index_data: vk::DeviceOrHostAddressConstKHR {
        device_address: info.index_buffer.va().unwrap()
      },
      transform_data: vk::DeviceOrHostAddressConstKHR {
        host_address: std::ptr::null()
      },
      ..Default::default()
    };

    let geometry = vk::AccelerationStructureGeometryKHR {
      geometry_type: vk::GeometryTypeKHR::TRIANGLES,
      geometry: vk::AccelerationStructureGeometryDataKHR {
        triangles: geometry_data
      },
      flags: if info.opaque { vk::GeometryFlagsKHR::OPAQUE } else { vk::GeometryFlagsKHR::empty() },
      ..Default::default()
    };

    let mut geometries = SmallVec::<[*const vk::AccelerationStructureGeometryKHR; 16]>::with_capacity(info.mesh_parts.len());
    let mut range_infos = SmallVec::<[vk::AccelerationStructureBuildRangeInfoKHR; 16]>::with_capacity(info.mesh_parts.len());
    for part in info.mesh_parts.iter() {
      geometries.push(&geometry as *const vk::AccelerationStructureGeometryKHR);
      range_infos.push(vk::AccelerationStructureBuildRangeInfoKHR {
        primitive_count: part.primitive_count,
        primitive_offset: part.primitive_start,
        first_vertex: 0,
        transform_offset: 0,
      });
    }

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
      ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
      flags: vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
      mode: vk::BuildAccelerationStructureModeKHR::BUILD,
      src_acceleration_structure: vk::AccelerationStructureKHR::null(),
      dst_acceleration_structure: acceleration_structure,
      geometry_count: geometries.len() as u32,
      p_geometries: std::ptr::null(),
      pp_geometries: geometries.as_ptr(),
      scratch_data: vk::DeviceOrHostAddressKHR {
        device_address: scratch_buffer.va().unwrap()
      },
      ..Default::default()
    };

    unsafe {
      rt.acceleration_structure.cmd_build_acceleration_structures(*cmd_buffer, &[build_info], &[&range_infos[..]]);
    }
    Self {
      buffer: target_buffer.clone(),
      device: device.clone(),
      acceleration_structure: acceleration_structure,
      bottom_level_structures: Vec::new(),
      va,
      vertex_buffer: Some(info.vertex_buffer.clone()),
      index_buffer: Some(info.index_buffer.clone()),
    }
  }

  fn va(&self) -> vk::DeviceAddress {
    self.va
  }

  pub(crate) fn handle(&self) -> &vk::AccelerationStructureKHR {
    &self.acceleration_structure
  }
}

impl PartialEq for VkAccelerationStructure {
  fn eq(&self, other: &Self) -> bool {
    self.acceleration_structure == other.acceleration_structure
  }
}

impl Eq for VkAccelerationStructure {}

impl Hash for VkAccelerationStructure {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.acceleration_structure.hash(state);
  }
}

impl Drop for VkAccelerationStructure {
  fn drop(&mut self) {
    let rt = self.device.rt.as_ref().unwrap();
    unsafe {
      rt.acceleration_structure.destroy_acceleration_structure(self.acceleration_structure, None);
    }
  }
}

impl AccelerationStructure for VkAccelerationStructure {}
