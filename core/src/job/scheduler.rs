use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use job::Job;
use job::jobthread::{JobThread, JobThreadContext};

pub struct Scheduler {
  jobs: Vec<Job>,
  threads: Vec<JobThread>
}

pub trait Run {
  fn run(&mut self);
}

impl Scheduler {
  pub fn new() -> Arc<Mutex<Scheduler>> {
    let mut scheduler = Scheduler {
      jobs: Vec::new(),
      threads: vec![]
    };

    scheduler.threads = vec![JobThread::new()];
    return Arc::new(Mutex::new(scheduler));
  }

  pub fn get_work(&mut self, contexts: &HashSet<String>) -> Option<Job> {
    let job_index: Option<usize> = self.jobs
      .iter()
      .enumerate()
      .find(|(_, job)| contexts.contains(job.requested_context_key()) && job.is_available())
      .map(|(index, _)| index);

    return job_index.map(|index| self.jobs.remove(index));
  }

  pub fn add_work(&mut self, job: Job) {
    self.jobs.push(job);
  }
}

impl Run for Arc<Mutex<Scheduler>> {
  fn run(&mut self) {
    for thread in &mut self.lock().unwrap().threads {
      thread.run(self.clone());
    }
  }
}
