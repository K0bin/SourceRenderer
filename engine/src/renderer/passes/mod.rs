#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod blit;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod blue_noise;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod clustering;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod compositing;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod conservative;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod light_binning;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod path_tracing;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod prepass;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod sharpen;
pub(crate) mod ssao;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod ssr;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod taa;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod modern;

#[cfg(not(target_arch = "wasm32"))]
use modern::rt_shadows;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod dear_imgui_renderer;
#[cfg(not(target_arch = "wasm32"))]
use dear_imgui_renderer::DearImguiRenderer;


pub(crate) mod web;

pub(crate) mod volume;
pub(crate) mod vbao;

#[cfg(target_arch = "wasm32")]
use crate::graphics::{BackendTexture, CommandBuffer, Device, TextureView};
#[cfg(target_arch = "wasm32")]
use crate::renderer::asset::{RendererAssets, RendererAssetsReadOnly};
#[cfg(target_arch = "wasm32")]
use crate::renderer::command::ImguiFrameSnapshot;
#[cfg(target_arch = "wasm32")]
use crate::renderer::renderer_resources::RendererResources;
#[cfg(target_arch = "wasm32")]
use sourcerenderer_core::gpu::Format;
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
struct DearImguiRenderer(());
#[cfg(target_arch = "wasm32")]
impl DearImguiRenderer {
    fn new(
        _device: &Device,
        _resources: &mut RendererResources,
        _assets: &RendererAssets,
        _rt_format: Format,) -> Self {
        Self(())
    }
    fn is_ready(&self, assets: &RendererAssetsReadOnly<'_>) -> bool {
        true
    }
    fn execute(
        &mut self,
        _device: &Device,
        _command_buffer: &mut CommandBuffer,
        _renderer_assets: &RendererAssets,
        _resources: &RendererResources,
        _snapshot: ImguiFrameSnapshot,
        _backbuffer_view: &Arc<TextureView>,
        _backbuffer_handle: &BackendTexture,
    ) {}
}