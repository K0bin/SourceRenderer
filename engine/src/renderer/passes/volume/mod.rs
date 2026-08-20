use std::sync::Arc;

use crate::asset::{AssetLoadPriority, AssetLoaderProgress, AssetType, TextureHandle};
use crate::graphics::{GraphicsContext, *};
use crate::renderer::asset::{RendererAssets, RendererAssetsReadOnly};
use crate::renderer::passes::marching_cubes::MarchingCubesPass;
use crate::renderer::passes::volume::background::BackgroundPass;
use crate::renderer::passes::volume::compositing::CompositingPass;
use crate::renderer::passes::volume::ibl::ImageBasedLightingPreparation;
use crate::renderer::passes::volume::ssao::SsaoPass;
use crate::renderer::passes::volume::subsurfacescattering::SSSPass;
use crate::renderer::render_path::{
    FrameInfo, RenderPassParameters, RenderPath, RenderPathResult, SceneInfo,
};
use crate::renderer::renderer_resources::RendererResources;
use sourcerenderer_core::{Matrix4, Vec2UI, Vec3, Vec3UI, Vec4};

mod background;
mod compositing;
mod geometry;
mod ibl;
mod ssao;
mod subsurfacescattering;

pub use self::geometry::GeometryPass;

#[allow(unused)]
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

pub struct VolumeRenderer {
    device: Arc<Device>,
    marching_cubes_pass: MarchingCubesPass,
    geometry: GeometryPass,
    ssao: SsaoPass,
    sss_pass: SSSPass,
    ibl_pass: ImageBasedLightingPreparation,
    texture_handle: TextureHandle,
    texture_progress: Arc<AssetLoaderProgress>,
    threshold: f32,
    lod: f32,
    compositing: CompositingPass,
    background: BackgroundPass,
}

impl VolumeRenderer {
    pub fn new(
        device: &Arc<Device>,
        swapchain: &Swapchain,
        context: &mut GraphicsContext,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        let (texture_handle, progress) = assets.asset_manager().request_asset(
            //"ct_head_256.raw.txt",
            "manix.raw.txt",
            AssetType::Texture,
            AssetLoadPriority::High,
        );

        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);

        let marching_cubes_pass = MarchingCubesPass::new(device, resources, assets);

        let geometry_pass =
            GeometryPass::new(device, assets, swapchain, &mut init_cmd_buffer, resources);

        let ssao = SsaoPass::new(
            device,
            Vec2UI::new(swapchain.width(), swapchain.height()),
            resources,
            assets,
            false,
        );

        let sss = SSSPass::new(
            device,
            Vec2UI::new(swapchain.width(), swapchain.height()),
            resources,
            assets,
        );

        let comp = CompositingPass::new(device, assets, swapchain);

        let ibl_prep =
            ImageBasedLightingPreparation::new(device, assets, &mut init_cmd_buffer, resources);

