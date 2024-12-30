use std::{borrow::Borrow, collections::HashMap, hash::Hash};

pub(crate) trait IndexHandle {
    fn new(index: u64) -> Self;
}

#[derive(Debug, Default)]
pub(crate) struct HandleMap<TKey, THandle, TValue>
where
    TKey: std::hash::Hash + PartialEq + Eq + Clone,
    THandle: std::hash::Hash + PartialEq + Eq + Copy + IndexHandle,
{
    key_to_handle: HashMap<TKey, THandle>,
    handle_to_key: HashMap<THandle, TKey>,
    handle_to_val: HashMap<THandle, TValue>,
    next_handle_index: u64,
}

impl<TKey, THandle, TValue> HandleMap<TKey, THandle, TValue>
where
    TKey: std::hash::Hash + Eq + Clone,
    THandle: std::hash::Hash + Eq + Copy + IndexHandle,
{
    pub(crate) fn new() -> Self {
        Self {
            key_to_handle: HashMap::new(),
            handle_to_key: HashMap::new(),
            handle_to_val: HashMap::new(),
            next_handle_index: 1u64,
        }
    }

    pub(crate) fn get_handle<TKeyRef>(&self, key: &TKeyRef) -> Option<THandle>
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized {
        self.key_to_handle.get(key).copied()
    }

    pub(crate) fn get_or_create_handle<'a, TKeyRef>(&mut self, key: &'a TKeyRef) -> THandle
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized + 'a,
        TKey: From<&'a TKeyRef> {
        if let Some(handle) = self.key_to_handle.get(key) {
            return *handle;
        }
        self.create_handle(key)
    }

    pub(crate) fn get_value(&self, handle: THandle) -> Option<&TValue> {
        self.handle_to_val.get(&handle)
    }

    pub(crate) fn get_value_by_path<TKeyRef>(&self, key: &TKeyRef) -> Option<&TValue>
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized {
        let handle = self.key_to_handle.get(key);
        if let Some(handle) = handle {
            self.handle_to_val.get(handle)
        } else {
            None
        }
    }

    pub(crate) fn contains(&self, handle: THandle) -> bool {
        self.handle_to_val.contains_key(&handle)
    }

    pub(crate) fn contains_handle(&self, handle: THandle) -> bool {
        self.handle_to_key.contains_key(&handle)
    }

    pub(crate) fn contains_value_for_key<TKeyRef>(&self, key: &TKeyRef) -> bool
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized {
        let handle = self.key_to_handle.get(key);
        if let Some(handle) = handle {
            self.handle_to_val.contains_key(handle)
        } else {
            false
        }
    }

    pub(crate) fn contains_key<TKeyRef>(&self, key: &TKeyRef) -> bool
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized {
        self.key_to_handle.contains_key(key)
    }

    pub(crate) fn create_handle<'a, TKeyRef>(&mut self, key: &'a TKeyRef) -> THandle
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized + 'a,
        TKey: From<&'a TKeyRef> {
        let handle = THandle::new(self.next_handle_index);
        self.next_handle_index += 1;
        self.key_to_handle.insert(key.into(), handle);
        self.handle_to_key.insert(handle, key.into());
        handle
    }

    pub(crate) fn insert<'a, TKeyRef>(&mut self, key: &'a TKeyRef, value: TValue) -> THandle
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized + 'a,
        TKey: From<&'a TKeyRef> {
        if let Some(existing_handle) = self.key_to_handle.get(key) {
            self.handle_to_val.insert(*existing_handle, value);
            return *existing_handle;
        }
        let handle: THandle = self.create_handle(key);
        self.handle_to_val.insert(handle, value);
        self.handle_to_key.insert(handle, key.into());
        handle
    }

    pub(crate) fn set(&mut self, handle: THandle, value: TValue) -> bool {
        if !self.handle_to_key.contains_key(&handle) {
            return false;
        }
        self.handle_to_val.insert(handle, value);
        return true;
    }

    pub(crate) fn remove(&mut self, handle: THandle) {
        let path = self.handle_to_key.remove(&handle).unwrap();
        self.key_to_handle.remove(&path);
        self.handle_to_val.remove(&handle);
    }

    pub(crate) fn remove_by_key<TKeyRef>(&mut self, key: &TKeyRef) -> Option<THandle>
    where TKey: Borrow<TKeyRef>,
        TKeyRef: Eq + Hash + ?Sized {
        let handle = self.key_to_handle.remove(key);
        if let Some(handle) = handle {
            self.handle_to_val.remove(&handle);
            self.handle_to_key.remove(&handle);
            Some(handle)
        } else {
            None
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.handle_to_val.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
