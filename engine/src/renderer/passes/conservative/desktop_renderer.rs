use std::sync::Arc;

use nalgebra::Vector3;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::{
    Matrix4,
    Platform,
    Vec2,
    Vec2UI,
    Vec3,
};

use super::acceleration_structure_update::AccelerationStructureUpdatePass;
use super::clustering::ClusteringPass;
use super::geometry::GeometryPass;
use super::light_binning::LightBinningPass;
//use super::occlusion::OcclusionPass;
use super::prepass::Prepass;
use super::rt_shadows::RTShadowPass;
use super::sharpen::SharpenPass;
use super::ssao::SsaoPass;
use super::taa::TAAPass;
use crate::input::Input;
use crate::renderer::passes::blit::BlitPass;
use crate::renderer::passes::blue_noise::BlueNoise;
use crate::renderer::passes::modern::gpu_scene::{BufferBinding, SceneBuffers};
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
use crate::renderer::LateLatching;

use crate::graphics::*;

pub struct ConservativeRenderer<P: Platform> {
    device: Arc<crate::graphics::Device<P::GPUBackend>>,
    barriers: RendererResources<P::GPUBackend>,
    clustering_pass: ClusteringPass,
    light_binning_pass: LightBinningPass,
    prepass: Prepass,
    geometry: GeometryPass<P>,
    taa: TAAPass,
    sharpen: SharpenPass,
    ssao: SsaoPass<P>,
    //occlusion: OcclusionPass<P>,
    rt_passes: Option<RTPasses<P>>,
    blue_noise: BlueNoise<P::GPUBackend>,
    blit_pass: BlitPass
}

pub struct RTPasses<P: Platform> {
    acceleration_structure_update: AccelerationStructureUpdatePass<P>,
    shadows: RTShadowPass,
}

pub struct FrameBindings<'a, B: GPUBackend> {
    gpu_scene_buffer: BufferRef<'a, B>,
    camera_buffer: BufferRef<'a, B>,
    camera_history_buffer: BufferRef<'a, B>,
    vertex_buffer: BufferRef<'a, B>,
    index_buffer: BufferRef<'a, B>,
    directional_lights: TransientBufferSlice<B>,
    point_lights: TransientBufferSlice<B>,
    setup_buffer: TransientBufferSlice<B>,
}

impl<P: Platform> ConservativeRenderer<P> {
    pub fn new(
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
        swapchain: &crate::graphics::Swapchain<P::GPUBackend>,
        context: &mut GraphicsContext<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        let resolution = Vec2UI::new(swapchain.width(), swapchain.height());

        let mut barriers = RendererResources::<P::GPUBackend>::new(device);

        let blue_noise = BlueNoise::new::<P>(device);

        let clustering = ClusteringPass::new::<P>(&mut barriers, shader_manager);
        let light_binning = LightBinningPass::new::<P>(&mut barriers, shader_manager);
        let prepass = Prepass::new::<P>(&mut barriers, shader_manager, resolution);
        let geometry = GeometryPass::<P>::new(device, resolution, &mut barriers, shader_manager);
        let taa = TAAPass::new::<P>(resolution, &mut barriers, shader_manager, false);
        let sharpen = SharpenPass::new::<P>(resolution, &mut barriers, shader_manager);
        let ssao = SsaoPass::<P>::new(device, resolution, &mut barriers, shader_manager, false);
        //let occlusion = OcclusionPass::<P>::new(device, shader_manager);
        let rt_passes = device.supports_ray_tracing().then(|| RTPasses {
            acceleration_structure_update: AccelerationStructureUpdatePass::<P>::new(
                device,
                &mut init_cmd_buffer,
            ),
            shadows: RTShadowPass::new::<P>(resolution, &mut barriers, shader_manager),
        });
        let blit = BlitPass::new::<P>(&mut barriers, shader_manager, swapchain.format());

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
            barriers,
            clustering_pass: clustering,
            light_binning_pass: light_binning,
            prepass,
            geometry,
            taa,
            sharpen,
            ssao,
            //occlusion,
            rt_passes,
            blue_noise,
            blit_pass: blit
        }
    }

    fn create_frame_bindings<'a, 'b>(
        &'b self,
        cmd_buf: &'b mut CommandBufferRecorder<P::GPUBackend>,
        scene: &'a SceneInfo<'a, P::GPUBackend>,
        swapchain: &'a Swapchain<P::GPUBackend>,
        gpu_scene_buffers: &'a SceneBuffers<P::GPUBackend>,
        camera_buffer: BufferRef<'a, P::GPUBackend>,
        camera_history_buffer: BufferRef<'a, P::GPUBackend>,
        rendering_resolution: &Vec2UI,
        frame: u64,
    ) -> FrameBindings<'a, P::GPUBackend>
        where 'a: 'b {
        let view = &scene.views[scene.active_view_index];

        let cluster_count = self.clustering_pass.cluster_count();
        let cluster_z_scale = (cluster_count.z as f32) / (view.far_plane / view.near_plane).log2();
        let cluster_z_bias = -(cluster_count.z as f32) * (view.near_plane).log2()
            / (view.far_plane / view.near_plane).log2();
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
        }
        let setup_buffer = cmd_buf.upload_dynamic_data(
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
            }],
            BufferUsage::CONSTANT,
        ).unwrap();
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

        FrameBindings {
            gpu_scene_buffer: BufferRef::Transient(&gpu_scene_buffers.buffer),
            camera_buffer: camera_buffer.clone(),
            camera_history_buffer: camera_history_buffer.clone(),
            vertex_buffer: scene.vertex_buffer.clone(),
            index_buffer: scene.index_buffer.clone(),
            directional_lights: directional_lights_buffer,
            point_lights: point_lights_buffer,
            setup_buffer: setup_buffer,
        }
    }
}