        let background = BackgroundPass::new(device, assets, &mut init_cmd_buffer, resources);

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
            texture_handle: TextureHandle::from(texture_handle),
            texture_progress: progress,
            //threshold: 0.0505f32,
            threshold: 0.0f32,
            lod: 0.0f32,
            compositing: comp,
            background,
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
            && self.texture_progress.is_done()
            && self.compositing.is_ready(assets)
            && self.background.is_ready(assets)
            && self.ibl_pass.is_ready(assets)
            && self.ssao.is_ready(assets)
            && self.sss_pass.is_ready(assets)
    }

    fn render(
        &mut self,
        context: &mut GraphicsContext,
        swapchain: &mut Swapchain,
        scene: &SceneInfo,
        _frame_info: &FrameInfo,
        resources: &mut RendererResources,
        assets: &RendererAssetsReadOnly<'_>,
    ) -> Result<RenderPathResult, sourcerenderer_core::gpu::SwapchainError> {
        //self.threshold += 0.0000005f32;
        //self.threshold += 0.00005f32;
        //self.threshold += 0.00005f32;
        //self.threshold = 0.0288;
        self.threshold = 0.55f32;
        //self.threshold = self.threshold % 0.10f32;
        self.threshold = self.threshold % 1.0f32;
        //self.lod += 0.0001f32 / self.lod.max(1.0f32);
        //self.lod = 3f32;
        self.lod = self.lod % 6.9f32;
        //self.threshold += 50.00005f32;
        //self.threshold = self.threshold % 1.0f32;

        //self.threshold = 0.00005f32 * 150.0f32;

        let geometry_lod = 0u32;
        let transparency_threshold = 0.0288;
        //let geometry_lod = self.lod as u32;

        let marching_cube_scale = Vec3::new(0.488281f32, 0.488281f32, 0.700012f32) * 8f32 * 0.01f32;
        let lod_scale = (1u32 << geometry_lod) as f32;

        let model_matrix = Matrix4::from_rotation_x(-1.57f32)
            * Matrix4::from_rotation_z(3.14)
            * Matrix4::from_scale(marching_cube_scale)
            * Matrix4::from_scale(Vec3::new(lod_scale, lod_scale, lod_scale));

        let backbuffer = swapchain.next_backbuffer()?;

        let mut cmd_buffer = context.get_command_buffer(QueueType::Graphics);

        let mut params = RenderPassParameters {
            device: self.device.as_ref(),
            scene,
            resources,
            assets,
        };

        let main_view = &scene.scene.views()[scene.active_view_index];

        let inv_view_proj = (main_view.proj_matrix * main_view.view_matrix).inverse();
        let inv_model = model_matrix.inverse();
        let mut start = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut end = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for x in 0..=1 {
            for y in 0..=1 {
                for z in 0..=1 {
                    let ndc_pos =
                        Vec4::new((x * 2 - 1) as f32, (y * 2 - 1) as f32, z as f32, 1.0f32);
                    let mut world_space_pos = inv_view_proj * ndc_pos;
                    world_space_pos.x /= world_space_pos.w;
                    world_space_pos.y /= world_space_pos.w;
                    world_space_pos.z /= world_space_pos.w;
                    let model_space_pos = inv_model * world_space_pos;
                    start = Vec3::new(
                        start.x.min(model_space_pos.x),
                        start.y.min(model_space_pos.y),
                        start.z.min(model_space_pos.z),
                    );
                    end = Vec3::new(
                        end.x.max(model_space_pos.x),
                        end.y.max(model_space_pos.y),
                        end.z.max(model_space_pos.z),
                    );
                }
            }
        }

        let volume_texture_info = {
            assets
                .get_texture(self.texture_handle)
                .view
                .texture()
                .unwrap()
                .info()
                .clone()
        };

        // TODO: Temporarily disabled, fix this!
        start = Vec3::new(0.0f32, 0.0f32, 0.0f32);
        end = Vec3::new(
            (volume_texture_info.width >> geometry_lod) as f32,
            (volume_texture_info.height >> geometry_lod) as f32,
            (volume_texture_info.depth >> geometry_lod) as f32,
        );

        self.marching_cubes_pass.execute(
            &mut cmd_buffer,
            &params,
            self.texture_handle,
            self.threshold,
            transparency_threshold,
            geometry_lod,
            Vec3UI::new(
                (start.x.max(0.0f32) as u32).min(volume_texture_info.width >> geometry_lod),
                (start.y.max(0.0f32) as u32).min(volume_texture_info.height >> geometry_lod),
                (start.z.max(0.0f32) as u32).min(volume_texture_info.depth >> geometry_lod),
            ),
            Vec3UI::new(
                ((end.x.max(0.0f32) + 0.5f32) as u32)
                    .min(volume_texture_info.width >> geometry_lod),
                ((end.y.max(0.0f32) + 0.5f32) as u32)
                    .min(volume_texture_info.height >> geometry_lod),
                ((end.z.max(0.0f32) + 0.5f32) as u32)
                    .min(volume_texture_info.depth >> geometry_lod),
            ),
        );

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
            assets,
        );

        self.geometry.execute(
            &mut cmd_buffer,
            scene.scene,
            main_view,
            &camera_buffer,
            &params,
            assets,
            self.texture_handle,
            model_matrix,
            self.threshold,
            transparency_threshold,
            geometry_lod,
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

        self.compositing.execute(
            &mut cmd_buffer,
            &backbuffer_view,
            backbuffer_handle,
            &params,
            GeometryPass::COLOR_TEXTURE_NAME,
            //GeometryPass::COLOR_TEXTURE_NAME,
            SsaoPass::SSAO_TEXTURE_NAME,
        );

        return Ok(RenderPathResult {
            cmd_buffer: cmd_buffer.finish(),
            backbuffer: Some(backbuffer),
        });
    }

    fn set_ui_data(&mut self, _data: crate::ui::UIDrawData) {}
}
