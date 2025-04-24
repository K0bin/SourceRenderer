use std::{collections::HashMap, sync::Arc};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSArray, NSUInteger};
use objc2_metal::{self, MTLAccelerationStructureCommandEncoder, MTLDevice, MTLHeap, MTLResource};

use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, Buffer as _};

use super::*;

pub struct MTLAccelerationStructure {
    acceleration_structure: Retained<ProtocolObject<dyn objc2_metal::MTLAccelerationStructure>>,
    is_blas: bool,
    shared: Arc<MTLShared>,
}

unsafe impl Send for MTLAccelerationStructure {}
unsafe impl Sync for MTLAccelerationStructure {}

impl MTLAccelerationStructure {
    pub(crate) unsafe fn upload_top_level_instances(
        shared: &Arc<MTLShared>,
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64,
        instances: &[gpu::AccelerationStructureInstance<MTLBackend>],
    ) {
        let mut instances_index_map = HashMap::<
            Retained<ProtocolObject<dyn objc2_metal::MTLAccelerationStructure>>,
            u32,
        >::with_capacity(instances.len());
        {
            let list = shared.acceleration_structure_list.lock().unwrap();
            for (index, blas) in list.iter().enumerate() {
                instances_index_map.insert(blas.clone(), index as u32);
            }
        }

        let instances: Vec<objc2_metal::MTLAccelerationStructureUserIDInstanceDescriptor> = instances
            .iter()
            .map(|instance| {
                let mut transform_data = [objc2_metal::MTLPackedFloat3 { x: 0.0f32, y: 0.0f32, z: 0.0f32 }; 4];
                for col in 0..4 {
                    transform_data[col].x = instance.transform.col(col as usize).x;
                    transform_data[col].y = instance.transform.col(col as usize).z;
                    transform_data[col].z = instance.transform.col(col as usize).y;
                }

                let mut options = objc2_metal::MTLAccelerationStructureInstanceOptions::Opaque;
                if instance.front_face == gpu::FrontFace::CounterClockwise {
                    options |= objc2_metal::MTLAccelerationStructureInstanceOptions::TriangleFrontFacingWindingCounterClockwise;
                }

                let index = instances_index_map.get(&instance.acceleration_structure.acceleration_structure);
                objc2_metal::MTLAccelerationStructureUserIDInstanceDescriptor {
                    transformationMatrix: objc2_metal::MTLPackedFloat4x3 { columns: transform_data },
                    options,
                    mask: 0xFFFFu32,
                    intersectionFunctionTableOffset: 0u32,
                    accelerationStructureIndex: *index.unwrap(),
                    userID: instance.id
                }
            })
            .collect();

        let size: u64 = std::mem::size_of_val(&instances) as u64;
        let ptr = target_buffer
            .map(target_buffer_offset, size, false)
            .expect("Failed to map buffer.")
            as *mut objc2_metal::MTLAccelerationStructureUserIDInstanceDescriptor;
        ptr.copy_from(instances.as_ptr(), instances.len());
        target_buffer.unmap(target_buffer_offset, size, true);
    }

    unsafe fn bottom_level_descriptor(
        info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>,
    ) -> Retained<objc2_metal::MTLPrimitiveAccelerationStructureDescriptor> {
        let descriptor = objc2_metal::MTLPrimitiveAccelerationStructureDescriptor::new();
        let mut geometries = SmallVec::<
            [Retained<objc2_metal::MTLAccelerationStructureGeometryDescriptor>; 16],
        >::with_capacity(info.mesh_parts.len());
        for part in info.mesh_parts {
            let geometry = objc2_metal::MTLAccelerationStructureTriangleGeometryDescriptor::new();
            geometry.setIndexBuffer(Some(info.index_buffer.handle()));
            geometry.setIndexBufferOffset(
                info.index_buffer_offset as NSUInteger
                    + (part.primitive_start as NSUInteger)
                        * 3
                        * (if info.index_format == gpu::IndexFormat::U16 {
                            2
                        } else {
                            4
                        }),
            );
            geometry.setVertexBuffer(Some(info.vertex_buffer.handle()));
            geometry.setVertexBufferOffset(
                info.vertex_buffer_offset as NSUInteger + info.vertex_position_offset as NSUInteger,
            );
            geometry.setIndexType(index_format_to_mtl(info.index_format));
            geometry.setVertexStride(info.vertex_stride as NSUInteger);
            geometry.setOpaque(info.opaque);
            geometry.setTriangleCount(part.primitive_count as NSUInteger);
            geometry.setVertexFormat(format_to_mtl_attribute_format(info.vertex_format));
            geometries.push(geometry.downcast().unwrap());
        }
        let mut geometry_refs = SmallVec::<
            [&objc2_metal::MTLAccelerationStructureGeometryDescriptor; 16],
        >::with_capacity(info.mesh_parts.len());
        for geometry in &geometries {
            geometry_refs.push(geometry.as_ref());
        }
        let geometries_array = NSArray::from_slice(&geometry_refs);
        descriptor.setGeometryDescriptors(Some(geometries_array.as_ref()));
        descriptor
    }

