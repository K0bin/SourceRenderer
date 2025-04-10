use std::{collections::{HashSet, VecDeque}, ffi::c_void, sync::Arc};
use crate::Mutex;

use sourcerenderer_core::{gpu::{CommandBuffer as _, CommandPool as _, Queue as _, Texture as _}, Vec3UI};
use super::gpu;

use super::*;

const DEBUG_FORCE_FAT_BARRIER: bool = false;

pub(crate) struct Transfer {
  device: Arc<active_gpu_backend::Device>,
  buffer_allocator: Arc<BufferAllocator>,
  inner: Mutex<TransferInner>,
}

pub enum OwnedBarrier {
  TextureBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_layout: TextureLayout,
    new_layout: TextureLayout,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    texture: Arc<Texture>,
    range: BarrierTextureRange,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
  BufferBarrier {
    old_sync: BarrierSync,
    new_sync: BarrierSync,
    old_access: BarrierAccess,
    new_access: BarrierAccess,
    buffer: Arc<BufferSlice>,
    offset: u64,
    length: u64,
    queue_ownership: Option<QueueOwnershipTransfer>
  },
}

enum TransferCopy {
  BufferToImage {
      src: Arc<BufferSlice>,
      dst: Arc<Texture>,
      region: BufferTextureCopyRegion
  },
  BufferToBuffer {
      src: Arc<BufferSlice>,
      dst: Arc<BufferSlice>,
      region: BufferCopyRegion
  },
}

struct TransferInner {
  graphics: TransferCommands,
  transfer: Option<TransferCommands>,
}

struct TransferCommands {
  pre_barriers: Vec<OwnedBarrier>,
  copies: Vec<TransferCopy>,
  post_barriers: Vec<(Option<SharedFenceValuePair>, OwnedBarrier)>,
  used_cmd_buffers: VecDeque<Box<TransferCommandBuffer>>,
  pool: active_gpu_backend::CommandPool,
  fence_value: SharedFenceValuePair,
  used_buffers_slices: Vec<Arc<BufferSlice>>,
  used_textures: Vec<Arc<super::Texture>>
}

pub struct TransferCommandBuffer {
  cmd_buffer: active_gpu_backend::CommandBuffer,
  fence_value: SharedFenceValuePair,
  is_used: bool,
  used_buffers_slices: Vec<Arc<BufferSlice>>,
  used_textures: Vec<Arc<super::Texture>>
}

