use std::sync::Arc;

use smallvec::SmallVec;
use sourcerenderer_core::{
    Matrix4,
    Vec2,
    Vec2UI,
    Vec3,
    Vec3UI,
};

use super::PathTracerPass;
use crate::graphics::gpu::TextureViewInfo;
use crate::graphics::{
    Barrier,
    BarrierAccess,
    BarrierSync,
    BarrierTextureRange,
    BindingFrequency,
    BufferRef,
    BufferUsage,
    CommandBuffer,
    Device,
    GraphicsContext,
    MemoryUsage,
    QueueSubmission,
    QueueType,
    Swapchain,
    SwapchainError,
    TextureLayout,
    WHOLE_BUFFER,
};
use crate::renderer::asset::{
    RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::passes::blit::BlitPass;
use crate::renderer::passes::blue_noise::BlueNoise;
use crate::renderer::passes::modern::acceleration_structure_update::AccelerationStructureUpdatePass;
use crate::renderer::passes::modern::gpu_scene::SceneBuffers;
use crate::renderer::render_path::{
    FrameInfo,
    RenderPassParameters,
    RenderPath,
    RenderPathResult,
    SceneInfo,
};
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::ui::UIDrawData;

pub struct PathTracingRenderer {
    device: Arc<Device>,
    ui_data: UIDrawData,
    blue_noise: BlueNoise,
    acceleration_structure_update: AccelerationStructureUpdatePass,
    blit_pass: crate::renderer::passes::blit::BlitPass,
    path_tracer: PathTracerPass,
}

impl PathTracingRenderer {
    #[allow(unused)]
    pub fn new(
        device: &Arc<crate::graphics::Device>,
        swapchain: &crate::graphics::Swapchain,
        context: &mut GraphicsContext,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        let resolution = Vec2UI::new(swapchain.width() * 2, swapchain.height() * 2);

        let blue_noise = BlueNoise::new(device);

        if !device.supports_ray_tracing_query() {
            panic!("Need ray tracing support to run the path tracer");
        }
        let acceleration_structure_update =
            AccelerationStructureUpdatePass::new(device, &mut init_cmd_buffer);
        let blit_pass = BlitPass::new(resources, assets, swapchain.format());
        let path_tracer_pass =
            PathTracerPass::new(device, resolution, resources, assets, &mut init_cmd_buffer);

        init_cmd_buffer.flush_barriers();
        device.flush_transfers();

        device.submit(
            QueueType::Graphics,
            QueueSubmission {
                command_buffer: init_cmd_buffer.finish(),
                wait_fences: &[],
                signal_fences: &[],
                acquire_swapchain: None,
                release_swapchain: None,
            },
        );
        let c_device = device.clone();
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        task_pool
            .spawn(async move {
                crate::autoreleasepool(|| {
                    c_device.flush(QueueType::Graphics);
                })
            })
            .detach();
        Self {
            device: device.clone(),
            ui_data: UIDrawData::default(),
            blue_noise,
            acceleration_structure_update,
            blit_pass,
            path_tracer: path_tracer_pass,
        }
    }

    fn setup_frame(
        &self,
        cmd_buf: &mut CommandBuffer,
        scene: &SceneInfo,
        swapchain: &Swapchain,
        gpu_scene_buffers: SceneBuffers,
        camera_buffer: BufferRef,
        camera_history_buffer: BufferRef,
        rendering_resolution: &Vec2UI,
        frame: u64,
    ) {
        let view = &scene.scene.views()[scene.active_view_index];

        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            0,
            BufferRef::Transient(&gpu_scene_buffers.buffer),
            gpu_scene_buffers.scene_buffer.offset,
            gpu_scene_buffers.scene_buffer.length,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            1,
            BufferRef::Transient(&gpu_scene_buffers.buffer),
            gpu_scene_buffers.draws_buffer.offset,
            gpu_scene_buffers.draws_buffer.length,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            2,
            BufferRef::Transient(&gpu_scene_buffers.buffer),
            gpu_scene_buffers.meshes_buffer.offset,
            gpu_scene_buffers.meshes_buffer.length,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            3,
            BufferRef::Transient(&gpu_scene_buffers.buffer),
            gpu_scene_buffers.drawables_buffer.offset,
            gpu_scene_buffers.drawables_buffer.length,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            4,
            BufferRef::Transient(&gpu_scene_buffers.buffer),
            gpu_scene_buffers.parts_buffer.offset,
            gpu_scene_buffers.parts_buffer.length,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            5,
            BufferRef::Transient(&gpu_scene_buffers.buffer),
            gpu_scene_buffers.materials_buffer.offset,
            gpu_scene_buffers.materials_buffer.length,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            6,
            BufferRef::Transient(&gpu_scene_buffers.buffer),
            gpu_scene_buffers.lights_buffer.offset,
            gpu_scene_buffers.lights_buffer.length,
        );

        cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 7, camera_buffer, 0, WHOLE_BUFFER);
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            8,
            camera_history_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            9,
            scene.vertex_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            10,
            scene.index_buffer,
            0,
            WHOLE_BUFFER,
        );
        let cluster_count = Vec3UI::new(1u32, 1u32, 1u32);
        let cluster_z_scale = (cluster_count.z as f32) / (view.far_plane / view.near_plane).log2();
        let cluster_z_bias = -(cluster_count.z as f32) * (view.near_plane).log2()
            / (view.far_plane / view.near_plane).log2();

        let gpu_cascade_data: [ShadowCascade; 5] = Default::default();

        #[repr(C)]
        #[derive(Debug, Clone, Default)]
        struct ShadowCascade {
            light_mat: Matrix4,
            z_min: f32,
            z_max: f32,
            _padding: [u32; 2],
        }

        #[repr(C)]
        #[derive(Debug, Clone)]
        struct SetupBuffer {
            point_light_count: u32,
            directional_light_count: u32,
            cluster_z_bias: f32,
            cluster_z_scale: f32,
            cluster_count: Vec3UI,
            _padding: u32,
            swapchain_transform: Matrix4,
            halton_point: Vec2,
            rt_size: Vec2UI,
            cascades: [ShadowCascade; 5],
            cascade_count: u32,
            frame: u32,
        }

        let setup_buffer = cmd_buf
            .upload_dynamic_data(
                &[SetupBuffer {
                    point_light_count: scene.scene.point_lights().len() as u32,
                    directional_light_count: scene.scene.directional_lights().len() as u32,
                    cluster_z_bias,
                    cluster_z_scale,
                    cluster_count,
                    _padding: 0,
                    swapchain_transform: swapchain.transform(),
                    halton_point: crate::renderer::passes::taa::scaled_halton_point(
                        rendering_resolution.x,
                        rendering_resolution.y,
                        (frame % 8) as u32 + 1,
                    ),
                    rt_size: *rendering_resolution,
                    cascade_count: 0u32,
                    cascades: gpu_cascade_data,
                    frame: frame as u32,
                }],
                BufferUsage::CONSTANT,
            )
            .unwrap();
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            11,
            BufferRef::Transient(&setup_buffer),
            0,
            WHOLE_BUFFER,
        );
        #[repr(C)]
        #[derive(Debug, Clone)]
        struct PointLight {
            position: Vec3,
            intensity: f32,
        }
        let point_lights: SmallVec<[PointLight; 16]> = scene
            .scene
            .point_lights()
            .iter()
            .map(|l| PointLight {
                position: l.position,
                intensity: l.intensity,
            })
            .collect();
        let point_lights_buffer = cmd_buf
            .upload_dynamic_data(&point_lights, BufferUsage::CONSTANT)
            .unwrap();
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            12,
            BufferRef::Transient(&point_lights_buffer),
            0,
            WHOLE_BUFFER,
        );
        #[repr(C)]
        #[derive(Debug, Clone)]
        struct DirectionalLight {
            direction: Vec3,
            intensity: f32,
        }
        let directional_lights: SmallVec<[DirectionalLight; 16]> = scene
            .scene
            .directional_lights()
            .iter()
            .map(|l| DirectionalLight {
                direction: l.direction,
                intensity: l.intensity,
            })
            .collect();
        let directional_lights_buffer = cmd_buf
            .upload_dynamic_data(&directional_lights, BufferUsage::CONSTANT)
            .unwrap();
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            13,
            BufferRef::Transient(&directional_lights_buffer),
            0,
            WHOLE_BUFFER,
        );
    }
}

