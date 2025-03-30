use std::sync::Arc;
use std::fmt::{Debug, Formatter, Error as FmtError};
use std::mem::ManuallyDrop;
use sourcerenderer_core::extend_lifetime;
use sourcerenderer_core::gpu::QueryPool as _;

use super::*;

struct QueryPool {
    pool: ManuallyDrop<active_gpu_backend::QueryPool>,
    destroyer: Arc<DeferredDestroyer>,
}

#[derive(Clone)]
pub struct QueryRange {
    pool: &'static QueryPool,
    first_index: u32,
    count: u32,
    frame: u64,
}

pub struct OutOfQueriesError {
    requested_count: u32,
    pool_next_index: u32,
    pool_total_count: u32,
}

impl Debug for OutOfQueriesError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        f.write_fmt(format_args!("Out of queries: requested queries: {:?}, next query index in pool: {:?}, total count of pool: {:?}", self.requested_count, self.pool_next_index, self.pool_total_count))
    }
}

// Simple fixed size bump allocator
pub(super) struct QueryAllocator {
    pool: Box<QueryPool>,
    count: u32,
    next_index: u32
}

impl QueryAllocator {
    pub(super) fn new(device: &Arc<active_gpu_backend::Device>, destroyer: &Arc<DeferredDestroyer>, query_count: u32) -> Self {
        let pool = unsafe { device.create_query_pool(query_count) };
        Self {
            pool: Box::new(QueryPool {
                pool: ManuallyDrop::new(pool),
                destroyer: destroyer.clone(),
            }),
            count: query_count,
            next_index: 0u32
        }
    }

    pub(super) fn get_queries(&mut self, frame: u64, query_count: u32) -> Result<QueryRange, OutOfQueriesError> {
        if self.next_index + query_count > self.count {
            return Err(OutOfQueriesError {
                requested_count: query_count,
                pool_next_index: self.next_index,
                pool_total_count: self.count
            });
        }

        let idx = self.next_index;
        self.next_index += query_count;
        Ok(QueryRange {
            pool: unsafe { extend_lifetime(&self.pool) },
            first_index: idx,
            count: query_count,
            frame,
        })
    }

    pub(super) fn reset(&mut self) {
        unsafe { self.pool.pool.reset(); }
        self.next_index = 0;
    }
}

impl Drop for QueryPool {
    fn drop(&mut self) {
        let query_pool = unsafe { ManuallyDrop::take(&mut self.pool) };
        self.destroyer.destroy_query_pool(query_pool);
    }
}

impl QueryRange {
    pub(super) fn pool_handle(&self, frame: u64) -> &active_gpu_backend::QueryPool {
        assert_eq!(self.frame, frame);
        &self.pool.pool
    }

    pub(super) fn query_index(&self, offset: u32) -> u32 {
        assert!(offset < self.count);
        self.first_index + offset
    }
}
