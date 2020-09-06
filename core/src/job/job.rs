use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use crossbeam_deque::{Steal, Worker, Injector, Stealer};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::collections::VecDeque;

// TODO: optimize orderings

pub type JobCounter = Arc<AtomicUsize>;

#[derive(Clone)]
pub struct JobCounterWait {
  pub counter: JobCounter,
  pub value: usize
}

pub struct Job {
  wait_counter: Option<JobCounterWait>,
  work: Box<FnOnce() -> () + Send>
}

impl Job {
  pub fn is_ready(&self) -> bool {
    self.wait_counter.as_ref().map_or(true, |wait_counter|
      wait_counter.counter.load(Ordering::SeqCst) == wait_counter.value
    )
  }

  pub fn run(self) {
    (self.work)();
  }
}

pub struct SystemJob {
  frequency_per_seconds: u32,
  last_iteration: Option<SystemJobIteration>,
  work: Box<FnMut(&dyn JobQueue) -> JobCounterWait + Send>
}

pub struct SystemJobIteration {
  wait_counter: JobCounterWait,
  timestamp: SystemTime
}

impl SystemJob {
  pub fn is_ready(&self, time: &SystemTime) -> bool {
    self.last_iteration.as_ref().map_or(true, |iteration| {
      iteration.wait_counter.counter.load(Ordering::SeqCst) == iteration.wait_counter.value
        && (self.frequency_per_seconds == 0 || time.duration_since(iteration.timestamp).unwrap().as_micros() as u32 >= 1_000_000 / self.frequency_per_seconds)
    })
  }

  pub fn run(&mut self, job_queue: &dyn JobQueue, time: &SystemTime) {
    if !self.is_ready(time) {
      return; // prevent two threads from executing the job at the same time
    }

    let new_wait = (self.work)(job_queue);
    std::mem::replace(&mut self.last_iteration, Some(SystemJobIteration {
      wait_counter: new_wait,
      timestamp: time.clone()
    }));
  }
}

unsafe impl Sync for SystemJob {}

pub trait JobQueue {
  fn enqueue_job(&self, work: Box<FnOnce() -> () + Send>, wait: Option<&JobCounterWait>);
}

pub struct JobScheduler {
  inner: Arc<JobSchedulerInner>
}

pub struct JobSchedulerInner {
  queue: Injector<Job>,
  stealers: Vec<Stealer<Job>>,
  system_jobs: RwLock<Vec<RwLock<SystemJob>>>,
  base_time: SystemTime
}

impl JobQueue for JobSchedulerInner {
  fn enqueue_job(&self, work: Box<FnOnce() -> () + Send>, wait: Option<&JobCounterWait>) {
    let job = Job {
      wait_counter: wait.map(|wait_ref| wait_ref.clone()),
      work
    };
    self.queue.push(job);
  }
}

impl JobScheduler {
  pub fn new_counter() -> JobCounter {
    Arc::new(AtomicUsize::new(0usize))
  }

  pub fn new() -> Self {
    let thread_count = 2;
    let global = Injector::new();
    let mut workers = Vec::new();
    let mut stealers = Vec::new();
    for _ in 0..thread_count {
      let worker = Worker::new_fifo();
      let stealer = worker.stealer();
      workers.push(worker);
      stealers.push(stealer);
    }

    let inner = Arc::new(JobSchedulerInner {
      queue: global,
      stealers,
      system_jobs: RwLock::new(Vec::new()),
      base_time: SystemTime::now()
    });

    for _ in 0..thread_count {
      let thread_worker = workers.pop().unwrap();
      let thread_self = inner.clone();
      std::thread::spawn(move || job_thread(thread_worker, thread_self));
    }

    JobScheduler {
      inner
    }
  }

  pub fn enqueue_system_job(&self, work: Box<FnMut(&dyn JobQueue) -> JobCounterWait + Send>) {
    let mut system_jobs = self.inner.system_jobs.write().unwrap();
    system_jobs.push(RwLock::new(SystemJob {
      frequency_per_seconds: 0,
      last_iteration: None,
      work
    }));
  }

