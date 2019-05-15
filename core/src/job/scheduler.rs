use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};
use job::Job;
use job::jobthread::{JobThread, JobThreadContext};

pub struct Scheduler {
  jobs: Vec<Arc<Job>>,
  threads: Vec<JobThread>
}

pub trait Run {
  fn run(&mut self);
}

impl Scheduler {
  pub fn new(thread_count: usize) -> Arc<Mutex<Scheduler>> {
    let final_thread_count = if thread_count > 0 { thread_count } else { num_cpus::get() };
    let mut threads: Vec<JobThread> = vec![];
    for _ in 0..final_thread_count {
      threads.push(JobThread::new());
    }

    let scheduler = Scheduler {
      jobs: Vec::new(),
      threads: threads
    };
    return Arc::new(Mutex::new(scheduler));
  }

  pub fn get_work(&mut self, contexts: &HashSet<String>) -> Option<Arc<Job>> {
    let job_index: Option<usize> = self.jobs
      .iter()
      .enumerate()
      .find(|(_, job)| {
        contexts.contains(job.requested_context_key()) && job.is_available()
      })
      .map(|(index, _)| index);

    return job_index.map(|index| self.jobs.remove(index));
  }

  pub fn add_work(&mut self, job: Job) {
    self.jobs.push(Arc::new(job));
  }

  pub fn add_context(&mut self, key: String, context: Box<dyn JobThreadContext + Send>) {
    self.threads.sort_by(|t1, t2| if t1.job_threads_count().unwrap() < t2.job_threads_count().unwrap() { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater } );
    let thread = self.threads.first_mut().unwrap();
    thread.add_context(key, context);
  }
}

impl Run for Arc<Mutex<Scheduler>> {
  fn run(&mut self) {
    for thread in &mut self.lock().unwrap().threads {
      thread.run(self.clone());
    }
  }
}
