pub struct Renderer {
  is_running: bool
}

impl Renderer {
  pub fn new() -> Self {
    Self {
      is_running: true
    }
  }

  pub fn is_running(&self) -> bool {
    self.is_running
  }

  pub fn stop(&mut self) {
    self.is_running = false;
  }

  pub fn render(&mut self) {

  }
}
