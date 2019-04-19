use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use job::jobthread::JobThreadContext;

pub struct Job {
  req_context_key: String,
  work: fn(&mut JobThreadContext) -> (),
  dependencies: Mutex<Vec<Arc<Job>>>,
  is_done: AtomicBool,
  is_running: AtomicBool
}

impl Job {
  pub fn run(&mut self, context: &mut (dyn JobThreadContext + Send)) {
    if self.is_done.load(Ordering::Relaxed) {
      panic!("Job is already done");
    }
    if self.is_running.swap(true, Ordering::Relaxed) {
      panic!("Job is already running");
    }
    (self.work)(context);
    self.is_running.store(false, Ordering::Relaxed);
    self.is_done.store(true, Ordering::Relaxed);
  }

  pub fn requested_context_key(&self) -> &str {
    return &self.req_context_key;
  }

  pub fn is_available(&self) -> bool {
    return !self.is_done.load(Ordering::Relaxed)
      && self.dependencies.lock().unwrap()
      .iter()
      .all(|job| job.is_done.load(Ordering::Relaxed));
  }
}
