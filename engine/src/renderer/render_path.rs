use std::sync::Arc;
use std::time::Duration;

use sourcerenderer_core::gpu::GPUBackend;
use sourcerenderer_core::Platform;

use super::drawable::View;
use super::renderer_assets::{
    RendererAssets,
    RendererTexture,
};
use super::renderer_resources::RendererResources;
use super::renderer_scene::RendererScene;
use super::shader_manager::ShaderManager;
use crate::graphics::{BufferRef, GraphicsContext, TextureView};
use crate::input::Input;
use crate::ui::UIDrawData;
use crate::graphics::*;

pub struct SceneInfo<'a, B: GPUBackend> {
    pub scene: &'a RendererScene<B>,
    pub active_view_index: usize,
    pub vertex_buffer: BufferRef<'a, B>,
    pub index_buffer: BufferRef<'a, B>,
    pub lightmap: Option<&'a RendererTexture<B>>,
}

#[derive(Clone)]
pub struct ZeroTextures<'a, B: GPUBackend> {
    pub zero_texture_view: &'a Arc<TextureView<B>>,
    pub zero_texture_view_black: &'a Arc<TextureView<B>>,
}

pub struct FrameInfo {
    pub frame: u64,
    pub delta: Duration,
}

pub struct RenderPassParameters<'a, P: Platform> {
    pub device: &'a Device<P::GPUBackend>,
    pub scene: &'a SceneInfo<'a, P::GPUBackend>,
    pub shader_manager: &'a ShaderManager<P>,
    pub resources: &'a mut RendererResources<P::GPUBackend>,
    pub zero_textures: &'a ZeroTextures<'a, P::GPUBackend>,
    pub assets: &'a RendererAssets<P>
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
        zero_textures: &ZeroTextures<P::GPUBackend>,
        frame_info: &FrameInfo,
        shader_manager: &ShaderManager<P>,
        assets: &RendererAssets<P>,
    ) -> Result<FinishedCommandBuffer<P::GPUBackend>, SwapchainError>;
}
