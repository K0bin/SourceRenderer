use crate::graphics::Backend;
use std::marker::PhantomData;

bitflags! {
  pub struct BufferUsage: u32 {
    const VERTEX        = 0b1;
    const INDEX         = 0b10;
    const CONSTANT      = 0b100;
    const STORAGE       = 0b1000;
    const INDIRECT      = 0b10000;
    const UNIFORM_TEXEL = 0b100000;
    const STORAGE_TEXEL = 0b1000000;
    const COPY_SRC      = 0b1000000000000000000;
    const COPY_DST      = 0b10000000000000000000;
  }
}

pub trait Buffer {
  fn map<T>(&self) -> Option<MappedBuffer<Self, T>>
    where Self: Sized, T: 'static + Send + Sync + Sized + Clone;
  unsafe fn map_unsafe(&self) -> Option<*mut u8>;
  unsafe fn unmap_unsafe(&self);

  fn get_length(&self) -> usize;
}

pub struct MappedBuffer<'a, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  buffer: &'a B,
  data: &'a mut T,
  phantom: PhantomData<*const u8>
}

impl<'a, B, T> MappedBuffer<'a, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  pub fn new(buffer: &'a B) -> Option<Self> {
    unsafe { buffer.map_unsafe() }.map(move |ptr|
      Self {
        buffer,
        data: unsafe { (ptr as *mut T).as_mut().unwrap() },
        phantom: PhantomData
      }
    )
  }

  pub fn data(&mut self) -> &mut T {
    return self.data;
  }
}

impl<B, T> Drop for MappedBuffer<'_, B, T>
  where B: Buffer, T: 'static + Send + Sync + Sized + Clone {
  fn drop(&mut self) {
    unsafe { self.buffer.unmap_unsafe(); }
  }
}
