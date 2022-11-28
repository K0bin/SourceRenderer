use std::cmp::max;
use std::collections::VecDeque;
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::sync::{
    Arc,
    Mutex,
};

use ash::vk;
use ash::vk::Handle;
use sourcerenderer_core::graphics::{
    Queue,
    Texture,
    WHOLE_BUFFER,
};

use crate::buffer::VkBufferSlice;
use crate::raw::{
    RawVkCommandPool,
    RawVkDevice,
};
use crate::{
    VkFence,
    VkLifetimeTrackers,
    VkQueue,
    VkShared,
    VkTexture,
};

pub(crate) struct VkTransfer {
    inner: Mutex<VkTransferInner>,
    transfer_queue: Option<Arc<VkQueue>>,
    graphics_queue: Arc<VkQueue>,
    device: Arc<RawVkDevice>,
    shared: Arc<VkShared>,
}

enum VkTransferBarrier {
    Image(vk::ImageMemoryBarrier),
    Buffer(vk::BufferMemoryBarrier),
}

unsafe impl Send for VkTransferBarrier {}
unsafe impl Sync for VkTransferBarrier {}

enum VkTransferCopy {
    BufferToImage {
        src: Arc<VkBufferSlice>,
        dst: Arc<VkTexture>,
        region: vk::BufferImageCopy,
    },
    BufferToBuffer {
        src: Arc<VkBufferSlice>,
        dst: Arc<VkBufferSlice>,
        region: vk::BufferCopy,
    },
}

unsafe impl Send for VkTransferCopy {}
unsafe impl Sync for VkTransferCopy {}

struct VkTransferInner {
    graphics: VkTransferCommands,
    transfer: Option<VkTransferCommands>,
}

struct VkTransferCommands {
    pre_barriers: Vec<VkTransferBarrier>,
    copies: Vec<VkTransferCopy>,
    post_barriers: Vec<(Option<Arc<VkFence>>, VkTransferBarrier)>,
    used_cmd_buffers: VecDeque<Box<VkTransferCommandBuffer>>,
    pool: Arc<RawVkCommandPool>,
    fence: Arc<VkFence>,
    queue_name: &'static str,
    queue_family_index: u32,
}

impl VkTransfer {
    pub fn new(
        device: &Arc<RawVkDevice>,
        graphics_queue: &Arc<VkQueue>,
        transfer_queue: &Option<Arc<VkQueue>>,
        shared: &Arc<VkShared>,
    ) -> Self {
        let graphics_pool_info = vk::CommandPoolCreateInfo {
            flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER
                | vk::CommandPoolCreateFlags::TRANSIENT,
            queue_family_index: graphics_queue.family_index(),
            ..Default::default()
        };
        let graphics_fence = shared.get_fence();
        let graphics_pool = Arc::new(RawVkCommandPool::new(device, &graphics_pool_info).unwrap());

        let transfer_commands = if let Some(transfer_queue) = transfer_queue {
            let transfer_pool_info = vk::CommandPoolCreateInfo {
                flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER
                    | vk::CommandPoolCreateFlags::TRANSIENT,
                queue_family_index: transfer_queue.family_index(),
                ..Default::default()
            };
            let transfer_pool =
                Arc::new(RawVkCommandPool::new(device, &transfer_pool_info).unwrap());
            let transfer_fence = shared.get_fence();
            Some(VkTransferCommands {
                pool: transfer_pool,
                pre_barriers: Vec::new(),
                copies: Vec::new(),
                post_barriers: Vec::new(),
                used_cmd_buffers: VecDeque::new(),
                fence: transfer_fence,
                queue_name: "Transfer",
                queue_family_index: transfer_queue.family_index(),
            })
        } else {
            None
        };

        Self {
            inner: Mutex::new(VkTransferInner {
                graphics: VkTransferCommands {
                    pre_barriers: Vec::new(),
                    copies: Vec::new(),
                    post_barriers: Vec::new(),
                    pool: graphics_pool,
                    used_cmd_buffers: VecDeque::new(),
                    fence: graphics_fence,
                    queue_name: "Graphics",
                    queue_family_index: graphics_queue.family_index(),
                },
                transfer: transfer_commands,
            }),
            transfer_queue: transfer_queue.clone(),
            graphics_queue: graphics_queue.clone(),
            device: device.clone(),
            shared: shared.clone(),
        }
    }

