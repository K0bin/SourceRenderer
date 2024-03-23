use std::sync::{Arc, Mutex};

use metal;
use block::ConcreteBlock;

use sourcerenderer_core::gpu;

use super::*;

struct PresentState {
    swapchain_release_scheduled: bool,
    present_called: bool,
    drawable: Option<metal::MetalDrawable>
}

pub struct MTLQueue {
    queue: metal::CommandQueue,
    present_state: Arc<Mutex<PresentState>>
}

impl MTLQueue {
    pub(crate) fn new(device: &metal::Device) -> Self {
        let queue = device.new_command_queue();
        Self {
            queue,
            present_state: Arc::new(Mutex::new(PresentState {
                swapchain_release_scheduled: false,
                present_called: false,
                drawable: None
            }))
        }

    }
    pub(crate) fn handle(&self) -> &metal::CommandQueue {
        &self.queue
    }
}

impl gpu::Queue<MTLBackend> for MTLQueue {
    unsafe fn create_command_pool(&self, command_pool_type: gpu::CommandPoolType, flags: gpu::CommandPoolFlags) -> MTLCommandPool {
        MTLCommandPool::new(&self.queue)
    }

    unsafe fn submit(&self, submissions: &[gpu::Submission<MTLBackend>]) {
        for submission in submissions {
            if let Some(swapchain) = submission.acquire_swapchain {
                let mut present_state = self.present_state.lock().unwrap();
                present_state.drawable = None;
                present_state.swapchain_release_scheduled = false;
                present_state.present_called = false;
            }

            if let Some(swapchain) = submission.release_swapchain {
                if let Some(cmd_buffer) = submission.command_buffers.last() {
                    {
                        let mut present_state = self.present_state.lock().unwrap();
                        assert_eq!(present_state.present_called, false);
                        assert_eq!(present_state.swapchain_release_scheduled, false);
                        assert!(present_state.drawable.is_none());

                        let drawable = swapchain.take_drawable();
                        present_state.drawable = Some(drawable);
                    }
                    let c_present_state = self.present_state.clone();
                    let callback = |cmd_buffer| {
                        let mut present_state = self.present_state.lock().unwrap();
                        assert_eq!(present_state.swapchain_release_scheduled, false);
                        assert!(present_state.drawable.is_some());
                        if present_state.present_called {
                            // Command Buffer was scheduled after the application called present()
                            present_state.drawable.take().unwrap().present();
                        } else {
                            // Command Buffer was scheduled before the application called present()
                            present_state.swapchain_release_scheduled = true;
                        }
                    };
                    let callback_block = ConcreteBlock::new(callback);
                    cmd_buffer.handle().add_scheduled_handler(&callback_block);
                }
            }
            for cmd_buf in submission.command_buffers {
                cmd_buf.handle().commit();
            }
        }
    }

    unsafe fn present(&self, swapchain: &MTLSwapchain) {
        let mut present_state = self.present_state.lock().unwrap();
        assert_eq!(present_state.present_called, false);
        if present_state.drawable.is_none() {
            // No submission used the swapchain with release_swapchain
            swapchain.take_drawable().present();
            return;
        }

        if present_state.swapchain_release_scheduled {
            // Command Buffer was scheduled before the application called present()
            present_state.drawable.take().unwrap().present();
        } else {
            // Command Buffer was scheduled after the application called present()
            present_state.present_called = true;
        }
    }
}
