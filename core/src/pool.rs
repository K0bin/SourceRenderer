use crossbeam_channel::{unbounded, Receiver, Sender};
use std::convert::{AsMut, AsRef};
use std::fmt::{Debug, Display, Formatter};
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};

pub struct Recyclable<T> {
    item: MaybeUninit<T>,
    sender: Sender<T>,
}

impl<T> Drop for Recyclable<T> {
    fn drop(&mut self) {
        let item =
            unsafe { std::mem::replace(&mut self.item, MaybeUninit::uninit()).assume_init() };
        if self.sender.send(item).is_err() {
            println!("Recycling failed. Channel disconnected.");
        }
    }
}

impl<T> DerefMut for Recyclable<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.item.as_mut_ptr()) }
    }
}

impl<T> AsMut<T> for Recyclable<T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.item.as_mut_ptr()) }
    }
}

impl<T> Deref for Recyclable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.item.as_ptr()) }
    }
}

impl<T> AsRef<T> for Recyclable<T> {
    fn as_ref(&self) -> &T {
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
    sender: Sender<T>,
    initializer: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T> Pool<T> {
    pub fn new(initializer: Box<dyn Fn() -> T + Send + Sync>) -> Self {
        let (sender, receiver) = unbounded();

        Self {
            receiver,
            sender,
            initializer,
        }
    }

    pub fn get(&self) -> Recyclable<T> {
        let item = self.receiver.try_recv().ok();

        item.map_or_else(
            || Recyclable {
                item: MaybeUninit::new((self.initializer)()),
                sender: self.sender.clone(),
            },
            |i| Recyclable {
                item: MaybeUninit::new(i),
                sender: self.sender.clone(),
            },
        )
    }
}

impl<T> Recyclable<T> {
    pub fn new(sender: Sender<T>, item: T) -> Self {
        Self {
            item: MaybeUninit::new(item),
            sender,
        }
    }

    pub fn from_parts(item: T, sender: Sender<T>) -> Recyclable<T> {
        Recyclable {
            item: MaybeUninit::new(item),
            sender,
        }
    }

    pub fn into_inner(r: Recyclable<T>) -> T {
        let mut r_mut = r;
        unsafe { std::mem::replace(&mut r_mut.item, MaybeUninit::uninit()).assume_init() }
    }

    pub fn into_parts(r: Recyclable<T>) -> (T, Sender<T>) {
        let mut r_mut = r;
        (
            unsafe { std::mem::replace(&mut r_mut.item, MaybeUninit::uninit()).assume_init() },
            r_mut.sender.clone(),
        )
    }
}
