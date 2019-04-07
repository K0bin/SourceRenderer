use job::jobthread::JobThreadContext;

pub struct Job {
  req_context_key: String,
  work: fn(&mut JobThreadContext) -> ()
}

impl Job {
  pub fn run(&self, context: &mut (dyn JobThreadContext + Send)) {
    (self.work)(context);
  }

  pub fn requested_context_key(&self) -> &str {
    return &self.req_context_key;
  }
}
