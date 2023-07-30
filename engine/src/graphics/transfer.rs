use std::{sync::{Mutex, Arc}, collections::VecDeque};

use sourcerenderer_core::{gpu::*, Vec3UI};

use super::*;

pub(crate) struct Transfer<B: GPUBackend> {
  device: Arc<B::Device>,
  inner: Mutex<TransferInner<B>>,
}

pub enum OwnedBarrier<B: GPUBackend> {
  TextureBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_layout: TextureLayout,
    new_layout: TextureLayout,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    texture: Arc<super::Texture<B>>,
    range: BarrierTextureRange,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
  BufferBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    buffer: Arc<BufferAndAllocation<B>>,
    offset: u64,
    length: u64,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
}

enum TransferCopy<B: GPUBackend> {
  BufferToImage {
      src: Arc<BufferAndAllocation<B>>,
      dst: Arc<super::Texture<B>>,
      region: BufferTextureCopyRegion
  },
  BufferToBuffer {
      src: Arc<BufferAndAllocation<B>>,
      dst: Arc<BufferAndAllocation<B>>,
      region: BufferCopyRegion
  },
}

struct TransferInner<B: GPUBackend> {
  graphics: TransferCommands<B>,
  transfer: Option<TransferCommands<B>>,
}

struct TransferCommands<B: GPUBackend> {
  pre_barriers: Vec<OwnedBarrier<B>>,
  copies: Vec<TransferCopy<B>>,
  post_barriers: Vec<(Option<SharedFenceValuePair<B>>, OwnedBarrier<B>)>,
  used_cmd_buffers: VecDeque<Box<TransferCommandBuffer<B>>>,
  pool: B::CommandPool,
  fence_value: SharedFenceValuePair<B>,
  used_buffers_slices: Vec<Arc<BufferSlice<B>>>,
  used_textures: Vec<Arc<super::Texture<B>>>
}

pub struct TransferCommandBuffer<B: GPUBackend> {
  cmd_buffer: B::CommandBuffer,
  device: Arc<B::Device>,
  fence_value: SharedFenceValuePair<B>,
  is_used: bool,
  used_buffers_slices: Vec<Arc<BufferSlice<B>>>,
  used_textures: Vec<Arc<super::Texture<B>>>
}

