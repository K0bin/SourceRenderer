use sourcerenderer_core::{Platform, Vec4};

use crate::math::Frustum;
use crate::renderer::drawable::View;
use crate::renderer::passes::modern::gpu_scene::{DRAWABLE_CAPACITY, PART_CAPACITY};
use crate::renderer::passes::modern::hi_z::HierarchicalZPass;
use crate::renderer::render_path::RenderPassParameters;
use crate::renderer::renderer_resources::{HistoryResourceEntry, RendererResources};
use crate::renderer::shader_manager::{ComputePipelineHandle, ShaderManager};

use crate::graphics::*;

pub struct DrawPrepPass {
    culling_pipeline: ComputePipelineHandle,
    prep_pipeline: ComputePipelineHandle,
}

impl DrawPrepPass {
    pub const VISIBLE_DRAWABLES_BITFIELD_BUFFER: &'static str = "VisibleDrawables";
    pub const INDIRECT_DRAW_BUFFER: &'static str = "IndirectDraws";

    pub fn new<P: Platform>(
        resources: &mut RendererResources<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let culling_pipeline = shader_manager.request_compute_pipeline("shaders/culling.comp.spv");
        let prep_pipeline = shader_manager.request_compute_pipeline("shaders/draw_prep.comp.spv");
        resources.create_buffer(
            Self::VISIBLE_DRAWABLES_BITFIELD_BUFFER,
            &BufferInfo {
                size: (DRAWABLE_CAPACITY as u64 + std::mem::size_of::<u32>() as u64 - 1)
                    / std::mem::size_of::<u32>() as u64,
                usage: BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            false,
        );
        resources.create_buffer(
            Self::INDIRECT_DRAW_BUFFER,
            &BufferInfo {
                size: 4 + 20 * PART_CAPACITY as u64,
                usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
                sharing_mode: QueueSharingMode::Exclusive
            },
            MemoryUsage::GPUMemory,
            false,
        );
        Self {
            culling_pipeline,
            prep_pipeline,
        }
    }

    pub fn execute<P: Platform>(
        &self,
        cmd_buffer: &mut CommandBufferRecorder<P::GPUBackend>,
        pass_params: &RenderPassParameters<'_, P>
    ) {
        {
            let view = &pass_params.scene.views[pass_params.scene.active_view_index];

            cmd_buffer.begin_label("Culling");
            let buffer = pass_params.resources.access_buffer(
                cmd_buffer,
                Self::VISIBLE_DRAWABLES_BITFIELD_BUFFER,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_WRITE,
                HistoryResourceEntry::Current,
            );

            let hi_z_mips = {
                let hi_z_info = pass_params.resources.texture_info(HierarchicalZPass::<P>::HI_Z_BUFFER_NAME);
                hi_z_info.mip_levels
            };
            let hi_z = pass_params.resources.access_view(
                cmd_buffer,
                HierarchicalZPass::<P>::HI_Z_BUFFER_NAME,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::SAMPLING_READ,
                TextureLayout::Sampled,
                false,
                &TextureViewInfo {
                    base_mip_level: 0,
                    mip_level_length: hi_z_mips,
                    base_array_layer: 0,
                    array_layer_length: 1,
                    format: None,
                },
                HistoryResourceEntry::Current,
            );

            #[repr(packed(16))]
            #[derive(Clone, Debug)]
            struct GPUFrustum {
                pub near_half_width: f32,
                pub near_half_height: f32,
                _padding: u32,
                _padding1: u32,
                pub planes: Vec4,
            }
            let frustum = Frustum::new(
                view.near_plane,
                view.far_plane,
                view.camera_fov,
                view.aspect_ratio,
            );
            let (frustum_x, frustum_y) = Frustum::extract_planes(&view.proj_matrix);

            cmd_buffer.bind_storage_buffer(
                BindingFrequency::VeryFrequent,
                0,
                BufferRef::Regular(&*buffer),
                0,
                WHOLE_BUFFER,
            );
            let frustum_buffer = cmd_buffer.upload_dynamic_data(
                &[GPUFrustum {
                    near_half_width: frustum.near_half_width,
                    near_half_height: frustum.near_half_height,
                    _padding: 0,
                    _padding1: 0,
                    planes: Vec4::new(frustum_x.x, frustum_x.z, frustum_y.y, frustum_y.z),
                }],
                BufferUsage::CONSTANT,
            ).unwrap();
            cmd_buffer.bind_uniform_buffer(
                BindingFrequency::VeryFrequent,
                1,
                BufferRef::Transient(&frustum_buffer),
                0,
                WHOLE_BUFFER,
            );
            cmd_buffer.bind_sampling_view_and_sampler(
                BindingFrequency::VeryFrequent,
                2,
                &*hi_z,
                pass_params.resources.nearest_sampler(),
            );
            let culling_pipeline = pass_params.shader_manager.get_compute_pipeline(self.culling_pipeline);
            cmd_buffer.set_pipeline(PipelineBinding::Compute(&culling_pipeline));
            cmd_buffer.flush_barriers();
            cmd_buffer.finish_binding();
            cmd_buffer.dispatch((pass_params.scene.scene.static_drawables().len() as u32 + 63) / 64, 1, 1);
            cmd_buffer.end_label();
        }

        cmd_buffer.begin_label("Preparing indirect draws");
        {
            let draw_buffer = pass_params.resources.access_buffer(
                cmd_buffer,
                Self::INDIRECT_DRAW_BUFFER,
                BarrierSync::COMPUTE_SHADER,
                BarrierAccess::STORAGE_WRITE,
                HistoryResourceEntry::Current,
            );
            cmd_buffer.flush_barriers();
            cmd_buffer.clear_storage_buffer(BufferRef::Regular(&draw_buffer), 0, 4, 0);
        }

        assert!(pass_params.scene.scene.static_drawables().len() as u32 <= DRAWABLE_CAPACITY);
        let part_count = pass_params.scene.scene
            .static_drawables()
            .iter()
            .map(|d| {
                pass_params.assets
                    .get_model(d.model)
                    .and_then(|m| pass_params.assets.get_mesh(m.mesh_handle()))
                    .map(|mesh| mesh.parts.len())
                    .unwrap_or(0)
            })
            .fold(0, |a, b| a + b) as u32;
        assert!(part_count <= PART_CAPACITY);

        let visibility_buffer = pass_params.resources.access_buffer(
            cmd_buffer,
            Self::VISIBLE_DRAWABLES_BITFIELD_BUFFER,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_READ,
            HistoryResourceEntry::Current,
        );
        let draw_buffer = pass_params.resources.access_buffer(
            cmd_buffer,
            Self::INDIRECT_DRAW_BUFFER,
            BarrierSync::COMPUTE_SHADER,
            BarrierAccess::STORAGE_WRITE,
            HistoryResourceEntry::Current,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            0,
            BufferRef::Regular(&*visibility_buffer),
            0,
            WHOLE_BUFFER,
        );
        cmd_buffer.bind_storage_buffer(
            BindingFrequency::VeryFrequent,
            1,
            BufferRef::Regular(&*draw_buffer),
            0,
            WHOLE_BUFFER,
        );
        let prep_pipeline = pass_params.shader_manager.get_compute_pipeline(self.prep_pipeline);
        cmd_buffer.set_pipeline(PipelineBinding::Compute(&prep_pipeline));
        cmd_buffer.flush_barriers();
        cmd_buffer.finish_binding();
        cmd_buffer.dispatch((part_count + 63) / 64, 1, 1);
        cmd_buffer.end_label();
    }
}

fn normalize_plane(p: Vec4) -> Vec4 {
    p / p.xyz().magnitude()
}
