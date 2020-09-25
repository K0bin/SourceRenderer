use std::collections::{HashMap, VecDeque};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::graph::{RenderGraph, StoreAction, LoadAction, AttachmentSizeClass, AttachmentInfo, GraphicsPassInfo, PassInfo, InputAttachmentReference};
use sourcerenderer_core::graphics::graph::RenderGraphInfo;
use sourcerenderer_core::graphics::graph::BACK_BUFFER_ATTACHMENT_NAME;
use sourcerenderer_core::graphics::{Texture, TextureInfo, AttachmentBlendInfo};

use crate::VkBackend;
use crate::VkDevice;
use crate::raw::RawVkDevice;
use crate::VkSwapchain;
use crate::format::format_to_vk;
use crate::pipeline::samples_to_vk;
use sourcerenderer_core::graphics::{Backend, CommandBufferType, CommandBuffer, RenderpassRecordingMode, Swapchain};
use context::VkThreadContextManager;
use std::cell::RefCell;
use ::{VkRenderPass, VkQueue};
use ::{VkFrameBuffer, VkSemaphore};
use ::{VkCommandBufferRecorder, VkFence};
use sourcerenderer_core::job::{JobQueue, JobScheduler, JobCounterWait};
use std::sync::atomic::Ordering;
use std::cmp::{max, min};
use std::iter::FromIterator;
use VkTexture;
use texture::VkTextureView;

pub struct VkAttachment {
  texture: Arc<VkTexture>,
  view: Arc<VkTextureView>,
  info: AttachmentInfo
}

pub struct VkRenderGraph {
  device: Arc<RawVkDevice>,
  passes: Vec<Arc<VkPass>>,
  attachments: HashMap<String, VkAttachment>,
  context: Arc<VkThreadContextManager>,
  swapchain: Arc<VkSwapchain>,
  does_render_to_frame_buffer: bool,
  graphics_queue: Arc<VkQueue>,
  compute_queue: Option<Arc<VkQueue>>,
  transfer_queue: Option<Arc<VkQueue>>
}

pub enum VkPass {
  Graphics(VkGraphicsPass),
  Compute,
  Copy
}

pub struct VkGraphicsPass {
  device: Arc<RawVkDevice>,
  render_pass: Arc<VkRenderPass>,
  frame_buffer: Vec<Arc<VkFrameBuffer>>,
  is_rendering_to_swap_chain: bool,
  infos: Vec<GraphicsPassInfo<VkBackend>>
}

