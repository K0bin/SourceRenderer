use std::sync::Arc;

use sourcerenderer_core::{Platform, Vec4, Matrix4};

use crate::graphics::GraphicsContext;
use crate::input::Input;
use crate::renderer::render_path::{
    FrameInfo,
    RenderPath,
    SceneInfo,
    ZeroTextures,
};
use crate::renderer::renderer_assets::RendererAssets;
use crate::renderer::renderer_resources::RendererResources;
use crate::renderer::shader_manager::ShaderManager;

use crate::graphics::*;

mod geometry;

use self::geometry::GeometryPass;

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

pub struct WebRenderer<P: Platform> {
    device: Arc<Device<P::GPUBackend>>,
    geometry: GeometryPass<P>,
    resources: RendererResources<P::GPUBackend>,
}

impl<P: Platform> WebRenderer<P> {
    pub fn new(
        device: &Arc<Device<P::GPUBackend>>,
        swapchain: &Swapchain<P::GPUBackend>,
        context: &mut GraphicsContext<P::GPUBackend>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let mut resources = RendererResources::<P::GPUBackend>::new(device);
        let mut init_cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        let geometry_pass = GeometryPass::<P>::new(
            device,
            swapchain,
            &mut init_cmd_buffer,
            &mut resources,
            shader_manager,
        );

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
        task_pool.spawn(async move { c_device.flush(QueueType::Graphics); });

        Self {
            device: device.clone(),
            geometry: geometry_pass,
            resources,
        }
    }
}

impl<P: Platform> RenderPath<P> for WebRenderer<P> {
    fn is_gpu_driven(&self) -> bool {
        false
    }

    fn write_occlusion_culling_results(&self, _frame: u64, bitset: &mut Vec<u32>) {
        bitset.fill(!0u32);
    }

    fn on_swapchain_changed(
        &mut self,
        _swapchain: &Swapchain<P::GPUBackend>,
    ) {
    }

    fn render(
        &mut self,
        context: &mut GraphicsContext<P::GPUBackend>,
        swapchain: &Arc<Swapchain<P::GPUBackend>>,
        scene: &SceneInfo<P::GPUBackend>,
        zero_textures: &ZeroTextures<P::GPUBackend>,
        frame_info: &FrameInfo,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>
    ) -> Result<FinishedCommandBuffer<P::GPUBackend>, sourcerenderer_core::gpu::SwapchainError> {
        let back_buffer_res = swapchain.next_backbuffer();
        if let Err(e) = back_buffer_res{
            return Err(e);
        }

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

        let camera_buffer = cmd_buffer.upload_dynamic_data(&[main_view.proj_matrix * main_view.view_matrix], BufferUsage::CONSTANT).unwrap();

        self.geometry.execute(
            &mut cmd_buffer,
            scene.scene,
            main_view,
            &camera_buffer,
            &self.resources,
            swapchain.backbuffer(),
            swapchain.backbuffer_handle(),
            swapchain.width(),
            swapchain.height(),
            shader_manager,
            assets,
        );

        return Ok(cmd_buffer.finish());
    }

    fn set_ui_data(&mut self, data: crate::ui::UIDrawData<<P as Platform>::GPUBackend>) {
    }
}
