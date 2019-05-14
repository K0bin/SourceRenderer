use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use job::jobthread::JobThreadContext;
use job::Scheduler;

pub type WorkFn = fn(&mut JobThreadContext) -> Option<Vec<Job>>;

pub struct Job {
  scheduler: Arc<Mutex<Scheduler>>,
  req_context_key: String,
  work: WorkFn,
  dependency: Option<Arc<Job>>,
  parent: Option<Arc<Job>>,
  children: AtomicUsize,
  children_done: AtomicUsize,
  is_done: AtomicBool,
  is_running: AtomicBool
}

impl Job {
  fn create_job(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn, dependency: Option<Arc<Job>>, parent: Option<Arc<Job>>) -> Job {
    return Job {
      scheduler: scheduler,
      req_context_key: requested_context_key,
      work: work,
      dependency: dependency,
      parent: parent,
      children: AtomicUsize::new(0),
      children_done: AtomicUsize::new(0),
      is_done: AtomicBool::new(false),
      is_running: AtomicBool::new(false)
    };
  }

  pub fn new(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn) -> Job {
    return Job::create_job(scheduler, requested_context_key, work, None, None);
  }

  pub fn new_child(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn, parent: Option<Arc<Job>>) -> Job {
    return Job::create_job(scheduler, requested_context_key, work, None, parent);
  }

  pub fn new_dependent(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn, dependency: Option<Arc<Job>>) -> Job {
    return Job::create_job(scheduler, requested_context_key, work, dependency, None);
  }

  pub fn child_done(&self) {
    if !self.is_running.load(Ordering::SeqCst) {
      panic!("Job is not running");
    }
    let children_done_before = self.children_done.fetch_add(1, Ordering::SeqCst);
    if children_done_before == self.children.load(Ordering::SeqCst) - 1 {
      self.is_done.store(true, Ordering::SeqCst);
    }
  }

  pub fn requested_context_key(&self) -> &str {
    return &self.req_context_key;
  }

  pub fn is_available(&self) -> bool {
    let dependency_satisfied = self.dependency
      .as_ref()
      .map_or(true, |ref d|
        d
        .as_ref()
        .is_done
        .load(Ordering::SeqCst)
      );
    return !self.is_done.load(Ordering::SeqCst)
      && dependency_satisfied;
  }

  pub fn run(self: Arc<Job>, context: &mut (dyn JobThreadContext + Send)) {
    if self.is_done.load(Ordering::SeqCst) {
      panic!("Job is already done");
    }
    if self.is_running.swap(true, Ordering::SeqCst) {
      panic!("Job is already running");
    }
    let spawned_jobs_opt = (self.work)(context);
    let mut has_spawned_jobs = false;
    if let Some(spawned_jobs) = spawned_jobs_opt {
      self.children.store(spawned_jobs.len(), Ordering::SeqCst);
      let mut scheduler_guard = self.scheduler.lock().unwrap();
      for mut job in spawned_jobs {
        has_spawned_jobs = true;
        job.parent = Some(self.clone());
        scheduler_guard.add_work(job);
      }
    }
    self.is_running.store(false, Ordering::SeqCst);
    if !has_spawned_jobs {
      self.is_done.store(true, Ordering::SeqCst);
    }
    if let Some(ref parent) = self.parent {
      parent.child_done();
    }
    self.is_done.store(true, Ordering::SeqCst);
  }
}
