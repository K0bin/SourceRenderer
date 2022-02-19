use std::sync::Arc;

use crate::command::index_format_to_vk;
use crate::format::format_to_vk;
use crate::VkBackend;
use crate::{raw::RawVkDevice, buffer::VkBufferSlice};

use ash::vk;
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{BottomLevelAccelerationStructureInfo, AccelerationStructure, AccelerationStructureSizes};

pub struct VkAccelerationStructure {
  device: Arc<RawVkDevice>,
  buffer: Arc<VkBufferSlice>,
  acceleration_structure: vk::AccelerationStructureKHR
}

impl VkAccelerationStructure {
  pub fn bottom_level_size(device: &Arc<RawVkDevice>, info: &BottomLevelAccelerationStructureInfo<VkBackend>) -> AccelerationStructureSizes {
    let rt = device.rt.as_ref().unwrap();

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
    let mut max_primitive_count = 0;
    for part in info.mesh_parts.iter() {
      geometries.push(&geometry as *const vk::AccelerationStructureGeometryKHR);
      range_infos.push(vk::AccelerationStructureBuildRangeInfoKHR {
        primitive_count: part.primitive_count,
        primitive_offset: part.primitive_start,
        first_vertex: 0,
        transform_offset: 0,
      });
      if part.primitive_count > max_primitive_count {
        max_primitive_count = part.primitive_count;
      }
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
        &[max_primitive_count]
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
      acceleration_structure: acceleration_structure
    }
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
