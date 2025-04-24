use std::sync::Arc;

use sourcerenderer_core::{
    Matrix4,
    Vec4,
};

use crate::graphics::{
    GraphicsContext,
    *,
};
use crate::renderer::asset::{
    RendererAssets,
    RendererAssetsReadOnly,
};
use crate::renderer::render_path::{
    FrameInfo,
    RenderPath,
    RenderPathResult,
    SceneInfo,
};
use crate::renderer::renderer_resources::RendererResources;

mod geometry;

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

pub struct WebRenderer {
    geometry: GeometryPass,
}

impl WebRenderer {
    pub fn new(
        device: &Arc<Device>,
        swapchain: &Swapchain,
        context: &mut GraphicsContext,
        resources: &mut RendererResources,
        assets: &RendererAssets,
    ) -> Self {
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        let geometry_pass =
            GeometryPass::new(device, assets, swapchain, &mut init_cmd_buffer, resources);

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
            geometry: geometry_pass,
        }
    }
}

impl RenderPath for WebRenderer {
    fn is_gpu_driven(&self) -> bool {
        false
    }

    fn write_occlusion_culling_results(&self, _frame: u64, bitset: &mut Vec<u32>) {
        bitset.fill(!0u32);
    }

    fn on_swapchain_changed(&mut self, _swapchain: &Swapchain) {}

    fn is_ready(&self, assets: &RendererAssetsReadOnly) -> bool {
        self.geometry.is_ready(&assets)
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