    pub fn init_texture(
        &self,
        texture: &Arc<VkTexture>,
        src_buffer: &Arc<VkBufferSlice>,
        mip_level: u32,
        array_layer: u32,
        buffer_offset: usize,
    ) {
        let mut guard = self.inner.lock().unwrap();
        guard
            .graphics
            .pre_barriers
            .push(VkTransferBarrier::Image(vk::ImageMemoryBarrier {
                src_access_mask: vk::AccessFlags::empty(),
                dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                src_queue_family_index: self.graphics_queue.family_index(),
                dst_queue_family_index: self.graphics_queue.family_index(),
                subresource_range: vk::ImageSubresourceRange {
                    base_mip_level: mip_level,
                    level_count: 1,
                    base_array_layer: array_layer,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    layer_count: 1,
                },
                image: *texture.handle(),
                ..Default::default()
            }));

        guard.graphics.copies.push(VkTransferCopy::BufferToImage {
            src: src_buffer.clone(),
            dst: texture.clone(),
            region: vk::BufferImageCopy {
                buffer_offset: (src_buffer.offset() + buffer_offset) as u64,
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_extent: vk::Extent3D {
                    width: max(texture.info().width >> mip_level, 1),
                    height: max(texture.info().height >> mip_level, 1),
                    depth: max(texture.info().depth >> mip_level, 1),
                },
                image_subresource: vk::ImageSubresourceLayers {
                    mip_level,
                    base_array_layer: array_layer,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    layer_count: 1,
                },
            },
        });

        guard.graphics.post_barriers.push((
            None,
            VkTransferBarrier::Image(vk::ImageMemoryBarrier {
                src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                src_queue_family_index: self.graphics_queue.family_index(),
                dst_queue_family_index: self.graphics_queue.family_index(),
                subresource_range: vk::ImageSubresourceRange {
                    base_mip_level: mip_level,
                    level_count: 1,
                    base_array_layer: array_layer,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    layer_count: 1,
                },
                image: *texture.handle(),
                ..Default::default()
            }),
        ));
    }

    pub fn init_buffer(
        &self,
        src_buffer: &Arc<VkBufferSlice>,
        dst_buffer: &Arc<VkBufferSlice>,
        src_offset: usize,
        dst_offset: usize,
        length: usize,
    ) {
        debug_assert_ne!(length, 0);

        let mut guard = self.inner.lock().unwrap();
        guard.graphics.copies.push(VkTransferCopy::BufferToBuffer {
            src: src_buffer.clone(),
            dst: dst_buffer.clone(),
            region: vk::BufferCopy {
                src_offset: (src_buffer.offset() + src_offset) as vk::DeviceSize,
                dst_offset: (dst_buffer.offset() + dst_offset) as vk::DeviceSize,
                size: if length == WHOLE_BUFFER {
                    (src_buffer.length() as vk::DeviceSize - src_offset as vk::DeviceSize)
                        .min(dst_buffer.length() as vk::DeviceSize - dst_offset as vk::DeviceSize)
                } else {
                    length as vk::DeviceSize
                },
            },
        });

        guard.graphics.post_barriers.push((
            None,
            VkTransferBarrier::Buffer(vk::BufferMemoryBarrier {
                src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                src_queue_family_index: self.graphics_queue.family_index(),
                dst_queue_family_index: self.graphics_queue.family_index(),
                buffer: *dst_buffer.buffer().handle(),
                offset: dst_buffer.offset() as vk::DeviceSize,
                size: if length == WHOLE_BUFFER {
                    (src_buffer.length() as vk::DeviceSize - src_offset as vk::DeviceSize)
                        .min(dst_buffer.length() as vk::DeviceSize - dst_offset as vk::DeviceSize)
                } else {
                    length as vk::DeviceSize
                },
                ..Default::default()
            }),
        ));
    }

