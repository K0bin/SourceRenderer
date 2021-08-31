use std::{ops::Deref, panic};

use wasm_bindgen::JsCast;

use web_sys::{WebGl2RenderingContext, WebglCompressedTextureS3tc};

use crate::WebGLSurface;

pub struct RawWebGLContext {
  context: WebGl2RenderingContext,
  extensions: WebGLExtensions
}

impl PartialEq for RawWebGLContext {
  fn eq(&self, other: &Self) -> bool {
    self.context == other.context
  }
}

impl Eq for RawWebGLContext {}

pub struct WebGLExtensions {
  pub compressed_textures: Option<WebglCompressedTextureS3tc>
}

impl RawWebGLContext {
  pub fn new(surface: &WebGLSurface) -> Self {
    let context_obj = surface.canvas().get_context("webgl2").unwrap();
    match context_obj {
      Some(context_obj) => {
        let webgl2_context = context_obj.dyn_into::<WebGl2RenderingContext>().unwrap();
        Self {
          context: webgl2_context,
          extensions: WebGLExtensions {
            compressed_textures: None
          }
        }
      }
      None => panic!("SourceRenderer Web needs WebGL2")
    }
  }

  pub fn extensions(&self) -> &WebGLExtensions {
    &self.extensions
  }
}

impl Deref for RawWebGLContext {
  type Target = WebGl2RenderingContext;

  fn deref(&self) -> &Self::Target {
    &self.context
  }
}
