use metal::{self, MetalDrawable};
use metal::foreign_types::ForeignTypeRef;

use sourcerenderer_core::gpu::{self, Backbuffer, Format, Texture};
use sourcerenderer_core::Matrix4;

use super::*;

pub struct MTLSurface {
    layer: metal::MetalLayer
}

impl MTLSurface {
    pub fn new(_instance: &MTLInstance, layer: &metal::MetalLayerRef) -> Self {
        Self {
            layer: layer.to_owned()
        }
    }

    pub(crate) fn handle(&self) -> &metal::MetalLayerRef {
        &self.layer
    }
}

impl PartialEq<MTLSurface> for MTLSurface {
    fn eq(&self, other: &MTLSurface) -> bool {
        self.layer.as_ptr() == other.layer.as_ptr()
    }
}

impl Eq for MTLSurface {}

pub struct MTLBackbuffer {
    texture: MTLTexture,
    drawable: MetalDrawable
}

impl Backbuffer for MTLBackbuffer {
    fn key(&self) -> u64 {
        self.drawable.drawable_id()
    }
}

pub struct MTLSwapchain {
    surface: MTLSurface,
    device: metal::Device,
    width: u32,
    height: u32,
    format: Format
}

const IMAGE_COUNT: u32 = 3;

impl MTLSwapchain {
    pub fn new(surface: MTLSurface, device: &metal::DeviceRef, extents: Option<(u32, u32)>) -> Self {
        surface.layer.set_device(device);
        assert!(IMAGE_COUNT == 2 || IMAGE_COUNT == 3);
        surface.layer.set_maximum_drawable_count(IMAGE_COUNT as u64);

        let width: u32;
        let height: u32;
        if let Some((param_width, param_height)) = extents {
            width = param_width;
            height = param_height;
        } else {
            width = surface.handle().drawable_size().width as u32;
            height = surface.handle().drawable_size().height as u32;
        }
        let format = format_from_metal(surface.layer.pixel_format());

        Self {
            surface,
            device: device.to_owned(),
            width,
            height,
            format
        }
    }

    pub(crate) fn present(&self, cmd_buffer: &metal::CommandBuffer, backbuffer: &MTLBackbuffer) {
        cmd_buffer.present_drawable(&backbuffer.drawable);
    }
}

impl gpu::Swapchain<MTLBackend> for MTLSwapchain {
    type Backbuffer = MTLBackbuffer;

    unsafe fn next_backbuffer(&mut self) -> Result<MTLBackbuffer, gpu::SwapchainError> {
        let drawable = self.surface.layer.next_drawable().unwrap().to_owned();
        let texture = MTLTexture::from_mtl_texture(drawable.texture(), true);

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