    pub fn init_texture_async(
        &self,
        texture: &Arc<VkTexture>,
        src_buffer: &Arc<VkBufferSlice>,
        mip_level: u32,
        array_layer: u32,
        buffer_offset: usize,
    ) -> Option<Arc<VkFence>> {
        let mut guard = self.inner.lock().unwrap();
        if guard.transfer.is_none() {
            std::mem::drop(guard);
            self.init_texture(texture, src_buffer, mip_level, array_layer, buffer_offset);
            return None;
        }

        let fence = {
            let transfer = guard.transfer.as_mut().unwrap();
            debug_assert!(!transfer.fence.is_signalled());
            transfer
                .pre_barriers
                .push(VkTransferBarrier::Image(vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::empty(),
                    dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    old_layout: vk::ImageLayout::UNDEFINED,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    src_queue_family_index: self.transfer_queue.as_ref().unwrap().family_index(),
                    dst_queue_family_index: self.transfer_queue.as_ref().unwrap().family_index(),
                    subresource_range: vk::ImageSubresourceRange {
                        base_mip_level: mip_level,
                        level_count: 1,
                        base_array_layer: array_layer,
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        layer_count: 1,
                    },
                    image: *texture.handle(),
                    ..Default::default()
                }));

            transfer.copies.push(VkTransferCopy::BufferToImage {
                src: src_buffer.clone(),
                dst: texture.clone(),
                region: vk::BufferImageCopy {
                    buffer_offset: (src_buffer.offset() + buffer_offset) as u64,
                    image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_extent: vk::Extent3D {
                        width: max(texture.info().width >> mip_level, 1),
                        height: max(texture.info().height >> mip_level, 1),
                        depth: max(texture.info().depth >> mip_level, 1),
                    },
                    image_subresource: vk::ImageSubresourceLayers {
                        mip_level,
                        base_array_layer: array_layer,
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        layer_count: 1,
                    },
                },
            });

            // release
            transfer.post_barriers.push((
                None,
                VkTransferBarrier::Image(vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags::empty(),
                    old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    src_queue_family_index: self.transfer_queue.as_ref().unwrap().family_index(),
                    dst_queue_family_index: self.graphics_queue.family_index(),
                    subresource_range: vk::ImageSubresourceRange {
                        base_mip_level: mip_level,
                        level_count: 1,
                        base_array_layer: array_layer,
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        layer_count: 1,
                    },
                    image: *texture.handle(),
                    ..Default::default()
                }),
            ));

