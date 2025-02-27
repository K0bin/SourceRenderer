#![allow(dead_code)]
use std::{ops::{Deref, DerefMut}, time::Duration};

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut, BorrowMutError};

pub struct RwLock<T> {
    refcell: AtomicRefCell<T>
}

impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            refcell: AtomicRefCell::new(value)
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        RwLockWriteGuard::<'_, T> {
            guard: self.refcell.borrow_mut()
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadGuard::<'_, T> {
            guard: self.refcell.borrow()
        }
    }
}

pub struct RwLockWriteGuard<'a, T: ?Sized + 'a> {
    guard: AtomicRefMut<'a, T>
}

impl<'a, T: ?Sized + 'a> Deref for RwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

impl<'a, T: ?Sized + 'a> DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.deref_mut()
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized + 'a> {
    guard: AtomicRef<'a, T>
}

impl<'a, T: ?Sized + 'a> Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}


pub struct Mutex<T> {
    refcell: AtomicRefCell<T>
}

impl<T> Mutex<T> {
    pub fn new(value: T) -> Self {
        Self {
            refcell: AtomicRefCell::new(value)
        }
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, T>, BorrowMutError> {
        Ok(MutexGuard::<'_, T> {
            guard: self.refcell.try_borrow_mut()?
        })
    }

    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, BorrowMutError> {
        Ok(MutexGuard::<'_, T> {
            guard: self.refcell.try_borrow_mut()?
        })
    }
}

pub struct MutexGuard<'a, T: ?Sized + 'a> {
    guard: AtomicRefMut<'a, T>
}

impl<'a, T: ?Sized + 'a> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

impl<'a, T: ?Sized + 'a> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.deref_mut()
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}

pub struct Condvar {}

impl Condvar {
    pub fn new() -> Self { Self {} }

    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> Result<MutexGuard<'a, T>, BorrowMutError> {
        Ok(guard)
    }

    pub fn wait_timeout<'a, T>(&self, guard: MutexGuard<'a, T>, _: Duration) -> Result<MutexGuard<'a, T>, BorrowMutError> {
        Ok(guard)
    }

    pub fn wait_timeout_ms<'a, T>(&self, guard: MutexGuard<'a, T>, _: u32) -> Result<MutexGuard<'a, T>, BorrowMutError> {
        Ok(guard)
    }

    pub fn wait_timeout_while<'a, T, F>(&self, mut guard: MutexGuard<'a, T>, _: Duration, mut condition: F) -> Result<MutexGuard<'a, T>, BorrowMutError>
        where F: FnMut(&mut T) -> bool
    {
        assert!(condition(&mut guard));
        Ok(guard)
    }

    pub fn wait_while<'a, T, F>(&self, mut guard: MutexGuard<'a, T>, mut condition: F) -> Result<MutexGuard<'a, T>, BorrowMutError>
        where F: FnMut(&mut T) -> bool
    {
        assert!(condition(&mut guard));
        Ok(guard)
    }

    pub fn notify_one(&self) {}
    pub fn notify_all(&self) {}
}
