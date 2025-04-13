use std::{future::{poll_fn, Future}, sync::{atomic::{AtomicU32, Ordering}, Mutex}, task::{Poll, Waker}};

pub struct AsyncCounter {
    counter: AtomicU32,
    wakers: Mutex<Vec<Waker>>,
    max_value_for_waking: u32
}
impl AsyncCounter {
    pub fn new(max_value_for_waking: u32) -> Self {
        Self {
            counter: AtomicU32::new(0u32),
            wakers: Mutex::new(Vec::new()),
            max_value_for_waking
        }
    }

    pub fn increment(&self) -> u32 {
        self.counter.fetch_add(1, Ordering::Acquire) + 1
    }

    pub fn decrement(&self) -> u32 {
        let mut count = self.counter.fetch_sub(1, Ordering::Release) - 1;
        while count <= self.max_value_for_waking {
            let waker = {
                let mut guard = self.wakers.lock().unwrap();
                guard.pop()
            };
            if let Some(waker) = waker {
                waker.wake();
            } else {
                break;
            }
            count = self.counter.load(Ordering::Relaxed);
        }
        count
    }

    pub fn set(&self, value: u32) -> u32 {
        self.counter.store(value, Ordering::Release);
        let mut count = value;
        while count <= self.max_value_for_waking {
            let waker = {
                let mut guard = self.wakers.lock().unwrap();
                guard.pop()
            };
            if let Some(waker) = waker {
                waker.wake();
            } else {
                break;
            }
            count = self.counter.load(Ordering::Relaxed);
        }
        count
    }

    #[allow(unused)]
    pub fn load(&self) -> u32 {
        self.counter.load(Ordering::Relaxed)
    }

    pub fn wait_for_zero<'a>(&'a self) -> impl Future<Output = ()> + 'a {
        self.wait_for_value(0)
    }

    pub fn wait_for_value<'a>(&'a self, value: u32) -> impl Future<Output = ()> + 'a {
        assert!(value <= self.max_value_for_waking);
        poll_fn(move |ctx| {
            let mut pending_count = self.counter.load(Ordering::Acquire);
            if pending_count <= value {
                Poll::Ready(())
            } else {
                let mut guard = self.wakers.lock().unwrap();
                pending_count = self.counter.load(Ordering::Relaxed);
                if pending_count <= value {
                    return Poll::Ready(());
                }
                guard.push(ctx.waker().clone());

                Poll::Pending
            }
        })
    }
}