            transfer.fence.clone()
        };

        // acquire
        guard.graphics.post_barriers.push((
            Some(fence.clone()),
            VkTransferBarrier::Image(vk::ImageMemoryBarrier {
                src_access_mask: vk::AccessFlags::empty(),
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                src_queue_family_index: self.transfer_queue.as_ref().unwrap().family_index(),
                dst_queue_family_index: self.graphics_queue.family_index(),
                subresource_range: vk::ImageSubresourceRange {
                    base_mip_level: mip_level,
                    level_count: 1,
                    base_array_layer: array_layer,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    layer_count: 1,
                },
                image: *texture.handle(),
                ..Default::default()
            }),
        ));

        Some(fence)
    }

    pub fn try_free_used_buffers(&self) {
        let mut did_free_buffer = false;

        let mut guard = self.inner.lock().unwrap();
        for cmd_buffer in &mut guard.graphics.used_cmd_buffers {
            if cmd_buffer.fence.is_signalled() {
                let new_fence = self.shared.get_fence();
                cmd_buffer.reset(&new_fence);
                did_free_buffer = true;
            }
        }
        if let Some(transfer) = guard.transfer.as_mut() {
            for cmd_buffer in &mut transfer.used_cmd_buffers {
                if cmd_buffer.fence.is_signalled() {
                    let new_fence = self.shared.get_fence();
                    cmd_buffer.reset(&new_fence);
                    did_free_buffer = true;
                }
            }
        }

        if did_free_buffer && false {
            unsafe {
                let mut stats = MaybeUninit::<vma_sys::VmaTotalStatistics>::uninit();
                vma_sys::vmaCalculateStatistics(self.device.allocator, stats.as_mut_ptr());
                let stats = stats.assume_init();
                println!("Freed transfer command buffers");
                println!(
                    "Total memory usage: Allocated: {} MiB, Used: {} MiB",
                    stats.total.statistics.blockBytes >> 20,
                    stats.total.statistics.allocationBytes >> 20
                );
                for i in 0..stats.memoryHeap.len() {
                    let heap = &stats.memoryHeap[i];
                    if heap.statistics.blockBytes != 0 {
                        println!(
                            "Heap {}: Allocated: {} MiB, Used: {} Mib",
                            i,
                            heap.statistics.blockBytes >> 20,
                            heap.statistics.allocationBytes >> 20
                        );
                    }
                }
            }
        }
    }

    fn flush_commands(
        &self,
        commands: &mut VkTransferCommands,
    ) -> Option<Box<VkTransferCommandBuffer>> {
        if commands.copies.is_empty()
            && (commands.post_barriers.is_empty()
                || commands
                    .post_barriers
                    .iter()
                    .all(|(fence, _)| fence.as_ref().map_or(false, |f| !f.is_signalled())))
        {
            return None;
        }

        let reuse_first_graphics_buffer = commands
            .used_cmd_buffers
            .front()
            .map(|cmd_buffer| cmd_buffer.fence.is_signalled())
            .unwrap_or(false);
        let mut cmd_buffer = if reuse_first_graphics_buffer {
            let mut cmd_buffer = commands.used_cmd_buffers.pop_front().unwrap();
            cmd_buffer.reset(&commands.fence);
            cmd_buffer
        } else {
            Box::new({
                VkTransferCommandBuffer::new(
                    &self.device,
                    &commands.pool,
                    &commands.fence,
                    commands.queue_name,
                    commands.queue_family_index,
                )
            })
        };
        debug_assert!(!cmd_buffer.is_used());
        unsafe {
            self.device
                .begin_command_buffer(
                    *cmd_buffer.handle(),
                    &vk::CommandBufferBeginInfo {
                        flags: vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                        ..Default::default()
                    },
                )
                .unwrap();
        }

        // commit pre barriers
        let mut image_barriers = Vec::<vk::ImageMemoryBarrier>::new();
        let mut buffer_barriers = Vec::<vk::BufferMemoryBarrier>::new();
        for barrier in commands.pre_barriers.drain(..) {
            match barrier {
                VkTransferBarrier::Buffer(buffer_memory_barrier) => {
                    buffer_barriers.push(buffer_memory_barrier);
                }
                VkTransferBarrier::Image(image_memory_barrier) => {
                    image_barriers.push(image_memory_barrier);
                }
            }
        }
        unsafe {
            self.device.cmd_pipeline_barrier(
                *cmd_buffer.handle(),
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &buffer_barriers,
                &image_barriers,
            );
        }

        // commit copies
        for copy in commands.copies.drain(..) {
            match copy {
                VkTransferCopy::BufferToBuffer { src, dst, region } => {
                    cmd_buffer.trackers.track_buffer(&src);
                    cmd_buffer.trackers.track_buffer(&dst);
                    unsafe {
                        self.device.cmd_copy_buffer(
                            *cmd_buffer.handle(),
                            *src.buffer().handle(),
                            *dst.buffer().handle(),
                            &[region],
                        );
                    }
                }
                VkTransferCopy::BufferToImage { src, dst, region } => {
                    cmd_buffer.trackers.track_buffer(&src);
                    cmd_buffer.trackers.track_texture(&dst);
                    unsafe {
                        self.device.cmd_copy_buffer_to_image(
                            *cmd_buffer.handle(),
                            *src.buffer().handle(),
                            *dst.handle(),
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            &[region],
                        );
                    }
                }
            }
        }

        // commit post barriers
        image_barriers.clear();
        buffer_barriers.clear();
        let mut retained_barriers = Vec::<(Option<Arc<VkFence>>, VkTransferBarrier)>::new();
        for (fence, barrier) in commands.post_barriers.drain(..) {
            if let Some(fence) = fence {
                if !fence.is_signalled() {
                    retained_barriers.push((Some(fence), barrier));
                    continue;
                }
            }
            match barrier {
                VkTransferBarrier::Buffer(buffer_memory_barrier) => {
                    buffer_barriers.push(buffer_memory_barrier);
                }
                VkTransferBarrier::Image(image_memory_barrier) => {
                    image_barriers.push(image_memory_barrier);
                }
            }
        }
        commands.post_barriers.extend(retained_barriers);
        unsafe {
            self.device.cmd_pipeline_barrier(
                *cmd_buffer.handle(),
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &buffer_barriers,
                &image_barriers,
            );
        }

        unsafe {
            self.device
                .end_command_buffer(*cmd_buffer.handle())
                .unwrap();
        }

        cmd_buffer.mark_used();
        commands.fence = self.shared.get_fence();

        Some(cmd_buffer)
    }

    pub fn flush(&self) {
        self.try_free_used_buffers();

        let mut guard = self.inner.lock().unwrap();
        if let Some(transfer) = guard.transfer.as_mut() {
            let cmd_buffer_opt = self.flush_commands(transfer);
            if let Some(cmd_buffer) = cmd_buffer_opt {
                self.transfer_queue
                    .as_ref()
                    .unwrap()
                    .submit_transfer(&cmd_buffer);
                transfer.used_cmd_buffers.push_back(cmd_buffer);
            }
        }

        let cmd_buffer_opt = self.flush_commands(&mut guard.graphics);
        if let Some(cmd_buffer) = cmd_buffer_opt {
            self.graphics_queue.submit_transfer(&cmd_buffer);
            guard.graphics.used_cmd_buffers.push_back(cmd_buffer);
        }

        let c_graphics_queue = self.graphics_queue.clone();
        let c_transfer_queue = self.transfer_queue.clone();
        rayon::spawn(move || {
            c_graphics_queue.process_submissions();
            if let Some(transfer_queue) = c_transfer_queue {
                transfer_queue.process_submissions();
            }
        });
    }
}

