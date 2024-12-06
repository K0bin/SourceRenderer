use sourcerenderer_core::gpu::{Format, TextureInfo};
use web_sys::{GpuDevice, GpuTexture, GpuTextureDescriptor, GpuTextureFormat};

pub(crate) fn format_to_webgpu(format: Format) -> GpuTextureFormat {
    match format {
        Format::Unknown => GpuTextureFormat::__Invalid,
        Format::R32UNorm => panic!("Unsupported format"),
        Format::R16UNorm => panic!("Unsupported format"),
        Format::R8Unorm => GpuTextureFormat::R8unorm,
        Format::RGBA8UNorm => GpuTextureFormat::Rgba8unorm,
        Format::RGBA8Srgb => GpuTextureFormat::Rgba8unormSrgb,
        Format::BGR8UNorm => panic!("Unsupported format"),
        Format::BGRA8UNorm => GpuTextureFormat::Bgra8unorm,
        Format::BC1 => GpuTextureFormat::Bc1RgbaUnorm,
        Format::BC1Alpha => GpuTextureFormat::Bc1RgbaUnorm,
        Format::BC2 => GpuTextureFormat::Bc2RgbaUnorm,
        Format::BC3 => GpuTextureFormat::Bc3RgbaUnorm,
        Format::R16Float => GpuTextureFormat::R16float,
        Format::R32Float => GpuTextureFormat::R32float,
        Format::RG32Float => GpuTextureFormat::Rg32float,
        Format::RG16Float => GpuTextureFormat::Rg16float,
        Format::RGB32Float => panic!("Unsupported format"),
        Format::RGBA32Float => GpuTextureFormat::Rgba32float,
        Format::RG16UNorm => panic!("Unsupported format"),
        Format::RG8UNorm => GpuTextureFormat::Rg8unorm,
        Format::R32UInt => GpuTextureFormat::R32uint,
        Format::RGBA16Float => GpuTextureFormat::Rgba16float,
        Format::R11G11B10Float => panic!("Unsupported format"),
        Format::RG16UInt => GpuTextureFormat::Rg16uint,
        Format::RG16SInt => GpuTextureFormat::Rg16sint,
        Format::R16UInt => GpuTextureFormat::R16uint,
        Format::R16SNorm => panic!("Unsupported format"),
        Format::R16SInt => GpuTextureFormat::R16sint,
        Format::D16 => GpuTextureFormat::Depth16unorm,
        Format::D16S8 => GpuTextureFormat::Depth24plusStencil8,
        Format::D32 => GpuTextureFormat::Depth32float,
        Format::D32S8 => GpuTextureFormat::Depth32floatStencil8,
        Format::D24S8 => GpuTextureFormat::Depth24plusStencil8,
    }
}

pub struct WebGPUTexture {
    texture: GpuTexture,
    info: TextureInfo
}

impl WebGPUTexture {
    pub fn new(device: &GpuDevice, info: &TextureInfo) -> Self {

        let descriptor = GpuTextureDescriptor::new(format_to_webgpu(info.format), size, usage)
    }
}