impl RenderPath for PathTracingRenderer {
    fn is_gpu_driven(&self) -> bool {
        true
    }

    fn write_occlusion_culling_results(&self, _frame: u64, _bitset: &mut Vec<u32>) {}

    fn on_swapchain_changed(&mut self, _swapchain: &Swapchain) {
        // TODO: resize render targets
    }

    fn is_ready(&self, assets: &RendererAssetsReadOnly) -> bool {
        self.path_tracer.is_ready(&assets)
    }

    #[profiling::function]
    fn render(
        &mut self,
        context: &mut GraphicsContext,
        swapchain: &mut Swapchain,
        scene: &SceneInfo,
        frame_info: &FrameInfo,
        resources: &mut RendererResources,
        assets: &RendererAssetsReadOnly<'_>,
    ) -> Result<RenderPathResult, SwapchainError> {
        let mut cmd_buf = context.get_command_buffer(QueueType::Graphics);

        let _main_view = &scene.scene.views()[scene.active_view_index];

        let camera_buffer = self
            .device
            .upload_data(
                &[0f32],
                MemoryUsage::MainMemoryWriteCombined,
                BufferUsage::CONSTANT,
            )
            .unwrap();
        let camera_history_buffer = self
            .device
            .upload_data(
                &[0f32],
                MemoryUsage::MainMemoryWriteCombined,
                BufferUsage::CONSTANT,
            )
            .unwrap();

        let scene_buffers = crate::renderer::passes::modern::gpu_scene::upload(
            &mut cmd_buf,
            scene.scene,
            0, /* TODO */
            &assets,
        );

        self.setup_frame(
            &mut cmd_buf,
            scene,
            swapchain,
            scene_buffers,
            BufferRef::Regular(&camera_buffer),
            BufferRef::Regular(&camera_history_buffer),
            &Vec2UI::new(swapchain.width(), swapchain.height()),
            frame_info.frame,
        );

        let params = RenderPassParameters {
            device: self.device.as_ref(),
            scene,
            resources,
            assets,
        };

        self.acceleration_structure_update
            .execute(&mut cmd_buf, &params);

        let blue_noise_sampler = params.resources.linear_sampler();
        self.path_tracer.execute(
            &mut cmd_buf,
            &params,
            self.acceleration_structure_update.acceleration_structure(),
            self.blue_noise.frame(frame_info.frame),
            blue_noise_sampler,
        );

        let backbuffer = swapchain.next_backbuffer()?;
        let backbuffer_view = swapchain.backbuffer_view(&backbuffer);
        let backbuffer_handle = swapchain.backbuffer_handle(&backbuffer);

        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::RENDER_TARGET,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::RENDER_TARGET_WRITE,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::RenderTarget,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None,
        }]);
        cmd_buf.flush_barriers();
        let rt_view = params.resources.access_view(
            &mut cmd_buf,
            PathTracerPass::PATH_TRACING_TARGET,
            BarrierSync::FRAGMENT_SHADER,
            BarrierAccess::SAMPLING_READ,
            TextureLayout::Sampled,
            false,
            &TextureViewInfo {
                base_mip_level: 0,
                mip_level_length: 1,
                base_array_layer: 0,
                array_layer_length: 1,
                format: None,
            },
            HistoryResourceEntry::Current,
        );
        let sampler = params.resources.linear_sampler();
        let resolution = Vec2UI::new(swapchain.width(), swapchain.height());
        self.blit_pass.execute(
            context,
            &mut cmd_buf,
            &params.assets,
            &rt_view,
            &backbuffer_view,
            sampler,
            resolution,
        );
        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::RENDER_TARGET,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::RENDER_TARGET_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::RenderTarget,
            new_layout: TextureLayout::Present,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None,
        }]);
        return Ok(RenderPathResult {
            cmd_buffer: cmd_buf.finish(),
            backbuffer: Some(backbuffer),
        });
    }

    fn set_ui_data(&mut self, data: crate::ui::UIDrawData) {
        self.ui_data = data;
    }
}
