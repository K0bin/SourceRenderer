use ash::vk;

use sourcerenderer_core::graphics::Format;

pub fn format_to_vk(format: Format) -> vk::Format {
  return match format {
    Format::RGBA8 => vk::Format::R8G8B8A8_UNORM,
    Format::RG32Float => vk::Format::R32G32_SFLOAT,
    Format::RGB32Float => vk::Format::R32G32B32_SFLOAT,
    Format::BGR8UNorm => vk::Format::B8G8R8_UNORM,
    Format::BGRA8UNorm => vk::Format::B8G8R8A8_UNORM,
    Format::D16 => vk::Format::D16_UNORM,
    Format::D16S8 => vk::Format::D16_UNORM_S8_UINT,
    Format::D24S8 => vk::Format::D24_UNORM_S8_UINT,
    Format::D32 => vk::Format::D32_SFLOAT,
    Format::D32S8 => vk::Format::D32_SFLOAT_S8_UINT,
    _ => vk::Format::R8G8B8A8_UINT
  };
}