impl VkRenderGraph {
  pub fn new(device: &Arc<RawVkDevice>,
             context: &Arc<VkThreadContextManager>,
             graphics_queue: &Arc<VkQueue>,
             compute_queue: &Option<Arc<VkQueue>>,
             transfer_queue: &Option<Arc<VkQueue>>,
             info: &RenderGraphInfo<VkBackend>,
             swapchain: &Arc<VkSwapchain>) -> Self {

    // TODO: figure out threading
    // TODO: recreate graph when swapchain changes
    // TODO: more generic support for external images / one time rendering
    // TODO: (async) compute

    let mut layouts: HashMap<String, vk::ImageLayout> = HashMap::new();
    layouts.insert(BACK_BUFFER_ATTACHMENT_NAME.to_owned(), vk::ImageLayout::UNDEFINED);
    let mut attachments: HashMap<String, VkAttachment> = HashMap::new();

    for (name, attachment) in &info.attachments {
      // TODO: aliasing
      match attachment {
        // TODO: transient
        AttachmentInfo::Texture(texture_info) => {
          let texture = Arc::new(VkTexture::new(&device, &TextureInfo {
            format: texture_info.format,
            width: if texture_info.size_class == AttachmentSizeClass::RelativeToSwapchain {
              (swapchain.get_width() as f32 * texture_info.width) as u32
            } else {
              texture_info.width as u32
            },
            height: if texture_info.size_class == AttachmentSizeClass::RelativeToSwapchain {
              (swapchain.get_height() as f32 * texture_info.height) as u32
            } else {
              texture_info.height as u32
            },
            depth: 1,
            mip_levels: texture_info.levels,
            array_length: 1,
            samples: texture_info.samples
          }));

          let view = Arc::new(VkTextureView::new_render_target_view(device, &texture));
          attachments.insert(name.clone(), VkAttachment {
            texture,
            view,
            info: attachment.clone()
          });
        },
        _ => unimplemented!()
      }
    }

    let mut did_render_to_backbuffer = false;

    let mut pass_infos = info.passes.clone();
    let mut reordered_passes = VkRenderGraph::reorder_passes(&info.attachments, &mut pass_infos);
    let mut reordered_passes_queue: VecDeque<PassInfo<VkBackend>> = VecDeque::from_iter(reordered_passes);

    let mut passes: Vec<Arc<VkPass>> = Vec::new();

    let mut pass_opt = reordered_passes_queue.pop_front();
    while pass_opt.is_some() {
      let pass = pass_opt.unwrap();
      match pass {
        PassInfo::Graphics(graphics_pass) => {
          let mut width = 0.0f32;
          let mut height = 0.0f32;
          let mut size_class = AttachmentSizeClass::RelativeToSwapchain;

          'first_texture_input: for input in &graphics_pass.inputs {
            match input {
              InputAttachmentReference::Texture(input_texture_ref) => {
                let input_attachment = attachments.get(&input_texture_ref.name).expect("Invalid attachment reference");
                let texture_attachment = if let AttachmentInfo::Texture(texture_attachment) = &input_attachment.info {
                  texture_attachment
                } else {
                  panic!("Attachment type does not match reference type")
                };

                width = texture_attachment.width;
                height = texture_attachment.height;
                size_class = texture_attachment.size_class;
                break 'first_texture_input;
              },
              _ => {}
            }
          }

          let mut merged_passes: Vec<GraphicsPassInfo<VkBackend>> = vec![graphics_pass];
          let mut can_be_merged = false;

          let mut next_pass = reordered_passes_queue.get(0);
          if let Some(pass) = next_pass {
            can_be_merged = VkRenderGraph::can_pass_be_merged(pass, width, height, size_class);
          }

          while can_be_merged {
            let next_graphics_pass = if let PassInfo::Graphics(next_graphics_pass) = reordered_passes_queue.pop_front().unwrap() {
              next_graphics_pass
            } else {
              unreachable!()
            };
            merged_passes.push(next_graphics_pass);

            next_pass = reordered_passes_queue.get(0);
            if let Some(pass) = next_pass {
              can_be_merged = VkRenderGraph::can_pass_be_merged(pass, width, height, size_class);
            }
          }


          // build subpasses, requires the attachment indices populated before
          let render_graph_pass = VkRenderGraph::build_render_pass(merged_passes, device, &attachments, &swapchain, &mut layouts);
          did_render_to_backbuffer |= render_graph_pass.is_rendering_to_swap_chain;
          passes.push(Arc::new(VkPass::Graphics(render_graph_pass)));

          pass_opt = reordered_passes_queue.pop_front();
        },
        _ => unimplemented!()
      }
    }

