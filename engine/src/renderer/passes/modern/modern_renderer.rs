use std::marker::PhantomData;
use std::sync::Arc;

use smallvec::SmallVec;
use crate::asset::AssetManager;
use crate::graphics::{Barrier, BarrierAccess, BarrierSync, BarrierTextureRange, BindingFrequency, BufferRef, BufferUsage, Device, QueueSubmission, QueueType, Swapchain, SwapchainError, TextureInfo, TextureLayout, WHOLE_BUFFER};
use crate::renderer::asset::RendererAssetsReadOnly;
use sourcerenderer_core::{
    Matrix4,
    Platform,
    Vec2,
    Vec2UI,
    Vec3, Vec3UI, Vec4,
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
use crate::graphics::{GraphicsContext, CommandBufferRecorder};
use crate::renderer::passes::blue_noise::BlueNoise;
use crate::renderer::passes::compositing::CompositingPass;
use crate::renderer::passes::modern::motion_vectors::MotionVectorPass;
use crate::renderer::passes::ssr::SsrPass;
use crate::renderer::passes::ui::UIPass;
use crate::renderer::render_path::{
    FrameInfo, RenderPassParameters, RenderPath, RenderPathResult, SceneInfo
};
use crate::renderer::renderer_resources::{
    HistoryResourceEntry,
    RendererResources,
};
use crate::renderer::passes::modern::gpu_scene::SceneBuffers;
use crate::ui::UIDrawData;

pub struct ModernRenderer<P: Platform> {
    device: Arc<Device<P::GPUBackend>>,
    barriers: RendererResources<P::GPUBackend>,
    ui_data: UIDrawData<P::GPUBackend>,

    clustering_pass: ClusteringPass,
    light_binning_pass: LightBinningPass,
    geometry_draw_prep: DrawPrepPass,
    ssao: SsaoPass<P>,
    rt_passes: Option<RTPasses<P>>,
    blue_noise: BlueNoise<P::GPUBackend>,
    hi_z_pass: HierarchicalZPass<P>,
    ssr_pass: SsrPass,
    visibility_buffer: VisibilityBufferPass,
    shading_pass: ShadingPass<P>,
    compositing_pass: CompositingPass,
    motion_vector_pass: MotionVectorPass,
    anti_aliasing: AntiAliasing<P>,
    shadow_map_pass: ShadowMapPass<P>,
    ui_pass: UIPass
}

enum AntiAliasing<P: Platform> {
    TAA { taa: TAAPass, sharpen: SharpenPass, _p: PhantomData<P> },
}

unsafe impl<P: Platform> Send for AntiAliasing<P> {}
unsafe impl<P: Platform> Sync for AntiAliasing<P> {}

pub struct RTPasses<P: Platform> {
    acceleration_structure_update: AccelerationStructureUpdatePass<P>,
    shadows: RTShadowPass,
}

#[derive(Clone)]
#[repr(C)]
struct CameraBuffer {
    view_proj: Matrix4,
    inv_proj: Matrix4,
    view: Matrix4,
    proj: Matrix4,
    inv_view: Matrix4,
    position: Vec4,
    inv_proj_view: Matrix4,
    z_near: f32,
    z_far: f32,
    aspect_ratio: f32,
    fov: f32,
}

impl<P: Platform> ModernRenderer<P> {
    #[allow(unused)]
    pub fn new(
        device: &Arc<crate::graphics::Device<P::GPUBackend>>,
        swapchain: &crate::graphics::Swapchain<P::GPUBackend>,
        context: &mut GraphicsContext<P::GPUBackend>,
        asset_manager: &Arc<AssetManager<P>>
    ) -> Self {
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        let resolution = Vec2UI::new(swapchain.width(), swapchain.height());

        let mut barriers = RendererResources::<P::GPUBackend>::new(device);

        let blue_noise = BlueNoise::new::<P>(device);

        let clustering = ClusteringPass::new::<P>(&mut barriers, asset_manager);
        let light_binning = LightBinningPass::new::<P>(&mut barriers, asset_manager);
        let ssao = SsaoPass::<P>::new(device, resolution, &mut barriers, asset_manager, true);
        let rt_passes = (device.supports_ray_tracing() && false).then(|| RTPasses {
            acceleration_structure_update: AccelerationStructureUpdatePass::<P>::new(
                device,
                &mut init_cmd_buffer,
            ),
            shadows: RTShadowPass::new::<P>(resolution, &mut barriers, asset_manager),
        });
        let visibility_buffer =
            VisibilityBufferPass::new::<P>(resolution, &mut barriers, asset_manager);
        let draw_prep = DrawPrepPass::new::<P>(&mut barriers, asset_manager);
        let hi_z_pass = HierarchicalZPass::<P>::new(
            device,
            &mut barriers,
            asset_manager,
            &mut init_cmd_buffer,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
        );
        let ssr_pass = SsrPass::new::<P>(resolution, &mut barriers, asset_manager, true);
        let shading_pass = ShadingPass::<P>::new(
            device,
            resolution,
            &mut barriers,
            asset_manager,
            &mut init_cmd_buffer,
        );
        let compositing_pass = CompositingPass::new::<P>(resolution, &mut barriers, asset_manager);
        let motion_vector_pass =
            MotionVectorPass::new::<P>(&mut barriers, resolution, asset_manager);

        let anti_aliasing = {
            let taa = TAAPass::new::<P>(resolution, &mut barriers, asset_manager, true);
            let sharpen = SharpenPass::new::<P>(resolution, &mut barriers, asset_manager);
            AntiAliasing::<P>::TAA { taa, sharpen, _p: PhantomData }
        };

        let shadow_map = ShadowMapPass::new(device, &mut barriers, &mut init_cmd_buffer, asset_manager);

        let ui_pass = UIPass::new(device, asset_manager);

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
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        task_pool.spawn(async move { c_device.flush(QueueType::Graphics); }).detach();

        Self {
            device: device.clone(),
            barriers,
            ui_data: UIDrawData::<P::GPUBackend>::default(),
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
            shadow_map_pass: shadow_map,
            ui_pass,
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
        let view = &scene.scene.views()[scene.active_view_index];

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
        let cluster_count = self.clustering_pass.cluster_count();
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
        }

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
            cluster_count: Vec3UI,
            _padding: u32,
            swapchain_transform: Matrix4,
            halton_point: Vec2,
            rt_size: Vec2UI,
            cascades: [ShadowCascade; 5],
            cascade_count: u32,
            frame: u32
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
                cascade_count: cascades.len() as u32,
                cascades: gpu_cascade_data,
                frame: frame as u32
            }],
            BufferUsage::CONSTANT,
        ).unwrap();
        cmd_buf.bind_uniform_buffer(BindingFrequency::Frame, 11, BufferRef::Transient(&setup_buffer), 0, WHOLE_BUFFER);
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
        _swapchain: &Swapchain<P::GPUBackend>,
    ) {
        // TODO: resize render targets
    }

    fn is_ready(&self, asset_manager: &Arc<AssetManager<P>>) -> bool {
        let assets = asset_manager.read_renderer_assets();
        self.clustering_pass.is_ready(&assets)
        && self.light_binning_pass.is_ready(&assets)
        && self.geometry_draw_prep.is_ready(&assets)
        && self.ssao.is_ready(&assets)
        && self.rt_passes.as_ref().map(|passes| passes.shadows.is_ready(&assets)).unwrap_or(true)
        && self.hi_z_pass.is_ready(&assets)
        && self.ssr_pass.is_ready(&assets)
        && self.visibility_buffer.is_ready(&assets)
        && self.shading_pass.is_ready(&assets)
        && self.compositing_pass.is_ready(&assets)
        && self.motion_vector_pass.is_ready(&assets)
        && match &self.anti_aliasing {
            AntiAliasing::TAA { taa, sharpen, _p: PhantomData::<P> } => taa.is_ready(&assets) && sharpen.is_ready(&assets),
        }
        && self.shadow_map_pass.is_ready(&assets)
        && self.ui_pass.is_ready(&assets)
    }

    #[profiling::function]
    fn render(
        &mut self,
        context: &mut GraphicsContext<P::GPUBackend>,
        swapchain: &mut Swapchain<P::GPUBackend>,
        scene: &SceneInfo<P::GPUBackend>,
        frame_info: &FrameInfo,
        assets: &RendererAssetsReadOnly<'_, P>,
    ) -> Result<RenderPathResult<P::GPUBackend>, SwapchainError> {
        let mut cmd_buf = context.get_command_buffer(QueueType::Graphics);

        let main_view = &scene.scene.views()[scene.active_view_index];

        let camera_buffer = cmd_buf.upload_dynamic_data(&[CameraBuffer {
            view_proj: main_view.proj_matrix * main_view.view_matrix,
            inv_proj: main_view.proj_matrix.inverse(),
            view: main_view.view_matrix,
            proj: main_view.proj_matrix,
            inv_view: main_view.view_matrix.inverse(),
            position: Vec4::new(main_view.camera_position.x, main_view.camera_position.y, main_view.camera_position.z, 1.0f32),
            inv_proj_view: (main_view.proj_matrix * main_view.view_matrix).inverse(),
            z_near: main_view.near_plane,
            z_far: main_view.far_plane,
            aspect_ratio: main_view.aspect_ratio,
            fov: main_view.camera_fov
        }], BufferUsage::CONSTANT).unwrap();

        let camera_history_buffer = &camera_buffer;

        let scene_buffers = super::gpu_scene::upload(&mut cmd_buf, scene.scene, 0 /* TODO */, &assets);

        self.shadow_map_pass.calculate_cascades(scene);

        self.setup_frame(
            &mut cmd_buf,
            scene,
            swapchain,
            scene_buffers,
            BufferRef::Transient(&camera_buffer),
            BufferRef::Transient(camera_history_buffer),
            &Vec2UI::new(swapchain.width(), swapchain.height()),
            frame_info.frame
        );

        let resolution = {
            let info: std::cell::Ref<'_, TextureInfo> = self
                .barriers
                .texture_info(VisibilityBufferPass::BARYCENTRICS_TEXTURE_NAME);
            Vec2UI::new(info.width, info.height)
        };

        let params = RenderPassParameters {
            device: self.device.as_ref(),
            scene,
            resources: &mut self.barriers,
            assets
        };

        if let Some(rt_passes) = self.rt_passes.as_mut() {
            rt_passes
                .acceleration_structure_update
                .execute(&mut cmd_buf, &params);
        }
        self.hi_z_pass.execute(
            &mut cmd_buf,
            &params,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
        );
        self.geometry_draw_prep.execute(
            &mut cmd_buf,
            &params
        );
        self.visibility_buffer.execute(
            &mut cmd_buf,
            &params
        );
        self.motion_vector_pass
            .execute(&mut cmd_buf, &params);
        self.clustering_pass.execute(
            &mut cmd_buf,
            &params,
            resolution,
            &camera_buffer
        );
        self.light_binning_pass.execute(
            &mut cmd_buf,
            &params,
            &camera_buffer
        );
        self.ssao.execute(
            &mut cmd_buf,
            &params,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
            None,
            &camera_buffer,
            self.blue_noise.frame(frame_info.frame),
            self.blue_noise.sampler(),
            true
        );
        if let Some(rt_passes) = self.rt_passes.as_mut() {
            let blue_noise = &self.blue_noise.frame(frame_info.frame);
            let blue_noise_sampler = &self.blue_noise.sampler();
            let acceleration_structure = rt_passes
                .acceleration_structure_update
                .acceleration_structure();
            rt_passes.shadows.execute(
                &mut cmd_buf,
                &params,
                VisibilityBufferPass::DEPTH_TEXTURE_NAME,
                acceleration_structure,
                blue_noise,
                blue_noise_sampler,
            );
        }
        self.shadow_map_pass.prepare(
            &mut cmd_buf,
            &params
        );

        self.shadow_map_pass.execute(
            &mut cmd_buf,
            &params
        );

        self.shading_pass.execute(
            &mut cmd_buf,
            &params
        );
        self.ssr_pass.execute(
            &mut cmd_buf,
            &params,
            ShadingPass::<P>::SHADING_TEXTURE_NAME,
            VisibilityBufferPass::DEPTH_TEXTURE_NAME,
            true,
        );
        self.compositing_pass.execute(
            &mut cmd_buf,
            &params,
            ShadingPass::<P>::SHADING_TEXTURE_NAME,
        );

        let output_texture_name = match &mut self.anti_aliasing {
            AntiAliasing::TAA { taa, sharpen, _p: PhantomData::<P> } => {
                taa.execute(
                    &mut cmd_buf,
                    &params,
                    CompositingPass::COMPOSITION_TEXTURE_NAME,
                    VisibilityBufferPass::DEPTH_TEXTURE_NAME,
                    None,
                    true,
                );
                sharpen.execute(&mut cmd_buf, &params);
                SharpenPass::SHAPENED_TEXTURE_NAME
            }
        };

        self.ui_pass.execute(&mut cmd_buf, &params, output_texture_name, &self.ui_data);

        let output_texture = params.resources.access_texture(
            &mut cmd_buf,
            output_texture_name,
            &BarrierTextureRange::default(),
            BarrierSync::COPY,
            BarrierAccess::COPY_READ,
            TextureLayout::CopySrc,
            false,
            HistoryResourceEntry::Current,
        );

        let backbuffer = swapchain.next_backbuffer()?;
        let backbuffer_handle = swapchain.backbuffer_handle(&backbuffer);

        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::COPY,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::COPY_WRITE,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::CopyDst,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None
        }]);
        cmd_buf.flush_barriers();
        cmd_buf.blit_to_handle(&*output_texture, 0, 0, backbuffer_handle, 0, 0);
        cmd_buf.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::COPY,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::COPY_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::CopyDst,
            new_layout: TextureLayout::Present,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None
        }]);
        std::mem::drop(output_texture);

        return Ok(RenderPathResult {
            cmd_buffer: cmd_buf.finish(),
            backbuffer: Some(backbuffer)
        });
    }

    fn set_ui_data(&mut self, data: crate::ui::UIDrawData<<P as Platform>::GPUBackend>) {
        self.ui_data = data;
    }
}
