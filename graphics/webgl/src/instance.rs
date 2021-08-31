use std::{sync::Arc, vec};

use sourcerenderer_core::graphics::{Adapter, AdapterType, Instance};

use web_sys::{WebGl2RenderingContext as WebGLContext};

use crate::{WebGLBackend, WebGLDevice, WebGLSurface};
pub struct WebGLInstance {
  adapters: Vec<Arc<WebGLAdapter>>
}

impl WebGLInstance {
  pub fn new() -> Self {
    Self {
      adapters: vec![Arc::new(WebGLAdapter {})]
    }
  }
}

impl Instance<WebGLBackend> for WebGLInstance {
  fn list_adapters(self: std::sync::Arc<Self>) -> Vec<Arc<WebGLAdapter>> {
    self.adapters.clone()
  }
}

pub struct WebGLAdapter {

}

impl Adapter<WebGLBackend> for WebGLAdapter {
  fn adapter_type(&self) -> AdapterType {
    AdapterType::Other
  }

  fn create_device(&self, surface: &WebGLSurface) -> WebGLDevice {
    WebGLDevice::new(surface)
  }
}