use std::{borrow::Borrow, collections::{HashMap, VecDeque}, hash::Hash};

pub struct FixedByteSizeCache<K: Hash + PartialEq + Eq + Clone, V> {
    buffers: HashMap<K, V>,
    queue: VecDeque<K>,
    current_size: usize,
    max_size: usize,
}

#[derive(Debug)]
pub struct FileTooLarge();

impl<K: Hash + PartialEq + Eq + Clone, V> FixedByteSizeCache<K, V> {
    pub fn new(max_size: usize, ) -> Self {
        Self {
            buffers: HashMap::new(),
            queue: VecDeque::new(),
            current_size: 0,
            max_size,
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<bool, FileTooLarge> {
        if self.buffers.contains_key(&key) {
            return Ok(true);
        }

        let value_size = std::mem::size_of_val(&value);
        if value_size > self.max_size {
            return Err(FileTooLarge());
        }

        while self.current_size + value_size > self.max_size {
            assert!(!self.queue.is_empty());
            let next_key_to_remove = self.queue.pop_front().unwrap();
            let removed_value = self.buffers.remove(&next_key_to_remove).unwrap();
            self.current_size -= std::mem::size_of_val(&removed_value);
        }

        self.buffers.insert(key.clone(), value);
        self.queue.push_back(key);
        Ok(false)
    }

    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
        where
        K: Borrow<Q>,
        Q: Hash + Eq {
        self.buffers.contains_key(key)
    }

    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
        where
        K: Borrow<Q>,
        Q: Hash + Eq {
        self.buffers.get(key)
    }

    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
        where
        K: Borrow<Q>,
        Q: Hash + Eq {
        let value = self.buffers.remove(key);
        if value.is_none() {
            return None;
        }

        let index_to_remove = self.queue.iter().enumerate()
            .find_map(|(idx, val)|
                if val.borrow() == key { Some(idx) } else { None }
            ).unwrap();
        self.queue.remove(index_to_remove).unwrap();
        value
    }

    #[inline(always)]
    pub fn current_size(&self) -> usize {
        self.current_size
    }

    #[inline(always)]
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.queue.len()
    }
}
