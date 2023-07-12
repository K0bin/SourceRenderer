use std::{hash::Hash, ffi::c_void};

bitflags! {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub struct BufferUsage: u32 {
    const VERTEX                             = 0b1;
    const INDEX                              = 0b10;
    const STORAGE                            = 0b100;
    const CONSTANT                           = 0b1000;
    const COPY_SRC                           = 0b100000;
    const COPY_DST                           = 0b1000000;
    const INDIRECT                           = 0b10000000;
    const ACCELERATION_STRUCTURE             = 0b100000000;
    const ACCELERATION_STRUCTURE_BUILD       = 0b1000000000;
    const SHADER_BINDING_TABLE               = 0b10000000000;
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueueSharingMode {
  Exclusive,
  Concurrent
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BufferInfo {
  pub size: u64,
  pub usage: BufferUsage,
  pub sharing_mode: QueueSharingMode
}


pub trait Buffer : Hash + PartialEq + Eq + Send + Sync {
  fn info(&self) -> &BufferInfo;

  unsafe fn map_unsafe(&self, offset: u64, length: u64, invalidate: bool) -> Option<*mut c_void>;
  unsafe fn unmap_unsafe(&self, offset: u64, length: u64, flush: bool);
}
