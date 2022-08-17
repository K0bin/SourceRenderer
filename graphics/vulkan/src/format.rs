use ash::vk;

use sourcerenderer_core::graphics::Format;

pub fn format_to_vk(format: Format, supports_d24: bool) -> vk::Format {
  match format {
    Format::RGBA8UNorm => vk::Format::R8G8B8A8_UNORM,
    Format::R16UNorm => vk::Format::R16_UNORM,
    Format::R16Float => vk::Format::R16_SFLOAT,
    Format::R32Float => vk::Format::R32_SFLOAT,
    Format::R8Unorm => vk::Format::R8_UNORM,
    Format::RG32Float => vk::Format::R32G32_SFLOAT,
    Format::RGB32Float => vk::Format::R32G32B32_SFLOAT,
    Format::RGBA32Float => vk::Format::R32G32B32A32_SFLOAT,
    Format::BGR8UNorm => vk::Format::B8G8R8_UNORM,
    Format::BGRA8UNorm => vk::Format::B8G8R8A8_UNORM,
    Format::D16 => vk::Format::D16_UNORM,
    Format::D16S8 => vk::Format::D16_UNORM_S8_UINT,
    Format::D24 => if supports_d24 { vk::Format::D24_UNORM_S8_UINT } else { vk::Format::D32_SFLOAT },
    Format::D32 => vk::Format::D32_SFLOAT,
    Format::D32S8 => vk::Format::D32_SFLOAT_S8_UINT,
    Format::DXT1 => vk::Format::BC1_RGB_UNORM_BLOCK,
    Format::DXT1Alpha => vk::Format::BC1_RGBA_UNORM_BLOCK,
    Format::DXT3 => vk::Format::BC2_UNORM_BLOCK,
    Format::DXT5 => vk::Format::BC3_UNORM_BLOCK,
    Format::RG16UNorm => vk::Format::R16G16_UNORM,
    Format::R32UInt => vk::Format::R32_UINT,
    Format::RG16Float => vk::Format::R16G16_SFLOAT,
    Format::RGBA16Float => vk::Format::R16G16B16A16_SFLOAT,
    Format::R11G11B10Float => vk::Format::B10G11R11_UFLOAT_PACK32,
    Format::RG16UInt => vk::Format::R16G16_UINT,
    Format::RG16UInt => vk::Format::R16G16_UINT,
    Format::R16UInt => vk::Format::R16_UINT,
    Format::R16SNorm => vk::Format::R16_SNORM,
    _ => vk::Format::UNDEFINED
  }
}