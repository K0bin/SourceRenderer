use std::sync::{Arc, Mutex};

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::CommandPool;
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::RenderPass;
use sourcerenderer_core::graphics::RenderpassRecordingMode;
use sourcerenderer_core::graphics::Viewport;
use sourcerenderer_core::graphics::Scissor;
use sourcerenderer_core::graphics::Resettable;

use sourcerenderer_core::pool::Recyclable;
use std::sync::mpsc::{ Sender, Receiver, channel };

use crate::VkQueue;
use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkRenderPass;
use crate::VkBuffer;
use crate::VkPipeline;
use crate::VkBackend;

use crate::raw::*;

pub struct VkCommandPool {
  raw: Arc<RawVkCommandPool>,
  buffers: Vec<Box<VkCommandBuffer>>,
  receiver: Receiver<Box<VkCommandBuffer>>,
  sender: Sender<Box<VkCommandBuffer>>
}

pub struct VkCommandBuffer {
  raw: RawVkCommandBuffer
}

pub type RecyclableCmdBuffer = Recyclable<Box<VkCommandBuffer>>;

impl VkCommandPool {
  pub fn new(device: &Arc<RawVkDevice>, queue_family_index: u32) -> Self {
    let create_info = vk::CommandPoolCreateInfo {
      queue_family_index,
      flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
      ..Default::default()
    };
    let vk_device = &device.device;
    let command_pool = unsafe { vk_device.create_command_pool(&create_info, None) }.unwrap();

    let (sender, receiver) = channel();

    return Self {
      raw: Arc::new(RawVkCommandPool {
        pool: command_pool,
        device: device.clone()
      }),
      buffers: Vec::new(),
      receiver,
      sender
    };
  }
}

impl CommandPool<VkBackend> for VkCommandPool {
  fn get_command_buffer(&mut self, command_buffer_type: CommandBufferType) -> RecyclableCmdBuffer {
    let buffer = self.buffers.pop().unwrap_or_else(|| Box::new(VkCommandBuffer::new(&self.raw.device, &self.raw, command_buffer_type)));
    return Recyclable::new(self.sender.clone(), buffer);
  }
}

impl Resettable for VkCommandPool {
  fn reset(&mut self) {
    unsafe {
      self.raw.device.reset_command_pool(**self.raw, vk::CommandPoolResetFlags::empty()).unwrap();
    }

    for cmd_buf in self.receiver.try_iter() {
      self.buffers.push(cmd_buf);
    }
  }
}

impl VkCommandBuffer {
  fn new(device: &Arc<RawVkDevice>, pool: &Arc<RawVkCommandPool>, command_buffer_type: CommandBufferType) -> Self {
    let buffers_create_info = vk::CommandBufferAllocateInfo {
      command_pool: ***pool,
      level: if command_buffer_type == CommandBufferType::PRIMARY { vk::CommandBufferLevel::PRIMARY } else { vk::CommandBufferLevel::SECONDARY }, // TODO: support secondary command buffers / bundles
      command_buffer_count: 1, // TODO: figure out how many buffers per pool (maybe create a new pool once we've run out?)
      ..Default::default()
    };
    let mut buffers = unsafe { device.allocate_command_buffers(&buffers_create_info) }.unwrap();
    let buffer = buffers.remove(0);
    return VkCommandBuffer {
      raw: RawVkCommandBuffer {
        buffer,
        device: device.clone(),
        pool: pool.clone()
      }
    };
  }

  pub fn get_handle(&self) -> &vk::CommandBuffer {
    return &self.raw;
  }
}

impl CommandBuffer<VkBackend> for VkCommandBuffer {
  fn begin(&mut self) {
    unsafe {
      let begin_info = vk::CommandBufferBeginInfo {
        ..Default::default()
      };
      self.raw.device.begin_command_buffer(*self.raw, &begin_info);
    }
  }

  fn end(&mut self) {
    unsafe {
      self.raw.device.end_command_buffer(*self.raw);
    }
  }

  fn begin_render_pass(&mut self, renderpass: &VkRenderPass, recording_mode: RenderpassRecordingMode) {
    unsafe {
      let begin_info = vk::RenderPassBeginInfo {
        framebuffer: *renderpass.get_framebuffer(),
        render_pass: *renderpass.get_layout().get_handle(),
        render_area: vk::Rect2D {
          offset: vk::Offset2D { x: 0i32, y: 0i32 },
          extent: vk::Extent2D { width: renderpass.get_info().width, height: renderpass.get_info().height }
        },
        clear_value_count: 1,
        p_clear_values: &[
          vk::ClearValue {
            color: vk::ClearColorValue {
              float32: [0.0f32, 0.0f32, 0.0f32, 1.0f32]
            }
         },
         vk::ClearValue {
           depth_stencil: vk::ClearDepthStencilValue {
            depth: 0.0f32,
            stencil: 0u32
          }
         }
        ] as *const vk::ClearValue,
        ..Default::default()
      };
      self.raw.device.cmd_begin_render_pass(*self.raw, &begin_info, if recording_mode == RenderpassRecordingMode::Commands { vk::SubpassContents::INLINE } else { vk::SubpassContents::SECONDARY_COMMAND_BUFFERS });
    }
  }

  fn end_render_pass(&mut self) {
    unsafe {
      self.raw.device.cmd_end_render_pass(*self.raw);
    }
  }

  fn set_pipeline(&mut self, pipeline: Arc<VkPipeline>) {
    unsafe {
      self.raw.device.cmd_bind_pipeline(*self.raw, vk::PipelineBindPoint::GRAPHICS, *pipeline.get_handle());
    }
  }

  fn set_vertex_buffer(&mut self, vertex_buffer: &VkBuffer) {
    unsafe {
      self.raw.device.cmd_bind_vertex_buffers(*self.raw, 0, &[*(*vertex_buffer).get_handle()], &[0]);
    }
  }

  fn set_viewports(&mut self, viewports: &[ Viewport ]) {
    unsafe {
      for i in 0..viewports.len() {
        self.raw.device.cmd_set_viewport(*self.raw, i as u32, &[vk::Viewport {
          x: viewports[i].position.x,
          y: viewports[i].position.y,
          width: viewports[i].extent.x,
          height: viewports[i].extent.y,
          min_depth: viewports[i].min_depth,
          max_depth: viewports[i].max_depth
        }]);
      }
    }
  }

  fn set_scissors(&mut self, scissors: &[ Scissor ])  {
    unsafe {
      let vk_scissors: Vec<vk::Rect2D> = scissors.iter().map(|scissor| vk::Rect2D {
        offset: vk::Offset2D {
          x: scissor.position.x,
          y: scissor.position.y
        },
        extent: vk::Extent2D {
          width: scissor.extent.x,
          height: scissor.extent.y
        }
      }).collect();
      self.raw.device.cmd_set_scissor(*self.raw, 0, &vk_scissors);
    }
  }

  fn draw(&mut self, vertices: u32, offset: u32) {
    unsafe {
      self.raw.device.cmd_draw(*self.raw, vertices, 1, offset, 0);
    }
  }
}