impl<B: GPUBackend> Transfer<B> {
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>) -> Self {
        let graphics_fence = Arc::new(super::Fence::new(device.as_ref(), destroyer));
        let graphics_pool = unsafe { device.graphics_queue().create_command_pool(CommandPoolType::CommandBuffers, CommandPoolFlags::INDIVIDUAL_RESET) };

        let transfer_commands = device.transfer_queue().map(|transfer_queue| {
            let transfer_fence = Arc::new(super::Fence::new(device.as_ref(), destroyer));
            let transfer_pool = unsafe { transfer_queue.create_command_pool(CommandPoolType::CommandBuffers, CommandPoolFlags::INDIVIDUAL_RESET) };
            TransferCommands::<B> {
                pre_barriers: Vec::new(),
                copies: Vec::new(),
                post_barriers: Vec::new(),
                used_cmd_buffers: VecDeque::new(),
                pool: transfer_pool,
                fence_value: SharedFenceValuePair {
                  fence: transfer_fence,
                  value: 1u64,
                  sync_before: BarrierSync::COPY
                },
                used_buffers_slices: Vec::new(),
                used_textures: Vec::new()
            }
        });

        let graphics_commands = TransferCommands::<B> {
            pre_barriers: Vec::new(),
            copies: Vec::new(),
            post_barriers: Vec::new(),
            used_cmd_buffers: VecDeque::new(),
            pool: graphics_pool,
            fence_value: SharedFenceValuePair {
              fence: graphics_fence,
              value: 1u64,
              sync_before: BarrierSync::COPY
            },
            used_buffers_slices: Vec::new(),
            used_textures: Vec::new()
        };

        Self {
            device: device.clone(),
            inner: Mutex::new(TransferInner::<B> {
                graphics: graphics_commands,
                transfer: transfer_commands,
            })
        }
    }

    pub fn init_texture(
      &self,
      texture: &Arc<super::Texture<B>>,
      src_buffer: &Arc<BufferSlice<B>>,
      mip_level: u32,
      array_layer: u32,
      buffer_offset: u64
    ) {
      let mut guard = self.inner.lock().unwrap();
      guard
        .graphics
        .pre_barriers
        .push(OwnedBarrier::TextureBarrier {
          old_sync: BarrierSync::empty(),
          new_sync: BarrierSync::COPY,
          old_layout: TextureLayout::Undefined,
          new_layout: TextureLayout::CopyDst,
          old_access: BarrierAccess::empty(),
          new_access: BarrierAccess::COPY_WRITE,
          texture: texture.clone(),
          range: BarrierTextureRange {
            base_mip_level: mip_level,
            mip_level_length: 1,
            base_array_layer: array_layer,
            array_layer_length: 1
          },
          queue_ownership: None
      });

      guard.graphics.copies.push(
        TransferCopy::BufferToImage {
          src: src_buffer.inner_arc().clone(),
          dst: texture.clone(),
          region:
            BufferTextureCopyRegion {
                buffer_offset: buffer_offset + src_buffer.offset(),
                buffer_row_pitch: 0u64,
                buffer_slice_pitch: 0u64,
                texture_subresource: TextureSubresource {
                  array_layer, mip_level
                },
                texture_offset: Vec3UI::new(0u32, 0u32, 0u32),
                texture_extent: Vec3UI::new(texture.handle().info().width, texture.handle().info().height, texture.handle().info().depth),
            }
          }
      );

      guard
        .graphics
        .post_barriers
        .push((None, OwnedBarrier::TextureBarrier {
          old_sync: BarrierSync::COPY,
          new_sync: BarrierSync::all(),
          old_layout: TextureLayout::CopyDst,
          new_layout: TextureLayout::Sampled,
          old_access: BarrierAccess::COPY_WRITE,
          new_access: BarrierAccess::SHADER_READ,
          texture: texture.clone(),
          range: BarrierTextureRange {
            base_mip_level: mip_level,
            mip_level_length: 1,
            base_array_layer: array_layer,
            array_layer_length: 1
          },
          queue_ownership: None
      }));

      guard.graphics.used_buffers_slices.push(src_buffer.clone());
      guard.graphics.used_textures.push(texture.clone());
    }

    pub fn init_buffer(
      &self,
      src_buffer: &Arc<BufferSlice<B>>,
      dst_buffer: &Arc<BufferSlice<B>>,
      src_offset: u64,
      dst_offset: u64,
      length: u64
    ) {
      debug_assert_ne!(length, 0);

      let actual_length = length.min(src_buffer.length() - src_offset).min(dst_buffer.length() - dst_offset);

      let mut guard = self.inner.lock().unwrap();
      guard.graphics.copies.push(TransferCopy::BufferToBuffer {
        src: src_buffer.inner_arc().clone(),
        dst: dst_buffer.inner_arc().clone(),
        region: BufferCopyRegion {
          src_offset,
          dst_offset,
          size: actual_length
        }
      });

      guard
        .graphics
        .post_barriers
        .push((None, OwnedBarrier::BufferBarrier {
          old_sync: BarrierSync::COPY,
          new_sync: BarrierSync::all(),
          old_access: BarrierAccess::COPY_WRITE,
          new_access: BarrierAccess::SHADER_READ,
          buffer: dst_buffer.inner_arc().clone(),
          offset: dst_offset + dst_buffer.offset(),
          length: actual_length,
          queue_ownership: None
      }));

      guard.graphics.used_buffers_slices.push(src_buffer.clone());
      guard.graphics.used_buffers_slices.push(dst_buffer.clone());
    }

    pub fn init_texture_async(
      &self,
      texture: &Arc<super::Texture<B>>,
      src_buffer: &Arc<BufferSlice<B>>,
      mip_level: u32,
      array_layer: u32,
      buffer_offset: u64
    ) -> Option<SharedFenceValuePair<B>> {
      let mut guard = self.inner.lock().unwrap();
      if guard.transfer.is_none() {
        std::mem::drop(guard);
        self.init_texture(texture, src_buffer, mip_level, array_layer, buffer_offset);
        return None;
      }

      let fence_value_pair = {
        let transfer = guard.transfer.as_mut().unwrap();
        transfer.used_buffers_slices.push(src_buffer.clone());
        transfer.used_textures.push(texture.clone());

        unsafe {
            debug_assert!(!transfer.fence_value.is_signalled());
        }
        transfer
          .pre_barriers
          .push(OwnedBarrier::TextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::COPY,
            old_layout: TextureLayout::Undefined,
            new_layout: TextureLayout::CopyDst,
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::COPY_WRITE,
            texture: texture.clone(),
            range: BarrierTextureRange {
              base_mip_level: mip_level,
              mip_level_length: 1,
              base_array_layer: array_layer,
              array_layer_length: 1
            },
            queue_ownership: None
          });

          transfer.copies.push(
            TransferCopy::BufferToImage {
              src: src_buffer.inner_arc().clone(),
              dst: texture.clone(),
              region:
                BufferTextureCopyRegion {
                    buffer_offset: buffer_offset + src_buffer.offset(),
                    buffer_row_pitch: 0u64,
                    buffer_slice_pitch: 0u64,
                    texture_subresource: TextureSubresource {
                      array_layer, mip_level
                    },
                    texture_offset: Vec3UI::new(0u32, 0u32, 0u32),
                    texture_extent: Vec3UI::new(texture.handle().info().width, texture.handle().info().height, texture.handle().info().depth),
                }
              }
          );

          // release
          transfer.post_barriers.push((
            None,
            OwnedBarrier::TextureBarrier {
              old_sync: BarrierSync::COPY,
              new_sync: BarrierSync::empty(),
              old_access: BarrierAccess::COPY_WRITE,
              new_access: BarrierAccess::empty(),
              old_layout: TextureLayout::CopyDst,
              new_layout: TextureLayout::Sampled,
              range: BarrierTextureRange {
                base_mip_level: mip_level,
                mip_level_length: 1,
                base_array_layer: array_layer,
                array_layer_length: 1
              },
              texture: texture.clone(),
              queue_ownership: Some(QueueOwnershipTransfer {
                from: QueueType::Transfer,
                to: QueueType::Graphics
              })
            }
          ));

          transfer.fence_value.clone()
      };

      // acquire
      guard.graphics.post_barriers.push((Some(fence_value_pair.clone()),
          OwnedBarrier::TextureBarrier {
            old_sync: BarrierSync::empty(),
            new_sync: BarrierSync::all(),
            old_access: BarrierAccess::empty(),
            new_access: BarrierAccess::MEMORY_READ,
            old_layout: TextureLayout::Sampled,
            new_layout: TextureLayout::Sampled,
            range: BarrierTextureRange {
              base_mip_level: mip_level,
              mip_level_length: 1,
              base_array_layer: array_layer,
              array_layer_length: 1
            },
            texture: texture.clone(),
            queue_ownership: Some(QueueOwnershipTransfer {
              from: QueueType::Transfer,
              to: QueueType::Graphics
            })
          }
      ));
      guard.graphics.used_textures.push(texture.clone());

      Some(fence_value_pair)
    }

    pub fn try_free_unused_buffers(&self) {
        let mut guard = self.inner.lock().unwrap();
        let mut signalled_counter: u64 = 0u64;
        for cmd_buffer in &mut guard.graphics.used_cmd_buffers {
            unsafe {
                if cmd_buffer.fence_value.is_signalled() {
                    signalled_counter = signalled_counter.max(cmd_buffer.fence_value.value);
                    cmd_buffer.reset();
                }
            }
        }
        if let Some(transfer) = guard.transfer.as_mut() {
            signalled_counter = 0u64;
            for cmd_buffer in &mut transfer.used_cmd_buffers {
                unsafe {
                    if cmd_buffer.fence_value.is_signalled() {
                        signalled_counter = signalled_counter.max(cmd_buffer.fence_value.value);
                        cmd_buffer.reset();
                    }
                }
            }
        }
    }

    fn flush_commands(
      &self,
      commands: &mut TransferCommands<B>
    ) -> Option<Box<TransferCommandBuffer<B>>> {
        if commands.copies.is_empty()
                && (commands.post_barriers.is_empty()
                    || commands
                        .post_barriers
                        .iter()
                        .all(|(fence, _)| fence.as_ref().map_or(false, |f| unsafe { !f.is_signalled() })))
            {
                return None;
            }

        let reuse_first_graphics_buffer = commands
            .used_cmd_buffers
            .front()
            .map(|cmd_buffer| unsafe { cmd_buffer.fence_value.is_signalled() })
            .unwrap_or(false);
        let mut cmd_buffer = if reuse_first_graphics_buffer {
            let mut cmd_buffer = commands.used_cmd_buffers.pop_front().unwrap();
            cmd_buffer.reset();
            cmd_buffer
        } else {
            Box::new({
                TransferCommandBuffer::<B>::new(
                    &self.device,
                    &mut commands.pool,
                    &commands.fence_value
                )
            })
        };
        debug_assert!(!cmd_buffer.is_used());

        cmd_buffer.used_buffers_slices.extend(commands.used_buffers_slices.drain(..));
        cmd_buffer.used_textures.extend(commands.used_textures.drain(..));

        unsafe {
          cmd_buffer.cmd_buffer.begin(None, 0u64);
        }

        // commit pre barriers
        let mut barriers = Vec::<Barrier<B>>::with_capacity(commands.pre_barriers.len());
        for barrier in commands.pre_barriers.iter() {
            barriers.push(
                match barrier {
                    OwnedBarrier::BufferBarrier {
                        old_sync,
                        new_sync,
                        old_access,
                        new_access,
                        buffer,
                        offset,
                        length,
                        queue_ownership
                    } => Barrier::BufferBarrier {
                        old_sync: *old_sync,
                        new_sync: *new_sync,
                        old_access: *old_access,
                        new_access: *new_access,
                        buffer: &buffer.buffer,
                        offset: *offset,
                        length: *length,
                        queue_ownership: queue_ownership.clone()
                    },

                    OwnedBarrier::TextureBarrier {
                      old_sync,
                      new_sync,
                      old_access,
                      new_access,
                      old_layout,
                      new_layout,
                      texture,
                      range,
                      queue_ownership
                  } => Barrier::TextureBarrier {
                      old_sync: *old_sync,
                      new_sync: *new_sync,
                      old_access: *old_access,
                      new_access: *new_access,
                      old_layout: *old_layout,
                      new_layout: *new_layout,
                      texture: texture.handle(),
                      range: range.clone(),
                      queue_ownership: queue_ownership.clone()
                  }
              }
          );
        }
        unsafe {
            cmd_buffer.cmd_buffer.barrier(&barriers);
        }
        std::mem::drop(barriers);
        commands.pre_barriers.clear();

        // commit copies
        for copy in commands.copies.drain(..) {
            match copy {
                TransferCopy::BufferToBuffer {
                    src,
                    dst,
                    region
                } => {
                    unsafe {
                        cmd_buffer.cmd_buffer.copy_buffer(&src.buffer, &dst.buffer, &region);
                    }
                }

                TransferCopy::BufferToImage {
                    src,
                    dst,
                    region
                } => {
                    unsafe  {
                        cmd_buffer.cmd_buffer.copy_buffer_to_texture(&src.buffer, dst.handle(), &region);
                    }
                }
            }
        }

        // commit post barriers
        let mut barriers = Vec::<Barrier<B>>::with_capacity(commands.pre_barriers.len());
        for (fence_opt, barrier) in commands.post_barriers.iter() {
            if let Some(fence) = fence_opt {
                unsafe {
                    if !fence.is_signalled() {
                        continue;
                    }
                }
            }

            barriers.push(
                match barrier {
                    OwnedBarrier::BufferBarrier {
                        old_sync,
                        new_sync,
                        old_access,
                        new_access,
                        buffer,
                        offset,
                        length,
                        queue_ownership
                    } => Barrier::BufferBarrier {
                        old_sync: *old_sync,
                        new_sync: *new_sync,
                        old_access: *old_access,
                        new_access: *new_access,
                        buffer: &buffer.buffer,
                        offset: *offset,
                        length: *length,
                        queue_ownership: queue_ownership.clone()
                    },

                    OwnedBarrier::TextureBarrier {
                        old_sync,
                        new_sync,
                        old_access,
                        new_access,
                        old_layout,
                        new_layout,
                        texture,
                        range,
                        queue_ownership
                    } => Barrier::TextureBarrier {
                        old_sync: *old_sync,
                        new_sync: *new_sync,
                        old_access: *old_access,
                        new_access: *new_access,
                        old_layout: *old_layout,
                        new_layout: *new_layout,
                        texture: texture.handle(),
                        range: range.clone(),
                        queue_ownership: queue_ownership.clone()
                    }
                }
            );
        }
        unsafe {
            cmd_buffer.cmd_buffer.barrier(&barriers);
        }
        commands.post_barriers.retain(|(fence_opt, _barrier)| fence_opt.as_ref().map_or(false, |f| unsafe { !f.is_signalled() }));

        unsafe {
            cmd_buffer.cmd_buffer.finish();
        }

        cmd_buffer.fence_value.value = commands.fence_value.value;
        cmd_buffer.mark_used();
        commands.fence_value.value += 1;

        Some(cmd_buffer)
    }

    pub fn flush(&self) {
        self.try_free_unused_buffers();

        let mut guard = self.inner.lock().unwrap();
        if let Some(transfer) = guard.transfer.as_mut() {
            let cmd_buffer_opt = self.flush_commands(transfer);
            if let Some(mut cmd_buffer) = cmd_buffer_opt {
                unsafe {
                    self.device.transfer_queue()
                        .as_ref()
                        .unwrap()
                        .submit(&mut [Submission {
                            command_buffers: &mut [&mut cmd_buffer.cmd_buffer],
                            wait_fences: &[],
                            signal_fences: &[],
                            acquire_swapchain: None,
                            release_swapchain: None
                        }]);
                }
                transfer.used_cmd_buffers.push_back(cmd_buffer);
            }
        }

        let cmd_buffer_opt = self.flush_commands(&mut guard.graphics);
        if let Some(mut cmd_buffer) = cmd_buffer_opt {
            unsafe {
                self.device.graphics_queue().submit(&mut [Submission {
                    command_buffers: &mut [&mut cmd_buffer.cmd_buffer],
                    signal_fences: &[],
                    wait_fences: &[],
                    acquire_swapchain: None,
                    release_swapchain: None
                }]);
            }
            guard.graphics.used_cmd_buffers.push_back(cmd_buffer);
        }
    }
}

