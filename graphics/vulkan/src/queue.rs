use std::iter::*;
use std::sync::atomic::Ordering;
use std::sync::{
    Arc,
    Mutex,
};

use ash::vk;
use parking_lot::ReentrantMutexGuard;
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{
    CommandBufferType,
    Queue,
    Swapchain,
};

use crate::command::VkInnerCommandBufferInfo;
use crate::raw::RawVkDevice;
use crate::swapchain::{
    VkSwapchain,
    VkSwapchainState,
};
use crate::sync::{
    VkFence,
    VkSemaphore,
};
use crate::thread_manager::VkThreadManager;
use crate::transfer::VkTransferCommandBuffer;
use crate::{
    VkBackend,
    VkCommandBufferRecorder,
    VkCommandBufferSubmission,
    VkShared,
};

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
    queue: Mutex<VkQueueInner>,
    device: Arc<RawVkDevice>,
    shared: Arc<VkShared>,
    threads: Arc<VkThreadManager>,
    queue_type: VkQueueType,
}

struct VkQueueInner {
    virtual_queue: Vec<VkVirtualSubmission>,
    signalled_semaphores: SmallVec<[Arc<VkSemaphore>; 8]>,
}

struct VkSemaphoreSignalOrWait {
    semaphore: Arc<VkSemaphore>,
    stage: vk::PipelineStageFlags2,
}

enum VkVirtualSubmission {
    CommandBuffer {
        command_buffer: vk::CommandBuffer,
        wait_semaphores: SmallVec<[VkSemaphoreSignalOrWait; 4]>,
        signal_semaphores: SmallVec<[VkSemaphoreSignalOrWait; 4]>,
        fence: Option<Arc<VkFence>>,
        submission: Option<VkCommandBufferSubmission>,
    },
    Present {
        wait_semaphores: SmallVec<[VkSemaphoreSignalOrWait; 4]>,
        image_index: u32,
        swapchain: Arc<VkSwapchain>,
        frame: u64,
    },
}

impl VkQueue {
    pub fn new(
        info: VkQueueInfo,
        queue_type: VkQueueType,
        device: &Arc<RawVkDevice>,
        shared: &Arc<VkShared>,
        threads: &Arc<VkThreadManager>,
    ) -> Self {
        Self {
            info,
            queue: Mutex::new(VkQueueInner {
                virtual_queue: Vec::new(),
                signalled_semaphores: SmallVec::new(),
            }),
            device: device.clone(),
            shared: shared.clone(),
            threads: threads.clone(),
            queue_type,
        }
    }

    pub fn family_index(&self) -> u32 {
        self.info.queue_family_index as u32
    }

    pub fn supports_presentation(&self) -> bool {
        self.info.supports_presentation
    }

    pub fn submit_transfer(&self, command_buffer: &VkTransferCommandBuffer) {
        debug_assert!(!command_buffer.fence().is_signalled());
        debug_assert_eq!(
            command_buffer.queue_family_index(),
            self.info.queue_family_index as u32
        );

        let vk_cmd_buffer = *command_buffer.handle();
        let submission = VkVirtualSubmission::CommandBuffer {
            command_buffer: vk_cmd_buffer,
            wait_semaphores: SmallVec::new(),
            signal_semaphores: SmallVec::new(),
            fence: Some(command_buffer.fence().clone()),
            submission: None,
        };
        let mut guard = self.queue.lock().unwrap();
        guard.virtual_queue.push(submission);
    }