impl Transfer {
    pub(super) fn new(device: &Arc<active_gpu_backend::Device>, destroyer: &Arc<DeferredDestroyer>, buffer_allocator: &Arc<BufferAllocator>) -> Self {
        let graphics_fence = Arc::new(super::Fence::new(device.as_ref(), destroyer));
        let graphics_pool = unsafe { device.graphics_queue().create_command_pool(gpu::CommandPoolType::CommandBuffers, gpu::CommandPoolFlags::INDIVIDUAL_RESET) };

        let transfer_commands = device.transfer_queue().map(|transfer_queue| {
            let transfer_fence = Arc::new(super::Fence::new(device.as_ref(), destroyer));
            let transfer_pool = unsafe { transfer_queue.create_command_pool(gpu::CommandPoolType::CommandBuffers, gpu::CommandPoolFlags::INDIVIDUAL_RESET) };
            TransferCommands {
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

        let graphics_commands = TransferCommands {
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
            inner: Mutex::new(TransferInner {
                graphics: graphics_commands,
                transfer: transfer_commands,
            })
        }
    }

    pub fn init_texture_from_buffer(
      &self,
      texture: &Arc<Texture>,
      src_buffer: &Arc<BufferSlice>,
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
          BufferTextureCopyRegion {
                buffer_offset: buffer_offset + src_buffer.offset(),
                buffer_row_pitch: 0u64,
                buffer_slice_pitch: 0u64,
                texture_subresource: TextureSubresource {
                  array_layer, mip_level
                },
                texture_offset: Vec3UI::new(0u32, 0u32, 0u32),
                texture_extent: Vec3UI::new(texture.info().width, texture.info().height, texture.info().depth),
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
      src_buffer: &Arc<BufferSlice>,
      dst_buffer: &Arc<BufferSlice>,
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
        region: BufferCopyRegion {
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
      dst_buffer: &Arc<BufferSlice>,
      dst_offset: u64,
    ) -> Result<(), OutOfMemoryError> {
      debug_assert_ne!(data.len(), 0);

      // Try to copy directly if possible
      if self.copy_to_host_visible_buffer(data, dst_buffer, dst_offset) {
        return Ok(());
      }

      let src_buffer = self.upload_data(data, dst_buffer.length() - dst_offset, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
      self.init_buffer_from_buffer(&src_buffer, dst_buffer, 0, dst_offset, data.len() as u64);
      Ok(())
    }

    pub fn init_buffer_box(
      &self,
      data: Box<[u8]>,
      dst_buffer: &Arc<BufferSlice>,
      dst_offset: u64,
    ) -> Result<(), OutOfMemoryError> {
      debug_assert_ne!(data.len(), 0);

      // Try to copy directly if possible
      if self.copy_to_host_visible_buffer(&data, dst_buffer, dst_offset) {
        return Ok(());
      }

      let src_buffer = self.upload_data(&data, dst_buffer.length() - dst_offset, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
      self.init_buffer_from_buffer(&src_buffer, dst_buffer, 0, dst_offset, data.len() as u64);
      Ok(())
    }

    pub fn init_texture(
      &self,
      data: &[u8],
      texture: &Arc<Texture>,
      mip_level: u32,
      array_layer: u32,
      do_async: bool
    ) -> Result<Option<SharedFenceValuePair>, OutOfMemoryError> {
      unsafe {
        if texture.handle().can_be_written_directly() {
          self.copy_to_host_visible_texture(data, texture, mip_level, array_layer);
          return Ok(None);
        }
      }

      let src_buffer = self.upload_data(data, 0, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
      if !do_async {
        self.init_texture_from_buffer(texture, &src_buffer, mip_level, array_layer, 0);
        Ok(None)
      } else {
        let fence_pair_opt = self.init_texture_from_buffer_async(texture, &src_buffer, mip_level, array_layer, 0);
        Ok(fence_pair_opt)
      }
    }

    pub fn init_texture_box(
      &self,
      data: Box<[u8]>,
      texture: &Arc<Texture>,
      mip_level: u32,
      array_layer: u32,
      do_async: bool
    ) -> Result<Option<SharedFenceValuePair>, OutOfMemoryError> {
      unsafe {
        if texture.handle().can_be_written_directly() {
          self.copy_to_host_visible_texture(&data, texture, mip_level, array_layer);
          return Ok(None);
        }
      }

      let src_buffer = self.upload_data(&data, 0, MemoryUsage::MainMemoryWriteCombined, BufferUsage::COPY_SRC)?;
      if !do_async {
        self.init_texture_from_buffer(texture, &src_buffer, mip_level, array_layer, 0);
        Ok(None)
      } else {
        let fence_pair_opt = self.init_texture_from_buffer_async(texture, &src_buffer, mip_level, array_layer, 0);
        Ok(fence_pair_opt)
      }
    }

    pub fn copy_to_host_visible_texture(
      &self,
      data: &[u8],
      texture: &Arc<Texture>,
      mip_level: u32,
      array_layer: u32
    ) {
      unsafe {
        self.device.transition_texture(texture.handle(), &gpu::CPUTextureTransition {
          old_layout: TextureLayout::Undefined,
          new_layout: TextureLayout::Sampled,
          texture: texture.handle(),
          range: BarrierTextureRange {
              base_mip_level: 0,
              mip_level_length: texture.info().mip_levels,
              base_array_layer: 0,
              array_layer_length: texture.info().array_length,
          }
        });
        self.device.copy_to_texture(data.as_ptr() as *const c_void, texture.handle(), TextureLayout::Sampled, &MemoryTextureCopyRegion {
          row_pitch: 0,
          slice_pitch: 0,
          texture_subresource: TextureSubresource {
            array_layer, mip_level
          },
          texture_offset: Vec3UI::new(0u32, 0u32, 0u32),
          texture_extent: Vec3UI::new(texture.info().width, texture.info().height, texture.info().depth),
        });
      }
    }

    fn copy_to_host_visible_buffer(
      &self,
      data: &[u8],
      dst_buffer: &Arc<BufferSlice>,
      dst_offset: u64
    ) -> bool {
      unsafe {
        let actual_len = data.len().min(dst_buffer.length() as usize - dst_offset as usize);
        let dst_ptr = dst_buffer.map_part(dst_offset, actual_len as u64, false);
        if let Some(ptr_void) = dst_ptr {
          let ptr = ptr_void as *mut u8;
          ptr.copy_from(data.as_ptr(), actual_len);
          dst_buffer.unmap_part(dst_offset, actual_len as u64, true);
          return true;
        }
      }
      return false;
    }

    pub fn init_texture_from_buffer_async(
      &self,
      texture: &Arc<super::Texture>,
      src_buffer: &Arc<BufferSlice>,
      mip_level: u32,
      array_layer: u32,
      buffer_offset: u64
    ) -> Option<SharedFenceValuePair> {
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
                BufferTextureCopyRegion {
                    buffer_offset: buffer_offset + src_buffer.offset(),
                    buffer_row_pitch: 0u64,
                    buffer_slice_pitch: 0u64,
                    texture_subresource: TextureSubresource {
                      array_layer, mip_level
                    },
                    texture_offset: Vec3UI::new(0u32, 0u32, 0u32),
                    texture_extent: Vec3UI::new(texture.info().width, texture.info().height, texture.info().depth),
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
      commands: &mut TransferCommands
    ) -> Option<Box<TransferCommandBuffer>> {
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
                TransferCommandBuffer::new(
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
        let mut barriers = Vec::<active_gpu_backend::Barrier>::with_capacity(commands.pre_barriers.len());
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
        let mut barriers = Vec::<active_gpu_backend::Barrier>::with_capacity(commands.pre_barriers.len());
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

    fn upload_data<T>(&self, data: &[T], length: u64, memory_usage: MemoryUsage, usage: BufferUsage) -> Result<Arc<BufferSlice>, OutOfMemoryError> {
      let required_size = std::mem::size_of_val(data);
      assert_ne!(required_size, 0);
      let size = align_up(
        if length == 0 { required_size } else { required_size.min(length as usize) },
        256
      );

      let slice = self.buffer_allocator.get_slice(&BufferInfo {
          size: size as u64,
          usage,
          sharing_mode: QueueSharingMode::Concurrent
      }, memory_usage, None)?;

      unsafe {
          let ptr_void = slice.map(false).unwrap();

          if required_size < size {
              let ptr_u8 = (ptr_void as *mut u8).offset(required_size as isize);
              std::ptr::write_bytes(ptr_u8, 0u8, size - required_size);
          }

          if required_size != 0 {
              let ptr = ptr_void as *mut u8;
              ptr.copy_from(data.as_ptr() as *const u8, required_size);
          }

          slice.unmap(true);
      }
      Ok(slice)
  }

    pub fn flush(&self) {
        self.try_free_unused_buffers();

        let mut guard = self.inner.lock().unwrap();
        if let Some(transfer) = guard.transfer.as_mut() {
            let cmd_buffer_opt: Option<Box<TransferCommandBuffer>> = self.flush_commands(transfer);
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

    fn fat_barrier(cmd_buffer: &mut active_gpu_backend::CommandBuffer) {
        let fat_core_barrier = [
            gpu::Barrier::GlobalBarrier { old_sync: BarrierSync::all(), new_sync: BarrierSync::all(), old_access: BarrierAccess::MEMORY_WRITE, new_access: BarrierAccess::MEMORY_READ | BarrierAccess::MEMORY_WRITE }
        ];

        unsafe {
            cmd_buffer.barrier(&fat_core_barrier);
        }
    }
}

impl TransferCommandBuffer {
    pub(super) fn new(
        _device: &Arc<active_gpu_backend::Device>,
        pool: &mut active_gpu_backend::CommandPool,
        fence_value: &SharedFenceValuePair
    ) -> Self {
        let cmd_buffer = unsafe { pool.create_command_buffer() };

        Self {
            cmd_buffer,
            fence_value: fence_value.clone(),
            is_used: false,
            used_buffers_slices: Vec::new(),
            used_textures: Vec::new()
        }
    }

    #[inline(always)]
    pub(super) fn mark_used(&mut self) {
        self.is_used = true;
    }

    #[inline(always)]
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

    #[allow(unused)]
    #[inline(always)]
    pub(super) fn handle(&self) -> &active_gpu_backend::CommandBuffer {
        &self.cmd_buffer
    }

    #[allow(unused)]
    #[inline(always)]
    pub(super) fn fence_value(&self) -> &SharedFenceValuePair {
        &self.fence_value
    }
}

impl Drop for TransferCommandBuffer {
    fn drop(&mut self) {
        if self.is_used {
            unsafe {
                self.fence_value.await_signal();
                self.cmd_buffer.reset(0u64);
            }
        }
    }
}
