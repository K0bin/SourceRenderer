use ash::vk;

use sourcerenderer_core::graphics::Format;

pub fn format_to_vk(format: Format) -> vk::Format {
  return match format {
    Format::RGBA8 => vk::Format::R8G8B8A8_UINT,
    Format::RGB32Float => vk::Format::R32G32B32_SFLOAT,
    _ => vk::Format::R8G8B8A8_UINT
  };
}