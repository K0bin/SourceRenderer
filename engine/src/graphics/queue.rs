use std::{sync::{Arc, Mutex}, mem::ManuallyDrop, collections::VecDeque};

use crossbeam_channel::Sender;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{*, Queue as GPUQueue};

use super::*;

enum StoredQueueSubmission<B: GPUBackend> {
    CommandBuffer {
        command_buffer: Box<super::CommandBuffer<B>>,
        return_sender: Sender<Box<super::CommandBuffer<B>>>,
        signal_swapchain: Option<Arc<B::Swapchain>>,
        wait_swapchain: Option<Arc<B::Swapchain>>,
        signal_fences: SmallVec<[SharedFenceValuePair<B>; 4]>,
        wait_fences: SmallVec<[SharedFenceValuePair<B>; 4]>,
    },
    TransferCommandBuffer {
        command_buffer: ManuallyDrop<Box<TransferCommandBuffer<B>>>,
        return_sender: Sender<Box<TransferCommandBuffer<B>>>
    }
}

pub struct QueueSubmission<'a, B: GPUBackend> {
    command_buffer: FinishedCommandBuffer<B>,
    wait_fences: &'a [SharedFenceValuePairRef<'a, B>],
    signal_fences: &'a [SharedFenceValuePairRef<'a, B>],
    acquire_swapchain: Option<&'a Arc<B::Swapchain>>,
    release_swapchain: Option<&'a Arc<B::Swapchain>>,
}

struct QueueInner<B: GPUBackend> {
    virtual_queue: VecDeque<StoredQueueSubmission<B>>
}

pub struct Queue<B: GPUBackend> {
    device: Arc<B::Device>,
    inner: Mutex<QueueInner<B>>,
    queue_type: QueueType
}

impl<B: GPUBackend> Queue<B> {
    pub fn submit(&self, submission: QueueSubmission<B>) {
        let mut guard = self.inner.lock().unwrap();

        let QueueSubmission { command_buffer: finished_cmd_buffer, wait_fences, signal_fences, acquire_swapchain, release_swapchain } = submission;
        let FinishedCommandBuffer { inner, sender } = finished_cmd_buffer;

        guard.virtual_queue.push_back(StoredQueueSubmission::CommandBuffer {
            command_buffer: inner,
            return_sender: sender,
            signal_fences: signal_fences.iter().map(|fence_ref| SharedFenceValuePair::<B>::from(fence_ref)).collect(),
            wait_fences: signal_fences.iter().map(|fence_ref| SharedFenceValuePair::<B>::from(fence_ref)).collect(),
            signal_swapchain: release_swapchain.cloned(),
            wait_swapchain: acquire_swapchain.cloned()
        });
    }

    pub fn process_submissions(&self) {
        let mut guard = self.inner.lock().unwrap();

        let queue = match self.queue_type {
            QueueType::Graphics => self.device.graphics_queue(),
            QueueType::Compute => self.device.compute_queue().unwrap(),
            QueueType::Transfer => self.device.transfer_queue().unwrap(),
        };

        let mut command_buffers: SmallVec<[&mut B::CommandBuffer; 16]> = SmallVec::new();
        let mut submissions: SmallVec<[Submission<B>; 4]> = SmallVec::new();

        for submission in guard.virtual_queue.iter_mut() {
            match submission {
                StoredQueueSubmission::CommandBuffer { command_buffer, return_sender } => {
                    submissions.push(Submission {
                        command_buffers: &mut [command_buffer.handle_mut()],
                        wait_fences: todo!(),
                        signal_fences: todo!(),
                    });
                },
                StoredQueueSubmission::TransferCommandBuffer { command_buffer, return_sender } => todo!(),
            }
        }

        for submission in guard.virtual_queue.drain(..) {
            match submission {
                StoredQueueSubmission::CommandBuffer { command_buffer, return_sender } => {
                    return_sender.send(command_buffer).unwrap();
                },
                StoredQueueSubmission::TransferCommandBuffer { command_buffer, return_sender } => todo!(),
            }
        }
    }
}