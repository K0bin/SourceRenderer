use std::sync::Arc;

use sourcerenderer_core::Platform;

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
use crate::renderer::LateLatching;

use crate::graphics::*;

mod geometry;

use self::geometry::GeometryPass;

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
        rayon::spawn(move || c_device.flush(QueueType::Graphics));

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
        late_latching: Option<&dyn LateLatching<P::GPUBackend>>,
        input: &Input,
        frame_info: &FrameInfo,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>
    ) -> Result<(), sourcerenderer_core::gpu::SwapchainError> {
        let back_buffer_res = swapchain.next_backbuffer();
        if back_buffer_res.is_err() {
            return Err(SwapchainError::Other);
        }

        let mut cmd_buffer = context.get_command_buffer(QueueType::Graphics);

        let view_ref = &scene.views[scene.active_view_index];
        let late_latching_buffer = late_latching.unwrap().buffer();
        self.geometry.execute(
            &mut cmd_buffer,
            scene.scene,
            &view_ref,
            &late_latching_buffer,
            &self.resources,
            swapchain.backbuffer(),
            swapchain.backbuffer_handle(),
            swapchain.width(),
            swapchain.height(),
            shader_manager,
            assets,
        );

        if let Some(late_latching) = late_latching {
            let input_state = input.poll();
            late_latching.before_submit(&input_state, &view_ref);
        }

        let frame_end_signal = context.end_frame();

        self.device.submit(
            QueueType::Graphics,
            QueueSubmission {
                command_buffer: cmd_buffer.finish(),
                wait_fences: &[],
                signal_fences: &[frame_end_signal],
                acquire_swapchain: Some(&swapchain),
                release_swapchain: Some(&swapchain)
            }
        );
        self.device.present(QueueType::Graphics, &swapchain);

        let c_device = self.device.clone();
        rayon::spawn(move || c_device.flush(QueueType::Graphics));

        if let Some(late_latching) = late_latching {
            late_latching.after_submit(&self.device);
        }

        Ok(())
    }

    fn set_ui_data(&mut self, data: crate::ui::UIDrawData<<P as Platform>::GPUBackend>) {
    }
}
