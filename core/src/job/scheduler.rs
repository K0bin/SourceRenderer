use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use job::Job;
use job::jobthread::{JobThread, JobThreadContext};
use std::collections::VecDeque;

pub struct Scheduler {
  jobs: Vec<Job>,
  threads: Vec<JobThread>
}

impl Scheduler {
  pub fn new() -> Arc<Scheduler> {
    let mut scheduler = Scheduler {
      jobs: Vec::new(),
      threads: vec![]
    };

    let arc = Arc::new(scheduler);
    //scheduler.threads = vec![JobThread::new(arc.clone(), JobThreadContext::None)];

    return arc;
  }

  pub fn get_work(&mut self, contexts: &HashSet<String>) -> Option<Job> {
    let job_index: Option<usize> = self.jobs
      .iter()
      .enumerate()
      .find(|(_, job)| contexts.contains(job.requested_context_key()))
      .map(|(index, _)| index);

    return job_index.map(|index| self.jobs.remove(index));
  }
}
