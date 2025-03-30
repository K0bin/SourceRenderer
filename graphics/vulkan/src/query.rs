use std::hash::Hash;
use std::sync::Arc;

use ash::vk;
use sourcerenderer_core::gpu;

use crate::raw::RawVkDevice;

pub struct VkQueryPool {
    device: Arc<RawVkDevice>,
    query_pool: vk::QueryPool,
    query_count: u32,
}

impl VkQueryPool {
    pub fn new(device: &Arc<RawVkDevice>, query_type: vk::QueryType, query_count: u32) -> Self {
        let query_pool = unsafe {
            device.create_query_pool(
                &vk::QueryPoolCreateInfo {
                    flags: vk::QueryPoolCreateFlags::empty(),
                    query_type,
                    query_count,
                    pipeline_statistics: vk::QueryPipelineStatisticFlags::empty(),
                    ..Default::default()
                },
                None,
            )
        }
        .unwrap();
        Self {
            query_pool,
            device: device.clone(),
            query_count,
        }
    }

    pub fn handle(&self) -> vk::QueryPool {
        self.query_pool
    }
}

impl gpu::QueryPool for VkQueryPool {
    unsafe fn reset(&self) {
        self.device.reset_query_pool(self.query_pool, 0, self.query_count);
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
