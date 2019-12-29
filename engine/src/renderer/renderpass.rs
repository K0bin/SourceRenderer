use std::collections::HashSet;

pub struct RenderPass {
  inputs: Vec<String>,
  outputs: Vec<String>,
  //record: fn(&mut RenderContext) -> ()
}

impl RenderPass {
  pub fn add_attachment_input(&mut self, name: &str) {
   // self.inputs.insert(name.to_string());
  }

  pub fn add_attachment_output(&mut self, name: &str) {
    //self.inputs.insert(name.to_string());
  }
}
