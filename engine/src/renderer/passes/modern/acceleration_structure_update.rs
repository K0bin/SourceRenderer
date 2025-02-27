use std::collections::HashMap;
use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::{Matrix4, Platform};

use crate::renderer::render_path::RenderPassParameters;
use crate::asset::ModelHandle;
use crate::graphics::*;

pub struct AccelerationStructureUpdatePass<P: Platform> {
    blas_map: HashMap<ModelHandle, Arc<AccelerationStructure<P::GPUBackend>>>,
    acceleration_structure: Arc<AccelerationStructure<P::GPUBackend>>,
}

impl<P: Platform> AccelerationStructureUpdatePass<P> {
    pub fn new(
        _device: &Arc<Device<P::GPUBackend>>,
        init_cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>
    ) -> Self {
        let info = TopLevelAccelerationStructureInfo {
            instances: &[]
        };
        let acceleration_structure = init_cmd_buffer.create_top_level_acceleration_structure(
            &info, true
        ).unwrap();

        Self {
            blas_map: HashMap::new(),
            acceleration_structure: Arc::new(acceleration_structure),
        }
    }

    pub fn execute(
        &mut self,
        cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>
    ) {
        // We never reuse handles, so this works.
        let mut removed_models = SmallVec::<[ModelHandle; 4]>::new();
        for (handle, _) in &self.blas_map {
            if pass_params.assets.get_model(*handle).is_none() {
                removed_models.push(*handle);
            }
        }
        for handle in removed_models {
            self.blas_map.remove(&handle);
        }

        let static_drawables = pass_params.scene.scene.static_drawables();

        let mut created_blas = false;
        let mut bl_acceleration_structures =
            Vec::<Arc<AccelerationStructure<P::GPUBackend>>>::new();

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

                    Arc::new(cmd_buffer.create_bottom_level_acceleration_structure(&info, true).unwrap())
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

        let mut instances = Vec::<AccelerationStructureInstance<P::GPUBackend>>::with_capacity(
            static_drawables.len(),
        );
        for ((index, bl), drawable) in bl_acceleration_structures
            .iter()
            .enumerate()
            .zip(static_drawables.iter())
        {
            instances.push(AccelerationStructureInstance::<P::GPUBackend> {
                acceleration_structure: bl,
                transform: Matrix4::from(drawable.transform),
                front_face: FrontFace::Clockwise,
                id: index as u32
            });
        }

        let tl_info = TopLevelAccelerationStructureInfo {
            instances: &instances[..]
        };

        self.acceleration_structure = Arc::new(cmd_buffer.create_top_level_acceleration_structure(
            &tl_info, true
        ).unwrap());

        cmd_buffer.barrier(&[Barrier::GlobalBarrier {
            old_sync: BarrierSync::ACCELERATION_STRUCTURE_BUILD,
            new_sync: BarrierSync::RAY_TRACING,
            old_access: BarrierAccess::ACCELERATION_STRUCTURE_WRITE,
            new_access: BarrierAccess::ACCELERATION_STRUCTURE_READ,
        }]);
    }

    #[inline(always)]
    pub fn acceleration_structure(
        &self,
    ) -> &Arc<AccelerationStructure<P::GPUBackend>> {
        &self.acceleration_structure
    }
}
