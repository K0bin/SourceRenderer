use std::{future::Future, pin::Pin};

use crate::FixedByteSizeCache;

type FilePageKey = (String, usize);

pub struct AsyncPageCache {
    cache: FixedByteSizeCache<FilePageKey, Box<[u8]>>,
    load_func: Box<dyn FnOnce(&str, usize, usize) -> Pin<Box<dyn Future<Output = Box<[u64]>>>> + 'static>,
}

impl AsyncPageCache {
    pub fn new<F, FFuture>(cache_size: usize, load_func: F) -> Self
    where
        F: (Fn(&str, usize, usize) -> FFuture) + 'static,
        FFuture: Future<Output = Box<[u64]>> + 'static {
        Self {
            cache: FixedByteSizeCache::new(cache_size),
            load_func: Box::new(move |path, offset, length| {
                Box::pin(async move { load_func(path, offset, length).await })
            }),
        }

    }
}
