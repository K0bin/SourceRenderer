use std::sync::Arc;

use sourcerenderer_core::{graphics::{BufferInfo, Backend, BufferUsage, Device, MemoryUsage}, atomic_refcell::AtomicRefCell};

/// We suballocate all mesh buffers from a large buffer
/// to be able use indirect rendering.
pub struct AssetBuffer<B: Backend> {
  buffer: Arc<B::Buffer>,
  free_ranges: AtomicRefCell<Vec<BufferRange>>
}

struct BufferRange {
  offset: u32,
  length: u32
}

impl<B: Backend> AssetBuffer<B> {
  pub fn new(device: &Arc<B::Device>) -> Self {
    const SIZE: u32 = 32 << 20;
    let buffer = device.create_buffer(&BufferInfo {
      size: SIZE as usize,
      usage: BufferUsage::VERTEX | BufferUsage::INDEX,
    }, MemoryUsage::GpuOnly, Some("AssetBuffer"));
    let free_range = BufferRange {
      offset: 0,
      length: SIZE
    };

    Self {
      buffer,
      free_ranges: AtomicRefCell::new(vec![free_range]),
    }
  }
}
