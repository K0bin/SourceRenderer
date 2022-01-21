use std::{sync::atomic::{AtomicU32, Ordering, AtomicU64}, ops::{Deref, DerefMut}, cell::UnsafeCell};

pub struct SpinLock<T>{
  inner: UnsafeCell<T>,
  lock: AtomicU32
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
  pub fn new(data: T) -> Self {
    Self {
      inner: UnsafeCell::new(data),
      lock: AtomicU32::new(0)
    }
  }

  pub fn lock(&self) -> SpinLockGuard<T> {
    while self.lock.compare_exchange_weak(0, 1, Ordering::SeqCst, Ordering::SeqCst).is_err() {}
    SpinLockGuard {
      lock: self
    }
  }
}

pub struct SpinLockGuard<'a, T> {
  lock: &'a SpinLock<T>
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
  fn drop(&mut self) {
    let val = self.lock.lock.swap(0, Ordering::SeqCst);
    assert_eq!(val, 1);
  }
}

impl<'a, T> Deref for SpinLockGuard<'a, T> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    unsafe { &*self.lock.inner.get() }
  }
}

impl<'a, T> DerefMut for SpinLockGuard<'a, T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { &mut *self.lock.inner.get() }
  }
}