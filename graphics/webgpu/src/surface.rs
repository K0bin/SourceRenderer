use js_sys::{wasm_bindgen::JsValue, Object};
use log::error;
use sourcerenderer_core::gpu::{Format, SampleCount, TextureDimension, TextureInfo, TextureUsage};
use web_sys::{gpu_texture_usage, Gpu, GpuCanvasConfiguration, GpuCanvasContext, GpuDevice, OffscreenCanvas};

use crate::texture::format_to_webgpu;

pub struct WebGPUSurface {
    canvas_context: GpuCanvasContext,
    texture_info: TextureInfo
}

unsafe impl Send for WebGPUSurface {}
unsafe impl Sync for WebGPUSurface {}

impl PartialEq for WebGPUSurface {
    fn eq(&self, other: &Self) -> bool {
        self.canvas_context == other.canvas_context
    }
}

impl Eq for WebGPUSurface {}

impl WebGPUSurface {
    pub fn new(device: &GpuDevice, canvas: OffscreenCanvas) -> Result<Self, ()> {
        let context_obj: JsValue = canvas.get_context("webgpu")
            .map_err(|_| {
                error!("Failed to retrieve context from OffscreenCanvas");
                ()
            })?
            .ok_or_else(|| {
                error!("Failed to retrieve context from OffscreenCanvas");
                ()
            })?
            .into();
        let context: GpuCanvasContext = context_obj.into();

        let texture_info = TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::RGBA8UNorm,
            width: canvas.width(),
            height: canvas.height(),
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::RENDER_TARGET
            | TextureUsage::COPY_DST
            | TextureUsage::BLIT_DST,
            supports_srgb: false,
        };

        let config = GpuCanvasConfiguration::new(device, format_to_webgpu(texture_info.format));
        config.set_usage(gpu_texture_usage::RENDER_ATTACHMENT | gpu_texture_usage::COPY_DST);
        context.configure(&config).unwrap();

        Ok(Self {
            canvas_context: context,
            texture_info
        })
    }

    pub(crate) fn canvas_context(&self) -> &GpuCanvasContext {
        &self.canvas_context
    }

    pub fn texture_info(&self) -> &TextureInfo {
        &self.texture_info
    }
}
