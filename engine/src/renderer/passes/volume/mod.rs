use crate::asset::{AssetLoadPriority, AssetLoaderProgress, AssetType};
use crate::graphics::{GraphicsContext, *};
use crate::renderer::asset::{RendererAssets, RendererAssetsReadOnly};
use crate::renderer::passes::dear_imgui_renderer::DearImguiRenderer;
use crate::renderer::passes::volume::background::BackgroundPass;
use crate::renderer::passes::volume::compositing::CompositingPass;
use crate::renderer::passes::volume::ibl::ImageBasedLightingPreparation;
use crate::renderer::passes::volume::ssao::SsaoPass;
use crate::renderer::passes::volume::subsurfacescattering::SSSPass;
use crate::renderer::render_path::{
    FrameInfo, RenderPassParameters, RenderPath, RenderPathResult, SceneInfo,
};
use crate::renderer::renderer_resources::RendererResources;
use bytemuck::{Pod, Zeroable};
use marching_cubes::MarchingCubesPass;
use sourcerenderer_core::{Matrix4, Vec2UI, Vec4};
use std::sync::Arc;

mod background;
mod compositing;
mod geometry;
mod ibl;
mod marching_cubes;
mod ssao;
mod subsurfacescattering;

pub use self::geometry::GeometryPass;

#[derive(Clone, Copy, Zeroable, Pod)]
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

pub struct VolumeRenderer {
    device: Arc<Device>,
    marching_cubes_pass: MarchingCubesPass,
    geometry: GeometryPass,
    ssao: SsaoPass,
    sss_pass: SSSPass,
    ibl_pass: ImageBasedLightingPreparation,
    compositing: CompositingPass,
    background: BackgroundPass,
    ui_pass: DearImguiRenderer,
}

impl VolumeRenderer {
    pub fn new(
        device: &Arc<Device>,
        swapchain: &Swapchain,
        context: &mut GraphicsContext,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);

        let marching_cubes_pass = MarchingCubesPass::new(device, resources, assets);

        let geometry_pass = GeometryPass::new(
            device,
            assets,
            resources,
            Vec2UI::new(swapchain.width(), swapchain.height()),
        );

        let ssao = SsaoPass::new(
            device,
            resources,
            assets,
            Vec2UI::new(swapchain.width(), swapchain.height()),
            false,
        );

        let sss = SSSPass::new(
            device,
            resources,
            Vec2UI::new(swapchain.width(), swapchain.height()),
            assets,
        );

        let comp = CompositingPass::new(device, assets, swapchain);

        let ibl_prep =
            ImageBasedLightingPreparation::new(device, assets, &mut init_cmd_buffer, resources);

        let background = BackgroundPass::new(device, assets, &mut init_cmd_buffer, resources);

        let ui = DearImguiRenderer::new(device, resources, assets, swapchain.format());

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
            marching_cubes_pass,
            geometry: geometry_pass,
            ssao,
            sss_pass: sss,
            ibl_pass: ibl_prep,
            compositing: comp,
            background,
            ui_pass: ui,
        }
    }
}

impl RenderPath for VolumeRenderer {
    fn is_gpu_driven(&self) -> bool {
        false
    }

    fn write_occlusion_culling_results(&self, _frame: u64, bitset: &mut Vec<u32>) {
        bitset.fill(!0u32);
    }

    fn on_swapchain_changed(&mut self, _swapchain: &Swapchain) {}

    fn is_ready(&self, assets: &RendererAssetsReadOnly) -> bool {
        self.marching_cubes_pass.is_ready(assets)
            && self.geometry.is_ready(assets)
            && self.compositing.is_ready(assets)
            && self.background.is_ready(assets)
            && self.ibl_pass.is_ready(assets)
            && self.ssao.is_ready(assets)
            && self.sss_pass.is_ready(assets)
            && self.ui_pass.is_ready(assets)
    }