impl Drop for VkTransfer {
    fn drop(&mut self) {
        // The queue keeps handles to transfer command buffers, so we need to make sure it doesn't
        // submit them to the Vulkan queue after we drop them.
        if let Some(queue) = self.transfer_queue.as_ref() {
            queue.wait_for_idle();
        }
        self.graphics_queue.wait_for_idle();
    }
}

pub struct VkTransferCommandBuffer {
    cmd_buffer: vk::CommandBuffer,
    device: Arc<RawVkDevice>,
    trackers: VkLifetimeTrackers,
    fence: Arc<VkFence>,
    is_used: bool,
    queue_family_index: u32,
}

impl VkTransferCommandBuffer {
    pub(super) fn new(
        device: &Arc<RawVkDevice>,
        pool: &Arc<RawVkCommandPool>,
        fence: &Arc<VkFence>,
        queue_name: &str,
        queue_family_index: u32,
    ) -> Self {
        debug_assert!(!fence.is_signalled());
        let buffer_info = vk::CommandBufferAllocateInfo {
            command_pool: ***pool,
            level: vk::CommandBufferLevel::PRIMARY,
            command_buffer_count: 1,
            ..Default::default()
        };
        let cmd_buffer = unsafe { device.allocate_command_buffers(&buffer_info) }
            .unwrap()
            .pop()
            .unwrap();

        let mut name_string = "TransferCommandBuffer".to_string();
        name_string += queue_name;
        if let Some(debug_utils) = device.instance.debug_utils.as_ref() {
            let name_cstring = CString::new(name_string).unwrap();
            unsafe {
                debug_utils
                    .debug_utils_loader
                    .debug_utils_set_object_name(
                        device.handle(),
                        &vk::DebugUtilsObjectNameInfoEXT {
                            object_type: vk::ObjectType::COMMAND_BUFFER,
                            object_handle: cmd_buffer.as_raw(),
                            p_object_name: name_cstring.as_ptr(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
        }

        Self {
            cmd_buffer,
            device: device.clone(),
            fence: fence.clone(),
            trackers: VkLifetimeTrackers::new(),
            is_used: false,
            queue_family_index,
        }
    }

    #[inline]
    pub(crate) fn handle(&self) -> &vk::CommandBuffer {
        &self.cmd_buffer
    }

    #[inline]
    pub(crate) fn fence(&self) -> &Arc<VkFence> {
        &self.fence
    }

    pub(super) fn mark_used(&mut self) {
        self.is_used = true;
    }

    pub(super) fn is_used(&self) -> bool {
        self.is_used
    }

    pub(super) fn queue_family_index(&self) -> u32 {
        self.queue_family_index
    }

    pub(super) fn reset(&mut self, fence: &Arc<VkFence>) {
        if !self.is_used {
            return;
        }
        debug_assert!(self.fence.is_signalled());
        debug_assert!(!fence.is_signalled());
        self.fence = fence.clone();
        unsafe {
            self.device
                .reset_command_buffer(
                    self.cmd_buffer,
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )
                .unwrap();
        }
        self.trackers.reset();
        self.is_used = false;
    }
}

impl Drop for VkTransferCommandBuffer {
    fn drop(&mut self) {
        if !self.trackers.is_empty() || self.is_used {
            self.fence.await_signal();
        }
    }
}
