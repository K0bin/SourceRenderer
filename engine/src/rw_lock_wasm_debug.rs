use std::{ops::{Deref, DerefMut}, sync::Arc, time::Duration};

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut, BorrowMutError};

pub struct RwLock<T> {
    refcell: AtomicRefCell<T>,
    immutable_stacks: AtomicRefCell<Vec<Arc<AtomicRefCell<Option<std::backtrace::Backtrace>>>>>,
    mutable_stack: Arc<AtomicRefCell<Option<std::backtrace::Backtrace>>>
}

impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            refcell: AtomicRefCell::new(value),
            immutable_stacks: AtomicRefCell::new(Vec::new()),
            mutable_stack: Arc::new(AtomicRefCell::new(None))
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        let borrow = self.refcell.try_borrow_mut();
        if borrow.is_err() {
            let mut immutable_stacks = self.immutable_stacks.borrow_mut();
            immutable_stacks.retain(|s| {
                let stack = s.borrow();
                stack.is_some()
            });
            log::err!("ERROR: WRITING. MUTABLE BORROW: {:?}, IMMUTABLE BORROWS: {:?}", self.mutable_stack, immutable_stacks);
        }
        {
            let mut stack = self.mutable_stack.borrow_mut();
            assert!(stack.replace(std::backtrace::Backtrace::capture()).is_none());
        }
        RwLockWriteGuard::<'_, T> {
            guard: borrow.unwrap(),
            stack: self.mutable_stack.clone()
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        let borrow = self.refcell.try_borrow();
        if borrow.is_err() {
            log::err!("ERROR: READING. MUTABLE BORROWS: {:?}", self.mutable_stack);
        }
        let stack = {
            let mut stacks = self.immutable_stacks.borrow_mut();
            stacks.retain(|s| {
                let stack = s.borrow();
                stack.is_some()
            });
            let stack = Arc::new(AtomicRefCell::new(Some(std::backtrace::Backtrace::capture())));
            stacks.push(stack.clone());
            stack
        };
        RwLockReadGuard::<'_, T> {
            guard: borrow.unwrap(),
            stack: stack
        }
    }
}

pub struct RwLockWriteGuard<'a, T: ?Sized + 'a> {
    guard: AtomicRefMut<'a, T>,
    stack: Arc<AtomicRefCell<Option<std::backtrace::Backtrace>>>,
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

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        let mut stack = self.stack.borrow_mut();
        stack.take();
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized + 'a> {
    guard: AtomicRef<'a, T>,
    stack: Arc<AtomicRefCell<Option<std::backtrace::Backtrace>>>
}

impl<'a, T: ?Sized + 'a> Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}
impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        let mut stack = self.stack.borrow_mut();
        stack.take();
    }
}


pub struct Mutex<T> {
    refcell: AtomicRefCell<T>,
    stack: Arc<AtomicRefCell<Option<std::backtrace::Backtrace>>>
}

impl<T> Mutex<T> {
    pub fn new(value: T) -> Self {
        Self {
            refcell: AtomicRefCell::new(value),
            stack: Arc::new(AtomicRefCell::new(None))
        }
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, T>, BorrowMutError> {
        let guard = self.refcell.try_borrow_mut();

        if let Some(err) = guard.err() {
            log::err!("LOCKING. Existing lock: {:?}", self.stack);

            return Err(err);
        }

        Ok(MutexGuard::<'_, T> {
            guard: self.refcell.try_borrow_mut()?,
            stack: self.stack.clone()
        })
    }

    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, BorrowMutError> {
        let guard = self.refcell.try_borrow_mut()?;
        Ok(MutexGuard::<'_, T> {
            guard,
            stack: self.stack.clone()
        })
    }
}

pub struct MutexGuard<'a, T: ?Sized + 'a> {
    guard: AtomicRefMut<'a, T>,
    stack: Arc<AtomicRefCell<Option<std::backtrace::Backtrace>>>
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