    return VkRenderGraph {
      device: device.clone(),
      context: context.clone(),
      passes,
      attachments,
      graphics_queue: graphics_queue.clone(),
      compute_queue: compute_queue.clone(),
      transfer_queue: transfer_queue.clone(),
      swapchain: swapchain.clone(),
      does_render_to_frame_buffer: did_render_to_backbuffer
    };
  }

  fn reorder_passes(attachments: &HashMap<String, AttachmentInfo>, passes: &Vec<PassInfo<VkBackend>>) -> Vec<PassInfo<VkBackend>> {
    let mut passes_mut = passes.clone();
    let mut reordered_passes = vec![];

    while !passes_mut.is_empty() {
      let pass = VkRenderGraph::find_next_suitable_pass(attachments, &reordered_passes, &mut passes_mut);
      reordered_passes.push(pass);
    }
    return reordered_passes;
  }

  fn build_render_pass(passes: Vec<GraphicsPassInfo<VkBackend>>,
                       device: &Arc<RawVkDevice>,
                       attachments: &HashMap<String, VkAttachment>,
                       swapchain: &VkSwapchain,
                       layouts: &mut HashMap<String, vk::ImageLayout>) -> VkGraphicsPass {
    let mut render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    let mut attachment_indices: HashMap<&str, u32> = HashMap::new();
    let mut dependencies: Vec<vk::SubpassDependency> = Vec::new();
    let mut pass_renders_to_backbuffer = false;
    let mut subpasses: Vec<vk::SubpassDescription> = Vec::new();
    let mut attachment_refs: Vec<vk::AttachmentReference> = Vec::new();
    let mut frame_buffer_attachments: Vec<Vec<vk::ImageView>> = Vec::new();
    let swapchain_views = swapchain.get_views();

    // Prepare attachments
    for merged_pass in &passes {
      for output in &merged_pass.outputs {
        let index = render_pass_attachments.len() as u32;
        if &output.name == BACK_BUFFER_ATTACHMENT_NAME {
          let info = swapchain.get_textures().first().unwrap().get_info();
          if output.load_action == LoadAction::Load {
            panic!("cant load back buffer");
          }
          if output.store_action != StoreAction::Store {
            panic!("cant discard back buffer");
          }
          pass_renders_to_backbuffer = true;
          render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(info.format),
              samples: samples_to_vk(info.samples),
              load_op: load_action_to_vk(output.load_action),
              store_op: store_action_to_vk(output.store_action),
              stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
              stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
              initial_layout: *(layouts.get(&output.name).unwrap_or(&vk::ImageLayout::UNDEFINED)),
              final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
              ..Default::default()
            }
          );
          layouts.insert(output.name.clone(), vk::ImageLayout::PRESENT_SRC_KHR);
          attachment_indices.insert(&output.name as &str, index);
        } else {
          let attachment = attachments.get(&output.name).expect("Output not attachment not declared.");
          let texture_attachment = if let AttachmentInfo::Texture(attachment_texture) = &attachment.info { attachment_texture } else { unreachable!() };
          render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(texture_attachment.format),
              samples: samples_to_vk(texture_attachment.samples),
              load_op: load_action_to_vk(output.load_action),
              store_op: store_action_to_vk(output.store_action),
              stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
              stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
              initial_layout:  *layouts.get(&output.name as &str).unwrap_or(&vk::ImageLayout::UNDEFINED),
              final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
              ..Default::default()
            }
          );
          layouts.insert(output.name.clone(), vk::ImageLayout::PRESENT_SRC_KHR);
          attachment_indices.insert(&output.name as &str, index);
        }
      }
    }

    let frame_buffer_count = if pass_renders_to_backbuffer { swapchain_views.len() } else { 1 };
    for _ in 0..frame_buffer_count {
      frame_buffer_attachments.push(Vec::new());
    }

    for merged_pass in &passes {
      let inputs_start = attachment_refs.len() as isize;
      let inputs_len = merged_pass.inputs.len() as u32;
      for input in &merged_pass.inputs {
        match input {
          InputAttachmentReference::Texture(texture_attachment) => {
            attachment_refs.push(vk::AttachmentReference {
              attachment: (*attachment_indices.get(&texture_attachment.name as &str).expect(format!("Couldn't find index for {}", &texture_attachment.name).as_str())) as u32,
              layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            });
          },
          _ => unimplemented!()
        }
      }

      let outputs_start = attachment_refs.len() as isize;
      let outputs_len = merged_pass.outputs.len() as u32;
      for output in &merged_pass.outputs {
        attachment_refs.push(vk::AttachmentReference {
          attachment: (*attachment_indices.get(&output.name as &str).expect(format!("Couldn't find index for {}", &output.name).as_str())) as u32,
          layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        });

        for i in 0..frame_buffer_count {
          frame_buffer_attachments.get_mut(i).unwrap().push(
            if &output.name == BACK_BUFFER_ATTACHMENT_NAME {
              *swapchain_views[i].get_view_handle()
            } else {
              *attachments[&output.name].view.get_view_handle()
            }
          );
        }
      }
      unsafe {
        subpasses.push(vk::SubpassDescription {
          p_input_attachments: attachment_refs.as_ptr().offset(inputs_start),
          input_attachment_count: inputs_len,
          p_color_attachments: attachment_refs.as_ptr().offset(outputs_start),
          color_attachment_count: outputs_len,
          ..Default::default()
        });
      }
    }


    let render_pass_create_info = vk::RenderPassCreateInfo {
      p_attachments: render_pass_attachments.as_ptr(),
      attachment_count: render_pass_attachments.len() as u32,
      p_subpasses: subpasses.as_ptr(),
      subpass_count: subpasses.len() as u32,
      p_dependencies: dependencies.as_ptr(),
      dependency_count: dependencies.len() as u32,
      ..Default::default()
    };
    let render_pass = Arc::new(VkRenderPass::new(device, &render_pass_create_info));


    let (width, height) = if pass_renders_to_backbuffer {
      (swapchain.get_width(), swapchain.get_height())
    } else {
      let attachment = attachments.get(
        &passes
          .iter()
          .find_map(|pass|
            pass.outputs
              .iter()
              .find_map(|output| if &output.name != BACK_BUFFER_ATTACHMENT_NAME {
                Some(output)
              } else {
                None
              })
          )
          .unwrap()
          .name
      )
        .unwrap();
      (0, 0)
    };

    let mut frame_buffers: Vec<Arc<VkFrameBuffer>> = Vec::with_capacity(frame_buffer_count);
    for fb_attachments in frame_buffer_attachments {
      let frame_buffer_info = vk::FramebufferCreateInfo {
        render_pass: *render_pass.get_handle(),
        attachment_count: fb_attachments.len() as u32,
        p_attachments: fb_attachments.as_ptr(),
        layers: 1,
        width,
        height,
        ..Default::default()
      };
      let frame_buffer = Arc::new(VkFrameBuffer::new(device, &frame_buffer_info));
      frame_buffers.push(frame_buffer);
    }

    VkGraphicsPass {
      device: device.clone(),
      frame_buffer: frame_buffers,
      render_pass,
      is_rendering_to_swap_chain: pass_renders_to_backbuffer,
      infos: passes
    }
  }

  fn can_pass_be_merged<B: Backend>(pass: &PassInfo<B>, base_width: f32, base_height: f32, base_size_class: AttachmentSizeClass) -> bool {
    match pass {
      PassInfo::Graphics(graphics_pass) => {
        let mut can_be_merged = true;
        for input in &graphics_pass.inputs {
          match input {
            InputAttachmentReference::Texture(texture_info) => {
              let input_attachment = attachments.get(&texture_info.name).expect("Invalid attachment reference");
              let texture_attachment = if let AttachmentInfo::Texture(texture_attachment) = input_attachment {
                texture_attachment
              } else {
                panic!("Attachment type does not match reference type")
              };

              can_be_merged &= texture_info.is_local && texture_attachment.size_class == size_class && (texture_attachment.width - width).abs() < 0.01f32 && (texture_attachment.height - height).abs() < 0.01f32;
            },
            _ => {}
          }
        }
        can_be_merged
      },
      _ => false
    }
  }

  fn find_next_suitable_pass(attachments: &HashMap<String, AttachmentInfo>, reordered_pass_infos: &[PassInfo<VkBackend>], pass_infos: &mut Vec<PassInfo<VkBackend>>) -> PassInfo<VkBackend> {
    let mut attachment_indices: HashMap<String, usize> = HashMap::new();
    for (index, pass) in reordered_pass_infos.iter().enumerate() {
      match pass {
        PassInfo::Graphics(graphics_pass) => {
          for output in &graphics_pass.outputs {
            attachment_indices.insert(output.name.clone(), index);
          }
        },
        _ => unimplemented!()
      }
    }

    let mut width = 0.0f32;
    let mut height = 0.0f32;
    let mut size_class = AttachmentSizeClass::RelativeToSwapchain;
    if !reordered_pass_infos.is_empty() {
      let last_pass = reordered_pass_infos.last().unwrap();
      match last_pass {
        PassInfo::Graphics(last_graphics_pass) => {
          let last_pass_output = last_graphics_pass.outputs.first().expect("Pass has no outputs");
          if &last_pass_output.name != &BACK_BUFFER_ATTACHMENT_NAME {
            let attachment = attachments.get(&last_pass_output.name).expect("Invalid attachment reference");
            let texture_attachment = if let AttachmentInfo::Texture(texture_info) = attachment { texture_info } else { unreachable!() };
            width = texture_attachment.width;
            height = texture_attachment.height;
            size_class = texture_attachment.size_class;
          } else {
            width = 1.0f32;
            height = 1.0f32;
            size_class = AttachmentSizeClass::RelativeToSwapchain;
          };
        },
        _ => unimplemented!()
      }
    }

    let mut best_pass_index_score: Option<(usize, usize)> = None;
    for (pass_index, pass) in pass_infos.iter().enumerate() {
      let mut is_ready = true;
      let mut passes_since_ready = usize::MAX;
      let mut can_be_merged = true;

      match pass {
        PassInfo::Graphics(graphics_info) => {
          for input in &graphics_info.inputs {
            match input {
              InputAttachmentReference::Texture(texture_info) => {
                let input_attachment = attachments.get(&texture_info.name).expect("Invalid attachment reference");
                let texture_attachment = if let AttachmentInfo::Texture(texture_attachment) = input_attachment{
                  texture_attachment
                } else {
                  panic!("Attachment type does not match reference type")
                };

                can_be_merged &= texture_info.is_local && texture_attachment.size_class == size_class && (texture_attachment.width - width).abs() < 0.01f32 && (texture_attachment.height - height).abs() < 0.01f32;
                let index_opt = attachment_indices.get(&texture_info.name);
                if let Some(index) = index_opt {
                  passes_since_ready = min(*index, passes_since_ready);
                } else {
                  is_ready = false;
                }
              },
              _ => {
                can_be_merged = false;
              }
            }
          }

          let first_output = graphics_info.outputs.first().expect("Pass has no outputs");
          let (output_width, output_height, output_size_class) = if &first_output.name != &BACK_BUFFER_ATTACHMENT_NAME {
            let first_output_attachment = attachments.get(&first_output.name).expect("Invalid attachment reference");
            let first_output_texture = if let AttachmentInfo::Texture(texture_attachment) = first_output_attachment { texture_attachment } else { unreachable!() };
            (first_output_texture.width, first_output_texture.height, first_output_texture.size_class)
          } else {
            (1.0f32, 1.0f32, AttachmentSizeClass::RelativeToSwapchain)
          };

          for output in &graphics_info.outputs {
            let (width, height, size_class) = if &output.name == &BACK_BUFFER_ATTACHMENT_NAME {
              (1.0f32, 1.0f32, AttachmentSizeClass::RelativeToSwapchain)
            } else {
              let attachment = attachments.get(&output.name).expect("Invalid attachment reference");
              let output_texture = if let AttachmentInfo::Texture(texture_attachment) = attachment { texture_attachment } else { unreachable!() };
              (output_texture.width, output_texture.height, output_texture.size_class)
            };
            if size_class != output_size_class || (output_width - width).abs() > 0.01f32 || (output_height - height).abs() > 0.01f32 {
              panic!("All outputs must have the same size");
            }
          }

          if is_ready && (can_be_merged || best_pass_index_score.is_none() || passes_since_ready > best_pass_index_score.unwrap().1) {
            best_pass_index_score = Some((pass_index, passes_since_ready));
          }
        },
        _ => unimplemented!()
      }
    }
    pass_infos.remove(best_pass_index_score.expect("Invalid render graph").0)
  }
}

