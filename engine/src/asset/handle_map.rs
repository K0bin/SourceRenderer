use std::collections::HashMap;

pub(crate) trait IndexHandle {
    fn new(index: u64) -> Self;
}

#[derive(Debug, Default)]
pub(crate) struct HandleMap<THandle, TValue>
where
    THandle: std::hash::Hash + PartialEq + Eq + Copy + IndexHandle,
{
    path_to_handle: HashMap<String, THandle>,
    handle_to_path: HashMap<THandle, String>,
    handle_to_val: HashMap<THandle, TValue>,
    next_handle_index: u64,
}

impl<THandle, TValue> HandleMap<THandle, TValue>
where
    THandle: std::hash::Hash + PartialEq + Eq + Copy + IndexHandle,
{
    pub(crate) fn new() -> Self {
        Self {
            path_to_handle: HashMap::new(),
            handle_to_path: HashMap::new(),
            handle_to_val: HashMap::new(),
            next_handle_index: 1u64,
        }
    }

    pub(crate) fn get_handle(&self, path: &str) -> Option<THandle> {
        self.path_to_handle.get(path).copied()
    }

    pub(crate) fn get_or_create_handle(&mut self, path: &str) -> THandle {
        if let Some(handle) = self.path_to_handle.get(path) {
            return *handle;
        }
        self.create_handle(path)
    }

    pub(crate) fn get_value(&self, handle: THandle) -> Option<&TValue> {
        self.handle_to_val.get(&handle)
    }

    pub(crate) fn contains(&self, handle: THandle) -> bool {
        self.handle_to_val.contains_key(&handle)
    }

    pub(crate) fn contains_handle(&self, handle: THandle) -> bool {
        self.handle_to_path.contains_key(&handle)
    }

    pub(crate) fn contains_value_for_path(&self, path: &str) -> bool {
        let handle = self.path_to_handle.get(path);
        if let Some(handle) = handle {
            self.handle_to_val.contains_key(handle)
        } else {
            false
        }
    }

    pub(crate) fn contains_path(&self, path: &str) -> bool {
        self.path_to_handle.contains_key(path)
    }

    pub(crate) fn create_handle(&mut self, path: &str) -> THandle {
        let handle = THandle::new(self.next_handle_index);
        self.next_handle_index += 1;
        self.path_to_handle.insert(path.to_string(), handle);
        self.handle_to_path.insert(handle, path.to_string());
        handle
    }

    pub(crate) fn insert(&mut self, path: &str, value: TValue) -> THandle {
        if let Some(existing_handle) = self.path_to_handle.get(path) {
            self.handle_to_val.insert(*existing_handle, value);
            return *existing_handle;
        }
        let handle: THandle = self.create_handle(path);
        self.handle_to_val.insert(handle, value);
        self.handle_to_path.insert(handle, path.to_string());
        handle
    }

    pub(crate) fn set(&mut self, handle: THandle, value: TValue) -> bool {
        if !self.handle_to_path.contains_key(&handle) {
            return false;
        }
        self.handle_to_val.insert(handle, value);
        return true;
    }

    pub(crate) fn remove(&mut self, handle: THandle) {
        let path = self.handle_to_path.remove(&handle).unwrap();
        self.path_to_handle.remove(&path);
        self.handle_to_val.remove(&handle);
    }

    pub(crate) fn remove_by_path(&mut self, path: &str) -> Option<THandle> {
        let handle = self.path_to_handle.remove(path);
        if let Some(handle) = handle {
            self.handle_to_val.remove(&handle);
            self.handle_to_path.remove(&handle);
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
