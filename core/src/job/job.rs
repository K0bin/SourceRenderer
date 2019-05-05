use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use job::jobthread::JobThreadContext;
use job::Scheduler;

pub type WorkFn = fn(&mut JobThreadContext) -> Option<Vec<Job>>;

pub struct Job {
  scheduler: Arc<Mutex<Scheduler>>,
  req_context_key: String,
  work: WorkFn,
  dependency: Option<Arc<RwLock<Job>>>,
  parent: Option<Arc<RwLock<Job>>>,
  children: u32,
  children_done: AtomicU32,
  is_done: AtomicBool,
  is_running: AtomicBool
}

pub trait JobExecution {
  fn run(&mut self, context: &mut (dyn JobThreadContext + Send));
}

impl Job {
  fn create_job(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn, dependency: Option<Arc<RwLock<Job>>>, parent: Option<Arc<RwLock<Job>>>) -> Job {
    return Job {
      scheduler: scheduler,
      req_context_key: requested_context_key,
      work: work,
      dependency: dependency,
      parent: parent,
      children: 0,
      children_done: AtomicU32::new(0),
      is_done: AtomicBool::new(false),
      is_running: AtomicBool::new(false)
    };
  }

  pub fn new(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn) -> Job {
    return Job::create_job(scheduler, requested_context_key, work, None, None);
  }

  pub fn new_child(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn, parent: Option<Arc<RwLock<Job>>>) -> Job {
    return Job::create_job(scheduler, requested_context_key, work, None, parent);
  }

  pub fn new_dependent(scheduler: Arc<Mutex<Scheduler>>, requested_context_key: String, work: WorkFn, dependency: Option<Arc<RwLock<Job>>>) -> Job {
    return Job::create_job(scheduler, requested_context_key, work, dependency, None);
  }

  pub fn child_done(&mut self) {
    if !self.is_running.load(Ordering::SeqCst) {
      panic!("Job is not running");
    }
    self.children_done.fetch_add(1, Ordering::SeqCst);
  }

  pub fn requested_context_key(&self) -> &str {
    return &self.req_context_key;
  }

  pub fn is_available(&self) -> bool {
    let dependency_satisfied = self.dependency
      .as_ref()
      .map_or(true, |ref d|
        d.read()
        .unwrap()
        .is_done
        .load(Ordering::SeqCst)
      );
    return !self.is_done.load(Ordering::SeqCst)
      && dependency_satisfied;
  }
}

impl JobExecution for Arc<RwLock<Job>> {
  fn run(&mut self, context: &mut (dyn JobThreadContext + Send)) {
    let mut self_guard = self.write().unwrap();
    if self_guard.is_done.load(Ordering::SeqCst) {
      panic!("Job is already done");
    }
    if self_guard.is_running.swap(true, Ordering::SeqCst) {
      panic!("Job is already running");
    }
    let spawned_jobs_opt = (self_guard.work)(context);
    if let Some(spawned_jobs) = spawned_jobs_opt {
      self_guard.children += spawned_jobs.len() as u32;
      let mut scheduler_guard = self_guard.scheduler.lock().unwrap();
      for mut job in spawned_jobs {
        job.parent = Some(self.clone());
        scheduler_guard.add_work(job);
      }
    }
    self_guard.is_running.store(false, Ordering::SeqCst);
    if let Some(ref mut parent) = self_guard.parent {
      parent.write().unwrap().child_done();
    }
    self_guard.is_done.store(true, Ordering::SeqCst);
  }
}
