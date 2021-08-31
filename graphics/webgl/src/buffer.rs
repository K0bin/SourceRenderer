use std::{cell::{Ref, RefCell}, rc::Rc};

use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, MappedBuffer, MemoryUsage, MutMappedBuffer};

use web_sys::{WebGlBuffer as WebGLBufferHandle, WebGl2RenderingContext as WebGLContext};

use crate::RawWebGLContext;

pub struct WebGLBuffer {
  context: Rc<RawWebGLContext>,
  buffer: Option<WebGLBufferHandle>,
  info: BufferInfo,
  length: usize,
  mapped_data: RefCell<Option<Box<[u8]>>>,
  gl_usage: u32,
  keep_data: bool
}

unsafe impl Send for WebGLBuffer {}
unsafe impl Sync for WebGLBuffer {}

impl WebGLBuffer {
  pub fn new(
    context: &Rc<RawWebGLContext>,
    info: &BufferInfo,
    _memory_usage: MemoryUsage,
  ) -> Self {
    let buffer_usage = info.usage;
    let mut usage = WebGLContext::STATIC_DRAW;
    if buffer_usage.intersects(BufferUsage::COPY_DST) {
      if buffer_usage.intersects(BufferUsage::CONSTANT) {
        usage = WebGLContext::STREAM_READ;
      } else {
        usage = WebGLContext::STATIC_READ;
      }
    }
    if buffer_usage.intersects(BufferUsage::COPY_SRC) {
      if buffer_usage.intersects(BufferUsage::CONSTANT) {
        usage = WebGLContext::STREAM_COPY;
      } else {
        usage = WebGLContext::STATIC_COPY;
      }
    }
    let buffer = if buffer_usage == BufferUsage::COPY_SRC {
      Some(context.create_buffer().unwrap())
    } else {
      None
    };
    Self {
      context: context.clone(),
      length: info.size,
      info: info.clone(),
      gl_usage: usage,
      mapped_data: RefCell::new(None),
      buffer,
      keep_data: buffer_usage.intersects(BufferUsage::COPY_SRC)
    }
  }

  pub fn gl_buffer(&self) -> Option<&WebGLBufferHandle> {
    self.buffer.as_ref()
  }

  pub fn data(&self) -> Ref<Option<Box<[u8]>>> {
    self.mapped_data.borrow()
  }
}

impl Drop for WebGLBuffer {
  fn drop(&mut self) {
    if let Some(buffer) = &self.buffer {
      self.context.delete_buffer(Some(buffer));
    }
  }
}

impl Buffer for WebGLBuffer {
  fn map_mut<T>(&self) -> Option<sourcerenderer_core::graphics::MutMappedBuffer<Self, T>>
  where Self: Sized, T: 'static + Send + Sync + Sized + Clone {
    MutMappedBuffer::new(self, true)
  }

  fn map<T>(&self) -> Option<sourcerenderer_core::graphics::MappedBuffer<Self, T>>
  where Self: Sized, T: 'static + Send + Sync + Sized + Clone {
    MappedBuffer::new(self, true)
  }

  unsafe fn map_unsafe(&self, _invalidate: bool) -> Option<*mut u8> {
    let mut mapped_data = Vec::with_capacity(self.length);
    mapped_data.set_len(self.length);
    let mut mapped_data_mut = self.mapped_data.borrow_mut();
    *mapped_data_mut = Some(mapped_data.into_boxed_slice());
    Some(mapped_data_mut.as_mut().unwrap().as_mut_ptr())
  }

  unsafe fn unmap_unsafe(&self, _flush: bool) {
    if let Some(buffer) = &self.buffer {
      let mut mapped_data_mut = self.mapped_data.borrow_mut();
      self.context.bind_buffer(WebGLContext::ARRAY_BUFFER, Some(buffer));
      self.context.buffer_data_with_u8_array(WebGLContext::ARRAY_BUFFER, &mapped_data_mut.as_ref().unwrap()[..], self.gl_usage);
      if self.keep_data {
        *mapped_data_mut = None;
      }
    }
  }

  fn get_length(&self) -> usize {
    self.length
  }

  fn get_info(&self) -> &BufferInfo {
    &self.info
  }
}
