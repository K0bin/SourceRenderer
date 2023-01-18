use std::sync::Arc;

use nalgebra::Vector3;
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{
    Backend,
    Barrier,
    BarrierAccess,
    BarrierSync,
    BarrierTextureRange,
    BindingFrequency,
    BufferUsage,
    CommandBuffer,
    Device,
    Queue,
    Swapchain,
    SwapchainError,
    TextureLayout,
    TextureView,
    WHOLE_BUFFER,
};
use sourcerenderer_core::{
    Matrix4,
    Platform,
    Vec2,
    Vec2UI,
    Vec3,
};

use super::acceleration_structure_update::AccelerationStructureUpdatePass;
use super::clustering::ClusteringPass;
use super::draw_prep::DrawPrepPass;
use super::hi_z::HierarchicalZPass;
use super::light_binning::LightBinningPass;
use super::rt_shadows::RTShadowPass;
use super::shading_pass::ShadingPass;
use super::shadow_map::ShadowMapPass;
use super::sharpen::SharpenPass;
use super::ssao::SsaoPass;
use super::taa::TAAPass;
use super::visibility_buffer::VisibilityBufferPass;
use crate::input::Input;
use crate::renderer::drawable::View;
use crate::renderer::passes::blue_noise::BlueNoise;
use crate::renderer::passes::compositing::CompositingPass;
use crate::renderer::passes::fsr2::Fsr2Pass;
use crate::renderer::passes::modern::motion_vectors::MotionVectorPass;
use crate::renderer::passes::ssr::SsrPass;
use crate::renderer::render_path::{
    FrameInfo,
    RenderPath,
    SceneInfo,
    ZeroTextures,
};
use crate::renderer::renderer_assets::RendererAssets;
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::renderer_scene::RendererScene;
use crate::renderer::shader_manager::ShaderManager;
use crate::renderer::LateLatching;
use crate::renderer::passes::modern::gpu_scene::SceneBuffers;

pub struct ModernRenderer<P: Platform> {
    swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    device: Arc<<P::GraphicsBackend as Backend>::Device>,
    barriers: RendererResources<P::GraphicsBackend>,
    clustering_pass: ClusteringPass,
    light_binning_pass: LightBinningPass,
    geometry_draw_prep: DrawPrepPass,
    ssao: SsaoPass<P>,
    rt_passes: Option<RTPasses<P>>,
    blue_noise: BlueNoise<P::GraphicsBackend>,
    hi_z_pass: HierarchicalZPass<P>,
    ssr_pass: SsrPass,
    visibility_buffer: VisibilityBufferPass,
    shading_pass: ShadingPass<P>,
    compositing_pass: CompositingPass,
    motion_vector_pass: MotionVectorPass,
    anti_aliasing: AntiAliasing<P::GraphicsBackend>,
    shadow_map_pass: ShadowMapPass<P>
}

enum AntiAliasing<B: Backend> {
    TAA { taa: TAAPass, sharpen: SharpenPass },
    FSR2 { fsr: Fsr2Pass<B> },
}

pub struct RTPasses<P: Platform> {
    acceleration_structure_update: AccelerationStructureUpdatePass<P>,
    shadows: RTShadowPass,
}

impl<P: Platform> ModernRenderer<P> {
    const USE_FSR2: bool = true;

