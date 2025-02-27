use std::{collections::HashMap, sync::Arc};

use metal;
use metal::foreign_types::ForeignType;

use objc::{msg_send, sel, sel_impl};
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Buffer as _};

use super::*;

pub struct MTLAccelerationStructure {
    acceleration_structure: metal::AccelerationStructure,
    is_blas: bool,
    shared: Arc<MTLShared>
}

impl MTLAccelerationStructure {
    pub(crate) unsafe fn upload_top_level_instances(
        shared: &Arc<MTLShared>,
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64,
        instances: &[gpu::AccelerationStructureInstance<MTLBackend>],
    ) {
        let mut instances_index_map = HashMap::<usize, u32>::with_capacity(instances.len());
        {
            let list = shared.acceleration_structure_list.lock().unwrap();
            for (index, blas) in list.iter().enumerate() {
                instances_index_map.insert(blas.as_ptr() as usize, index as u32);
            }
        }

        let instances: Vec<metal::MTLAccelerationStructureUserIDInstanceDescriptor> = instances
            .iter()
            .map(|instance| {
                let mut transform_data = [[0f32; 3]; 4];
                for col in 0..4 {
                    for row in 0..3 {
                        transform_data[col][row] = instance.transform.col(col as usize)[row as usize];
                    }
                }

                let mut options = metal::MTLAccelerationStructureInstanceOptions::Opaque;
                if instance.front_face == gpu::FrontFace::CounterClockwise {
                    options |= metal::MTLAccelerationStructureInstanceOptions::TriangleFrontFacingWindingCounterClockwise;
                }

                let index = instances_index_map.get(&(instance.acceleration_structure.acceleration_structure.as_ptr() as usize));
                metal::MTLAccelerationStructureUserIDInstanceDescriptor {
                    transformation_matrix: transform_data,
                    options,
                    mask: 0xFFFFu32,
                    intersection_function_table_offset: 0u32,
                    acceleration_structure_index: *index.unwrap(),
                    user_id: instance.id
                }
            })
            .collect();

        let size: u64 = std::mem::size_of_val(&instances) as u64;
        let ptr = target_buffer.map(target_buffer_offset, size, false).expect("Failed to map buffer.") as *mut metal::MTLAccelerationStructureUserIDInstanceDescriptor;
        ptr.copy_from(instances.as_ptr(), instances.len());
        target_buffer.unmap(target_buffer_offset, size, true);
    }

    fn bottom_level_descriptor(info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>) -> metal::PrimitiveAccelerationStructureDescriptor {
        let descriptor = metal::PrimitiveAccelerationStructureDescriptor::descriptor();
        let mut geometries = SmallVec::<[metal::AccelerationStructureGeometryDescriptor; 16]>::with_capacity(info.mesh_parts.len());
        for part in info.mesh_parts {
            let geometry: metal::AccelerationStructureTriangleGeometryDescriptor = metal::AccelerationStructureTriangleGeometryDescriptor::descriptor();
            geometry.set_index_buffer(Some(info.index_buffer.handle()));
            geometry.set_index_buffer_offset(info.index_buffer_offset
                + (part.primitive_start as u64) * 3 * (if info.index_format == gpu::IndexFormat::U16 { 2 } else { 4 }));
            geometry.set_vertex_buffer(Some(info.vertex_buffer.handle()));
            geometry.set_vertex_buffer_offset(info.vertex_buffer_offset + info.vertex_position_offset as u64);
            geometry.set_index_type(index_format_to_mtl(info.index_format));
            geometry.set_vertex_stride(info.vertex_stride as u64);
            geometry.set_opaque(info.opaque);
            geometry.set_triangle_count(part.primitive_count as u64);
            geometry.set_vertex_format(format_to_mtl_attribute_format(info.vertex_format));
            let _: () = unsafe { msg_send![&geometry as &metal::AccelerationStructureTriangleGeometryDescriptorRef, retain] };
            geometries.push(metal::AccelerationStructureGeometryDescriptor::from(geometry));
        }
        let geometries_array = metal::Array::from_owned_slice(&geometries);
        descriptor.set_geometry_descriptors(geometries_array);
        let _: () = unsafe { msg_send![&descriptor as &metal::PrimitiveAccelerationStructureDescriptorRef, retain] };
        descriptor
    }

