use ash::vk;
use sourcerenderer_core::gpu;

pub fn format_to_vk(format: gpu::Format, supports_d24: bool) -> vk::Format {
    match format {
        gpu::Format::RGBA8UNorm => vk::Format::R8G8B8A8_UNORM,
        gpu::Format::RGBA8Srgb => vk::Format::R8G8B8A8_SRGB,
        gpu::Format::R16UNorm => vk::Format::R16_UNORM,
        gpu::Format::R16Float => vk::Format::R16_SFLOAT,
        gpu::Format::R32Float => vk::Format::R32_SFLOAT,
        gpu::Format::R8Unorm => vk::Format::R8_UNORM,
        gpu::Format::RG32Float => vk::Format::R32G32_SFLOAT,
        gpu::Format::RGB32Float => vk::Format::R32G32B32_SFLOAT,
        gpu::Format::RGBA32Float => vk::Format::R32G32B32A32_SFLOAT,
        gpu::Format::BGR8UNorm => vk::Format::B8G8R8_UNORM,
        gpu::Format::BGRA8UNorm => vk::Format::B8G8R8A8_UNORM,
        gpu::Format::D16 => vk::Format::D16_UNORM,
        gpu::Format::D16S8 => vk::Format::D16_UNORM_S8_UINT,
        gpu::Format::D24 => {
            if supports_d24 {
                vk::Format::D24_UNORM_S8_UINT
            } else {
                vk::Format::D32_SFLOAT
            }
        }
        gpu::Format::D32 => vk::Format::D32_SFLOAT,
        gpu::Format::D32S8 => vk::Format::D32_SFLOAT_S8_UINT,
        gpu::Format::BC1 => vk::Format::BC1_RGB_UNORM_BLOCK,
        gpu::Format::BC1Alpha => vk::Format::BC1_RGBA_UNORM_BLOCK,
        gpu::Format::BC2 => vk::Format::BC2_UNORM_BLOCK,
        gpu::Format::BC3 => vk::Format::BC3_UNORM_BLOCK,
        gpu::Format::RG16UNorm => vk::Format::R16G16_UNORM,
        gpu::Format::RG8UNorm => vk::Format::R8G8_UNORM,
        gpu::Format::R32UInt => vk::Format::R32_UINT,
        gpu::Format::RG16Float => vk::Format::R16G16_SFLOAT,
        gpu::Format::RGBA16Float => vk::Format::R16G16B16A16_SFLOAT,
        gpu::Format::R11G11B10Float => vk::Format::B10G11R11_UFLOAT_PACK32,
        gpu::Format::RG16UInt => vk::Format::R16G16_UINT,
        gpu::Format::R16UInt => vk::Format::R16_UINT,
        gpu::Format::R16SNorm => vk::Format::R16_SNORM,
        _ => vk::Format::UNDEFINED,
    }
}
