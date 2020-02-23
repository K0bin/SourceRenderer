use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};
use std::rc::Rc;
use std::cell::RefCell;
use std::ops::{ Deref, DerefMut };
use std::fmt::{Debug, Formatter, Display};
use std::error::Error;
use graphics::Format;
use std::sync::mpsc::{ Sender, Receiver, channel };

pub struct Recyclable<T> {
  item: MaybeUninit<T>,
  sender: Sender<T>
}

impl<T> Drop for Recyclable<T> {
  fn drop(&mut self) {
    let item = unsafe {
      std::mem::replace(&mut self.item, MaybeUninit::uninit()).assume_init()
    };
    self.sender.send(item);
  }
}

impl<T> DerefMut for Recyclable<T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { &mut *(self.item.as_mut_ptr()) }
  }
}

impl<T> Deref for Recyclable<T> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    unsafe { &*(self.item.as_ptr()) }
  }
}

impl<T: Display> std::fmt::Display for Recyclable<T> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    self.item.fmt(f)
  }
}

impl<T: Debug> std::fmt::Debug for Recyclable<T> {
  fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
    self.item.fmt(f)
  }
}

pub struct Pool<T> {
  receiver: Receiver<T>,
  sender: Sender<T>
}

impl<T> Pool<T> {
  pub fn new<F>(capacity: usize, initializer: F) -> Self
    where F: Fn() -> T {
    let (sender, receiver) = channel();

    for _ in 0..capacity {
      sender.send(initializer());
    }

    Self {
      receiver,
      sender
    }
  }

  pub fn get(&mut self) -> Option<Recyclable<T>> {
    let item = self.receiver.try_recv().ok();

    return item.map(|i| Recyclable {
      item: MaybeUninit::new(i),
      sender: self.sender.clone()
    });
  }
}

impl<T> Recyclable<T> {
  pub fn new(sender: Sender<T>, item: T) -> Self {
    Self {
      item: MaybeUninit::new(item),
      sender
    }
  }
}