impl RenderGraph<VkBackend> for VkRenderGraph {
  fn recreate(&mut self, swap_chain: &VkSwapchain) {

  }

  fn render(&mut self, job_queue: &dyn JobQueue) -> JobCounterWait {
    let counter = JobScheduler::new_counter();

    self.context.begin_frame();

    let mut prepare_semaphore = self.context.get_shared().get_semaphore();
    let cmd_semaphore = self.context.get_shared().get_semaphore();
    let cmd_fence = self.context.get_shared().get_fence();
    let mut image_index: u32 = 0;

    let mut recreate = false;
    if self.does_render_to_frame_buffer {
      let mut result = self.swapchain.prepare_back_buffer(&prepare_semaphore);
      if false && (result.is_err() || !result.unwrap().1) {
        let new_swapchain = Arc::new(self.swapchain.recreate());
        std::mem::replace(&mut self.swapchain, new_swapchain);
        prepare_semaphore = self.context.get_shared().get_semaphore();
        result = self.swapchain.prepare_back_buffer(&prepare_semaphore);
        recreate = true;
      }
      let (index, _) = result.unwrap();
      image_index = index
    }

    let mut expected_counter = 0usize;
    for pass in &self.passes {
      let c_context = self.context.clone();
      let c_pass = pass.clone();
      let c_queue = self.graphics_queue.clone();
      let c_prepare_semaphore = prepare_semaphore.clone();
      let c_cmd_semaphore = cmd_semaphore.clone();
      let c_counter = counter.clone();
      let c_wait_counter = counter.clone();
      let c_cmd_fence = cmd_fence.clone();
      let frame_buffer_index = image_index as usize;
      job_queue.enqueue_job(
        Box::new(move || {
          let thread_context = c_context.get_thread_context();
          let mut frame_context = thread_context.get_frame_context();
          let mut cmd_buffer = frame_context.get_command_pool().get_command_buffer(CommandBufferType::PRIMARY);

          match &c_pass as &VkPass {
            VkPass::Graphics(graphics_pass) => {
            cmd_buffer.begin_render_pass(&graphics_pass.render_pass, &graphics_pass.frame_buffer[frame_buffer_index], RenderpassRecordingMode::Commands);
            for i in 0..graphics_pass.infos.len() {
              if i != 0 {
                cmd_buffer.advance_subpass();
              }
              let callback = &graphics_pass.infos.get(i).unwrap().render;
              (callback)(&mut cmd_buffer);
            }
            cmd_buffer.end_render_pass();
            let submission = cmd_buffer.finish();

            let prepare_semaphores = [&**c_prepare_semaphore];
            let cmd_semaphores = [&**c_cmd_semaphore];

            let wait_semaphores: &[&VkSemaphore] = if graphics_pass.is_rendering_to_swap_chain {
              &prepare_semaphores
            } else {
              &[]
            };
            let signal_semaphores: &[&VkSemaphore] = if graphics_pass.is_rendering_to_swap_chain {
              &cmd_semaphores
            } else {
              &[]
            };

            let fence = if graphics_pass.is_rendering_to_swap_chain {
              Some(&c_cmd_fence as &VkFence)
            } else {
              None
            };

            c_queue.submit(submission, fence, &wait_semaphores, &signal_semaphores);

            if graphics_pass.is_rendering_to_swap_chain {
              frame_context.track_semaphore(&c_prepare_semaphore);
            }

            },
            _ => unimplemented!()
          }

          c_counter.fetch_add(1, Ordering::SeqCst);
        }),
        Some(&JobCounterWait {
          counter: c_wait_counter,
          value: expected_counter
        })
      );
      expected_counter += 1;
    }

    let c_cmd_semaphore = cmd_semaphore.clone();
    let c_context = self.context.clone();
    let c_queue = self.graphics_queue.clone();
    let c_swapchain = self.swapchain.clone();
    let c_cmd_fence = cmd_fence.clone();
    let c_counter = counter.clone();
    let c_wait_counter = counter.clone();
    let c_does_render_to_frame_buffer = self.does_render_to_frame_buffer;
    job_queue.enqueue_job(Box::new(move || {
        let thread_context = c_context.get_thread_context();
        let mut frame_context = thread_context.get_frame_context();

        if c_does_render_to_frame_buffer {
          c_queue.present(&c_swapchain, image_index, &[&c_cmd_semaphore]);
          frame_context.track_semaphore(&c_cmd_semaphore);
        }

        c_context.end_frame(&c_cmd_fence);
        c_counter.store(100, Ordering::SeqCst);
      }), Some(&JobCounterWait {
        counter: c_wait_counter,
        value: expected_counter
    }));

    JobCounterWait {
      counter,
      value: 100
    }
  }
}

fn store_action_to_vk(store_action: StoreAction) -> vk::AttachmentStoreOp {
  match store_action {
    StoreAction::DontCare => vk::AttachmentStoreOp::DONT_CARE,
    StoreAction::Store => vk::AttachmentStoreOp::STORE
  }
}

fn load_action_to_vk(load_action: LoadAction) -> vk::AttachmentLoadOp {
  match load_action {
    LoadAction::DontCare => vk::AttachmentLoadOp::DONT_CARE,
    LoadAction::Load => vk::AttachmentLoadOp::LOAD,
    LoadAction::Clear => vk::AttachmentLoadOp::CLEAR
  }
}
