use std::sync::Arc;

use web_time::Duration;

use super::asset::{
    RendererAssetsReadOnly,
    RendererTexture,
};
use super::renderer_resources::RendererResources;
use super::renderer_scene::RendererScene;
use crate::graphics::{
    Backbuffer,
    BufferRef,
    GraphicsContext,
    *,
};
use crate::ui::UIDrawData;

pub struct SceneInfo<'a> {
    pub scene: &'a RendererScene,
    pub active_view_index: usize,
    pub vertex_buffer: BufferRef<'a>,
    pub index_buffer: BufferRef<'a>,
    pub lightmap: Option<&'a RendererTexture>,
}

pub struct FrameInfo {
    pub frame: u64,
    pub delta: Duration,
}

pub struct RenderPassParameters<'a> {
    pub device: &'a Device,
    pub scene: &'a SceneInfo<'a>,
    pub resources: &'a mut RendererResources,
    pub assets: &'a RendererAssetsReadOnly<'a>,
}

pub struct RenderPathResult {
    pub cmd_buffer: FinishedCommandBuffer,
    pub backbuffer: Option<Arc<Backbuffer>>,
}

pub trait RenderPath {
    fn is_gpu_driven(&self) -> bool;
    fn write_occlusion_culling_results(&self, frame: u64, bitset: &mut Vec<u32>);
    fn on_swapchain_changed(&mut self, swapchain: &Swapchain);
    fn set_ui_data(&mut self, data: UIDrawData);
    fn is_ready(&self, assets: &RendererAssetsReadOnly) -> bool;
    fn render(
        &mut self,
        context: &mut GraphicsContext,
        swapchain: &mut Swapchain,
        scene: &SceneInfo,
        frame_info: &FrameInfo,
        resources: &mut RendererResources,
        assets: &RendererAssetsReadOnly<'_>,
    ) -> Result<RenderPathResult, SwapchainError>;
}

pub struct NoOpRenderPath;

impl RenderPath for NoOpRenderPath {
    fn is_gpu_driven(&self) -> bool {
        false
    }
    fn write_occlusion_culling_results(&self, _frame: u64, _bitset: &mut Vec<u32>) {}
    fn on_swapchain_changed(&mut self, _swapchain: &Swapchain) {}
    fn set_ui_data(&mut self, _data: UIDrawData) {}
    fn is_ready(&self, _asset_manager: &RendererAssetsReadOnly) -> bool {
        true
    }
    fn render(
        &mut self,
        context: &mut GraphicsContext,
        swapchain: &mut Swapchain,
        _scene: &SceneInfo,
        _frame_info: &FrameInfo,
        _resources: &mut RendererResources,
        _assets: &RendererAssetsReadOnly<'_>,
    ) -> Result<RenderPathResult, SwapchainError> {
        let backbuffer = swapchain.next_backbuffer()?;
        let mut cmd_buffer = context.get_command_buffer(QueueType::Graphics);
        cmd_buffer.barrier(&[Barrier::RawTextureBarrier {
            old_sync: BarrierSync::all(),
            new_sync: BarrierSync::all(),
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::Present,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::empty(),
            texture: swapchain.backbuffer_handle(&backbuffer),
            range: BarrierTextureRange::default(),
            queue_ownership: None,
        }]);
        Ok(RenderPathResult {
            cmd_buffer: cmd_buffer.finish(),
            backbuffer: Some(backbuffer),
        })
    }
}