impl<P: Platform> RenderPath<P> for ConservativeRenderer<P> {
    fn is_gpu_driven(&self) -> bool {
        false
    }

    fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>) {
        //self.occlusion.write_occlusion_query_results(frame, bitset);
    }

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
        late_latching: Option<&dyn LateLatching<P::GPUBackend>>,
        input: &Input,
        frame_info: &FrameInfo,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>,
    ) -> Result<(), SwapchainError> {
        let mut cmd_buf = context.get_command_buffer(QueueType::Graphics);

        let late_latching_buffer = late_latching.unwrap().buffer();
        let late_latching_history_buffer = late_latching.unwrap().history_buffer().unwrap();

        let primary_view = &scene.views[scene.active_view_index];

        let empty_buffer = cmd_buf.create_temporary_buffer(
            &BufferInfo {
                size: 16,
                usage: BufferUsage::STORAGE,
                sharing_mode: QueueSharingMode::Concurrent
            },
            MemoryUsage::GPUMemory,
        ).unwrap();
        let gpu_scene = SceneBuffers {
            buffer: empty_buffer,
            scene_buffer: BufferBinding { offset: 0, length: 0 },
            draws_buffer: BufferBinding { offset: 0, length: 0 },
            meshes_buffer:  BufferBinding { offset: 0, length: 0 },
            drawables_buffer: BufferBinding { offset: 0, length: 0 },
            parts_buffer: BufferBinding { offset: 0, length: 0 },
            materials_buffer: BufferBinding { offset: 0, length: 0 },
            lights_buffer: BufferBinding { offset: 0, length: 0 }
        };

        let frame_bindings = self.create_frame_bindings(
            &mut cmd_buf,
            scene,
            swapchain,
            &gpu_scene,
            BufferRef::Regular(&late_latching_buffer),
            BufferRef::Regular(&late_latching_history_buffer),
            &Vec2UI::new(swapchain.width(), swapchain.height()),
            frame_info.frame,
        );
        setup_frame::<P::GPUBackend>(&mut cmd_buf, &frame_bindings);

        let params = RenderPassParameters {
            device: self.device.as_ref(),
            scene,
            shader_manager,
            resources: &mut self.barriers,
            zero_textures,
            assets
        };

        if let Some(rt_passes) = self.rt_passes.as_mut() {
            rt_passes
                .acceleration_structure_update
                .execute(&mut cmd_buf, &params);
        }

        /*self.occlusion.execute(
            context,
            &mut cmd_buf,
            &params,
            frame_info.frame,
            &late_latching_buffer,
            Prepass::DEPTH_TEXTURE_NAME,
        );*/
        self.clustering_pass.execute::<P>(
            &mut cmd_buf,
            &params,
            Vec2UI::new(swapchain.width(), swapchain.height()),
            &late_latching_buffer
        );
        self.light_binning_pass.execute(
            &mut cmd_buf,
            &params,
            &late_latching_buffer
        );
        self.prepass.execute(
            context,
            &mut cmd_buf,
            &params,
            swapchain.transform(),
            frame_info.frame,
            &late_latching_buffer,
            &late_latching_history_buffer
        );
        self.ssao.execute(
            &mut cmd_buf,
            &params,
            Prepass::DEPTH_TEXTURE_NAME,
            Some(Prepass::MOTION_TEXTURE_NAME),
            &late_latching_buffer,
            self.blue_noise.frame(frame_info.frame),
            self.blue_noise.sampler(),
            false
        );
        if let Some(rt_passes) = self.rt_passes.as_mut() {
            rt_passes.shadows.execute(
                &mut cmd_buf,
                &params,
                Prepass::DEPTH_TEXTURE_NAME,
                rt_passes
                    .acceleration_structure_update
                    .acceleration_structure(),
                &self.blue_noise.frame(frame_info.frame),
                &self.blue_noise.sampler(),
            );
        }
        self.geometry.execute(
            context,
            &mut cmd_buf,
            &params,
            Prepass::DEPTH_TEXTURE_NAME,
            &frame_bindings
        );
        self.taa.execute(
            &mut cmd_buf,
            &params,
            GeometryPass::<P>::GEOMETRY_PASS_TEXTURE_NAME,
            Prepass::DEPTH_TEXTURE_NAME,
            Some(Prepass::MOTION_TEXTURE_NAME),
            false
        );
        self.sharpen
            .execute(&mut cmd_buf, &params);

        let sharpened_texture = self.barriers.access_texture(
            &mut cmd_buf,
            SharpenPass::SHAPENED_TEXTURE_NAME,
            &BarrierTextureRange::default(),
            BarrierSync::COPY,
            BarrierAccess::COPY_READ,
            TextureLayout::CopySrc,
            false,
            HistoryResourceEntry::Current,
        );

        let back_buffer_res = swapchain.next_backbuffer();
        if back_buffer_res.is_err() {
            return Err(SwapchainError::Other);
        }

        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::RENDER_TARGET, // BarrierSync::COPY,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::RENDER_TARGET_WRITE, // BarrierAccess::COPY_WRITE,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::RenderTarget, // TextureLayout::CopyDst,
            texture: swapchain.backbuffer_handle(),
            range: BarrierTextureRange::default(),
            queue_ownership: None
        }]);
        cmd_buf.flush_barriers();
        //cmd_buf.blit_to_handle(&*sharpened_texture, 0, 0, swapchain.backbuffer_handle(), 0, 0);
        std::mem::drop(sharpened_texture);
        let sharpened_view = self.barriers.get_view(SharpenPass::SHAPENED_TEXTURE_NAME,
            &TextureViewInfo {
                base_mip_level: 0,
                mip_level_length: 1,
                base_array_layer: 0,
                array_layer_length: 1,
                format: None
            }, HistoryResourceEntry::Current);
        let sampler = self.barriers.linear_sampler();
        self.blit_pass.execute::<P>(context, &mut cmd_buf, shader_manager, &sharpened_view, swapchain.backbuffer(), sampler);
        std::mem::drop(sharpened_view);
        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::COPY,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::COPY_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::CopyDst,
            new_layout: TextureLayout::Present,
            texture: swapchain.backbuffer_handle(),
            range: BarrierTextureRange::default(),
            queue_ownership: None
        }]);

        self.barriers.swap_history_resources();

        if let Some(late_latching) = late_latching {
            let input_state = input.poll();
            late_latching.before_submit(&input_state, primary_view);
        }

        let frame_end_signal = context.get_frame_end_fence_signal();

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
        rayon::spawn(move || c_device.flush(QueueType::Graphics) );

        if let Some(late_latching) = late_latching {
            late_latching.after_submit(&self.device);
        }

        Ok(())
    }

    fn set_ui_data(&mut self, data: crate::ui::UIDrawData<<P as Platform>::GPUBackend>) {
    }
}

pub fn setup_frame<B: GPUBackend>(cmd_buf: &mut CommandBufferRecorder<B>, frame_bindings: &FrameBindings<B>) {
    for i in 0..7 {
        cmd_buf.bind_storage_buffer(
            BindingFrequency::Frame,
            i,
            frame_bindings.gpu_scene_buffer,
            0,
            WHOLE_BUFFER,
        );
    }
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        7,
        frame_bindings.camera_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        8,
        frame_bindings.camera_history_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_storage_buffer(
        BindingFrequency::Frame,
        9,
        frame_bindings.vertex_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_storage_buffer(
        BindingFrequency::Frame,
        10,
        frame_bindings.index_buffer,
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        11,
        BufferRef::Transient(&frame_bindings.setup_buffer),
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        12,
        BufferRef::Transient(&frame_bindings.point_lights),
        0,
        WHOLE_BUFFER,
    );
    cmd_buf.bind_uniform_buffer(
        BindingFrequency::Frame,
        13,
        BufferRef::Transient(&frame_bindings.directional_lights),
        0,
          WHOLE_BUFFER,
    );
}
