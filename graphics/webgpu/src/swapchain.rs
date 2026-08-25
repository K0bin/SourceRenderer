use js_sys::wasm_bindgen::{JsCast, JsValue};
use sourcerenderer_core::{Matrix4, gpu};
use std::marker::PhantomData;
use web_sys::{
    DedicatedWorkerGlobalScope, Gpu, GpuCanvasConfiguration, GpuCanvasContext, GpuDevice,
    gpu_texture_usage, window,
};

use crate::{
    WebGPUBackend, surface::WebGPUSurface, texture::WebGPUTexture, texture::format_from_webgpu,
    texture::format_to_webgpu,
};

pub struct WebGPUBackbuffer {
    texture: WebGPUTexture,
    key: u64,
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
    _p: PhantomData<*const std::ffi::c_void>,
}

impl WebGPUSwapchain {
    pub fn new(device: &GpuDevice, surface: WebGPUSurface, width: u32, height: u32) -> Self {
        let context = surface.context();

        let instance_handle: Gpu = window().map_or_else(
            || {
                let global = js_sys::global();
                let worker_scope: DedicatedWorkerGlobalScope = global.dyn_into().unwrap();
                worker_scope.navigator().gpu()
            },
            |window| window.navigator().gpu(),
        );

        let preferred_format = instance_handle.get_preferred_canvas_format();

        let texture_info = gpu::TextureInfo {
            dimension: gpu::TextureDimension::Dim2D,
            format: format_from_webgpu(preferred_format),
            width,
            height,
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
            canvas_context: context,
            _p: PhantomData,
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
        let web_texture = self
            .canvas_context
            .get_current_texture()
            .map_err(|_e| gpu::SwapchainError::Other)?;

        if web_texture.width() != self.texture_info.width
            || web_texture.height() != self.texture_info.height
            || self.surface.width() != self.texture_info.width
            || self.surface.height() != self.texture_info.height
        {
            return Err(gpu::SwapchainError::NeedsRecreation);
        }

        let key = self.backbuffer_counter;
        self.backbuffer_counter += 1;
        let texture = WebGPUTexture::from_texture(&self.device, web_texture);
        let backbuffer = WebGPUBackbuffer { texture, key };

        Ok(backbuffer)
    }

    unsafe fn texture_for_backbuffer<'a>(
        &'a self,
        backbuffer: &'a WebGPUBackbuffer,
    ) -> &'a WebGPUTexture {
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
