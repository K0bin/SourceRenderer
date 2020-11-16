use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use crossbeam_deque::{Steal, Worker, Injector, Stealer};
use std::time::{SystemTime, Duration};

// TODO: optimize orderings

pub type JobCounter = Arc<JobCounterInner>;
pub struct JobCounterInner {
  counter: AtomicUsize
}

impl JobCounterInner {
  pub fn load(&self) -> usize {
    self.counter.load(Ordering::SeqCst)
  }

  pub fn inc(&self) -> usize {
    self.counter.fetch_add(1, Ordering::SeqCst) + 1
  }

  pub fn set(&self, value: usize) {
    self.counter.store(value, Ordering::SeqCst);
  }
}

pub struct Job {
  work: Box<dyn FnOnce() -> () + Send>
}

impl Job {
  pub fn run(self) {
    (self.work)();
  }
}

pub struct JobScheduler {
  inner: Arc<JobSchedulerInner>
}

pub struct JobSchedulerInner {
  queue: Injector<Job>,
  stealers: Vec<Stealer<Job>>,
  base_time: SystemTime
}

impl JobScheduler {
  pub fn new_counter() -> JobCounter {
    Arc::new(JobCounterInner {
      counter: AtomicUsize::new(0)
    })
  }

  pub fn new() -> Self {
    let thread_count = num_cpus::get();
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

  pub fn spawn<F>(&self, work: F)
    where F: FnOnce() -> () + Send + 'static {
    let job = Job {
      work: Box::new(work)
    };
    self.inner.queue.push(job);
  }

  pub fn busy_wait(&self, counter: &JobCounter, value: usize) {
    while counter.load() < value {
      if let Some(job) = self.pop_job() {
        job.run();
      }
    }
  }

  pub fn pop_job(&self) -> Option<Job> {
    let job_opt: Option<Job> = None;
    for stealer in &self.inner.stealers {
      'stealing: loop {
        match stealer.steal() {
          Steal::Success(job) => {
            return Some(job);
          },
          Steal::Retry => { continue 'stealing; },
          Steal::Empty => { break 'stealing; }
        }
      }
    }

    if job_opt.is_none() {
      'queue_stealing: loop {
        match self.inner.queue.steal() {
          Steal::Success(job) => {
            return Some(job);
          },
          Steal::Retry => { continue 'queue_stealing; },
          Steal::Empty => { break 'queue_stealing; }
        }
      }
    }
    None
  }

  pub fn scope<'scope, F>(&self, f: F)
    where F: FnOnce(&mut Scope<'scope>) + 'scope + Send {
    let scheduler: &'scope JobScheduler = unsafe { std::mem::transmute(self) };
    let mut scope = Scope::<'scope> {
      scheduler,
      counter: JobScheduler::new_counter(),
      expected_value: 0
    };
    (f)(&mut scope);

    self.busy_wait(&scope.counter, scope.expected_value);
  }
}

const IDLE_SPINS_YIELD_THRESHOLD: u32 = 128;
const IDLE_SPINS_SLEEP_SHORT_THRESHOLD: u32 = 256;
const IDLE_SPINS_SLEEP_LONG_THRESHOLD: u32 = 1024;
const IDLE_SPINS_SLEEP_AGES_THRESHOLD: u32 = 16384;
fn job_thread(local: Worker<Job>, scheduler: Arc<JobSchedulerInner>) {
  let others_refs: Vec<&Stealer<Job>> = scheduler.stealers.iter().map(|other| other).collect();
  let mut idle_spins = 0;
  loop {
    let job_opt = find_job(&local, &scheduler.queue, &others_refs);
    if let Some(job) = job_opt {
      idle_spins = 0;
      job.run();
    } else {
      idle_spins += 1;
      if idle_spins > IDLE_SPINS_SLEEP_AGES_THRESHOLD {
        std::thread::sleep(Duration::new(4, 0));
      } else if idle_spins > IDLE_SPINS_SLEEP_LONG_THRESHOLD {
        std::thread::sleep(Duration::new(1, 0));
      } else if idle_spins > IDLE_SPINS_SLEEP_SHORT_THRESHOLD {
        std::thread::sleep(Duration::new(0, 250));
      } else if idle_spins > IDLE_SPINS_YIELD_THRESHOLD {
        std::thread::yield_now();
      }
    }
  }
}

fn find_job(local: &Worker<Job>, global: &Injector<Job>, others: &[&Stealer<Job>]) -> Option<Job> {
  let mut job_opt = local.pop();
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
  let _ = injector.steal_batch(worker);
  let mut job_opt = worker.pop();
  while !injector.is_empty() && job_opt.is_none() {
    let _ = injector.steal_batch(worker);
    job_opt = worker.pop();
  }
  job_opt
}

fn find_job_in_stealer(worker: &Worker<Job>, stealer: &Stealer<Job>) -> Option<Job> {
  let _ = stealer.steal_batch(worker);
  let mut job_opt = worker.pop();
  while !stealer.is_empty() && job_opt.is_none() {
    let _ = stealer.steal_batch(worker);
    job_opt = worker.pop();
  }
  job_opt
}

pub struct Scope<'a> {
  scheduler: &'a JobScheduler,
  counter: JobCounter,
  expected_value: usize
}

impl<'a> Scope<'a> {
  pub fn spawn<F>(&mut self, work: F)
    where F: FnOnce() -> () + Send + 'static {
    let c_counter = self.counter.clone();
    self.scheduler.spawn(move || {
      (work)();
      c_counter.inc();
    });
    self.expected_value += 1;
  }
}
