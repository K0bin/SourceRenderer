use sourcerenderer_core::renderer::Mesh;
use ash::vk::Buffer;
use vk_mem::{Allocator, Allocation, AllocationInfo};

pub struct VkMesh {
  vertex_buffer: Buffer,
  vertex_buffer_allocation: Allocation,
  vertex_buffer_allocation_info: AllocationInfo,
  index_buffer: Buffer,
  index_buffer_allocation: Allocation,
  index_buffer_allocation_info: AllocationInfo
}

impl VkMesh {
  pub fn new(allocator: &mut Allocator, vertex_size: u64, index_size: u64) -> Box<Self> {
    let create_info = vk_mem::AllocationCreateInfo {
      usage: vk_mem::MemoryUsage::GpuOnly,
      ..Default::default()
    };

    let (vertex_buffer, vertex_allocation, vertex_allocation_info) = allocator
    .create_buffer(
        &ash::vk::BufferCreateInfo::builder()
            .size(vertex_size)
            .usage(ash::vk::BufferUsageFlags::VERTEX_BUFFER | ash::vk::BufferUsageFlags::TRANSFER_DST)
            .build(),
        &create_info
    )
    .unwrap();

    let (index_buffer, index_allocation, index_allocation_info) = allocator
    .create_buffer(
        &ash::vk::BufferCreateInfo::builder()
            .size(index_size)
            .usage(ash::vk::BufferUsageFlags::INDEX_BUFFER | ash::vk::BufferUsageFlags::TRANSFER_DST)
            .build(),
        &create_info
    )
    .unwrap();

    return Box::new(
      VkMesh {
        vertex_buffer: vertex_buffer,
        vertex_buffer_allocation: vertex_allocation,
        vertex_buffer_allocation_info: vertex_allocation_info,
        index_buffer: index_buffer,
        index_buffer_allocation: index_allocation,
        index_buffer_allocation_info: index_allocation_info
      }
    );
  }
}

impl Mesh for VkMesh {
}
