use std::iter::*;
use std::sync::Arc;

use ash::vk;
use parking_lot::ReentrantMutexGuard;
use smallvec::SmallVec;

use sourcerenderer_core::gpu::*;

use super::*;

use crate::queue::VkQueueInfo; // the RawVkDevice uses this, so we cannot use the new one

/*#[derive(Clone, Debug, Copy)]
pub struct VkQueueInfo {
    pub queue_family_index: usize,
    pub queue_index: usize,
    pub supports_presentation: bool,
}*/

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
    unsafe fn submit(&self, submissions: &mut [Submission<VkBackend>]) {
        let guard = self.lock_queue();

        let mut command_buffers = SmallVec::<[vk::CommandBufferSubmitInfo; 2]>::with_capacity(submissions.len());
        let mut semaphores = SmallVec::<[vk::SemaphoreSubmitInfo; 2]>::with_capacity(submissions.len());

        for submission in submissions.iter_mut() {
            for cmd_buffer in submission.command_buffers.iter_mut() {
                cmd_buffer.mark_submitted();
                command_buffers.push(vk::CommandBufferSubmitInfo {
                    command_buffer: cmd_buffer.handle(),
                    device_mask: 0u32,
                    ..Default::default()
                });
            }
            for fence in submission.wait_fences {
                match fence {
                    FenceRef::Fence(fence_value_pair) => {
                        semaphores.push(vk::SemaphoreSubmitInfo {
                            semaphore: fence_value_pair.fence.handle(),
                            value: fence_value_pair.value,
                            stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                            device_index: 0u32,
                            ..Default::default()
                        });
                    }
                    FenceRef::WSIFence(fence) => {
                        semaphores.push(vk::SemaphoreSubmitInfo {
                            semaphore: fence.handle(),
                            value: 0u64,
                            stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                            device_index: 0u32,
                            ..Default::default()
                        });
                    }
                }
            }
            for fence in submission.signal_fences {
                match fence {
                    FenceRef::Fence(fence_value_pair) => {
                        semaphores.push(vk::SemaphoreSubmitInfo {
                            semaphore: fence_value_pair.fence.handle(),
                            value: fence_value_pair.value,
                            stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                            device_index: 0u32,
                            ..Default::default()
                        });
                    }
                    FenceRef::WSIFence(fence) => {
                        semaphores.push(vk::SemaphoreSubmitInfo {
                            semaphore: fence.handle(),
                            value: 0u64,
                            stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                            device_index: 0u32,
                            ..Default::default()
                        });
                    }
                }
            }
        }

        let mut cmd_buffer_ptr = command_buffers.as_ptr();
        let mut semaphore_ptr = semaphores.as_ptr();
        let vk_submissions: SmallVec<[vk::SubmitInfo2; 2]> = submissions.iter().map(|submission| {
            let submission_cmd_buffer_ptr = cmd_buffer_ptr;
            cmd_buffer_ptr = cmd_buffer_ptr.add(submission.command_buffers.len());
            let submission_wait_semaphores_ptr = semaphore_ptr;
            semaphore_ptr = semaphore_ptr.add(submission.wait_fences.len());
            let submission_signal_semaphores_ptr = semaphore_ptr;
            semaphore_ptr = semaphore_ptr.add(submission.signal_fences.len());

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
        }).collect();

        self.device.queue_submit2(*guard, &vk_submissions, vk::Fence::null()).unwrap();
    }

    unsafe fn present(&self, swapchain: &VkSwapchain, wait_fence: &VkBinarySemaphore) {
        //swapchain.loader().qu
    }

    unsafe fn create_command_pool(&self, _command_pool_type: CommandPoolType) -> VkCommandPool {
        VkCommandPool::new(&self.device, self.info.queue_family_index as u32, &self.shared)
    }
}

// Vulkan queues are implicitly freed with the logical device
