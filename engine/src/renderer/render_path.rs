use std::sync::{Arc, RwLockReadGuard};
use std::time::Duration;

use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::Platform;

use super::asset::{RendererAssetsReadOnly, RendererTexture};
use super::renderer_resources::RendererResources;
use super::renderer_scene::RendererScene;
use crate::asset::AssetManager;
use crate::graphics::{BufferRef, GraphicsContext, TextureView};
use crate::ui::UIDrawData;
use crate::graphics::*;

pub struct SceneInfo<'a, B: GPUBackend> {
    pub scene: &'a RendererScene<B>,
    pub active_view_index: usize,
    pub vertex_buffer: BufferRef<'a, B>,
    pub index_buffer: BufferRef<'a, B>,
    pub lightmap: Option<&'a RendererTexture<B>>,
}

pub struct FrameInfo {
    pub frame: u64,
    pub delta: Duration,
}

pub struct RenderPassParameters<'a, P: Platform> {
    pub device: &'a Device<P::GPUBackend>,
    pub scene: &'a SceneInfo<'a, P::GPUBackend>,
    pub resources: &'a mut RendererResources<P::GPUBackend>,
    pub assets: &'a RendererAssetsReadOnly<'a, P>
}

pub(super) trait RenderPath<P: Platform> : Send {
    fn is_gpu_driven(&self) -> bool;
    fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>);
    fn on_swapchain_changed(&mut self, swapchain: &Swapchain<P::GPUBackend>);
    fn set_ui_data(&mut self, data: UIDrawData<P::GPUBackend>);
    fn render(
        &mut self,
        context: &mut GraphicsContext<P::GPUBackend>,
        swapchain: &Arc<Swapchain<P::GPUBackend>>,
        scene: &SceneInfo<P::GPUBackend>,
        frame_info: &FrameInfo,
        assets: &RendererAssetsReadOnly<'_, P>,
    ) -> Result<FinishedCommandBuffer<P::GPUBackend>, SwapchainError>;
}

pub struct NoOpRenderPath(());

impl<P: Platform> RenderPath<P> for NoOpRenderPath {
    fn is_gpu_driven(&self) -> bool {
        false
    }
    fn write_occlusion_culling_results(&self, _frame: u64, _bitset: &mut Vec<u32>) {}
    fn on_swapchain_changed(&mut self, _swapchain: &Swapchain<<P as Platform>::GPUBackend>) {}
    fn set_ui_data(&mut self, _data: UIDrawData<<P as Platform>::GPUBackend>) {}
    fn render(
        &mut self,
        context: &mut GraphicsContext<<P as Platform>::GPUBackend>,
        _swapchain: &Arc<Swapchain<<P as Platform>::GPUBackend>>,
        _scene: &SceneInfo<<P as Platform>::GPUBackend>,
        _frame_info: &FrameInfo,
        _assets: &RendererAssetsReadOnly<'_, P>,
    ) -> Result<FinishedCommandBuffer<<P as Platform>::GPUBackend>, SwapchainError> {
        let cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        Ok(cmd_buffer.finish())
    }
}
