use std::sync::atomic::{AtomicBool, Ordering};
use std::error::Error;
use std::sync::{Arc, Mutex};
use job::{Scheduler, Job};
use std::thread;
use std::thread::{JoinHandle, Thread};
use std::collections::{HashMap, HashSet};
use std::borrow::BorrowMut;

pub trait JobThreadContext {}

pub enum JobThreadStatus {
  PREPARING(HashMap<String, Box<JobThreadContext + Send>>),
  RUNNING(JoinHandle<()>),
  UNKNWON
}


pub struct JobThread {
  status: JobThreadStatus,
  is_busy: Arc<AtomicBool>
}

impl JobThread {
  pub fn new() -> JobThread {
    return JobThread {
      status: JobThreadStatus::PREPARING(HashMap::new()),
      is_busy: Arc::new(AtomicBool::new(false))
    };
  }

  pub fn run(self: &mut JobThread, scheduler: Arc<Mutex<Scheduler>>) -> Result<(), String> {
    let old_status = std::mem::replace(&mut self.status, JobThreadStatus::UNKNWON);
    return match old_status {
      JobThreadStatus::RUNNING(_) => return Err("Thread is running already".to_string()),
      JobThreadStatus::PREPARING(contexts) => {
        let is_busy_clone = self.is_busy.clone();
        let handle = thread::spawn(move || JobThread::thread_func(scheduler, contexts, is_busy_clone));
        self.status = JobThreadStatus::RUNNING(handle);
        Ok(())
      },
      JobThreadStatus::UNKNWON => unreachable!()
    }
  }

  fn thread_func(scheduler: Arc<Mutex<Scheduler>>, mut contexts: HashMap<String, Box<JobThreadContext + Send>>, mut is_busy: Arc<AtomicBool>) {
    let mut context_keys: HashSet<String> = HashSet::new();
    for key in contexts.keys() {
      context_keys.insert((*key).clone());
    }

    loop {
      let job_res = {
        let mut scheduler_guard = scheduler.lock().unwrap();
        scheduler_guard.get_work(&context_keys)
      };

      if let Some(mut job) = job_res {
        is_busy.store(true, Ordering::Relaxed);
        let requested_context = contexts.get_mut(job.requested_context_key()).unwrap();
        let requested_context_ref: &mut (JobThreadContext + Send + 'static) = requested_context.borrow_mut();
        job.run(requested_context_ref);
      } else {
        is_busy.store(false, Ordering::Relaxed);
      }
    }
  }

  pub fn add_arg(&mut self, key: String, context: Box<JobThreadContext + Send>) -> Result<bool, String> {
    return match self.status {
      JobThreadStatus::PREPARING(ref mut contexts) => Ok({
          if contexts.contains_key(&key) {
            false
          } else {
            contexts.insert(key, context);
            true
          }
      }),
      _ => Err("Wrong state".to_string())
    }
  }
}