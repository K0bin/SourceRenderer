use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};
use std::rc::Rc;
use std::cell::RefCell;
use std::ops::{ Deref, DerefMut };
use std::fmt::{Debug, Formatter, Display};
use std::error::Error;
use graphics::Format;

pub struct Recyclable<T, R : Recycler<T>> {
  value: MaybeUninit<T>,
  pool: R
}

impl<T, R : Recycler<T>> Drop for Recyclable<T, R> {
  fn drop(&mut self) {
    let value = unsafe {
      std::mem::replace(&mut self.value, MaybeUninit::uninit()).assume_init()
    };
    self.pool.recycle(value)
  }
}

pub trait Recycler<T> {
  fn recycle(&self, item: T);
}

impl<T, R : Recycler<T>> DerefMut for Recyclable<T, R> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { &mut *(self.value.as_mut_ptr()) }
  }
}

impl<T, R : Recycler<T>> Deref for Recyclable<T, R> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    unsafe { &*(self.value.as_ptr()) }
  }
}

impl<T: Display, R : Recycler<T>> std::fmt::Display for Recyclable<T, R> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    self.value.fmt(f)
  }
}

impl<T: Debug, R : Recycler<T>> std::fmt::Debug for Recyclable<T, R> {
  fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
    self.value.fmt(f)
  }
}

pub struct Pool<T> {
  inner: Arc<Mutex<PoolInner<T>>>
}

pub struct PoolInner<T> {
  items: Vec<T>
}

impl<T> Pool<T> {
  pub fn new<F>(capacity: usize, initializer: F) -> Self
    where F: Fn() -> T {
    let mut items = Vec::with_capacity(capacity);

    for _ in 0..items.capacity() {
      items.push(initializer());
    }

    Self {
      inner: Arc::new(Mutex::new(PoolInner {
        items: items
      }))
    }
  }

  pub fn get(&mut self) -> Option<Recyclable<T, Arc<Mutex<PoolInner<T>>>>> {
    let item = {
      let mut inner_guard = self.inner.lock().expect("Failed to lock pool");
      inner_guard.items.pop()
    };

    return item.map(|i| Recyclable {
      value: MaybeUninit::new(i),
      pool: self.inner.clone()
    });
  }
}

impl<T> Recycler<T> for Arc<Mutex<PoolInner<T>>> {
  fn recycle(&self, item: T) {
    let mut guard = self.lock().unwrap();
    guard.items.push(item);
  }
}

impl<T, R: Recycler<T>> Recyclable<T, R> {
  pub fn new(value: T, recycler: R) -> Self {
    return Recyclable {
      value: MaybeUninit::new(value),
      pool: recycler
    }
  }
}