    pub fn new(
        device: &Arc<<P::GraphicsBackend as Backend>::Device>,
        swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();
        let resolution = if Self::USE_FSR2 {
            Vec2UI::new(swapchain.width() / 4 * 3, swapchain.height() / 4 * 3)
        } else {
            Vec2UI::new(swapchain.width(), swapchain.height())
        };

        let mut barriers = RendererResources::<P::GraphicsBackend>::new(device);

        let blue_noise = BlueNoise::new::<P>(device);

        let clustering = ClusteringPass::new::<P>(&mut barriers, shader_manager);
        let light_binning = LightBinningPass::new::<P>(&mut barriers, shader_manager);
        let ssao = SsaoPass::<P>::new(device, resolution, &mut barriers, shader_manager, true);
        let rt_passes = device.supports_ray_tracing().then(|| RTPasses {
            acceleration_structure_update: AccelerationStructureUpdatePass::<P>::new(
                device,
                &mut init_cmd_buffer,
            ),
            shadows: RTShadowPass::new::<P>(resolution, &mut barriers, shader_manager),
        });
        let visibility_buffer =
            VisibilityBufferPass::new::<P>(resolution, &mut barriers, shader_manager);
        let draw_prep = DrawPrepPass::new::<P>(&mut barriers, shader_manager);
        let hi_z_pass = HierarchicalZPass::<P>::new(
            device,
            &mut barriers,
            shader_manager,
            &mut init_cmd_buffer,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
        );
        let ssr_pass = SsrPass::new::<P>(resolution, &mut barriers, shader_manager, true);
        let shading_pass = ShadingPass::<P>::new(
            device,
            resolution,
            &mut barriers,
            shader_manager,
            &mut init_cmd_buffer,
        );
        let compositing_pass = CompositingPass::new::<P>(resolution, &mut barriers, shader_manager);
        let motion_vector_pass =
            MotionVectorPass::new::<P>(&mut barriers, resolution, shader_manager);

        let anti_aliasing = if Self::USE_FSR2 {
            let fsr_pass = Fsr2Pass::<P::GraphicsBackend>::new::<P>(
                device,
                &mut barriers,
                resolution,
                swapchain,
            );
            AntiAliasing::FSR2 { fsr: fsr_pass }
        } else {
            let taa = TAAPass::new::<P>(resolution, &mut barriers, shader_manager, true);
            let sharpen = SharpenPass::new::<P>(resolution, &mut barriers, shader_manager);
            AntiAliasing::TAA { taa, sharpen }
        };

        let shadow_map = ShadowMapPass::new(device, &mut barriers, &mut init_cmd_buffer, shader_manager);

        init_cmd_buffer.flush_barriers();
        device.flush_transfers();

        let c_graphics_queue = device.graphics_queue().clone();
        c_graphics_queue.submit(init_cmd_buffer.finish(), None, &[], &[], true);
        rayon::spawn(move || c_graphics_queue.process_submissions());

        Self {
            swapchain: swapchain.clone(),
            device: device.clone(),
            barriers,
            clustering_pass: clustering,
            light_binning_pass: light_binning,
            geometry_draw_prep: draw_prep,
            ssao,
            rt_passes,
            blue_noise,
            hi_z_pass,
            ssr_pass,
            visibility_buffer,
            shading_pass,
            compositing_pass,
            motion_vector_pass,
            anti_aliasing,
            shadow_map_pass: shadow_map
        }
    }