    unsafe fn top_level_descriptor(
        info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>,
        instances: &[Retained<ProtocolObject<dyn objc2_metal::MTLAccelerationStructure>>],
    ) -> Retained<objc2_metal::MTLInstanceAccelerationStructureDescriptor> {
        let mut instances_refs = SmallVec::<
            [&ProtocolObject<dyn objc2_metal::MTLAccelerationStructure>; 16],
        >::with_capacity(instances.len());
        for instance in instances {
            instances_refs.push(instance.as_ref());
        }
        let instances_nsarray = objc2_foundation::NSArray::from_slice(&instances_refs);

        let descriptor = objc2_metal::MTLInstanceAccelerationStructureDescriptor::descriptor();
        descriptor.setInstanceDescriptorType(
            objc2_metal::MTLAccelerationStructureInstanceDescriptorType::UserID,
        );
        descriptor.setInstanceCount(info.instances_count as NSUInteger);
        descriptor.setInstancedAccelerationStructures(Some(&instances_nsarray));
        descriptor.setInstanceDescriptorBufferOffset(info.instances_buffer_offset as NSUInteger);
        descriptor.setInstanceDescriptorBuffer(Some(info.instances_buffer.handle()));
        descriptor.setInstanceDescriptorStride(std::mem::size_of::<
            objc2_metal::MTLAccelerationStructureUserIDInstanceDescriptor,
        >() as NSUInteger);
        descriptor
    }

    pub(crate) unsafe fn bottom_level_size(
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>,
    ) -> gpu::AccelerationStructureSizes {
        let descriptor: Retained<objc2_metal::MTLAccelerationStructureDescriptor> =
            Self::bottom_level_descriptor(info).downcast().unwrap();
        let sizes = device.accelerationStructureSizesWithDescriptor(&descriptor);
        gpu::AccelerationStructureSizes {
            size: sizes.accelerationStructureSize as u64,
            build_scratch_size: sizes.buildScratchBufferSize as u64,
            update_scratch_size: sizes.refitScratchBufferSize as u64,
        }
    }

    pub(crate) unsafe fn top_level_size(
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        shared: &Arc<MTLShared>,
        info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>,
    ) -> gpu::AccelerationStructureSizes {
        let guard = shared.acceleration_structure_list.lock().unwrap();
        let descriptor = Self::top_level_descriptor(info, &guard);
        let sizes = device.accelerationStructureSizesWithDescriptor(&descriptor);
        gpu::AccelerationStructureSizes {
            size: sizes.accelerationStructureSize as u64,
            build_scratch_size: sizes.buildScratchBufferSize as u64,
            update_scratch_size: sizes.refitScratchBufferSize as u64,
        }
    }

    pub(crate) unsafe fn new_bottom_level(
        encoder: &ProtocolObject<dyn objc2_metal::MTLAccelerationStructureCommandEncoder>,
        shared: &Arc<MTLShared>,
        size: u64,
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &MTLBuffer,
        scratch_buffer_offset: u64,
        info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>,
        _cmd_buffer: &ProtocolObject<dyn objc2_metal::MTLCommandBuffer>,
    ) -> Self {
        let descriptor = Self::bottom_level_descriptor(info);
        let heap = target_buffer.handle().heap().unwrap();
        let acceleration_structure = heap
            .newAccelerationStructureWithSize_offset(
                size as NSUInteger,
                target_buffer_offset as NSUInteger,
            )
            .unwrap();
        encoder.buildAccelerationStructure_descriptor_scratchBuffer_scratchBufferOffset(
            &acceleration_structure,
            &descriptor,
            scratch_buffer.handle(),
            scratch_buffer_offset as NSUInteger,
        );
        {
            let mut list = shared.acceleration_structure_list.lock().unwrap();
            list.push(acceleration_structure.clone());
        }
        Self {
            acceleration_structure,
            shared: shared.clone(),
            is_blas: true,
        }
    }

    pub(crate) unsafe fn new_top_level(
        encoder: &ProtocolObject<dyn objc2_metal::MTLAccelerationStructureCommandEncoder>,
        shared: &Arc<MTLShared>,
        size: u64,
        target_buffer: &MTLBuffer,
        target_buffer_offset: u64,
        scratch_buffer: &MTLBuffer,
        scratch_buffer_offset: u64,
        info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>,
        _cmd_buffer: &ProtocolObject<dyn objc2_metal::MTLCommandBuffer>,
    ) -> Self {
        let guard = shared.acceleration_structure_list.lock().unwrap();
        let descriptor = Self::top_level_descriptor(info, &guard);
        let heap = target_buffer.handle().heap().unwrap();
        let acceleration_structure = heap
            .newAccelerationStructureWithSize_offset(
                size as NSUInteger,
                target_buffer_offset as NSUInteger,
            )
            .unwrap();
        encoder.buildAccelerationStructure_descriptor_scratchBuffer_scratchBufferOffset(
            &acceleration_structure,
            &descriptor,
            scratch_buffer.handle(),
            scratch_buffer_offset as NSUInteger,
        );
        Self {
            acceleration_structure,
            shared: shared.clone(),
            is_blas: false,
        }
    }

    pub(crate) fn handle(&self) -> &ProtocolObject<dyn objc2_metal::MTLAccelerationStructure> {
        &self.acceleration_structure
    }
}

impl gpu::AccelerationStructure for MTLAccelerationStructure {}

impl Drop for MTLAccelerationStructure {
    fn drop(&mut self) {
        if self.is_blas {
            let mut lock_guard = self.shared.acceleration_structure_list.lock().unwrap();
            let index = lock_guard.iter().enumerate().find_map(|(index, blas)| {
                if blas == &self.acceleration_structure {
                    Some(index)
                } else {
                    None
                }
            });
            lock_guard.remove(index.unwrap());
        }
    }
}
