use std::{
    ffi::c_void,
    sync::Arc,
};

use ash::vk;
use sourcerenderer_core::gpu;

use super::*;

pub struct VkTimelineSemaphore {
    device: Arc<RawVkDevice>,
    semaphore: vk::Semaphore,
}

impl VkTimelineSemaphore {
    pub fn new(device: &Arc<RawVkDevice>) -> Self {
        let semaphore = unsafe {
            let semaphore_type = vk::SemaphoreTypeCreateInfo {
                semaphore_type: vk::SemaphoreType::TIMELINE_KHR,
                initial_value: 0,
                ..Default::default()
            };
            device
                .create_semaphore(
                    &vk::SemaphoreCreateInfo {
                        p_next: &semaphore_type as *const vk::SemaphoreTypeCreateInfo
                            as *const c_void,
                        flags: vk::SemaphoreCreateFlags::empty(),
                        ..Default::default()
                    },
                    None,
                )
                .unwrap()
        };
        Self {
            device: device.clone(),
            semaphore,
        }
    }

    pub fn handle(&self) -> vk::Semaphore {
        self.semaphore
    }

    pub unsafe fn await_value(&self, value: u64) {
        unsafe {
            self.device
                .wait_semaphores(
                    &vk::SemaphoreWaitInfo {
                        flags: vk::SemaphoreWaitFlags::empty(),
                        semaphore_count: 1,
                        p_semaphores: &self.handle() as *const vk::Semaphore,
                        p_values: &[value] as *const u64,
                        ..Default::default()
                    },
                    std::u64::MAX,
                )
                .unwrap();
        }
    }

    pub unsafe fn value(&self) -> u64 {
        unsafe {
            self.device
                .get_semaphore_counter_value(self.semaphore)
                .unwrap()
        }
    }
}

impl Drop for VkTimelineSemaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
        }
    }
}

impl gpu::Fence for VkTimelineSemaphore {
    unsafe fn value(&self) -> u64 {
        unsafe {
            self.device
                .get_semaphore_counter_value(self.semaphore)
                .unwrap()
        }
    }

    unsafe fn await_value(&self, value: u64) {
        unsafe {
            let wait_info = vk::SemaphoreWaitInfo {
                flags: vk::SemaphoreWaitFlags::ANY,
                semaphore_count: 1,
                p_semaphores: &self.semaphore as *const vk::Semaphore,
                p_values: &value as *const u64,
                ..Default::default()
            };
            self.device.wait_semaphores(&wait_info, std::u64::MAX).unwrap();
        }
    }
}
