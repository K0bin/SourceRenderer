use std::sync::Arc;

use sourcerenderer_core::graphics::{
    Backend,
    CommandBuffer,
    Device,
    Queue,
    Swapchain,
    SwapchainError, FenceRef,
};
use sourcerenderer_core::Platform;

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

mod geometry;

use self::geometry::GeometryPass;

pub struct WebRenderer<P: Platform> {
    device: Arc<<P::GraphicsBackend as Backend>::Device>,
    swapchain: Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    geometry: GeometryPass<P>,
    resources: RendererResources<P::GraphicsBackend>,
}

impl<P: Platform> WebRenderer<P> {
    pub fn new(
        device: &Arc<<P::GraphicsBackend as Backend>::Device>,
        swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
        shader_manager: &mut ShaderManager<P>,
    ) -> Self {
        let mut resources = RendererResources::<P::GraphicsBackend>::new(device);
        let mut init_cmd_buffer = device.graphics_queue().create_command_buffer();
        let geometry_pass = GeometryPass::<P>::new(
            device,
            swapchain,
            &mut init_cmd_buffer,
            &mut resources,
            shader_manager,
        );
        let init_submission = init_cmd_buffer.finish();
        device
            .graphics_queue()
            .submit(init_submission, &[], &[], false);
        Self {
            device: device.clone(),
            swapchain: swapchain.clone(),
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
        _swapchain: &Arc<<P::GraphicsBackend as Backend>::Swapchain>,
    ) {
    }

    fn render(
        &mut self,
        scene: &SceneInfo<P::GraphicsBackend>,
        _zero_textures: &ZeroTextures<P::GraphicsBackend>,
        late_latching: Option<&dyn LateLatching<P::GraphicsBackend>>,
        input: &Input,
        _frame_info: &FrameInfo,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>,
    ) -> Result<(), sourcerenderer_core::graphics::SwapchainError> {
        let back_buffer_res = self.swapchain.prepare_back_buffer();
        if back_buffer_res.is_none() {
            return Err(SwapchainError::Other);
        }
        let back_buffer = back_buffer_res.unwrap();

        let queue = self.device.graphics_queue();
        let mut cmd_buffer = queue.create_command_buffer();

        let view_ref = &scene.views[scene.active_view_index];
        let late_latching_buffer = late_latching.unwrap().buffer();
        self.geometry.execute(
            &mut cmd_buffer,
            scene.scene,
            &view_ref,
            &late_latching_buffer,
            &self.resources,
            back_buffer.texture_view,
            shader_manager,
            assets,
        );

        if let Some(late_latching) = late_latching {
            let input_state = input.poll();
            late_latching.before_submit(&input_state, &view_ref);
        }

        queue.submit(
            cmd_buffer.finish(),
            &[FenceRef::WSIFence(back_buffer.prepare_fence)],
            &[FenceRef::WSIFence(back_buffer.present_fence)],
            false,
        );
        queue.present(&self.swapchain, back_buffer.present_fence, false);

        if let Some(late_latching) = late_latching {
            late_latching.after_submit(&self.device);
        }

        Ok(())
    }

    fn set_ui_data(&mut self, data: crate::ui::UIDrawData<<P as Platform>::GraphicsBackend>) {
        todo!()
    }
}