    pub fn submit(
        &self,
        command_buffer: VkCommandBufferSubmission,
        fence: Option<&Arc<VkFence>>,
        wait_semaphores: &[&Arc<VkSemaphore>],
        signal_semaphores: &[&Arc<VkSemaphore>],
    ) {
        assert_eq!(
            command_buffer.command_buffer_type(),
            CommandBufferType::Primary
        );
        debug_assert_eq!(
            command_buffer.queue_family_index(),
            self.info.queue_family_index as u32
        );
        debug_assert!(fence.is_none() || !fence.unwrap().is_signalled());
        if wait_semaphores.len() > 4 || signal_semaphores.len() > 4 {
            panic!("Can only signal and wait for 4 semaphores each.");
        }

        let mut cmd_buffer_mut = command_buffer;
        cmd_buffer_mut.mark_submitted();

        let wait_semaphores = wait_semaphores
            .iter()
            .map(|semaphore| VkSemaphoreSignalOrWait {
                semaphore: (*semaphore).clone(),
                stage: vk::PipelineStageFlags2::ALL_COMMANDS,
            })
            .collect::<SmallVec<[VkSemaphoreSignalOrWait; 4]>>();
        let signal_semaphores = signal_semaphores
            .iter()
            .map(|semaphore| VkSemaphoreSignalOrWait {
                semaphore: (*semaphore).clone(),
                stage: vk::PipelineStageFlags2::ALL_COMMANDS,
            })
            .collect::<SmallVec<[VkSemaphoreSignalOrWait; 4]>>();

        let vk_cmd_buffer = *cmd_buffer_mut.handle();
        let submission = VkVirtualSubmission::CommandBuffer {
            command_buffer: vk_cmd_buffer,
            wait_semaphores,
            signal_semaphores,
            fence: fence.cloned(),
            submission: Some(cmd_buffer_mut),
        };

        let mut guard = self.queue.lock().unwrap();
        guard.virtual_queue.push(submission);
    }

    pub fn present(
        &self,
        swapchain: &Arc<VkSwapchain>,
        image_index: u32,
        wait_semaphores: &[&Arc<VkSemaphore>],
    ) {
        if wait_semaphores.len() > 4 {
            panic!("Can only wait for 4 semaphores.");
        }

        let wait_semaphores = wait_semaphores
            .iter()
            .map(|semaphore| VkSemaphoreSignalOrWait {
                semaphore: (*semaphore).clone(),
                stage: vk::PipelineStageFlags2::ALL_COMMANDS,
            })
            .collect::<SmallVec<[VkSemaphoreSignalOrWait; 4]>>();

        let frame = self.threads.end_frame();
        let submission = VkVirtualSubmission::Present {
            wait_semaphores,
            image_index,
            swapchain: swapchain.clone(),
            frame,
        };
        let mut guard = self.queue.lock().unwrap();
        guard.virtual_queue.push(submission);
    }

    fn lock_queue(&self) -> ReentrantMutexGuard<vk::Queue> {
        match self.queue_type {
            VkQueueType::Graphics => self.device.graphics_queue(),
            VkQueueType::Compute => self.device.compute_queue().unwrap(),
            VkQueueType::Transfer => self.device.transfer_queue().unwrap(),
        }
    }

    pub(crate) fn wait_for_idle(&self) {
        self.process_submissions();
        let _queue_guard = self.queue.lock().unwrap();
        let queue = self.lock_queue();
        unsafe {
            self.device.queue_wait_idle(*queue).unwrap();
        }
    }
}

impl Queue<VkBackend> for VkQueue {
    fn create_command_buffer(&self) -> VkCommandBufferRecorder {
        self.threads
            .get_thread_local()
            .get_frame_local()
            .get_command_buffer()
    }

    fn submit(
        &self,
        submission: VkCommandBufferSubmission,
        fence: Option<&Arc<VkFence>>,
        wait_semaphores: &[&Arc<VkSemaphore>],
        signal_semaphores: &[&Arc<VkSemaphore>],
        delayed: bool,
    ) {
        let frame_local = self.threads.get_thread_local().get_frame_local();

        if let Some(fence) = fence {
            frame_local.track_fence(fence);
        }

        for semaphore in wait_semaphores {
            frame_local.track_semaphore(semaphore);
        }

        for semaphore in signal_semaphores {
            frame_local.track_semaphore(semaphore);
        }

        self.submit(submission, fence, wait_semaphores, signal_semaphores);

        if !delayed {
            self.process_submissions();
        }
    }

    fn present(
        &self,
        swapchain: &Arc<VkSwapchain>,
        wait_semaphores: &[&Arc<VkSemaphore>],
        delayed: bool,
    ) {
        let frame_local = self.threads.get_thread_local().get_frame_local();
        for sem in wait_semaphores {
            frame_local.track_semaphore(*sem);
        }
        self.present(swapchain, swapchain.acquired_image(), wait_semaphores);

        if !delayed {
            self.process_submissions();
        }
    }

    fn create_inner_command_buffer(
        &self,
        inheritance: &VkInnerCommandBufferInfo,
    ) -> VkCommandBufferRecorder {
        self.threads
            .get_thread_local()
            .get_frame_local()
            .get_inner_command_buffer(Some(inheritance))
    }

