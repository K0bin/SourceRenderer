use std::{error::Error, sync::Arc};

use sourcerenderer_core::{Platform, platform::InputState};
use sourcerenderer_webgl::{WebGLBackend, WebGLInstance};
use web_sys::HtmlCanvasElement;

use crate::{io::WebIO, window::WebWindow};

pub struct WebPlatform {
  window: WebWindow
}

impl WebPlatform {
  pub(crate) fn new(canvas: &HtmlCanvasElement) -> Self {
    Self {
      window: WebWindow::new(canvas)
    }
  }
}

impl Platform for WebPlatform {
  type GraphicsBackend = WebGLBackend;
  type Window = WebWindow;
  type IO = WebIO;

  fn window(&self) -> &Self::Window {
    &self.window
  }

  fn create_graphics(&self, _debug_layers: bool) -> Result<Arc<WebGLInstance>, Box<dyn Error>> {
    Ok(Arc::new(WebGLInstance::new()))
  }

  fn input_state(&self) -> InputState {
    todo!()
  }
}