    fn setup_frame(
        &self,
        cmd_buf: &mut <P::GraphicsBackend as Backend>::CommandBuffer,
        swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
        gpu_scene_buffers: SceneBuffers<P::GraphicsBackend>,
        camera_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
        camera_history_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
        vertex_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
        index_buffer: &Arc<<P::GraphicsBackend as Backend>::Buffer>,
        scene: &RendererScene<P::GraphicsBackend>,
        view: &View,
        rendering_resolution: &Vec2UI,
        frame: u64,
    ) {
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 0, &gpu_scene_buffers.buffer, gpu_scene_buffers.scene_buffer.offset, gpu_scene_buffers.scene_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 1, &gpu_scene_buffers.buffer, gpu_scene_buffers.draws_buffer.offset, gpu_scene_buffers.draws_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 2, &gpu_scene_buffers.buffer, gpu_scene_buffers.meshes_buffer.offset, gpu_scene_buffers.meshes_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 3, &gpu_scene_buffers.buffer, gpu_scene_buffers.drawables_buffer.offset, gpu_scene_buffers.drawables_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 4, &gpu_scene_buffers.buffer, gpu_scene_buffers.parts_buffer.offset, gpu_scene_buffers.parts_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 5, &gpu_scene_buffers.buffer, gpu_scene_buffers.materials_buffer.offset, gpu_scene_buffers.materials_buffer.length);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 6, &gpu_scene_buffers.buffer, gpu_scene_buffers.lights_buffer.offset, gpu_scene_buffers.lights_buffer.length);

        cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 7, &camera_buffer, 0, WHOLE_BUFFER);
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            8,
            &camera_history_buffer,
            0,
            WHOLE_BUFFER,
        );
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 9, &vertex_buffer, 0, WHOLE_BUFFER);
        cmd_buf.bind_storage_buffer(BindingFrequency::Frame, 10, &index_buffer, 0, WHOLE_BUFFER);
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
                point_light_count: scene.point_lights().len() as u32,
                directional_light_count: scene.directional_lights().len() as u32,
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
        );
        cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 11, &setup_buffer, 0, WHOLE_BUFFER);
        #[repr(C)]
        #[derive(Debug, Clone)]
        struct PointLight {
            position: Vec3,
            intensity: f32,
        }
        let point_lights: SmallVec<[PointLight; 16]> = scene
            .point_lights()
            .iter()
            .map(|l| PointLight {
                position: l.position,
                intensity: l.intensity,
            })
            .collect();
        let point_lights_buffer = cmd_buf.upload_dynamic_data(&point_lights, BufferUsage::CONSTANT);
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            12,
            &point_lights_buffer,
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
            .directional_lights()
            .iter()
            .map(|l| DirectionalLight {
                direction: l.direction,
                intensity: l.intensity,
            })
            .collect();
        let directional_lights_buffer =
            cmd_buf.upload_dynamic_data(&directional_lights, BufferUsage::CONSTANT);
        cmd_buf.bind_uniform_buffer(
            BindingFrequency::Frame,
            13,
            &directional_lights_buffer,
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
        swapchain: &std::sync::Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    ) {
        // TODO: resize render targets
        self.swapchain = swapchain.clone();
    }

    #[profiling::function]
    fn render(
        &mut self,
        scene: &SceneInfo<P::GraphicsBackend>,
        zero_textures: &ZeroTextures<P::GraphicsBackend>,
        late_latching: Option<&dyn LateLatching<P::GraphicsBackend>>,
        input: &Input,
        frame_info: &FrameInfo,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>,
    ) -> Result<(), SwapchainError> {
        let graphics_queue = self.device.graphics_queue();
        let mut cmd_buf = graphics_queue.create_command_buffer();

        let main_view = &scene.views[scene.active_view_index];

        let camera_buffer = late_latching.unwrap().buffer();
        let camera_history_buffer = late_latching.unwrap().history_buffer().unwrap();

        let scene_buffers = super::gpu_scene::upload(&mut cmd_buf, scene.scene, 0 /* TODO */, assets);

        self.setup_frame(
            &mut cmd_buf,
            &self.swapchain,
            scene_buffers,
            &camera_buffer,
            &camera_history_buffer,
            scene.vertex_buffer,
            scene.index_buffer,
            scene.scene,
            main_view,
            &Vec2UI::new(self.swapchain.width(), self.swapchain.height()),
            frame_info.frame,
        );

        let resolution = {
            let info = self
                .barriers
                .texture_info(VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME);
            Vec2UI::new(info.width, info.height)
        };

        if let Some(rt_passes) = self.rt_passes.as_mut() {
            rt_passes
                .acceleration_structure_update
                .execute(&mut cmd_buf, scene.scene, assets);
        }
        self.hi_z_pass.execute(
            &mut cmd_buf,
            &self.barriers,
            shader_manager,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
        );
        self.geometry_draw_prep.execute(
            &mut cmd_buf,
            &self.barriers,
            scene.scene,
            main_view,
            shader_manager,
            assets,
        );
        self.visibility_buffer.execute(
            &mut cmd_buf,
            &self.barriers,
            scene.vertex_buffer,
            scene.index_buffer,
            shader_manager,
        );
        self.motion_vector_pass
            .execute(&mut cmd_buf, &self.barriers, shader_manager);
        self.clustering_pass.execute(
            &mut cmd_buf,
            resolution,
            main_view,
            &camera_buffer,
            &mut self.barriers,
            shader_manager,
        );
        self.light_binning_pass.execute(
            &mut cmd_buf,
            scene.scene,
            &camera_buffer,
            &mut self.barriers,
            shader_manager,
        );
        self.ssao.execute(
            &mut cmd_buf,
            &self.barriers,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
            None,
            &camera_buffer,
            self.blue_noise.frame(frame_info.frame),
            self.blue_noise.sampler(),
            shader_manager,
            true,
        );
        if let Some(rt_passes) = self.rt_passes.as_mut() {
            let blue_noise = &self.blue_noise.frame(frame_info.frame);
            let blue_noise_sampler = &self.blue_noise.sampler();
            let acceleration_structure = rt_passes
                .acceleration_structure_update
                .acceleration_structure();
            rt_passes.shadows.execute(
                &mut cmd_buf,
                &self.barriers,
                shader_manager,
                VisibilityBufferPass::DEPTH_TEXTURE_NAME,
                acceleration_structure,
                blue_noise,
                blue_noise_sampler,
            );
        }

        self.shadow_map_pass.prepare(
            &mut cmd_buf,
            &self.barriers,
            &shader_manager,
            &scene.scene
        );

        self.shadow_map_pass.execute(
            &mut cmd_buf,
            &self.barriers,
            shader_manager,
            scene.vertex_buffer,
            scene.index_buffer,
            &scene.scene,
            &scene.views[scene.active_view_index]
        );


        let light = scene.scene.directional_lights().first();
        let light_view_proj = if let Some(light) = light {
            const Z_MULT: f32 = 100.0f32;
            let view = &scene.views[scene.active_view_index];
            let view_proj = view.proj_matrix * view.view_matrix;
            let inv_camera_view = view_proj.try_inverse().unwrap();
            ShadowMapPass::<P>::build_directional_light_view_proj(light, inv_camera_view, 10f32)
        } else {
            Matrix4::identity()
        };

        self.shading_pass.execute(
            &mut cmd_buf,
            &self.device,
            scene.lightmap.unwrap(),
            zero_textures.zero_texture_view,
            &self.barriers,
            shader_manager,
            &light_view_proj
        );
        self.ssr_pass.execute(
            &mut cmd_buf,
            &self.barriers,
            shader_manager,
            ShadingPass::<P>::SHADING_TEXTURE_NAME,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
            true,
        );
        self.compositing_pass.execute(
            &mut cmd_buf,
            &self.barriers,
            ShadingPass::<P>::SHADING_TEXTURE_NAME,
            shader_manager,
        );

        let output_texture_name = match &mut self.anti_aliasing {
            AntiAliasing::FSR2 { fsr } => {
                fsr.execute(
                    &mut cmd_buf,
                    &self.barriers,
                    CompositingPass::COMPOSITION_TEXTURE_NAME,
                    VisibilityBufferPass::DEPTH_TEXTURE_NAME,
                    MotionVectorPass::MOTION_TEXTURE_NAME,
                    main_view,
                    frame_info,
                );
                Fsr2Pass::<P::GraphicsBackend>::UPSCALED_TEXTURE_NAME
            }
            AntiAliasing::TAA { taa, sharpen } => {
                taa.execute(
                    &mut cmd_buf,
                    &self.barriers,
                    shader_manager,
                    CompositingPass::COMPOSITION_TEXTURE_NAME,
                    VisibilityBufferPass::DEPTH_TEXTURE_NAME,
                    None,
                    true,
                );
                sharpen.execute(&mut cmd_buf, &self.barriers, shader_manager);
                SharpenPass::SHAPENED_TEXTURE_NAME
            }
        };

        let output_texture = self.barriers.access_texture(
            &mut cmd_buf,
            output_texture_name,
            &BarrierTextureRange::default(),
            BarrierSync::COPY,
            BarrierAccess::COPY_READ,
            TextureLayout::CopySrc,
            false,
            HistoryResourceEntry::Current,
        );

        let prepare_sem = self.device.create_semaphore();
        let cmd_buf_sem = self.device.create_semaphore();
        let back_buffer_res = self.swapchain.prepare_back_buffer(&prepare_sem);
        if back_buffer_res.is_none() {
            return Err(SwapchainError::Other);
        }

        let back_buffer = back_buffer_res.unwrap();

        cmd_buf.barrier(&[Barrier::TextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::COPY,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::COPY_WRITE,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::CopyDst,
            texture: back_buffer.texture(),
            range: BarrierTextureRange::default(),
        }]);
        cmd_buf.flush_barriers();
        cmd_buf.blit(&*output_texture, 0, 0, back_buffer.texture(), 0, 0);
        cmd_buf.barrier(&[Barrier::TextureBarrier {
            old_sync: BarrierSync::COPY,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::COPY_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::CopyDst,
            new_layout: TextureLayout::Present,
            texture: back_buffer.texture(),
            range: BarrierTextureRange::default(),
        }]);
        std::mem::drop(output_texture);

        self.barriers.swap_history_resources();

        if let Some(late_latching) = late_latching {
            let input_state = input.poll();
            late_latching.before_submit(&input_state, main_view);
        }
        graphics_queue.submit(
            cmd_buf.finish(),
            None,
            &[&prepare_sem],
            &[&cmd_buf_sem],
            true,
        );
        graphics_queue.present(&self.swapchain, &[&cmd_buf_sem], true);

        let c_graphics_queue = graphics_queue.clone();
        rayon::spawn(move || c_graphics_queue.process_submissions());

        if let Some(late_latching) = late_latching {
            late_latching.after_submit(&self.device);
        }

        Ok(())
    }
}