    fn top_level_descriptor(info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>, instances: &[metal::AccelerationStructure]) -> metal::InstanceAccelerationStructureDescriptor {
        let descriptor = metal::InstanceAccelerationStructureDescriptor::descriptor();
        descriptor.set_instance_descriptor_type(metal::MTLAccelerationStructureInstanceDescriptorType::UserID);
        descriptor.set_instance_count(info.instances_count as u64);
        descriptor.set_instanced_acceleration_structures(&metal::Array::from_owned_slice(instances));
        descriptor.set_instance_descriptor_buffer_offset(info.instances_buffer_offset);
        descriptor.set_instance_descriptor_buffer(info.instances_buffer.handle());
        descriptor.set_instance_descriptor_stride(std::mem::size_of::<metal::MTLAccelerationStructureUserIDInstanceDescriptor>() as u64);
        let _: () = unsafe { msg_send![&descriptor as &metal::InstanceAccelerationStructureDescriptorRef, retain] };
        descriptor
    }

    pub(crate) fn bottom_level_size(device: &metal::DeviceRef, info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>) -> gpu::AccelerationStructureSizes {
        let sizes = device.acceleration_structure_sizes_with_descriptor(&Self::bottom_level_descriptor(info));
        gpu::AccelerationStructureSizes {
            size: sizes.acceleration_structure_size,
            build_scratch_size: sizes.build_scratch_buffer_size,
            update_scratch_size: sizes.refit_scratch_buffer_size,
        }
    }

    pub(crate) fn top_level_size(device: &metal::DeviceRef, shared: &Arc<MTLShared>, info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>) -> gpu::AccelerationStructureSizes {
        let guard = shared.acceleration_structure_list.lock().unwrap();
        let descriptor = Self::top_level_descriptor(info, &guard);
        let sizes = device.acceleration_structure_sizes_with_descriptor(&descriptor);
        gpu::AccelerationStructureSizes {
            size: sizes.acceleration_structure_size,
            build_scratch_size: sizes.build_scratch_buffer_size,
            update_scratch_size: sizes.refit_scratch_buffer_size,
        }
    }

    pub(crate) fn new_bottom_level(encoder: &metal::AccelerationStructureCommandEncoderRef, shared: &Arc<MTLShared>, size: u64, target_buffer: &MTLBuffer, target_buffer_offset: u64, scratch_buffer: &MTLBuffer, scratch_buffer_offset: u64, info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>, _cmd_buffer: &metal::CommandBuffer) -> Self {
        let descriptor = Self::bottom_level_descriptor(info);
        let heap = target_buffer.handle().heap();
        let acceleration_structure: metal::AccelerationStructure = unsafe { msg_send![heap, newAccelerationStructureWithSize: size offset:target_buffer_offset] };
        encoder.build_acceleration_structure(&acceleration_structure, &descriptor, scratch_buffer.handle(), scratch_buffer_offset);
        {
            let mut list = shared.acceleration_structure_list.lock().unwrap();
            list.push(acceleration_structure.clone());
        }
        Self {
            acceleration_structure,
            shared: shared.clone(),
            is_blas: true
        }
    }

    pub(crate) fn new_top_level(encoder: &metal::AccelerationStructureCommandEncoderRef, shared: &Arc<MTLShared>, size: u64, target_buffer: &MTLBuffer, target_buffer_offset: u64, scratch_buffer: &MTLBuffer, scratch_buffer_offset: u64, info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>, _cmd_buffer: &metal::CommandBuffer) -> Self {
        let guard = shared.acceleration_structure_list.lock().unwrap();
        let descriptor = Self::top_level_descriptor(info, &guard);
        let heap = target_buffer.handle().heap();
        let acceleration_structure: metal::AccelerationStructure = unsafe { msg_send![heap, newAccelerationStructureWithSize: size offset:target_buffer_offset] };
        encoder.build_acceleration_structure(&acceleration_structure, &descriptor, scratch_buffer.handle(), scratch_buffer_offset);
        Self {
            acceleration_structure,
            shared: shared.clone(),
            is_blas: false
        }
    }

    pub(crate) fn handle(&self) -> &metal::AccelerationStructureRef {
        &self.acceleration_structure
    }
}

impl gpu::AccelerationStructure for MTLAccelerationStructure {}

impl Drop for MTLAccelerationStructure {
    fn drop(&mut self) {
        if self.is_blas {
            let mut lock_guard = self.shared.acceleration_structure_list.lock().unwrap();
            let index = lock_guard.iter().enumerate().find_map(|(index, blas)| if blas.as_ptr() == self.acceleration_structure.as_ptr() {
                Some(index)
            } else {
                None
            });
            lock_guard.remove(index.unwrap());
        }
    }
}
