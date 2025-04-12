use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSUInteger;
use objc2_metal::{self, MTLCommandBuffer as _, MTLDrawable as _};
use objc2_quartz_core::{self, CAMetalDrawable as _};

use sourcerenderer_core::gpu::{self, Texture as _};
use sourcerenderer_core::Matrix4;

use super::*;

pub struct MTLSurface {
    layer: Retained<objc2_quartz_core::CAMetalLayer>
}

unsafe impl Send for MTLSurface {}
unsafe impl Sync for MTLSurface {}

impl MTLSurface {
    pub fn new(_instance: &MTLInstance, layer: Retained<objc2_quartz_core::CAMetalLayer>) -> Self {
        Self {
            layer
        }
    }

    pub(crate) fn handle(&self) -> &objc2_quartz_core::CAMetalLayer {
        &self.layer
    }
}

impl PartialEq<MTLSurface> for MTLSurface {
    fn eq(&self, other: &MTLSurface) -> bool {
        self.layer == other.layer
    }
}

impl Eq for MTLSurface {}

impl gpu::Surface<MTLBackend> for MTLSurface {
    unsafe fn create_swapchain(self, width: u32, height: u32, _vsync: bool, device: &MTLDevice) -> Result<MTLSwapchain, gpu::SwapchainError> {
        Ok(MTLSwapchain::new(self, device.handle(), Some((width, height))))
    }
}

pub struct MTLBackbuffer {
    texture: MTLTexture,
    drawable: Retained<ProtocolObject<dyn objc2_quartz_core::CAMetalDrawable>>
}

unsafe impl Send for MTLBackbuffer {}
unsafe impl Sync for MTLBackbuffer {}

impl gpu::Backbuffer for MTLBackbuffer {
    fn key(&self) -> u64 {
        self.drawable.drawableID() as u64
    }
}

pub struct MTLSwapchain {
    surface: MTLSurface,
    _device: Retained<ProtocolObject<dyn objc2_metal::MTLDevice>>,
    width: u32,
    height: u32,
    format: gpu::Format
}
unsafe impl Send for MTLSwapchain {}
unsafe impl Sync for MTLSwapchain {}

const IMAGE_COUNT: u32 = 3;

impl MTLSwapchain {
    pub unsafe fn new(surface: MTLSurface, device: &ProtocolObject<dyn objc2_metal::MTLDevice>, extents: Option<(u32, u32)>) -> Self {
        surface.layer.setDevice(Some(device));
        assert!(IMAGE_COUNT == 2 || IMAGE_COUNT == 3);
        surface.layer.setMaximumDrawableCount(IMAGE_COUNT as NSUInteger);

        let width: u32;
        let height: u32;
        if let Some((param_width, param_height)) = extents {
            width = param_width;
            height = param_height;
        } else {
            width = surface.handle().drawableSize().width as u32;
            height = surface.handle().drawableSize().height as u32;
        }
        let format = format_from_metal(surface.layer.pixelFormat());

        Self {
            surface,
            _device: Retained::from(device),
            width,
            height,
            format
        }
    }

    pub(crate) fn present(&self, cmd_buffer: &ProtocolObject<dyn objc2_metal::MTLCommandBuffer>, backbuffer: &MTLBackbuffer) {
        cmd_buffer.presentDrawable(ProtocolObject::from_ref::<ProtocolObject<dyn objc2_metal::MTLDrawable>>(backbuffer.drawable.as_ref()));
    }
}

impl gpu::Swapchain<MTLBackend> for MTLSwapchain {
    type Backbuffer = MTLBackbuffer;

    fn will_reuse_backbuffers(&self) -> bool {
        false
    }

    unsafe fn next_backbuffer(&mut self) -> Result<MTLBackbuffer, gpu::SwapchainError> {
        let drawable = self.surface.layer.nextDrawable().unwrap();
        let texture = MTLTexture::from_mtl_texture(drawable.texture());

        self.width = texture.info().width;
        self.height = texture.info().height;
        self.format = texture.info().format;

        return Ok(MTLBackbuffer {
            texture,
            drawable
        });
    }

    unsafe fn texture_for_backbuffer<'a>(&'a self, backbuffer: &'a MTLBackbuffer) -> &'a MTLTexture {
        &backbuffer.texture
    }

    unsafe fn recreate(&mut self) {}

    fn format(&self) -> gpu::Format {
        self.format
    }

    fn surface(&self) -> &MTLSurface {
        &self.surface
    }

    fn transform(&self) -> Matrix4 {
        Matrix4::IDENTITY
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}
