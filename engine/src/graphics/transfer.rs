use std::sync::{Mutex, Arc};

use sourcerenderer_core::gpu::*;

use super::*;

pub(crate) struct Transfer<B: GPUBackend> {
  device: Arc<B::Device>,
  inner: Mutex<TransferInner>,
}

enum TransferBarrier {
  Image(vk::ImageMemoryBarrier),
  Buffer(vk::BufferMemoryBarrier),
}

enum TransferCopy<B: GPUBackend> {
  BufferToImage {
      src: Arc<B::Buffer>,
      dst: Arc<B::Texture>,
      region: vk::BufferImageCopy,
  },
  BufferToBuffer {
      src: Arc<B::Buffer>,
      dst: Arc<B::Buffer>,
      region: vk::BufferCopy,
  },
}

struct TransferInner {
  graphics: VkTransferCommands,
  transfer: Option<VkTransferCommands>,
}

struct TransferCommands {
  pre_barriers: Vec<VkTransferBarrier>,
  copies: Vec<TransferCopy>,
  post_barriers: Vec<(Option<FenceValuePair<VkBackend>>, VkTransferBarrier)>,
  used_cmd_buffers: VecDeque<Box<VkTransferCommandBuffer>>,
  pool: ,
  fence_value: FenceValuePair<VkBackend>,
  queue_name: &'static str,
  queue_family_index: u32,
}