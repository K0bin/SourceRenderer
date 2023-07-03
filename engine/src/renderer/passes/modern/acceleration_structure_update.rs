use std::collections::HashMap;
use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::graphics::{
    AccelerationStructureInstance,
    AccelerationStructureMeshRange,
    Backend,
    Barrier,
    BarrierAccess,
    BarrierSync,
    BottomLevelAccelerationStructureInfo,
    BufferInfo,
    BufferUsage,
    CommandBuffer,
    Device,
    Format,
    FrontFace,
    IndexFormat,
    MemoryUsage,
    TopLevelAccelerationStructureInfo,
};
use sourcerenderer_core::Platform;

use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_assets::{
    ModelHandle
};

pub struct AccelerationStructureUpdatePass<P: Platform> {
    device: Arc<<P::GraphicsBackend as Backend>::Device>,
    blas_map: HashMap<ModelHandle, Arc<<P::GraphicsBackend as Backend>::AccelerationStructure>>,
    acceleration_structure: Arc<<P::GraphicsBackend as Backend>::AccelerationStructure>,
}

impl<P: Platform> AccelerationStructureUpdatePass<P> {
    pub fn new(
        device: &Arc<<P::GraphicsBackend as Backend>::Device>,
        init_cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
    ) -> Self {
        let instances_buffer = init_cmd_buffer.upload_top_level_instances(&[]);
        let info = TopLevelAccelerationStructureInfo {
            instances_buffer: &instances_buffer,
            instances: &[],
        };
        let sizes = device.get_top_level_acceleration_structure_size(&info);
        let scratch_buffer = init_cmd_buffer.create_temporary_buffer(
            &BufferInfo {
                size: sizes.build_scratch_size as usize,
                usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
            },
            MemoryUsage::VRAM,
        );
        let buffer = device.create_buffer(
            &BufferInfo {
                size: sizes.size as usize,
                usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
            },
            MemoryUsage::VRAM,
            Some("AccelerationStructure"),
        );
        let acceleration_structure = init_cmd_buffer.create_top_level_acceleration_structure(
            &info,
            sizes.size as usize,
            &buffer,
            &scratch_buffer,
        );

        Self {
            device: device.clone(),
            blas_map: HashMap::new(),
            acceleration_structure,
        }
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        pass_params: &RenderPassParameters<'_, P>
    ) {
        // We never reuse handles, so this works.
        let mut removed_models = SmallVec::<[ModelHandle; 4]>::new();
        for (handle, _) in &self.blas_map {
            if !pass_params.assets.has_model(*handle) {
                removed_models.push(*handle);
            }
        }
        for handle in removed_models {
            self.blas_map.remove(&handle);
        }

        let static_drawables = pass_params.scene.scene.static_drawables();

        let mut created_blas = false;
        let mut bl_acceleration_structures =
            Vec::<Arc<<P::GraphicsBackend as Backend>::AccelerationStructure>>::new();

        for drawable in static_drawables {
            let blas = self.blas_map.get(&drawable.model).cloned().or_else(|| {
                let model = pass_params.assets.get_model(drawable.model);
                if model.is_none() {
                    return None;
                }
                let model = model.unwrap();
                let mesh = pass_params.assets.get_mesh(model.mesh_handle());
                if mesh.is_none() {
                    return None;
                }
                let mesh = mesh.unwrap();

                let blas = {
                    let parts: Vec<AccelerationStructureMeshRange> = mesh
                        .parts
                        .iter()
                        .map(|p| {
                            debug_assert_eq!(p.start % 3, 0);
                            debug_assert_eq!(p.count % 3, 0);
                            AccelerationStructureMeshRange {
                                primitive_start: p.start / 3,
                                primitive_count: p.count / 3,
                            }
                        })
                        .collect();

                    debug_assert_ne!(mesh.vertex_count, 0);
                    let info = BottomLevelAccelerationStructureInfo {
                        vertex_buffer: mesh.vertices.buffer(),
                        vertex_buffer_offset: mesh.vertices.offset() as usize,
                        index_buffer: mesh.indices.as_ref().unwrap().buffer(),
                        index_buffer_offset: mesh.indices.as_ref().unwrap().offset() as usize,
                        index_format: IndexFormat::U32,
                        vertex_position_offset: 0,
                        vertex_format: Format::RGB32Float,
                        vertex_stride: std::mem::size_of::<crate::renderer::Vertex>() as u32,
                        mesh_parts: &parts,
                        opaque: true,
                        max_vertex: mesh.vertex_count - 1,
                    };
                    let sizes = self
                        .device
                        .get_bottom_level_acceleration_structure_size(&info);

                    let scratch_buffer = cmd_buffer.create_temporary_buffer(
                        &BufferInfo {
                            size: sizes.build_scratch_size as usize,
                            usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
                        },
                        MemoryUsage::VRAM,
                    );
                    let buffer = self.device.create_buffer(
                        &BufferInfo {
                            size: sizes.size as usize,
                            usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
                        },
                        MemoryUsage::VRAM,
                        Some("AccelerationStructure"),
                    );
                    cmd_buffer.create_bottom_level_acceleration_structure(
                        &info,
                        sizes.size as usize,
                        &buffer,
                        &scratch_buffer,
                    )
                };
                self.blas_map.insert(drawable.model, blas.clone());
                created_blas = true;
                Some(blas)
            });

            if let Some(blas) = blas {
                bl_acceleration_structures.push(blas);
            }
        }

        if created_blas {
            cmd_buffer.barrier(&[Barrier::GlobalBarrier {
                old_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                new_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
                old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
                new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ
                    | BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
            }]);
            cmd_buffer.flush_barriers();
        }

        let mut instances = Vec::<AccelerationStructureInstance<P::GraphicsBackend>>::with_capacity(
            static_drawables.len(),
        );
        for (bl, drawable) in bl_acceleration_structures
            .iter()
            .zip(static_drawables.iter())
        {
            instances.push(AccelerationStructureInstance::<P::GraphicsBackend> {
                acceleration_structure: bl,
                transform: drawable.transform,
                front_face: FrontFace::Clockwise,
            });
        }

        let tl_instances_buffer = cmd_buffer.upload_top_level_instances(&instances[..]);

        let tl_info = TopLevelAccelerationStructureInfo {
            instances_buffer: &tl_instances_buffer,
            instances: &instances[..],
        };

        let sizes = self
            .device
            .get_top_level_acceleration_structure_size(&tl_info);
        let scratch_buffer = cmd_buffer.create_temporary_buffer(
            &BufferInfo {
                size: sizes.build_scratch_size as usize,
                usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
            },
            MemoryUsage::VRAM,
        );
        let buffer = self.device.create_buffer(
            &BufferInfo {
                size: sizes.size as usize,
                usage: BufferUsage::ACCELERATION_STRUCTURE | BufferUsage::STORAGE,
            },
            MemoryUsage::VRAM,
            Some("AccelerationStructure"),
        );

        self.acceleration_structure = cmd_buffer.create_top_level_acceleration_structure(
            &tl_info,
            sizes.size as usize,
            &buffer,
            &scratch_buffer,
        );

        cmd_buffer.barrier(&[Barrier::GlobalBarrier {
            old_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
            new_sync: BarrierSync::RAY_TRACING,
            old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
            new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ,
        }]);
    }

    pub fn acceleration_structure(
        &self,
    ) -> &Arc<<P::GraphicsBackend as Backend>::AccelerationStructure> {
        &self.acceleration_structure
    }
}
