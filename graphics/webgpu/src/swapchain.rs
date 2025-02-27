
use sourcerenderer_core::{gpu, Matrix4};
use js_sys::wasm_bindgen::JsValue;
use web_sys::{gpu_texture_usage, Gpu, GpuCanvasConfiguration, GpuCanvasContext, GpuDevice};

use crate::{surface::WebGPUSurface, texture::WebGPUTexture, WebGPUBackend, texture::format_from_webgpu, texture::format_to_webgpu};

pub struct WebGPUBackbuffer {
    texture: WebGPUTexture,
    key: u64
}

impl gpu::Backbuffer for WebGPUBackbuffer {
    fn key(&self) -> u64 {
        self.key
    }
}

pub struct WebGPUSwapchain {
    device: GpuDevice,
    surface: WebGPUSurface,
    texture_info: gpu::TextureInfo,
    canvas_context: GpuCanvasContext,
    backbuffer_counter: u64,
}

unsafe impl Send for WebGPUSwapchain {}
unsafe impl Sync for WebGPUSwapchain {}

impl WebGPUSwapchain {
    pub fn new(device: &GpuDevice, surface: WebGPUSurface) -> Self {
        let context_obj: JsValue = surface.canvas().get_context("webgpu")
            .expect("Failed to retrieve context from OffscreenCanvas")
            .expect("Failed to retrieve context from OffscreenCanvas")
            .into();
        let context: GpuCanvasContext = context_obj.into();
        let preferred_format = surface.instance_handle().get_preferred_canvas_format();

        let texture_info = gpu::TextureInfo {
            dimension: gpu::TextureDimension::Dim2D,
            format: format_from_webgpu(preferred_format),
            width: surface.canvas().width(),
            height: surface.canvas().height(),
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: gpu::SampleCount::Samples1,
            usage: gpu::TextureUsage::RENDER_TARGET
            | gpu::TextureUsage::COPY_DST
            | gpu::TextureUsage::BLIT_DST,
            supports_srgb: false,
        };

        let config = GpuCanvasConfiguration::new(device, format_to_webgpu(texture_info.format));
        config.set_usage(gpu_texture_usage::RENDER_ATTACHMENT | gpu_texture_usage::COPY_DST);
        context.configure(&config).unwrap();

        Self {
            device: device.clone(),
            surface,
            backbuffer_counter: 0u64,
            texture_info,
            canvas_context: context
        }
    }
}

impl gpu::Swapchain<WebGPUBackend> for WebGPUSwapchain {
    type Backbuffer = WebGPUBackbuffer;

    unsafe fn recreate(&mut self) {}

    fn will_reuse_backbuffers(&self) -> bool {
        false
    }

    unsafe fn next_backbuffer(&mut self) -> Result<WebGPUBackbuffer, gpu::SwapchainError> {
        let web_texture = self.canvas_context.get_current_texture()
            .map_err(|_e| gpu::SwapchainError::Other)?;

        let key = self.backbuffer_counter;
        self.backbuffer_counter += 1;
        let texture = WebGPUTexture::from_texture(&self.device, web_texture);
        let backbuffer = WebGPUBackbuffer {
            texture,
            key
        };

        Ok(backbuffer)
    }

    unsafe fn texture_for_backbuffer<'a>(&'a self, backbuffer: &'a WebGPUBackbuffer) -> &'a WebGPUTexture {
        &backbuffer.texture
    }

    fn format(&self) -> gpu::Format {
        self.texture_info.format
    }

    fn surface(&self) -> &WebGPUSurface {
        &self.surface
    }

    fn transform(&self) -> Matrix4 {
        Matrix4::IDENTITY
    }

    fn width(&self) -> u32 {
        self.texture_info.width
    }

    fn height(&self) -> u32 {
        self.texture_info.height
    }
}
