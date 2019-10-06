use ash::vk;

use sourcerenderer_core::graphics::Format;

pub fn format_to_vk(format: Format) -> vk::Format {
  return match format {
    Format::RGBA8 => vk::Format::R8G8B8A8_UINT,
    Format::RGB32Float => vk::Format::R32G32B32_SFLOAT,
    Format::BGR8UNorm => vk::Format::B8G8R8_UNORM,
    Format::BGRA8UNorm => vk::Format::B8G8R8A8_UNORM,
    _ => vk::Format::R8G8B8A8_UINT
  };
}