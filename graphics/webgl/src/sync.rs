use sourcerenderer_core::gpu::Fence;

pub struct WebGLFence {

}

impl WebGLFence {
  pub fn new() -> Self {
    Self {}
  }
}

impl Fence for WebGLFence {
  fn is_signaled(&self) -> bool {
    true
  }

  fn await_signal(&self) {}
}

pub struct WebGLSemaphore {

}
