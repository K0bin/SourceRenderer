use std::hash::Hash;
use std::sync::Arc;

use ash::vk::{
    self,
    AccelerationStructureBuildSizesInfoKHR,
    AccelerationStructureReferenceKHR,
    DeviceOrHostAddressConstKHR,
};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{
    self,
    Buffer as _,
};

use super::*;

pub struct VkAccelerationStructure {
    device: Arc<RawVkDevice>,
    buffer: vk::Buffer,
    acceleration_structure: vk::AccelerationStructureKHR,
    va: vk::DeviceAddress,
}

impl VkAccelerationStructure {
    pub unsafe fn upload_top_level_instances(
        target_buffer: &VkBuffer,
        target_buffer_offset: u64,
        instances: &[gpu::AccelerationStructureInstance<VkBackend>],
    ) {
        let instances: Vec<vk::AccelerationStructureInstanceKHR> = instances
            .iter()
            .map(|instance| {
                assert!(instance.id < ((1u32 << 25) - 1));

                let data = instance.transform.transpose().to_cols_array();

                let mut transform_data = [0f32; 12];
                transform_data.copy_from_slice(&data[0..12]);

                vk::AccelerationStructureInstanceKHR {
                    transform: vk::TransformMatrixKHR {
                        matrix: transform_data,
                    },
                    instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xFF),
                    instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                        instance.id,
                        if instance.front_face == gpu::FrontFace::CounterClockwise {
                            vk::GeometryInstanceFlagsKHR::TRIANGLE_FRONT_COUNTERCLOCKWISE.as_raw()
                                as u8
                        } else {
                            0
                        },
                    ),
                    acceleration_structure_reference: AccelerationStructureReferenceKHR {
                        device_handle: instance.acceleration_structure.va(),
                    },
                }
            })
            .collect();

        let size: u64 = std::mem::size_of_val(&instances) as u64;
        let ptr = target_buffer
            .map(target_buffer_offset, size, false)
            .expect("Failed to map buffer.")
            as *mut vk::AccelerationStructureInstanceKHR;
        ptr.copy_from(instances.as_ptr(), instances.len());
        target_buffer.unmap(target_buffer_offset, size, true);
    }

    pub fn top_level_size(
        device: &Arc<RawVkDevice>,
        info: &gpu::TopLevelAccelerationStructureInfo<VkBackend>,
    ) -> gpu::AccelerationStructureSizes {
        let acceleration_structure_funcs = &device.rt.as_ref().unwrap().acceleration_structure;

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
                instances: instances_data,
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
                host_address: std::ptr::null_mut(),
            },
            ..Default::default()
        };

        let size_info = unsafe {
            let mut size_info = AccelerationStructureBuildSizesInfoKHR::default();

            acceleration_structure_funcs.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[info.instances_count],
                &mut size_info,
            );

            size_info
        };
        gpu::AccelerationStructureSizes {
            build_scratch_size: size_info.build_scratch_size,
            update_scratch_size: size_info.update_scratch_size,
            size: size_info.acceleration_structure_size,
        }
    }

    pub fn new_top_level(
        device: &Arc<RawVkDevice>,
        info: &gpu::TopLevelAccelerationStructureInfo<VkBackend>,
        size: u64,
        target_buffer: &VkBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &VkBuffer,
        scratch_buffer_offset: u64,
        cmd_buffer: &vk::CommandBuffer,
    ) -> Self {
        let acceleration_structure_funcs = &device.rt.as_ref().unwrap().acceleration_structure;

        let acceleration_structure = unsafe {
            acceleration_structure_funcs.create_acceleration_structure(
                &vk::AccelerationStructureCreateInfoKHR {
                    create_flags: vk::AccelerationStructureCreateFlagsKHR::empty(),
                    buffer: target_buffer.handle(),
                    offset: target_buffer_offset as vk::DeviceSize,
                    size: size as vk::DeviceSize,
                    ty: vk::AccelerationStructureTypeKHR::TOP_LEVEL,
                    device_address: 0,
                    ..Default::default()
                },
                None,
            )
        }
        .unwrap();

        let va = unsafe {
            acceleration_structure_funcs.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR {
                    acceleration_structure,
                    ..Default::default()
                },
            )
        };

        let instances_data = vk::AccelerationStructureGeometryInstancesDataKHR {
            array_of_pointers: vk::FALSE,
            data: DeviceOrHostAddressConstKHR {
                device_address: info
                    .instances_buffer
                    .va_offset(info.instances_buffer_offset)
                    .unwrap(),
            },
            ..Default::default()
        };
        let geometry = vk::AccelerationStructureGeometryKHR {
            geometry_type: vk::GeometryTypeKHR::INSTANCES,
            geometry: vk::AccelerationStructureGeometryDataKHR {
                instances: instances_data,
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
                device_address: scratch_buffer.va_offset(scratch_buffer_offset).unwrap(),
            },
            ..Default::default()
        };

        unsafe {
            acceleration_structure_funcs.cmd_build_acceleration_structures(
                *cmd_buffer,
                &[build_info],
                &[&[vk::AccelerationStructureBuildRangeInfoKHR {
                    primitive_count: info.instances_count,
                    primitive_offset: 0,
                    first_vertex: 0,
                    transform_offset: 0,
                }]],
            );
        }

        Self {
            buffer: target_buffer.handle(),
            device: device.clone(),
            acceleration_structure,
            va,
        }
    }

    pub fn bottom_level_size(
        device: &Arc<RawVkDevice>,
        info: &gpu::BottomLevelAccelerationStructureInfo<VkBackend>,
    ) -> gpu::AccelerationStructureSizes {
        let acceleration_structure_funcs = &device.rt.as_ref().unwrap().acceleration_structure;

        let geometry_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
            vertex_format: format_to_vk(info.vertex_format, false),
            vertex_data: vk::DeviceOrHostAddressConstKHR {
                device_address: info.vertex_buffer.va().unwrap()
                    + info.vertex_position_offset as vk::DeviceSize,
            },
            vertex_stride: info.vertex_stride as vk::DeviceSize,
            max_vertex: info.max_vertex,
            index_type: index_format_to_vk(info.index_format),
            index_data: vk::DeviceOrHostAddressConstKHR {
                device_address: info.index_buffer.va().unwrap(),
            },
            transform_data: vk::DeviceOrHostAddressConstKHR {
                host_address: std::ptr::null(),
            },
            ..Default::default()
        };

        let geometry = vk::AccelerationStructureGeometryKHR {
            geometry_type: vk::GeometryTypeKHR::TRIANGLES,
            geometry: vk::AccelerationStructureGeometryDataKHR {
                triangles: geometry_data,
            },
            flags: if info.opaque {
                vk::GeometryFlagsKHR::OPAQUE
            } else {
                vk::GeometryFlagsKHR::empty()
            },
            ..Default::default()
        };

        let mut geometries =
            SmallVec::<[*const vk::AccelerationStructureGeometryKHR; 16]>::with_capacity(
                info.mesh_parts.len(),
            );
        let mut range_infos =
            SmallVec::<[vk::AccelerationStructureBuildRangeInfoKHR; 16]>::with_capacity(
                info.mesh_parts.len(),
            );
        let mut max_primitive_counts = SmallVec::<[u32; 16]>::with_capacity(info.mesh_parts.len());
        for part in info.mesh_parts.iter() {
            geometries.push(&geometry as *const vk::AccelerationStructureGeometryKHR);
            range_infos.push(vk::AccelerationStructureBuildRangeInfoKHR {
                primitive_count: part.primitive_count,
                primitive_offset: part.primitive_start
                    * 3
                    * if info.index_format == gpu::IndexFormat::U32 {
                        std::mem::size_of::<u32>() as u32
                    } else {
                        std::mem::size_of::<u16>() as u32
                    },
                first_vertex: 0,
                transform_offset: 0,
            });
            max_primitive_counts.push(part.primitive_count);
        }

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
            ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            flags: vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION
                | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            mode: vk::BuildAccelerationStructureModeKHR::BUILD,
            src_acceleration_structure: vk::AccelerationStructureKHR::null(),
            dst_acceleration_structure: vk::AccelerationStructureKHR::null(),
            geometry_count: geometries.len() as u32,
            p_geometries: std::ptr::null(),
            pp_geometries: geometries.as_ptr(),
            scratch_data: vk::DeviceOrHostAddressKHR {
                host_address: std::ptr::null_mut(),
            },
            ..Default::default()
        };

        let size_info = unsafe {
            let mut size_info = AccelerationStructureBuildSizesInfoKHR::default();

            acceleration_structure_funcs.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &max_primitive_counts[..],
                &mut size_info,
            );

            size_info
        };
        gpu::AccelerationStructureSizes {
            build_scratch_size: size_info.build_scratch_size,
            update_scratch_size: size_info.update_scratch_size,
            size: size_info.acceleration_structure_size,
        }
    }

    pub fn new_bottom_level(
        device: &Arc<RawVkDevice>,
        info: &gpu::BottomLevelAccelerationStructureInfo<VkBackend>,
        size: u64,
        target_buffer: &VkBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &VkBuffer,
        scratch_buffer_offset: u64,
        cmd_buffer: &vk::CommandBuffer,
    ) -> Self {
        let acceleration_structure_funcs = &device.rt.as_ref().unwrap().acceleration_structure;

        let acceleration_structure = unsafe {
            acceleration_structure_funcs.create_acceleration_structure(
                &vk::AccelerationStructureCreateInfoKHR {
                    create_flags: vk::AccelerationStructureCreateFlagsKHR::empty(),
                    buffer: target_buffer.handle(),
                    offset: target_buffer_offset as vk::DeviceSize,
                    size: size as vk::DeviceSize,
                    ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                    device_address: 0,
                    ..Default::default()
                },
                None,
            )
        }
        .unwrap();

        let va = unsafe {
            acceleration_structure_funcs.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR {
                    acceleration_structure,
                    ..Default::default()
                },
            )
        };

        let geometry_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
            vertex_format: format_to_vk(info.vertex_format, false),
            vertex_data: vk::DeviceOrHostAddressConstKHR {
                device_address: info
                    .vertex_buffer
                    .va_offset(info.vertex_buffer_offset)
                    .unwrap()
                    + info.vertex_position_offset as vk::DeviceSize,
            },
            vertex_stride: info.vertex_stride as vk::DeviceSize,
            max_vertex: info.max_vertex,
            index_type: index_format_to_vk(info.index_format),
            index_data: vk::DeviceOrHostAddressConstKHR {
                device_address: info
                    .index_buffer
                    .va_offset(info.index_buffer_offset)
                    .unwrap(),
            },
            transform_data: vk::DeviceOrHostAddressConstKHR {
                host_address: std::ptr::null(),
            },
            ..Default::default()
        };

        let geometry = vk::AccelerationStructureGeometryKHR {
            geometry_type: vk::GeometryTypeKHR::TRIANGLES,
            geometry: vk::AccelerationStructureGeometryDataKHR {
                triangles: geometry_data,
            },
            flags: if info.opaque {
                vk::GeometryFlagsKHR::OPAQUE
            } else {
                vk::GeometryFlagsKHR::empty()
            },
            ..Default::default()
        };

        let mut geometries =
            SmallVec::<[*const vk::AccelerationStructureGeometryKHR; 16]>::with_capacity(
                info.mesh_parts.len(),
            );
        let mut range_infos =
            SmallVec::<[vk::AccelerationStructureBuildRangeInfoKHR; 16]>::with_capacity(
                info.mesh_parts.len(),
            );
        for part in info.mesh_parts.iter() {
            geometries.push(&geometry as *const vk::AccelerationStructureGeometryKHR);
            range_infos.push(vk::AccelerationStructureBuildRangeInfoKHR {
                primitive_count: part.primitive_count,
                primitive_offset: part.primitive_start
                    * 3
                    * if info.index_format == gpu::IndexFormat::U32 {
                        std::mem::size_of::<u32>() as u32
                    } else {
                        std::mem::size_of::<u16>() as u32
                    },
                first_vertex: 0,
                transform_offset: 0,
            });
        }

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
            ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            flags: vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION
                | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            mode: vk::BuildAccelerationStructureModeKHR::BUILD,
            src_acceleration_structure: vk::AccelerationStructureKHR::null(),
            dst_acceleration_structure: acceleration_structure,
            geometry_count: geometries.len() as u32,
            p_geometries: std::ptr::null(),
            pp_geometries: geometries.as_ptr(),
            scratch_data: vk::DeviceOrHostAddressKHR {
                device_address: scratch_buffer
                    .va_offset(scratch_buffer_offset as u64)
                    .unwrap(),
            },
            ..Default::default()
        };

        unsafe {
            acceleration_structure_funcs.cmd_build_acceleration_structures(
                *cmd_buffer,
                &[build_info],
                &[&range_infos[..]],
            );
        }
        Self {
            buffer: target_buffer.handle(),
            device: device.clone(),
            acceleration_structure,
            va,
        }
    }

    #[inline(always)]
    fn va(&self) -> vk::DeviceAddress {
        self.va
    }

    #[inline(always)]
    pub(crate) fn handle(&self) -> vk::AccelerationStructureKHR {
        self.acceleration_structure
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
        let acceleration_structure_funcs = &self.device.rt.as_ref().unwrap().acceleration_structure;
        unsafe {
            acceleration_structure_funcs
                .destroy_acceleration_structure(self.acceleration_structure, None);

            self.device.destroy_buffer(self.buffer, None);
        }
    }
}

impl gpu::AccelerationStructure for VkAccelerationStructure {}
