use metal::MTLPixelFormat;

use sourcerenderer_core::gpu::Format;

pub(crate) fn format_to_mtl(format: Format) -> MTLPixelFormat {
    match format {
        Format::Unknown => MTLPixelFormat::Invalid,
        Format::R32UNorm => panic!("Unsupported format R32Unorm"),
        Format::R16UNorm => MTLPixelFormat::R16Unorm,
        Format::R8Unorm => MTLPixelFormat::R8Unorm,
        Format::RGBA8UNorm => MTLPixelFormat::RGBA8Unorm,
        Format::RGBA8Srgb => MTLPixelFormat::RGBA8Unorm_sRGB,
        Format::BGR8UNorm => panic!("Unsupported format BGR8Unorm"),
        Format::BGRA8UNorm => MTLPixelFormat::BGRA8Unorm,
        Format::BC1 => MTLPixelFormat::BC1_RGBA,
        Format::BC1Alpha => MTLPixelFormat::BC1_RGBA,
        Format::BC2 => MTLPixelFormat::BC2_RGBA,
        Format::BC3 => MTLPixelFormat::BC3_RGBA,
        Format::R16Float => MTLPixelFormat::R16Float,
        Format::R32Float => MTLPixelFormat::R32Float,
        Format::RG32Float => MTLPixelFormat::RG32Float,
        Format::RG16Float => MTLPixelFormat::RG16Float,
        Format::RGB32Float => panic!("Unsupported format RGB32Float"),
        Format::RGBA32Float => MTLPixelFormat::RGBA32Float,
        Format::RG16UNorm => MTLPixelFormat::RG16Unorm,
        Format::RG8UNorm => MTLPixelFormat::RG8Unorm,
        Format::R32UInt => MTLPixelFormat::R32Uint,
        Format::RGBA16Float => MTLPixelFormat::RGBA16Float,
        Format::R11G11B10Float => MTLPixelFormat::RG11B10Float,
        Format::RG16UInt => MTLPixelFormat::RG16Uint,
        Format::R16UInt => MTLPixelFormat::R16Uint,
        Format::R16SNorm => MTLPixelFormat::R16Snorm,
        Format::D16 => MTLPixelFormat::Depth16Unorm,
        Format::D16S8 => panic!("Unsupported format D16S8"),
        Format::D32 => MTLPixelFormat::Depth32Float,
        Format::D32S8 => MTLPixelFormat::Depth32Float_Stencil8,
        Format::D24 => MTLPixelFormat::Depth32Float,
    }
}
