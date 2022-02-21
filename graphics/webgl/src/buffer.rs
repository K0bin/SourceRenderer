use std::{cell::UnsafeCell, sync::Mutex};

use js_sys::{ArrayBuffer, WebAssembly::Memory, SharedArrayBuffer, Uint8Array};
use log::trace;
use sourcerenderer_core::graphics::{Buffer, BufferInfo, BufferUsage, MappedBuffer, MemoryUsage, MutMappedBuffer};

use wasm_bindgen::JsCast;
use web_sys::{WebGlRenderingContext, WebGl2RenderingContext};

use crate::{GLThreadSender, thread::BufferHandle, messages::{send_message, WebGLCreateBufferCommand, WebGLDestroyBufferCommand, WebGLSetBufferDataCommand}};

pub struct WebGLBuffer {
  handle: crate::thread::BufferHandle,
  info: BufferInfo,
  mapped_data: Mutex<Option<UnsafeCell<Box<[u8]>>>>
}

impl WebGLBuffer {
  pub fn new(handle: BufferHandle, info: &BufferInfo, _memory_usage: MemoryUsage) -> Self {
    send_message(&WebGLCreateBufferCommand::new(handle as u32, info.size as u32));

    Self {
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
    send_message(&WebGLDestroyBufferCommand::new(self.handle as u32));
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
    *mapped_data = Some(UnsafeCell::new(vec![0; self.length()].into_boxed_slice()));
    Some(mapped_data.as_mut().unwrap().get_mut().as_mut_ptr())
  }

  unsafe fn unmap_unsafe(&self, _flush: bool) {
    let mut mapped_data = self.mapped_data.lock().unwrap();
    let data = mapped_data.take().unwrap();
    let handle = self.handle;
    let data = &*(data.get());
    let array_buffer = ArrayBuffer::new(data.len() as u32);
    let u8_array = Uint8Array::new(&array_buffer);
    u8_array.copy_from(&data[..]);
    send_message(&WebGLSetBufferDataCommand::new(handle as u32, array_buffer.dyn_ref().unwrap()));

    /*
    let usage = self.info.usage;
    let expected_size = self.info.size;
    self.sender.send(Box::new(move |device| {
      let buffer = device.buffer(handle).clone();
      let target = buffer_usage_to_target(usage);
      device.bind_buffer(target, Some(buffer.gl_buffer()));
      device.buffer_data_with_u8_array(target, &data[..], buffer.gl_usage());
      #[cfg(debug_assertions)]
      {
        let size = device.get_buffer_parameter(target, WebGl2RenderingContext::BUFFER_SIZE).as_f64().unwrap() as u32;
        assert_eq!(size, expected_size as u32);
      }
    }));*/
  }

  fn length(&self) -> usize {
    self.info.size
  }

  fn info(&self) -> &BufferInfo {
    &self.info
  }
}

pub(crate) fn buffer_usage_to_target(usage: BufferUsage) -> u32 {
  if usage.contains(BufferUsage::INDEX) {
    // Index buffers must take priority!
    WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER
  } else if usage.contains(BufferUsage::VERTEX) {
    WebGl2RenderingContext::ARRAY_BUFFER
  } else if usage.contains(BufferUsage::COPY_SRC) {
    WebGl2RenderingContext::PIXEL_UNPACK_BUFFER
  } else if usage.contains(BufferUsage::COPY_DST) {
    WebGl2RenderingContext::PIXEL_PACK_BUFFER
  } else if usage.intersects(BufferUsage::CONSTANT) {
    WebGl2RenderingContext::UNIFORM_BUFFER
  } else {
    panic!("Can not determine buffer target {:?}", usage)
  }
}