    fn render(
        &mut self,
        context: &mut GraphicsContext,
        swapchain: &mut Swapchain,
        scene: &SceneInfo,
        _frame_info: &FrameInfo,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Result<RenderPathResult, SwapchainError> {
        let backbuffer = swapchain.next_backbuffer()?;

        let mut cmd_buffer = context.get_command_buffer(QueueType::Graphics);

        let read_assets = assets.read();

        let mut all_done = true;
        for d in scene.scene.volume_mesh_instances() {
            all_done = all_done && read_assets.get_texture_opt(d.volume_texture).is_some();
        }
        if !all_done {
            return Ok(RenderPathResult {
                cmd_buffer: cmd_buffer.finish(),
                backbuffer: Some(backbuffer),
            });
        }

        let mut params = RenderPassParameters {
            device: self.device.as_ref(),
            scene,
            resources,
            assets: &read_assets,
        };

        let main_view = &scene.scene.views()[scene.active_view_index];
        let marching_cubes_map = self
            .marching_cubes_pass
            .execute(&mut cmd_buffer, &mut params);

        self.ibl_pass.execute(&mut cmd_buffer, &mut params);

        let camera_buffer = cmd_buffer
            .upload_dynamic_data(
                &[CameraBuffer {
                    view_proj: main_view.proj_matrix * main_view.view_matrix,
                    inv_proj: main_view.proj_matrix.inverse(),
                    view: main_view.view_matrix,
                    proj: main_view.proj_matrix,
                    inv_view: main_view.view_matrix.inverse(),
                    position: Vec4::new(
                        main_view.camera_position.x,
                        main_view.camera_position.y,
                        main_view.camera_position.z,
                        1.0f32,
                    ),
                    inv_proj_view: (main_view.proj_matrix * main_view.view_matrix).inverse(),
                    z_near: main_view.near_plane,
                    z_far: main_view.far_plane,
                    aspect_ratio: main_view.aspect_ratio,
                    fov: main_view.camera_fov,
                }],
                BufferUsage::CONSTANT,
            )
            .unwrap();

        self.background.execute(
            &mut cmd_buffer,
            scene.scene,
            main_view,
            &camera_buffer,
            &params,
        );

        self.geometry.execute(
            &mut cmd_buffer,
            &camera_buffer,
            &params,
            &marching_cubes_map,
        );

        self.ssao.execute(
            &mut cmd_buffer,
            &params,
            GeometryPass::DEPTH_TEXTURE_NAME,
            &camera_buffer,
        );

        /*
         * Renderer tries to use 1 unit = 1 meter
         * SSS width for skin is typically between 0.012 to 0.015.
         */

        self.sss_pass.execute(
            &mut cmd_buffer,
            &params,
            GeometryPass::COLOR_TEXTURE_NAME,
            GeometryPass::SSS_INTENSITY_TEXTURE_NAME,
            GeometryPass::DEPTH_TEXTURE_NAME,
            &camera_buffer,
            0.015,
        );

        let backbuffer_view = swapchain.backbuffer_view(&backbuffer);
        let backbuffer_handle = swapchain.backbuffer_handle(&backbuffer);

        cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::RENDER_TARGET,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::RENDER_TARGET_WRITE | BarrierAccess::RENDER_TARGET_READ,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::RenderTarget,
            texture: backbuffer_handle,
            range: BarrierTextureRange::default(),
            queue_ownership: None,
        }]);

        self.compositing.execute(
            &mut cmd_buffer,
            &backbuffer_view,
            backbuffer_handle,
            &params,
            GeometryPass::COLOR_TEXTURE_NAME,
            SsaoPass::SSAO_TEXTURE_NAME,
        );

        std::mem::drop(params);
        std::mem::drop(read_assets);
        if let Some(ui_data) = scene.scene.take_ui_data() {
            cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
                old_sync: BarrierSync::RENDER_TARGET,
                new_sync: BarrierSync::RENDER_TARGET,
                old_access: BarrierAccess::RENDER_TARGET_WRITE,
                new_access: BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE,
                old_layout: TextureLayout::RenderTarget,
                new_layout: TextureLayout::RenderTarget,
                texture: backbuffer_handle,
                queue_ownership: None,
                range: BarrierTextureRange::default(),
            }]);

            self.ui_pass.execute(
                &mut cmd_buffer,
                assets,
                resources,
                ui_data,
                &backbuffer_view,
                backbuffer_handle,
            );
        } else {
            println!("no ui data");
        }

        cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::RENDER_TARGET,
            new_sync: BarrierSync::empty(),
            old_access: BarrierAccess::RENDER_TARGET_WRITE,
            new_access: BarrierAccess::empty(),
            old_layout: TextureLayout::RenderTarget,
            new_layout: TextureLayout::Present,
            texture: backbuffer_handle,
            queue_ownership: None,
            range: BarrierTextureRange::default(),
        }]);

        cmd_buffer.flush_barriers();

        Ok(RenderPathResult {
            cmd_buffer: cmd_buffer.finish(),
            backbuffer: Some(backbuffer),
        })
    }

    fn recreate_swapchain(
        &mut self,
        new_swapchain: &mut Swapchain,
        resources: &mut RendererResources,
    ) {
        let resolution = Vec2UI::new(new_swapchain.width(), new_swapchain.height());
        SSSPass::create_textures(resources, resolution);
        GeometryPass::create_textures(resources, resolution);
        SsaoPass::create_textures(resources, resolution);
    }
}
