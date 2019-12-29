use vk_mem::{Allocator, Allocation, AllocationInfo};
use ash::vk::Buffer;

pub struct VkStaging {
  buffer: Buffer,
  buffer_allocation: Allocation,
  buffer_allocation_info: AllocationInfo,
}

impl VkStaging {
  pub fn new(allocator: &mut Allocator, size: u64) -> Box<Self> {
     let create_info = vk_mem::AllocationCreateInfo {
      usage: vk_mem::MemoryUsage::CpuOnly,
      ..Default::default()
    };

    let (buffer, allocation, allocation_info) = allocator
    .create_buffer(
        &ash::vk::BufferCreateInfo::builder()
            .size(size)
            .usage(ash::vk::BufferUsageFlags::TRANSFER_SRC)
            .build(),
        &create_info
    )
    .unwrap();

    return Box::new(VkStaging {
      buffer: buffer,
      buffer_allocation: allocation,
      buffer_allocation_info: allocation_info
    });
  }
}
