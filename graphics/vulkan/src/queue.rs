use std::{
    iter::*,
    sync::Arc,
};

use ash::vk;
use parking_lot::ReentrantMutexGuard;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::*;

use super::*;

#[derive(Clone, Debug, Copy)]
pub struct VkQueueInfo {
    pub queue_family_index: usize,
    pub queue_index: usize,
    pub supports_presentation: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum VkQueueType {
    Graphics,
    Compute,
    Transfer,
}

pub struct VkQueue {
    info: VkQueueInfo,
    device: Arc<RawVkDevice>,
    shared: Arc<VkShared>,
    queue_type: VkQueueType,
}

struct VkSemaphoreSignalOrWait {
    semaphore: vk::Semaphore,
    value: u64,
    stage: vk::PipelineStageFlags2,
}

impl VkQueue {
    pub fn new(
        info: VkQueueInfo,
        queue_type: VkQueueType,
        device: &Arc<RawVkDevice>,
        shared: &Arc<VkShared>,
    ) -> Self {
        Self {
            info,
            device: device.clone(),
            shared: shared.clone(),
            queue_type,
        }
    }

    pub fn family_index(&self) -> u32 {
        self.info.queue_family_index as u32
    }

    pub fn supports_presentation(&self) -> bool {
        self.info.supports_presentation
    }

    fn lock_queue(&self) -> ReentrantMutexGuard<vk::Queue> {
        match self.queue_type {
            VkQueueType::Graphics => self.device.graphics_queue(),
            VkQueueType::Compute => self.device.compute_queue().unwrap(),
            VkQueueType::Transfer => self.device.transfer_queue().unwrap(),
        }
    }
}

impl Queue<VkBackend> for VkQueue {
    unsafe fn submit(&self, submissions: &[Submission<VkBackend>]) {
        let guard = self.lock_queue();

        let mut command_buffers =
            SmallVec::<[vk::CommandBufferSubmitInfo; 2]>::with_capacity(submissions.len());
        let mut semaphores =
            SmallVec::<[vk::SemaphoreSubmitInfo; 2]>::with_capacity(submissions.len());

        for submission in submissions.iter() {
            for cmd_buffer in submission.command_buffers.iter() {
                cmd_buffer.mark_submitted();
                command_buffers.push(vk::CommandBufferSubmitInfo {
                    command_buffer: cmd_buffer.handle(),
                    device_mask: 0u32,
                    ..Default::default()
                });
            }
            for fence in submission.wait_fences {
                semaphores.push(vk::SemaphoreSubmitInfo {
                    semaphore: fence.fence.handle(),
                    value: fence.value,
                    stage_mask: barrier_sync_to_stage(fence.sync_before),
                    device_index: 0u32,
                    ..Default::default()
                });
            }

            for fence in submission.signal_fences {
                semaphores.push(vk::SemaphoreSubmitInfo {
                    semaphore: fence.fence.handle(),
                    value: fence.value,
                    stage_mask: barrier_sync_to_stage(fence.sync_before),
                    device_index: 0u32,
                    ..Default::default()
                });
            }

            if let Some(swapchain) = submission.acquire_swapchain {
                semaphores.push(vk::SemaphoreSubmitInfo {
                    semaphore: swapchain.acquire_semaphore().handle(),
                    value: 0u64,
                    stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                    device_index: 0u32,
                    ..Default::default()
                });
            }

            if let Some(swapchain) = submission.release_swapchain {
                semaphores.push(vk::SemaphoreSubmitInfo {
                    semaphore: swapchain.present_semaphore().handle(),
                    value: 0u64,
                    stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                    device_index: 0u32,
                    ..Default::default()
                });
            }
        }

        let mut cmd_buffer_ptr = command_buffers.as_ptr();
        let mut semaphore_ptr = semaphores.as_ptr();
        let vk_submissions: SmallVec<[vk::SubmitInfo2; 2]> = submissions
            .iter()
            .map(|submission| {
                let submission_cmd_buffer_ptr = cmd_buffer_ptr;
                cmd_buffer_ptr = cmd_buffer_ptr.add(submission.command_buffers.len());
                let submission_wait_semaphores_ptr = semaphore_ptr;
                semaphore_ptr = semaphore_ptr.add(submission.wait_fences.len() + submission.acquire_swapchain.map_or(0, |_| 1));
                let submission_signal_semaphores_ptr = semaphore_ptr;
                semaphore_ptr = semaphore_ptr.add(submission.signal_fences.len() + submission.release_swapchain.map_or(0, |_| 1));

                vk::SubmitInfo2 {
                    flags: vk::SubmitFlags::empty(),
                    wait_semaphore_info_count: submission.wait_fences.len() as u32,
                    p_wait_semaphore_infos: submission_wait_semaphores_ptr,
                    command_buffer_info_count: submission.command_buffers.len() as u32,
                    p_command_buffer_infos: submission_cmd_buffer_ptr,
                    signal_semaphore_info_count: submission.signal_fences.len() as u32,
                    p_signal_semaphore_infos: submission_signal_semaphores_ptr,
                    ..Default::default()
                }
            })
            .collect();

        self.device.synchronization2
            .queue_submit2(*guard, &vk_submissions, vk::Fence::null())
            .unwrap();
    }

    unsafe fn present(&self, swapchain: &VkSwapchain) {
        let guard = self.lock_queue();
        let present_info = vk::PresentInfoKHR {
            wait_semaphore_count: 1,
            p_wait_semaphores: &swapchain.acquire_semaphore().handle() as *const vk::Semaphore,
            swapchain_count: 1,
            p_swapchains: &swapchain.handle() as *const vk::SwapchainKHR,
            p_image_indices: &swapchain.backbuffer_index() as *const u32,
            p_results: std::ptr::null_mut(),
            ..Default::default()
        };
        let result = swapchain.loader().queue_present(*guard, &present_info);
        match result {
            Ok(suboptimal) => {
                if suboptimal {
                    swapchain.set_state(VkSwapchainState::Suboptimal);
                }
            }
            Err(err) => match err {
                vk::Result::ERROR_OUT_OF_DATE_KHR => {
                    swapchain.set_state(VkSwapchainState::OutOfDate);
                }
                vk::Result::ERROR_SURFACE_LOST_KHR => {
                    swapchain.surface().mark_lost();
                }
                _ => {
                    panic!("Present failed: {:?}", err);
                }
            }
        }
    }

    unsafe fn create_command_pool(&self, command_pool_type: CommandPoolType, flags: CommandPoolFlags) -> VkCommandPool {
        VkCommandPool::new(
            &self.device,
            self.info.queue_family_index as u32,
            flags,
            &self.shared,
            command_pool_type
        )
    }
}

// Vulkan queues are implicitly freed with the logical device
