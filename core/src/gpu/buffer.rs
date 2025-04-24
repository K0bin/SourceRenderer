use std::{ffi::c_void, hash::Hash};

use bitflags::bitflags;

bitflags! {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub struct BufferUsage: u16 {
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
    const INITIAL_COPY                       = 0b100000000000;
    const QUERY_RESOLVE                      = 0b1000000000000;

    const GPU_WRITABLE = 0b100 | 0b1000000 | 0b100000000 | 0b1000000000 | 0b1000000000000;
    const GPU_READABLE = 0b1 | 0b10 | 0b100 | 0b1000 | 0b100000 | 0b10000000 | 0b100000000 | 0b1000000000 | 0b10000000000;
  }
}

impl BufferUsage {
    pub fn gpu_writable(&self) -> bool {
        self.intersects(Self::GPU_WRITABLE)
    }
    pub fn gpu_readable(&self) -> bool {
        self.intersects(Self::GPU_READABLE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueueSharingMode {
    Exclusive,
    Concurrent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BufferInfo {
    pub size: u64,
    pub usage: BufferUsage,
    pub sharing_mode: QueueSharingMode,
}

pub trait Buffer: Hash + PartialEq + Eq {
    fn info(&self) -> &BufferInfo;

    unsafe fn map(&self, offset: u64, length: u64, invalidate: bool) -> Option<*mut c_void>;
    unsafe fn unmap(&self, offset: u64, length: u64, flush: bool);
}
