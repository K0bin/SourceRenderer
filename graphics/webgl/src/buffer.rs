use std::{cell::UnsafeCell, sync::Mutex};

use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, MappedBuffer, MemoryUsage, MutMappedBuffer};

use web_sys::{WebGlRenderingContext, WebGl2RenderingContext};

use crate::{GLThreadSender, thread::BufferHandle};

pub struct WebGLBuffer {
  handle: crate::thread::BufferHandle,
  sender: GLThreadSender,
  info: BufferInfo,
  mapped_data: Mutex<Option<UnsafeCell<Box<[u8]>>>>
}

impl WebGLBuffer {
  pub fn new(handle: BufferHandle, info: &BufferInfo, memory_usage: MemoryUsage, sender: &GLThreadSender) -> Self {
    let c_info = info.clone();
    sender.send(Box::new(move |device| {
      device.create_buffer(handle, &c_info, memory_usage, None);
    })).unwrap();

    Self {
      sender: sender.clone(),
      handle,
      info: info.clone(),
      mapped_data: Mutex::new(None)
    }
  }

  pub fn handle(&self) -> crate::thread::BufferHandle {
    self.handle
  }
}

impl Drop for WebGLBuffer {
  fn drop(&mut self) {
    let handle = self.handle;
    self.sender.send(Box::new(move |device| device.remove_buffer(handle))).unwrap();
  }
}

impl Buffer for WebGLBuffer {
  fn map_mut<T>(&self) -> Option<MutMappedBuffer<Self, T>>
  where Self: Sized, T: 'static + Send + Sync + Sized + Clone {
    MutMappedBuffer::new(self, true)
  }

  fn map<T>(&self) -> Option<MappedBuffer<Self, T>>
  where Self: Sized, T: 'static + Send + Sync + Sized + Clone {
    MappedBuffer::new(self, true)
  }

  unsafe fn map_unsafe(&self, _invalidate: bool) -> Option<*mut u8> {
    let mut mapped_data = self.mapped_data.lock().unwrap();
    *mapped_data = Some(UnsafeCell::new(vec![0; self.get_length()].into_boxed_slice()));
    Some(mapped_data.as_mut().unwrap().get_mut().as_mut_ptr())
  }

  unsafe fn unmap_unsafe(&self, _flush: bool) {
    let mut mapped_data = self.mapped_data.lock().unwrap();
    let data = mapped_data.take().unwrap();
    let handle = self.handle;
    let usage = self.info.usage;
    self.sender.send(Box::new(move |device| {
      let buffer = device.buffer(handle).clone();
      let target = buffer_usage_to_target(usage);
      device.bind_buffer(target, Some(buffer.gl_buffer()));
      let data = &*(data.get());
      device.buffer_data_with_u8_array(target, &data[..], buffer.gl_usage());
    })).unwrap();
  }

  fn get_length(&self) -> usize {
    self.info.size
  }

  fn get_info(&self) -> &BufferInfo {
    &self.info
  }
}

fn buffer_usage_to_target(usage: BufferUsage) -> u32 {
  if usage.contains(BufferUsage::VERTEX) {
    WebGl2RenderingContext::ARRAY_BUFFER
  } else if usage.contains(BufferUsage::INDEX) {
    WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER
  } else if usage.contains(BufferUsage::COPY_SRC) {
    WebGl2RenderingContext::PIXEL_UNPACK_BUFFER
  } else if usage.contains(BufferUsage::COPY_DST) {
    WebGl2RenderingContext::PIXEL_PACK_BUFFER
  } else if usage.intersects(BufferUsage::VERTEX_SHADER_CONSTANT | BufferUsage::FRAGMENT_SHADER_CONSTANT) {
    WebGl2RenderingContext::UNIFORM_BUFFER
  } else {
    panic!("Can not determine buffer target {:?}", usage)
  }
}