  pub fn enqueue_system_job_fixed_frequency(&self, work: Box<FnMut(&dyn JobQueue) -> JobCounterWait + Send>, frequency: u32) {
    let mut system_jobs = self.inner.system_jobs.write().unwrap();
    system_jobs.push(RwLock::new(SystemJob {
      frequency_per_seconds: frequency,
      last_iteration: None,
      work
    }));
  }
}

impl JobQueue for JobScheduler {
  fn enqueue_job(&self, work: Box<FnOnce() -> () + Send>, wait: Option<&JobCounterWait>) {
    self.inner.enqueue_job(work, wait)
  }
}

const IDLE_SPINS_YIELD_THRESHOLD: u32 = 128;
fn job_thread(local: Worker<Job>, scheduler: Arc<JobSchedulerInner>) {
  let others_refs: Vec<&Stealer<Job>> = scheduler.stealers.iter().map(|other| other).collect();
  let mut idle_spins = 0;
  'worker_loop: loop {
    {
      let now = SystemTime::now();
      let sys_jobs = scheduler.system_jobs.read().unwrap();
      let job_opt = sys_jobs.iter().find(|job| job.read().unwrap().is_ready(&now));
      if let Some(job) = job_opt {
        let mut lock = job.write().unwrap();
        let res = lock.run(scheduler.as_ref(), &now);
        res
      }
    }

    let job_opt = find_job(&local, &scheduler.queue, &others_refs);
    if let Some(job) = job_opt {
      idle_spins = 0;
      job.run();
    } else {
      idle_spins += 1;
      if idle_spins > IDLE_SPINS_YIELD_THRESHOLD {
        std::thread::yield_now();
      }
    }
  }
}

fn find_job(local: &Worker<Job>, global: &Injector<Job>, others: &[&Stealer<Job>]) -> Option<Job> {
  let mut job_opt = find_job_in_worker(local);
  if job_opt.is_some() {
    return job_opt;
  }

  job_opt = find_job_in_injector(local, global);
  if job_opt.is_some() {
    return job_opt;
  }

  for stealer in others {
    job_opt = find_job_in_stealer(local, stealer);
    if job_opt.is_some() {
      return job_opt;
    }
  }

  None
}

fn find_job_in_injector(worker: &Worker<Job>, injector: &Injector<Job>) -> Option<Job> {
  injector.steal_batch(worker);
  let mut job_opt = find_job_in_worker(worker);
  while !injector.is_empty() && (job_opt.is_none() || !job_opt.as_ref().unwrap().is_ready()) {
    injector.steal_batch(worker);
    job_opt = find_job_in_worker(worker);
  }
  job_opt
}

fn find_job_in_stealer(worker: &Worker<Job>, stealer: &Stealer<Job>) -> Option<Job> {
  stealer.steal_batch(worker);
  let mut job_opt = find_job_in_worker(worker);
  while !stealer.is_empty() && (job_opt.is_none() || !job_opt.as_ref().unwrap().is_ready()) {
    stealer.steal_batch(worker);
    job_opt = find_job_in_worker(worker);
  }
  job_opt
}

fn find_job_in_worker(worker: &Worker<Job>) -> Option<Job> {
  let mut job_opt = worker.pop();

  let mut temp_jobs: [Option<Job>; 16] = Default::default();
  let mut i = 0;
  while i < temp_jobs.len() && job_opt.is_some() && !job_opt.as_ref().unwrap().is_ready() {
    temp_jobs[i] = Some(job_opt.unwrap());
    job_opt = worker.pop();
    i += 1;
  }

  for job_opt in &mut temp_jobs {
    if let Some(job) = std::mem::replace(job_opt, None) {
      worker.push(job);
    }
  }

  job_opt
}
