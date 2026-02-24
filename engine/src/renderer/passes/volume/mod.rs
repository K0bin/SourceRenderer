use std::sync::Arc;

use crate::asset::{AssetLoadPriority, AssetLoaderProgress, AssetType, TextureHandle};
use crate::graphics::{GraphicsContext, *};
use crate::renderer::asset::{RendererAssets, RendererAssetsReadOnly};
use crate::renderer::passes::marching_cubes::MarchingCubesPass;
use crate::renderer::passes::ssao::SsaoPass;
use crate::renderer::render_path::{
    FrameInfo, RenderPassParameters, RenderPath, RenderPathResult, SceneInfo,
};
use crate::renderer::renderer_resources::RendererResources;
use sourcerenderer_core::{Matrix4, Vec2UI, Vec3, Vec4};

mod geometry;
mod ssao;

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
    texture_handle: TextureHandle,
    texture_progress: Arc<AssetLoaderProgress>,
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
            texture_handle: TextureHandle::from(texture_handle),
            texture_progress: progress,
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
        self.marching_cubes_pass.is_ready(&assets)
            && self.geometry.is_ready(&assets)
            && self.texture_progress.is_done()
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
        let backbuffer = swapchain.next_backbuffer()?;

        let mut cmd_buffer = context.get_command_buffer(QueueType::Graphics);

        let params = RenderPassParameters {
            device: self.device.as_ref(),
            scene,
            resources,
            assets,
        };

        self.marching_cubes_pass.execute(
            &mut cmd_buffer,
            &params,
            self.texture_handle,
            0.00001f32,
            Vec3::new(0.488281f32, 0.488281f32, 0.700012f32),
        );

        let main_view = &scene.scene.views()[scene.active_view_index];

        /*let camera_buffer = cmd_buffer.upload_dynamic_data(&[CameraBuffer {
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
        }], BufferUsage::CONSTANT).unwrap();*/

        let camera_buffer = cmd_buffer
            .upload_dynamic_data(
                &[main_view.proj_matrix * main_view.view_matrix],
                BufferUsage::CONSTANT,
            )
            .unwrap();

        let backbuffer_view = swapchain.backbuffer_view(&backbuffer);
        let backbuffer_handle = swapchain.backbuffer_handle(&backbuffer);
        self.geometry.execute(
            &mut cmd_buffer,
            scene.scene,
            main_view,
            &camera_buffer,
            resources,
            &backbuffer_view,
            backbuffer_handle,
            swapchain.width(),
            swapchain.height(),
            assets,
        );

        return Ok(RenderPathResult {
            cmd_buffer: cmd_buffer.finish(),
            backbuffer: Some(backbuffer),
        });
    }

    fn set_ui_data(&mut self, _data: crate::ui::UIDrawData) {}
}
