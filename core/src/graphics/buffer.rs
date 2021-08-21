use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use super::MemoryUsage;

bitflags! {
  pub struct BufferUsage: u32 {
    const VERTEX                             = 0b1;
    const INDEX                              = 0b10;
    const FRAGMENT_SHADER_STORAGE_READ       = 0b100;
    const VERTEX_SHADER_STORAGE_READ         = 0b1000;
    const COMPUTE_SHADER_STORAGE_READ        = 0b10000;
    const FRAGMENT_SHADER_STORAGE_WRITE      = 0b100000;
    const VERTEX_SHADER_STORAGE_WRITE        = 0b1000000;
    const COMPUTE_SHADER_STORAGE_WRITE       = 0b10000000;
    const COPY_SRC                           = 0b100000000;
    const COPY_DST                           = 0b1000000000;
    const FRAGMENT_SHADER_CONSTANT           = 0b10000000000;
    const VERTEX_SHADER_CONSTANT             = 0b100000000000;
    const COMPUTE_SHADER_CONSTANT            = 0b1000000000000;
    const INDIRECT                           = 0b10000000000000;

    const STORAGE = Self::VERTEX_SHADER_STORAGE_READ.bits() | Self::VERTEX_SHADER_STORAGE_WRITE.bits()
     | Self::FRAGMENT_SHADER_STORAGE_READ.bits() | Self::FRAGMENT_SHADER_STORAGE_WRITE.bits()
     | Self::COMPUTE_SHADER_STORAGE_READ.bits() | Self::COMPUTE_SHADER_STORAGE_WRITE.bits();
    const STORAGE_READ = Self::VERTEX_SHADER_STORAGE_READ.bits()
    | Self::FRAGMENT_SHADER_STORAGE_READ.bits()
    | Self::COMPUTE_SHADER_STORAGE_READ.bits();
    const CONSTANT = Self::VERTEX_SHADER_CONSTANT.bits() | Self::FRAGMENT_SHADER_CONSTANT.bits() | Self::COMPUTE_SHADER_CONSTANT.bits();
    const READ = Self::VERTEX_SHADER_STORAGE_READ.bits()
    | Self::FRAGMENT_SHADER_STORAGE_READ.bits()
    | Self::COMPUTE_SHADER_STORAGE_READ.bits()
    | Self::VERTEX_SHADER_CONSTANT.bits()
    | Self::FRAGMENT_SHADER_CONSTANT.bits()
    | Self::COMPUTE_SHADER_CONSTANT.bits()
    | Self::COPY_SRC.bits()
    | Self::VERTEX.bits()
    | Self::INDEX.bits()
    | Self::INDIRECT.bits();
  }
}

pub fn get_default_state(memory_usage: MemoryUsage) -> BufferUsage {
  match memory_usage {
    MemoryUsage::CpuOnly | MemoryUsage::CpuToGpu => BufferUsage::READ,
    MemoryUsage::GpuToCpu => BufferUsage::COPY_DST,
    MemoryUsage::GpuOnly => BufferUsage::empty()
  }
}

#[derive(Debug, Clone)]
pub struct BufferInfo {
  pub size: usize,
  pub usage: BufferUsage
}


pub trait Buffer {
  fn map_mut<T>(&self) -> Option<MutMappedBuffer<Self, T>>
    where Self: Sized, T: 'static + Send + Sync + Sized + Clone;
  fn map<T>(&self) -> Option<MappedBuffer<Self, T>>
    where Self: Sized, T: 'static + Send + Sync + Sized + Clone;

  unsafe fn map_unsafe(&self, invalidate: bool) -> Option<*mut u8>;
  unsafe fn unmap_unsafe(&self, flush: bool);

  fn get_length(&self) -> usize;

  fn get_info(&self) -> &BufferInfo;
}

pub struct MutMappedBuffer<'a, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  buffer: &'a B,
  data: &'a mut T,
  phantom: PhantomData<*const u8>
}

impl<'a, B, T> MutMappedBuffer<'a, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  pub fn new(buffer: &'a B, invalidate: bool) -> Option<Self> {
    unsafe { buffer.map_unsafe(invalidate) }.map(move |ptr|
      Self {
        buffer,
        data: unsafe { (ptr as *mut T).as_mut().unwrap() },
        phantom: PhantomData
      }
    )
  }
}

impl<B, T> Drop for MutMappedBuffer<'_, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  fn drop(&mut self) {
    unsafe { self.buffer.unmap_unsafe(true); }
  }
}

impl<B, T> Deref for MutMappedBuffer<'_, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    self.data
  }
}

impl<B, T> DerefMut for MutMappedBuffer<'_, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.data
  }
}

pub struct MappedBuffer<'a, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  buffer: &'a B,
  data: &'a T,
  phantom: PhantomData<*const u8>
}

impl<'a, B, T> MappedBuffer<'a, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  pub fn new(buffer: &'a B, invalidate: bool) -> Option<Self> {
    unsafe { buffer.map_unsafe(invalidate) }.map(move |ptr|
      Self {
        buffer,
        data: unsafe { (ptr as *const T).as_ref().unwrap() },
        phantom: PhantomData
      }
    )
  }
}

impl<B, T> Drop for MappedBuffer<'_, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  fn drop(&mut self) {
    unsafe { self.buffer.unmap_unsafe(false); }
  }
}

impl<B, T> Deref for MappedBuffer<'_, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    self.data
  }
}
