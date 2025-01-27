use std::{collections::{HashSet, VecDeque}, sync::{Arc, Mutex}};

use sourcerenderer_core::{gpu::{CommandBuffer as _, CommandPool as _, Queue as _, Texture as _}, Vec3UI};
use sourcerenderer_core::gpu;

use super::*;

const DEBUG_FORCE_FAT_BARRIER: bool = false;

pub(crate) struct Transfer<B: GPUBackend> {
  device: Arc<B::Device>,
  buffer_allocator: Arc<BufferAllocator<B>>,
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
    texture: Arc<Texture<B>>,
    range: BarrierTextureRange,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
  BufferBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    buffer: Arc<BufferSlice<B>>,
    offset: u64,
    length: u64,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
}

enum TransferCopy<B: GPUBackend> {
  BufferToImage {
      src: Arc<BufferSlice<B>>,
      dst: Arc<Texture<B>>,
      region: gpu::BufferTextureCopyRegion
  },
  BufferToBuffer {
      src: Arc<BufferSlice<B>>,
      dst: Arc<BufferSlice<B>>,
      region: gpu::BufferCopyRegion
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
    pub(super) fn new(device: &Arc<B::Device>, destroyer: &Arc<DeferredDestroyer<B>>, buffer_allocator: &Arc<BufferAllocator<B>>) -> Self {
        let graphics_fence = Arc::new(super::Fence::new(device.as_ref(), destroyer));
        let graphics_pool = unsafe { device.graphics_queue().create_command_pool(gpu::CommandPoolType::CommandBuffers, gpu::CommandPoolFlags::INDIVIDUAL_RESET) };

        let transfer_commands = device.transfer_queue().map(|transfer_queue| {
            let transfer_fence = Arc::new(super::Fence::new(device.as_ref(), destroyer));
            let transfer_pool = unsafe { transfer_queue.create_command_pool(gpu::CommandPoolType::CommandBuffers, gpu::CommandPoolFlags::INDIVIDUAL_RESET) };
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
            buffer_allocator: buffer_allocator.clone(),
            inner: Mutex::new(TransferInner::<B> {
                graphics: graphics_commands,
                transfer: transfer_commands,
            })
        }
    }

    pub fn init_texture_from_buffer(
      &self,
      texture: &Arc<Texture<B>>,
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
          src: src_buffer.clone(),
          dst: texture.clone(),
          region:
          gpu::BufferTextureCopyRegion {
                buffer_offset: buffer_offset + src_buffer.offset(),
                buffer_row_pitch: 0u64,
                buffer_slice_pitch: 0u64,
                texture_subresource: gpu::TextureSubresource {
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

    pub fn init_buffer_from_buffer(
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
        src: src_buffer.clone(),
        dst: dst_buffer.clone(),
        region: gpu::BufferCopyRegion {
          src_offset: src_offset + src_buffer.offset(),
          dst_offset: dst_offset + dst_buffer.offset(),
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
          new_access: BarrierAccess::MEMORY_READ | BarrierAccess::MEMORY_WRITE,
          buffer: dst_buffer.clone(),
          offset: dst_offset + dst_buffer.offset(),
          length: actual_length,
          queue_ownership: None
      }));

      guard.graphics.used_buffers_slices.push(src_buffer.clone());
      guard.graphics.used_buffers_slices.push(dst_buffer.clone());
    }

    pub fn init_buffer(
      &self,
      data: &[u8],
      dst_buffer: &Arc<BufferSlice<B>>,
      dst_offset: u64,
    ) {
      debug_assert_ne!(data.len(), 0);

      // Try to copy directly if possible
      if self.copy_to_host_visible_buffer(data, dst_buffer, dst_offset) {
        return;
      }

      let src_buffer = self.upload_data(data, dst_buffer.length() - dst_offset, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC).unwrap();
      self.init_buffer_from_buffer(&src_buffer, dst_buffer, 0, dst_offset, data.len() as u64);
    }

    pub fn init_buffer_owned(
      &self,
      data: Box<[u8]>,
      dst_buffer: &Arc<BufferSlice<B>>,
      dst_offset: u64,
    ) {
      debug_assert_ne!(data.len(), 0);

      // Try to copy directly if possible
      if self.copy_to_host_visible_buffer(&data, dst_buffer, dst_offset) {
        return;
      }

      let src_buffer = self.upload_data(&data, dst_buffer.length() - dst_offset, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC).unwrap();
      self.init_buffer_from_buffer(&src_buffer, dst_buffer, 0, dst_offset, data.len() as u64);
    }

    fn copy_to_host_visible_buffer(
      &self,
      data: &[u8],
      dst_buffer: &Arc<BufferSlice<B>>,
      dst_offset: u64
    ) -> bool {
      unsafe {
        let dst_ptr = dst_buffer.map(false);
        if let Some(ptr_void) = dst_ptr {
          let actual_len = data.len().min(dst_buffer.length() as usize - dst_offset as usize);
          let ptr = ptr_void as *mut u8;
          ptr.offset(dst_offset as isize).copy_from(data.as_ptr(), actual_len);
          dst_buffer.unmap(true);
          return true;
        }
      }
      return false;
    }

    pub fn init_texture_from_buffer_async(
      &self,
      texture: &Arc<super::Texture<B>>,
      src_buffer: &Arc<BufferSlice<B>>,
      mip_level: u32,
      array_layer: u32,
      buffer_offset: u64
    ) -> Option<SharedFenceValuePair<B>> {
      let mut guard = self.inner.lock().unwrap();
      if guard.transfer.is_none() || DEBUG_FORCE_FAT_BARRIER {
        std::mem::drop(guard);
        self.init_texture_from_buffer(texture, src_buffer, mip_level, array_layer, buffer_offset);
        return None;
      }

      let fence_value_pair = {
        let transfer = guard.transfer.as_mut().unwrap();
        transfer.used_buffers_slices.push(src_buffer.clone());
        transfer.used_textures.push(texture.clone());

        debug_assert!(!transfer.fence_value.is_signalled());
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
              src: src_buffer.clone(),
              dst: texture.clone(),
              region:
                gpu::BufferTextureCopyRegion {
                    buffer_offset: buffer_offset + src_buffer.offset(),
                    buffer_row_pitch: 0u64,
                    buffer_slice_pitch: 0u64,
                    texture_subresource: gpu::TextureSubresource {
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
      guard.graphics.used_textures.push(texture.clone());

      Some(fence_value_pair)
    }

    pub fn try_free_unused_buffers(&self) {
        let mut guard = self.inner.lock().unwrap();
        let mut signalled_counter: u64 = 0u64;
        for cmd_buffer in &mut guard.graphics.used_cmd_buffers {
            if cmd_buffer.fence_value.is_signalled() {
                signalled_counter = signalled_counter.max(cmd_buffer.fence_value.value);
                cmd_buffer.reset();
            }
        }
        if let Some(transfer) = guard.transfer.as_mut() {
            signalled_counter = 0u64;
            for cmd_buffer in &mut transfer.used_cmd_buffers {
                if cmd_buffer.fence_value.is_signalled() {
                    signalled_counter = signalled_counter.max(cmd_buffer.fence_value.value);
                    cmd_buffer.reset();
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
                        .all(|(fence, _)| fence.as_ref().map_or(false, |f| !f.is_signalled())))
            {
                return None;
            }

        let reuse_first_graphics_buffer = commands
            .used_cmd_buffers
            .front()
            .map(|cmd_buffer| cmd_buffer.fence_value.is_signalled())
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
          cmd_buffer.cmd_buffer.begin(0u64, None);
        }

        if DEBUG_FORCE_FAT_BARRIER {
            Self::fat_barrier(&mut cmd_buffer.cmd_buffer);
        }

        // commit pre barriers
        let mut barriers = Vec::<gpu::Barrier<B>>::with_capacity(commands.pre_barriers.len());
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
                    } => gpu::Barrier::BufferBarrier {
                        old_sync: *old_sync,
                        new_sync: *new_sync,
                        old_access: *old_access,
                        new_access: *new_access,
                        buffer: buffer.handle(),
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
                  } => gpu::Barrier::TextureBarrier {
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
            if DEBUG_FORCE_FAT_BARRIER {
                Self::fat_barrier(&mut cmd_buffer.cmd_buffer);
            }

            match copy {
                TransferCopy::BufferToBuffer {
                    src,
                    dst,
                    region
                } => {
                    unsafe {
                        cmd_buffer.cmd_buffer.copy_buffer(src.handle(), dst.handle(), &region);
                    }
                }

                TransferCopy::BufferToImage {
                    src,
                    dst,
                    region
                } => {
                    unsafe  {
                        cmd_buffer.cmd_buffer.copy_buffer_to_texture(src.handle(), dst.handle(), &region);
                    }
                }
            }

            if DEBUG_FORCE_FAT_BARRIER {
                Self::fat_barrier(&mut cmd_buffer.cmd_buffer);
            }
        }

        // commit post barriers
        let mut barriers = Vec::<gpu::Barrier<B>>::with_capacity(commands.pre_barriers.len());
        let mut retained_barrier_indices = HashSet::<u32>::new();
        for (index, (fence_opt, barrier)) in commands.post_barriers.iter().enumerate() {
            if let Some(fence) = fence_opt {
                if !fence.is_signalled() {
                    retained_barrier_indices.insert(index as u32);
                    continue;
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
                    } => gpu::Barrier::BufferBarrier {
                        old_sync: *old_sync,
                        new_sync: *new_sync,
                        old_access: *old_access,
                        new_access: *new_access,
                        buffer: buffer.handle(),
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
                    } => gpu::Barrier::TextureBarrier {
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
        let mut index = 0u32;
        commands.post_barriers.retain(|_| {
          let keep = retained_barrier_indices.contains(&index);
          index += 1;
          keep
        });

        unsafe {
            cmd_buffer.cmd_buffer.finish();
        }

        cmd_buffer.fence_value.value = commands.fence_value.value;
        cmd_buffer.mark_used();
        commands.fence_value.value += 1;

        Some(cmd_buffer)
    }

    fn upload_data<T>(&self, data: &[T], length: u64, memory_usage: MemoryUsage, usage: BufferUsage) -> Result<Arc<BufferSlice<B>>, OutOfMemoryError> {
      let required_size = std::mem::size_of_val(data) as u64;
      assert_ne!(required_size, 0u64);
      let size = align_up_64(required_size.max(length), 256u64);

      let slice = self.buffer_allocator.get_slice(&BufferInfo {
          size,
          usage,
          sharing_mode: QueueSharingMode::Concurrent
      }, memory_usage, None)?;

      unsafe {
          let ptr_void = slice.map(false).unwrap();

          if required_size < size {
              let ptr_u8 = (ptr_void as *mut u8).offset(required_size as isize);
              std::ptr::write_bytes(ptr_u8, 0u8, (size - required_size) as usize);
          }

          let ptr = ptr_void as *mut T;
          ptr.copy_from(data.as_ptr(), data.len());

          slice.unmap(true);
      }
      Ok(slice)
  }

    pub fn flush(&self) {
        self.try_free_unused_buffers();

        let mut guard = self.inner.lock().unwrap();
        if let Some(transfer) = guard.transfer.as_mut() {
            let cmd_buffer_opt: Option<Box<TransferCommandBuffer<B>>> = self.flush_commands(transfer);
            if let Some(mut cmd_buffer) = cmd_buffer_opt {
                unsafe {
                    self.device.transfer_queue()
                        .as_ref()
                        .unwrap()
                        .submit(&mut [gpu::Submission {
                            command_buffers: &mut [&mut cmd_buffer.cmd_buffer],
                            wait_fences: &[],
                            signal_fences: &[cmd_buffer.fence_value.as_handle_ref()],
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
                self.device.graphics_queue().submit(&mut [gpu::Submission {
                    command_buffers: &mut [&mut cmd_buffer.cmd_buffer],
                    signal_fences: &[cmd_buffer.fence_value.as_handle_ref()],
                    wait_fences: &[],
                    acquire_swapchain: None,
                    release_swapchain: None
                }]);
            }
            guard.graphics.used_cmd_buffers.push_back(cmd_buffer);
        }
    }

    fn fat_barrier(cmd_buffer: &mut B::CommandBuffer) {
        let fat_core_barrier = [
            gpu::Barrier::GlobalBarrier { old_sync: gpu::BarrierSync::all(), new_sync: gpu::BarrierSync::all(), old_access: gpu::BarrierAccess::MEMORY_WRITE, new_access: gpu::BarrierAccess::MEMORY_READ | gpu::BarrierAccess::MEMORY_WRITE }
        ];

        unsafe {
            cmd_buffer.barrier(&fat_core_barrier);
        }
    }
}

impl<B: GPUBackend> TransferCommandBuffer<B> {
    pub(super) fn new(
        device: &Arc<B::Device>,
        pool: &mut B::CommandPool,
        fence_value: &SharedFenceValuePair<B>
    ) -> Self {
        let cmd_buffer = unsafe { pool.create_command_buffer() };

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
