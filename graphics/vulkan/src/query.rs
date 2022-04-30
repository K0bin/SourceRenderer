use std::{sync::{Arc, Mutex}, collections::HashMap, hash::Hash};

use ash::vk;

use sourcerenderer_core::atomic_refcell::AtomicRefCell;

use crate::raw::RawVkDevice;

pub struct VkQueryAllocator {
  device: Arc<RawVkDevice>,
  inner: AtomicRefCell<VkQueryAllocatorInner>,
}

pub struct VkQueryAllocatorInner {
  pools: HashMap<vk::QueryType, Vec<Arc<VkQueryPool>>>,
}

impl VkQueryAllocator {
  pub fn new(device: &Arc<RawVkDevice>) -> Self {
    Self {
      device: device.clone(),
      inner: AtomicRefCell::new(VkQueryAllocatorInner {
        pools: HashMap::new(),
      })
    }
  }

  pub fn get(&self, query_type: vk::QueryType, count: u32) -> VkQueryRange {
    let mut inner = self.inner.borrow_mut();
    let pools_map = &mut inner.pools;
    let type_pools = pools_map.entry(query_type).or_default();
    if let Some((pool, index)) = type_pools.iter().find_map(|pool| pool.get(count).map(|index| (pool.clone(), index))) {
      return VkQueryRange {
        pool,
        index,
        count
      };
    }

    let new_pool = VkQueryPool::new(&self.device, query_type, count.max(4096));
    let index = new_pool.get(count).unwrap();
    let new_pool = Arc::new(new_pool);
    let range = VkQueryRange {
      pool: new_pool.clone(),
      index,
      count
    };
    type_pools.push(new_pool);
    range
  }

  pub fn reset(&self) {
    let mut inner = self.inner.borrow_mut();
    let pools_map = &mut inner.pools;
    for pools in pools_map.values() {
      for pool in pools {
        pool.reset();
      }
    }
  }
}

pub struct VkQueryPool {
  device: Arc<RawVkDevice>,
  query_pool: vk::QueryPool,
  query_count: u32,
  inner: Mutex<VkQueryPoolInner>
}

pub struct VkQueryPoolInner {
  used_queries: u32,
  is_reset: bool
}

impl VkQueryPool {
  pub fn new(device: &Arc<RawVkDevice>, query_type: vk::QueryType, query_count: u32) -> Self {
    let query_pool = unsafe {
      device.create_query_pool(&vk::QueryPoolCreateInfo {
        flags: vk::QueryPoolCreateFlags::empty(),
        query_type,
        query_count,
        pipeline_statistics: vk::QueryPipelineStatisticFlags::empty(),
        ..Default::default()
      }, None)
    }.unwrap();
    Self {
      query_pool,
      device: device.clone(),
      query_count,
      inner: Mutex::new(VkQueryPoolInner {
        used_queries: 0,
        is_reset: false
      })
    }
  }

  pub fn is_reset(&self) -> bool {
    let inner = self.inner.lock().unwrap();
    inner.is_reset
  }

  pub fn mark_reset(&self) {
    let mut inner = self.inner.lock().unwrap();
    inner.is_reset = true;
  }

  pub fn query_count(&self) -> u32 {
    self.query_count
  }

  pub fn reset(&self) {
    let mut inner = self.inner.lock().unwrap();
    // Requires host_query_reset
    // FIXME
    /*unsafe {
      self.device.reset_query_pool(self.query_pool, 0, self.query_count);
    }
    inner.is_reset = true;*/

    inner.used_queries = 0;
    // Reset happens on the GPU, so assume the query is used until vkCmdResetQueryPool is called
    inner.is_reset = false;
  }

  pub fn get(&self, count: u32) -> Option<u32> {
    let mut inner = self.inner.lock().unwrap();

    if self.query_count - inner.used_queries < count {
      return None;
    }

    let query_index = inner.used_queries;
    inner.used_queries += count;
    Some(query_index)
  }

  pub fn handle(&self) -> &vk::QueryPool {
    &self.query_pool
  }
}

impl Hash for VkQueryPool {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.query_pool.hash(state);
  }
}

impl PartialEq for VkQueryPool {
  fn eq(&self, other: &Self) -> bool {
    self.query_pool == other.query_pool
  }
}

impl Eq for VkQueryPool {}

impl Drop for VkQueryPool {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_query_pool(self.query_pool, None);
    }
  }
}

#[derive(PartialEq, Eq, Hash)]
pub struct VkQueryRange {
  pub(crate) pool: Arc<VkQueryPool>,
  pub(crate) index: u32,
  pub(crate) count: u32
}