    fn process_submissions(&self) {
        let mut guard = self.queue.lock().unwrap();
        if guard.virtual_queue.is_empty() {
            return;
        }

        if !self.device.is_alive.load(Ordering::SeqCst) {
            guard.virtual_queue.clear();
            return;
        }

        let mut command_buffers = SmallVec::<[vk::CommandBufferSubmitInfo; 32]>::new();
        let mut batch = SmallVec::<[vk::SubmitInfo2; 8]>::new();
        let mut submissions = SmallVec::<[VkCommandBufferSubmission; 8]>::new();
        let mut semaphores = SmallVec::<[vk::SemaphoreSubmitInfo; 8]>::new();
        let vk_queue = self.lock_queue();
        for submission in guard.virtual_queue.drain(..) {
            let mut append = false;
            match submission {
                VkVirtualSubmission::CommandBuffer {
                    command_buffer,
                    wait_semaphores,
                    signal_semaphores,
                    fence,
                    submission,
                } => {
                    if fence.is_none() && wait_semaphores.is_empty() && signal_semaphores.is_empty()
                    {
                        if let Some(last_info) = batch.last_mut() {
                            if last_info.wait_semaphore_info_count == 0
                                && last_info.signal_semaphore_info_count == 0
                                && command_buffers.len() < command_buffers.capacity()
                            {
                                command_buffers.push(vk::CommandBufferSubmitInfo {
                                    command_buffer,
                                    device_mask: 0,
                                    ..Default::default()
                                });
                                last_info.command_buffer_info_count += 1;
                                append = true;
                            }
                        }
                    }

                    let p_signal_semaphores = unsafe { semaphores.as_ptr().add(semaphores.len()) };
                    for semaphore in &signal_semaphores {
                        assert!(semaphores.len() < semaphores.capacity());
                        semaphores.push(vk::SemaphoreSubmitInfo {
                            semaphore: *semaphore.semaphore.handle(),
                            value: 0,
                            stage_mask: semaphore.stage,
                            device_index: 0,
                            ..Default::default()
                        });
                    }

                    let p_wait_semaphores = unsafe { semaphores.as_ptr().add(semaphores.len()) };
                    for semaphore in &wait_semaphores {
                        assert!(semaphores.len() < semaphores.capacity());
                        semaphores.push(vk::SemaphoreSubmitInfo {
                            semaphore: *semaphore.semaphore.handle(),
                            value: 0,
                            stage_mask: semaphore.stage,
                            device_index: 0,
                            ..Default::default()
                        });
                    }

                    if !append {
                        if let Some(fence) = fence {
                            if !batch.is_empty() {
                                unsafe {
                                    let result = self.device.synchronization2.queue_submit2(
                                        *vk_queue,
                                        &batch,
                                        vk::Fence::null(),
                                    );
                                    if result.is_err() {
                                        self.device.is_alive.store(true, Ordering::SeqCst);
                                        self.device.queue_wait_idle(*vk_queue).unwrap();
                                        panic!("Submit failed: {:?}", result);
                                    }
                                }
                                batch.clear();
                                submissions.clear();
                                command_buffers.clear();
                                semaphores.clear();
                            }

                            let command_buffer_info = vk::CommandBufferSubmitInfo {
                                command_buffer,
                                device_mask: 0,
                                ..Default::default()
                            };

                            let submit = vk::SubmitInfo2 {
                                flags: vk::SubmitFlags::empty(),
                                wait_semaphore_info_count: wait_semaphores.len() as u32,
                                p_wait_semaphore_infos: p_wait_semaphores,
                                command_buffer_info_count: 1,
                                p_command_buffer_infos: &command_buffer_info
                                    as *const vk::CommandBufferSubmitInfo,
                                signal_semaphore_info_count: signal_semaphores.len() as u32,
                                p_signal_semaphore_infos: p_signal_semaphores,
                                ..Default::default()
                            };

                            fence.mark_submitted();
                            let fence_handle = fence.handle();
                            unsafe {
                                let result = self.device.synchronization2.queue_submit2(
                                    *vk_queue,
                                    &[submit],
                                    *fence_handle,
                                );
                                if result.is_err() {
                                    self.device.is_alive.store(true, Ordering::SeqCst);
                                    self.device.queue_wait_idle(*vk_queue).unwrap();
                                    panic!("Submit failed: {:?}", result);
                                }
                            }
                        } else {
                            if batch.len() == batch.capacity() {
                                unsafe {
                                    let result = self.device.synchronization2.queue_submit2(
                                        *vk_queue,
                                        &batch,
                                        vk::Fence::null(),
                                    );
                                    if result.is_err() {
                                        self.device.is_alive.store(true, Ordering::SeqCst);
                                        self.device.queue_wait_idle(*vk_queue).unwrap();
                                        panic!("Submit failed: {:?}", result);
                                    }
                                }
                                submissions.clear();
                                batch.clear();
                                command_buffers.clear();
                                semaphores.clear();
                            }

                            command_buffers.push(vk::CommandBufferSubmitInfo {
                                command_buffer,
                                device_mask: 0,
                                ..Default::default()
                            });
                            let submit = vk::SubmitInfo2 {
                                flags: vk::SubmitFlags::empty(),
                                wait_semaphore_info_count: wait_semaphores.len() as u32,
                                p_wait_semaphore_infos: p_wait_semaphores,
                                command_buffer_info_count: 1,
                                p_command_buffer_infos: unsafe {
                                    command_buffers.as_ptr().add(command_buffers.len() - 1)
                                },
                                signal_semaphore_info_count: signal_semaphores.len() as u32,
                                p_signal_semaphore_infos: p_signal_semaphores,
                                ..Default::default()
                            };
                            if let Some(submission) = submission {
                                submissions.push(submission);
                            }
                            batch.push(submit);
                        }
                    }
                }

                VkVirtualSubmission::Present {
                    wait_semaphores,
                    image_index,
                    swapchain,
                    frame,
                } => {
                    if !batch.is_empty() {
                        unsafe {
                            let result = self.device.synchronization2.queue_submit2(
                                *vk_queue,
                                &batch,
                                vk::Fence::null(),
                            );
                            if result.is_err() {
                                self.device.is_alive.store(true, Ordering::SeqCst);
                                self.device.queue_wait_idle(*vk_queue).unwrap();
                                panic!("Submit failed: {:?}", result);
                            }
                        }
                        submissions.clear();
                        batch.clear();
                        command_buffers.clear();
                        semaphores.clear();
                    }

                    let mut wait_semaphore_handles = SmallVec::<[vk::Semaphore; 4]>::new();
                    let p_wait_semaphores = wait_semaphore_handles.as_ptr();
                    for semaphore in &wait_semaphores {
                        assert!(wait_semaphores.len() < wait_semaphore_handles.capacity());
                        wait_semaphore_handles.push(*semaphore.semaphore.handle());
                    }

                    let swapchain_handle = swapchain.handle();
                    let present_info = vk::PresentInfoKHR {
                        p_swapchains: &*swapchain_handle,
                        swapchain_count: 1,
                        p_image_indices: &image_index as *const u32,
                        p_wait_semaphores,
                        wait_semaphore_count: wait_semaphores.len() as u32,
                        ..Default::default()
                    };
                    unsafe {
                        let result = swapchain.loader().queue_present(*vk_queue, &present_info);
                        swapchain.set_presented_image(image_index);
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
                                    self.device.is_alive.store(true, Ordering::SeqCst);
                                    self.device.queue_wait_idle(*vk_queue).unwrap();
                                    panic!("Present failed: {:?}", err);
                                }
                            },
                        }
                        self.device
                            .synchronization2
                            .queue_submit2(
                                *vk_queue,
                                &[vk::SubmitInfo2 {
                                    wait_semaphore_info_count: 0,
                                    p_wait_semaphore_infos: std::ptr::null(),
                                    command_buffer_info_count: 0,
                                    p_command_buffer_infos: std::ptr::null(),
                                    signal_semaphore_info_count: 1,
                                    p_signal_semaphore_infos: &[vk::SemaphoreSubmitInfo {
                                        semaphore: *self.threads.timeline_semaphore().handle(),
                                        value: frame,
                                        stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
                                        device_index: 0,
                                        ..Default::default()
                                    }]
                                        as *const vk::SemaphoreSubmitInfo,
                                    ..Default::default()
                                }],
                                vk::Fence::null(),
                            )
                            .unwrap();
                    }
                }
            }
        }

        if !batch.is_empty() {
            unsafe {
                let result = self.device.synchronization2.queue_submit2(
                    *vk_queue,
                    &batch,
                    vk::Fence::null(),
                );
                if result.is_err() {
                    self.device.is_alive.store(true, Ordering::SeqCst);
                    panic!("Submit failed: {:?}", result);
                }
            }
        }
    }
}

// Vulkan queues are implicitly freed with the logical device
