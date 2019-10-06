#![feature(optin_builtin_traits)]

use std::rc::Rc;
use std::rc::Weak;
use std::sync::Arc;
use std::cell::RefCell;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::CommandPool;
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::Buffer;
use sourcerenderer_core::graphics::RenderPass;
use sourcerenderer_core::graphics::RenderPassLayout;
use sourcerenderer_core::graphics::RenderpassRecordingMode;
use sourcerenderer_core::graphics::Pipeline;

use crate::VkQueue;
use crate::VkDevice;
use crate::VkRenderPass;
use crate::VkRenderPassLayout;
use crate::VkBuffer;
use crate::VkPipeline;

struct VkCommandPoolState {
  pub free_buffers: Vec<Rc<VkCommandBuffer>>,
  pub used_buffers: Vec<Rc<VkCommandBuffer>>
}

pub struct VkCommandPool {
  device: Arc<VkDevice>,
  command_pool: vk::CommandPool,
  queue: Arc<VkQueue>,
  state: RefCell<VkCommandPoolState>
}

pub struct VkCommandBuffer {
  device: Arc<VkDevice>,
  command_buffer: vk::CommandBuffer,
  pool: Weak<VkCommandPool>
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
      device: device,
      command_pool: command_pool,
      queue: queue,
      state: RefCell::new(VkCommandPoolState {
        free_buffers: Vec::new(),
        used_buffers: Vec::new()
      })
    };
  }

  pub fn get_pool(&self) -> &vk::CommandPool {
    return &self.command_pool;
  }

  pub fn get_queue(&self) -> &VkQueue {
    return self.queue.as_ref();
  }
}

impl Drop for VkCommandPool {
  fn drop(&mut self) {
    let mut state = self.state.borrow_mut();
    while let Some(ref mut cmd_buffer_ref) = state.free_buffers.pop() {
      let cmd_buffer = Rc::get_mut(cmd_buffer_ref).expect("Dropping command pool that is still in use!");
      cmd_buffer.drop_vk(self);
    }
    while let Some(ref mut cmd_buffer_ref) = state.used_buffers.pop() {
      let cmd_buffer = Rc::get_mut(cmd_buffer_ref).expect("Dropping command pool that is still in use!");
      cmd_buffer.drop_vk(self);
    }
    unsafe {
      let vk_device = self.queue.get_device().get_ash_device();
      vk_device.destroy_command_pool(self.command_pool, None);
    }
  }
}

impl CommandPool for VkCommandPool {
  fn create_command_buffer(self: Rc<Self>, command_buffer_type: CommandBufferType) -> Rc<dyn CommandBuffer> {
    let mut state = self.state.borrow_mut();
    let free_cmd_buffer_option = state.free_buffers.pop();
    return match free_cmd_buffer_option {
      Some(free_cmd_buffer) => {
        free_cmd_buffer
      }
      None => {
        let rc = Rc::new(VkCommandBuffer::new(self.device.clone(), &self, command_buffer_type));
        state.used_buffers.push(rc.clone());
        rc
      }
    };
  }

  fn reset(&self) {
    let mut state = self.state.borrow_mut();
    while let Some(buffer) = state.used_buffers.pop()
    {
      state.free_buffers.push(buffer);
    }
    unsafe {
      self.queue.get_device().get_ash_device().reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty()).unwrap();
    }
  }
}

impl VkCommandBuffer {
  pub fn new(device: Arc<VkDevice>, pool: &Rc<VkCommandPool>, command_buffer_type: CommandBufferType) -> Self {
    let vk_device = pool.get_queue().get_device().get_ash_device();
    let command_pool = pool.get_pool();
    let buffers_create_info = vk::CommandBufferAllocateInfo {
      command_pool: *command_pool,
      level: if command_buffer_type == CommandBufferType::PRIMARY { vk::CommandBufferLevel::PRIMARY } else { vk::CommandBufferLevel::SECONDARY }, // TODO: support secondary command buffers / bundles
      command_buffer_count: 1, // TODO: figure out how many buffers per pool (maybe create a new pool once we've run out?)
      ..Default::default()
    };
    let mut buffers = unsafe { vk_device.allocate_command_buffers(&buffers_create_info) }.unwrap();
    let buffer = buffers.remove(0);
    return VkCommandBuffer {
      command_buffer: buffer,
      device: device,
      pool: Rc::downgrade(&pool)
    };
  }

  fn drop_vk(&mut self, pool: &VkCommandPool) {
    unsafe {
      let device = pool
        .get_queue()
        .get_device()
        .get_ash_device();
      device.free_command_buffers(*pool.get_pool(), &[ self.command_buffer ] );
    }
  }
}

impl CommandBuffer for VkCommandBuffer {
  fn begin(&self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      let begin_info = vk::CommandBufferBeginInfo {
        ..Default::default()
      };
      vk_device.begin_command_buffer(self.command_buffer, &begin_info);
    }
  }

  fn end(&self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.end_command_buffer(self.command_buffer);
    }
  }

  fn begin_render_pass(&self, renderpass: &dyn RenderPass, recording_mode: RenderpassRecordingMode) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      let vk_renderpass = (renderpass as *const dyn RenderPass) as *const VkRenderPass;
      let vk_renderpass_layout = Arc::from_raw(Arc::into_raw(renderpass.get_layout()) as *const VkRenderPassLayout);
      let begin_info = vk::RenderPassBeginInfo {
        framebuffer: *(*vk_renderpass).get_framebuffer(),
        render_pass: *vk_renderpass_layout.get_handle(),
        render_area: vk::Rect2D {
          offset: vk::Offset2D { x: 0i32, y: 0i32 },
          extent: vk::Extent2D { width: renderpass.get_info().width, height: renderpass.get_info().height }
        },
        clear_value_count: 1,
        p_clear_values: &[
          vk::ClearValue {
            color: vk::ClearColorValue {
              float32: [1.0f32, 1.0f32, 1.0f32, 1.0f32]
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

  fn end_render_pass(&self) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.cmd_end_render_pass(self.command_buffer);
    }
  }

  fn set_pipeline(&self, pipeline: Arc<dyn Pipeline>) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      let vk_pipeline = Arc::from_raw(Arc::into_raw(pipeline) as *const VkPipeline);
      vk_device.cmd_bind_pipeline(self.command_buffer, vk::PipelineBindPoint::GRAPHICS, *vk_pipeline.get_handle());
    }
  }

  fn set_vertex_buffer(&self, vertex_buffer: &dyn Buffer) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      let vk_buffer = (vertex_buffer as *const Buffer) as *const VkBuffer;
      vk_device.cmd_bind_vertex_buffers(self.command_buffer, 0, &[*(*vk_buffer).get_handle()], &[0])
    }
  }

  fn draw(&self, vertices: u32, offset: u32) {
    unsafe {
      let vk_device = self.device.get_ash_device();
      vk_device.cmd_draw(self.command_buffer, vertices, 1, offset, 0);
    }
  }
}
