use std::rc::Rc;
use std::sync::Arc;
use std::cell::RefCell;

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

use crate::VkQueue;
use crate::VkDevice;
use crate::VkRenderPass;
use crate::VkBuffer;
use crate::VkPipeline;
use crate::VkBackend;

struct VkCommandPoolInner {
  command_pool: vk::CommandPool,
  device: Arc<VkDevice>
}

pub struct VkCommandPool {
  queue: Arc<VkQueue>,
  buffers: Vec<Rc<RefCell<VkCommandBuffer>>>,
  used_buffers_len: usize,
  inner: Rc<VkCommandPoolInner>
}

pub struct VkCommandBuffer {
  device: Arc<VkDevice>,
  command_buffer: vk::CommandBuffer,
  command_pool_inner: Rc<VkCommandPoolInner>
}

impl Drop for VkCommandPoolInner {
  fn drop(&mut self) {
    let vk_device = self.device.get_ash_device();
    unsafe {
      vk_device.destroy_command_pool(self.command_pool, None);
    }
  }
}

impl VkCommandPool {
  pub fn new(device: Arc<VkDevice>, queue: Arc<VkQueue>) -> Self {
    let create_info = vk::CommandPoolCreateInfo {
      queue_family_index: queue.get_queue_family_index(),
      flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
      ..Default::default()
    };
    let vk_device = device.get_ash_device();
    let command_pool = unsafe { vk_device.create_command_pool(&create_info, None) }.unwrap();

    return VkCommandPool {
      queue: queue,
      inner: Rc::new(VkCommandPoolInner {
        command_pool: command_pool,
        device: device
      }),
      buffers: Vec::new(),
      used_buffers_len: 0
    };
  }

  pub fn get_queue(&self) -> &VkQueue {
    return &self.queue;
  }
}

impl CommandPool<VkBackend> for VkCommandPool {
  fn get_command_buffer(&mut self, command_buffer_type: CommandBufferType) -> Rc<RefCell<VkCommandBuffer>> {
    if self.used_buffers_len == self.buffers.len() {
      let new_buffer = Rc::new(RefCell::new(VkCommandBuffer::new(self.inner.device.clone(), self.inner.clone(), command_buffer_type)));
      self.buffers.push(new_buffer);
    }
    let buffer = self.buffers[self.used_buffers_len].clone();
    self.used_buffers_len += 1;
    return buffer;
  }
}

impl Resettable for VkCommandPool {
  fn reset(&mut self) {
    let vk_device = self.inner.device.get_ash_device();
    unsafe {
      vk_device.reset_command_pool(self.inner.command_pool, vk::CommandPoolResetFlags::empty()).unwrap();
    }
    self.used_buffers_len = 0;
  }
}

impl VkCommandBuffer {
  fn new(device: Arc<VkDevice>, command_pool_inner: Rc<VkCommandPoolInner>, command_buffer_type: CommandBufferType) -> Self {
    let vk_device = device.get_ash_device();
    let buffers_create_info = vk::CommandBufferAllocateInfo {
      command_pool: command_pool_inner.command_pool,
      level: if command_buffer_type == CommandBufferType::PRIMARY { vk::CommandBufferLevel::PRIMARY } else { vk::CommandBufferLevel::SECONDARY }, // TODO: support secondary command buffers / bundles
      command_buffer_count: 1, // TODO: figure out how many buffers per pool (maybe create a new pool once we've run out?)
      ..Default::default()
    };
    let mut buffers = unsafe { vk_device.allocate_command_buffers(&buffers_create_info) }.unwrap();
    let buffer = buffers.remove(0);
    return VkCommandBuffer {
      command_buffer: buffer,
      device: device,
      command_pool_inner: command_pool_inner
    };
  }

  pub fn get_handle(&self) -> &vk::CommandBuffer {
    return &self.command_buffer;
  }
}

impl Drop for VkCommandBuffer {
  fn drop(&mut self) {
    let device = self.device.get_ash_device();
    unsafe {
      device.free_command_buffers(self.command_pool_inner.command_pool, &[ self.command_buffer ] );
    }
  }
}

impl CommandBuffer<VkBackend> for VkCommandBuffer {
  fn begin(&mut self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      let begin_info = vk::CommandBufferBeginInfo {
        ..Default::default()
      };
      vk_device.begin_command_buffer(self.command_buffer, &begin_info);
    }
  }

  fn end(&mut self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.end_command_buffer(self.command_buffer);
    }
  }

  fn begin_render_pass(&mut self, renderpass: &VkRenderPass, recording_mode: RenderpassRecordingMode) {
    unsafe {
      let vk_device = self.device.get_ash_device();
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
      vk_device.cmd_begin_render_pass(self.command_buffer, &begin_info, if recording_mode == RenderpassRecordingMode::Commands { vk::SubpassContents::INLINE } else { vk::SubpassContents::SECONDARY_COMMAND_BUFFERS });
    }
  }

  fn end_render_pass(&mut self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.cmd_end_render_pass(self.command_buffer);
    }
  }

  fn set_pipeline(&mut self, pipeline: Arc<VkPipeline>) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.cmd_bind_pipeline(self.command_buffer, vk::PipelineBindPoint::GRAPHICS, *pipeline.get_handle());
    }
  }

  fn set_vertex_buffer(&mut self, vertex_buffer: &VkBuffer) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      let vk_buffer = vertex_buffer;
      vk_device.cmd_bind_vertex_buffers(self.command_buffer, 0, &[*(*vk_buffer).get_handle()], &[0]);
    }
  }

  fn set_viewports(&mut self, viewports: &[ Viewport ]) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      for i in 0..viewports.len() {
        vk_device.cmd_set_viewport(self.command_buffer, i as u32, &[vk::Viewport {
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
      let vk_device = self.device.get_ash_device();
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
      vk_device.cmd_set_scissor(self.command_buffer, 0, &vk_scissors);
    }
  }

  fn draw(&mut self, vertices: u32, offset: u32) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.cmd_draw(self.command_buffer, vertices, 1, offset, 0);
    }
  }
}