impl<B: GPUBackend> TransferCommandBuffer<B> {
    pub(super) fn new(
        device: &Arc<B::Device>,
        pool: &mut B::CommandPool,
        fence_value: &SharedFenceValuePair<B>
    ) -> Self {
        let cmd_buffer = unsafe { pool.create_command_buffer(None, 0u64) };

        Self {
            cmd_buffer,
            device: device.clone(),
            fence_value: fence_value.clone(),
            is_used: false,
            used_buffers_slices: Vec::new(),
            used_textures: Vec::new()
        }
    }

    pub(super) fn mark_used(&mut self) {
        self.is_used = true;
    }

    pub(super) fn is_used(&self) -> bool {
        self.is_used
    }

    pub(super) fn reset(&mut self) {
        if !self.is_used {
            return;
        }

        unsafe {
            debug_assert!(self.fence_value.is_signalled());
            self.cmd_buffer.reset(0u64);
        }
        self.is_used = false;
        self.used_buffers_slices.clear();
        self.used_textures.clear();
    }

    pub(super) fn handle(&self) -> &B::CommandBuffer {
        &self.cmd_buffer
    }

    pub(super) fn fence_value(&self) -> &SharedFenceValuePair<B> {
        &self.fence_value
    }
}

impl<B: GPUBackend> Drop for TransferCommandBuffer<B> {
    fn drop(&mut self) {
        if self.is_used {
            unsafe {
                self.fence_value.await_signal();
                self.cmd_buffer.reset(0u64);
            }
        }
    }
}
