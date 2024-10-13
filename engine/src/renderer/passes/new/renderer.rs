use std::sync::Arc;

use nalgebra::Vector3;
use smallvec::SmallVec;
use crate::graphics::{Barrier, BarrierAccess, BarrierSync, BarrierTextureRange, BindingFrequency, BufferRef, BufferUsage, CommandBufferRecorder, Device, GraphicsContext, MemoryUsage, QueueSubmission, QueueType, Swapchain, SwapchainError, TextureInfo, TextureLayout, WHOLE_BUFFER};
use crate::input::Input;
use sourcerenderer_core::{
    Matrix4,
    Platform,
    Vec2,
    Vec2UI,
    Vec3,
};
use crate::renderer::render_path::{
    FrameInfo,
    RenderPath,
    SceneInfo,
    ZeroTextures, RenderPassParameters,
};
use crate::renderer::renderer_assets::RendererAssets;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::shader_manager::ShaderManager;
use crate::renderer::passes::modern::gpu_scene::SceneBuffers;
use crate::ui::UIDrawData;

pub struct ModernRenderer<P: Platform> {
    device: Arc<Device<P::GPUBackend>>,
    resources: RendererResources<P::GPUBackend>,
    ui_data: UIDrawData<P::GPUBackend>,
}

/*enum AntiAliasing<P: Platform> {
    TAA { taa: TAAPass, sharpen: SharpenPass },
    FSR2 { fsr: Fsr2Pass<P> },
}*/

/*pub struct RTPasses<P: Platform> {
    acceleration_structure_update: AccelerationStructureUpdatePass<P>,
    shadows: RTShadowPass,
}*/

impl<P: Platform> ModernRenderer<P> {
    const USE_FSR2: bool = true;

    pub fn new(
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
        swapchain: &crate::graphics::Swapchain<P::GPUBackend>,
        context: &mut GraphicsContext<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        let resolution = if Self::USE_FSR2 {
            Vec2UI::new(swapchain.width() / 4 * 3, swapchain.height() / 4 * 3)
        } else {
            Vec2UI::new(swapchain.width(), swapchain.height())
        };

        let mut resources = RendererResources::<P::GPUBackend>::new(device);

        init_cmd_buffer.flush_barriers();
        device.flush_transfers();

        device.submit(QueueType::Graphics, QueueSubmission {
            command_buffer: init_cmd_buffer.finish(),
            wait_fences: &[],
            signal_fences: &[],
            acquire_swapchain: None,
            release_swapchain: None
        });
        let c_device = device.clone();
        rayon::spawn(move || c_device.flush(QueueType::Graphics));

        Self {
            device: device.clone(),
            resources,
            ui_data: UIDrawData::<P::GPUBackend>::default(),
        }
    }

    fn setup_frame(
        &self,
        cmd_buf: &mut CommandBufferRecorder<P::GPUBackend>,
        scene: &SceneInfo<P::GPUBackend>,
        swapchain: &Swapchain<P::GPUBackend>,
        gpu_scene_buffers: SceneBuffers<P::GPUBackend>,
        camera_buffer: BufferRef<P::GPUBackend>,
        camera_history_buffer: BufferRef<P::GPUBackend>,
        rendering_resolution: &Vec2UI,
        frame: u64,
    ) {
        let view = &scene.views[scene.active_view_index];

        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 0, BufferRef::Transient(&gpu_scene_buffers.buffer), gpu_scene_buffers.scene_buffer.offset, gpu_scene_buffers.scene_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 1, BufferRef::Transient(&gpu_scene_buffers.buffer), gpu_scene_buffers.draws_buffer.offset, gpu_scene_buffers.draws_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 2, BufferRef::Transient(&gpu_scene_buffers.buffer), gpu_scene_buffers.meshes_buffer.offset, gpu_scene_buffers.meshes_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 3, BufferRef::Transient(&gpu_scene_buffers.buffer), gpu_scene_buffers.drawables_buffer.offset, gpu_scene_buffers.drawables_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 4, BufferRef::Transient(&gpu_scene_buffers.buffer), gpu_scene_buffers.parts_buffer.offset, gpu_scene_buffers.parts_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 5, BufferRef::Transient(&gpu_scene_buffers.buffer), gpu_scene_buffers.materials_buffer.offset, gpu_scene_buffers.materials_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 6, BufferRef::Transient(&gpu_scene_buffers.buffer), gpu_scene_buffers.lights_buffer.offset, gpu_scene_buffers.lights_buffer.length);

        cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 7, camera_buffer, 0, WHOLE_BUFFER);
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            8,
            camera_history_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 9, scene.vertex_buffer, 0, WHOLE_BUFFER);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 10, scene.index_buffer, 0, WHOLE_BUFFER);
        /*let cluster_count = self.clustering_pass.cluster_count();
        let cluster_z_scale = (cluster_count.z as f32) / (view.far_plane / view.near_plane).log2();
        let cluster_z_bias = -(cluster_count.z as f32) * (view.near_plane).log2()
            / (view.far_plane / view.near_plane).log2();

        let cascades = self.shadow_map_pass.cascades();
        let mut gpu_cascade_data: [ShadowCascade; 5] = Default::default();
        for i in 0..cascades.len() {
            let gpu_cascade = &mut gpu_cascade_data[i];
            let cascade = &cascades[i];
            gpu_cascade.light_mat = cascade.view_proj;
            gpu_cascade.z_max = cascade.z_max;
            gpu_cascade.z_min = cascade.z_min;
        }*/

        #[repr(C)]
        #[derive(Debug, Clone, Default)]
        struct ShadowCascade {
            light_mat: Matrix4,
            z_min: f32,
            z_max: f32,
            _padding: [u32; 2]
        }

        #[repr(C)]
        #[derive(Debug, Clone)]
        struct SetupBuffer {
            point_light_count: u32,
            directional_light_count: u32,
            cluster_z_bias: f32,
            cluster_z_scale: f32,
            cluster_count: Vector3<u32>,
            _padding: u32,
            swapchain_transform: Matrix4,
            halton_point: Vec2,
            rt_size: Vec2UI,
            cascades: [ShadowCascade; 5],
            cascade_count: u32,
            frame: u32
        }

        /*let setup_buffer = cmd_buf.upload_dynamic_data(
            &[SetupBuffer {
                point_light_count: scene.scene.point_lights().len() as u32,
                directional_light_count: scene.scene.directional_lights().len() as u32,
                cluster_z_bias,
                cluster_z_scale,
                cluster_count,
                _padding: 0,
                swapchain_transform: swapchain.transform(),
                halton_point: super::taa::scaled_halton_point(
                    rendering_resolution.x,
                    rendering_resolution.y,
                    (frame % 8) as u32 + 1,
                ),
                rt_size: *rendering_resolution,
                cascade_count: cascades.len() as u32,
                cascades: gpu_cascade_data,
                frame: frame as u32
            }],
            BufferUsage::CONSTANT,
        ).unwrap();
        cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 11, BufferRef::Transient(&setup_buffer), 0, WHOLE_BUFFER);*/
        #[repr(C)]
        #[derive(Debug, Clone)]
        struct PointLight {
            position: Vec3,
            intensity: f32,
        }
        let point_lights: SmallVec<[PointLight; 16]> = scene.scene
            .point_lights()
            .iter()
            .map(|l| PointLight {
                position: l.position,
                intensity: l.intensity,
            })
            .collect();
        let point_lights_buffer = cmd_buf.upload_dynamic_data(&point_lights, BufferUsage::CONSTANT).unwrap();
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
        let directional_lights: SmallVec<[DirectionalLight; 16]> = scene.scene
            .directional_lights()
            .iter()
            .map(|l| DirectionalLight {
                direction: l.direction,
                intensity: l.intensity,
            })
            .collect();
        let directional_lights_buffer =
            cmd_buf.upload_dynamic_data(&directional_lights, BufferUsage::CONSTANT).unwrap();
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            13,
            BufferRef::Transient(&directional_lights_buffer),
            0,
            WHOLE_BUFFER,
        );
    }
}

impl<P: Platform> RenderPath<P> for ModernRenderer<P> {
    fn is_gpu_driven(&self) -> bool {
        true
    }

    fn write_occlusion_culling_results(&self, _frame: u64, _bitset: &mut Vec<u32>) {}

    fn on_swapchain_changed(
        &mut self,
        swapchain: &Swapchain<P::GPUBackend>,
    ) {
        // TODO: resize render targets
    }

    #[profiling::function]
    fn render(
        &mut self,
        context: &mut GraphicsContext<P::GPUBackend>,
        swapchain: &Arc<Swapchain<P::GPUBackend>>,
        scene: &SceneInfo<P::GPUBackend>,
        zero_textures: &ZeroTextures<P::GPUBackend>,
        frame_info: &FrameInfo,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>,
    ) -> Result<(), SwapchainError> {
        let mut cmd_buf = context.get_command_buffer(QueueType::Graphics);

        let main_view = &scene.views[scene.active_view_index];

        /*let camera_buffer = late_latching.unwrap().buffer();
        let camera_history_buffer = late_latching.unwrap().history_buffer().unwrap();*/
        let camera_buffer = self.device.upload_data(&[0f32], MemoryUsage::MainMemoryWriteCombined, BufferUsage::CONSTANT).unwrap();
        let camera_history_buffer = self.device.upload_data(&[0f32], MemoryUsage::MainMemoryWriteCombined, BufferUsage::CONSTANT).unwrap();

        let scene_buffers = crate::renderer::passes::modern::gpu_scene::upload(&mut cmd_buf, scene.scene, 0 /* TODO */, assets);

        //self.shadow_map_pass.calculate_cascades(scene);

        self.setup_frame(
            &mut cmd_buf,
            scene,
            swapchain,
            scene_buffers,
            BufferRef::Regular(&camera_buffer),
            BufferRef::Regular(&camera_history_buffer),
            &Vec2UI::new(swapchain.width(), swapchain.height()),
            frame_info.frame
        );

        self.resources.swap_history_resources();

        let frame_end_signal = context.end_frame();

        self.device.submit(
            QueueType::Graphics,
            QueueSubmission {
                command_buffer: cmd_buf.finish(),
                wait_fences: &[],
                signal_fences: &[frame_end_signal],
                acquire_swapchain: Some(&swapchain),
                release_swapchain: Some(&swapchain)
            }
        );
        self.device.present(QueueType::Graphics, &swapchain);

        let c_device = self.device.clone();
        rayon::spawn(move || c_device.flush(QueueType::Graphics));

        Ok(())
    }

    fn set_ui_data(&mut self, data: crate::ui::UIDrawData<<P as Platform>::GPUBackend>) {
        self.ui_data = data;
    }
}
