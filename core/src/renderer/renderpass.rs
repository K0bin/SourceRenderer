use std::collections::HashSet;

pub enum RenderPassOperation {

}

pub struct RenderPassDescription {
  inputs: HashSet<String>,
  outputs: HashSet<String>,
  operation: RenderPassOperation
}

impl RenderPassDescription {
  pub fn add_attachment_input(&mut self, name: &str) {
    self.inputs.insert(name.to_string());
  }

  pub fn add_attachment_output(&mut self, name: &str) {
    self.inputs.insert(name.to_string());
  }
}
