use std::collections::{HashMap, VecDeque};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{RenderGraph, AttachmentSizeClass, AttachmentInfo, StoreAction, LoadAction, PassInfo, GraphicsPassInfo, InputAttachmentReference, RenderGraphTemplate, RenderGraphTemplateInfo, Format, SampleCount};
use sourcerenderer_core::graphics::RenderGraphInfo;
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;
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
use graph::VkRenderGraph;

pub struct VkRenderGraphTemplate {
  device: Arc<RawVkDevice>,
  does_render_to_frame_buffer: bool,
  passes: Vec<VkPassTemplate>,
  attachments: HashMap<String, AttachmentInfo>
}

pub enum VkPassTemplate {
  Graphics {
    render_pass: Arc<VkRenderPass>,
    attachments: Vec<String>,
    renders_to_swapchain: bool,
    pass_indices: Vec<u32>
  },
  Compute,
  Copy
}

impl VkRenderGraphTemplate {
  pub fn new(device: &Arc<RawVkDevice>,
             info: &RenderGraphTemplateInfo) -> Self {

    let mut did_render_to_backbuffer = false;
    let mut layouts: HashMap<String, vk::ImageLayout> = HashMap::new();
    layouts.insert(BACK_BUFFER_ATTACHMENT_NAME.to_owned(), vk::ImageLayout::UNDEFINED);

    // TODO: figure out threading
    // TODO: more generic support for external images / one time rendering
    // TODO: (async) compute

    let mut pass_index = 0u32;

    let mut passes: Vec<VkPassTemplate> = Vec::new();
    let mut pass_infos = info.passes.clone();
    let mut reordered_passes = VkRenderGraphTemplate::reorder_passes(&info.attachments, &mut pass_infos);
    let mut reordered_passes_queue: VecDeque<PassInfo> = VecDeque::from_iter(reordered_passes);

    let mut pass_opt = reordered_passes_queue.pop_front();
    let mut merged_pass: Vec<PassInfo> = Vec::new();
    let mut pass_indices: Vec<u32> = Vec::new();
    while pass_opt.is_some() {
      let pass = pass_opt.unwrap();
      let previous_pass = merged_pass.last();
      let can_be_merged = if let Some(previous_pass) = previous_pass {
        match previous_pass {
          PassInfo::Graphics(graphics_pass) => {
            let mut width = 0.0f32;
            let mut height = 0.0f32;
            let mut size_class = AttachmentSizeClass::RelativeToSwapchain;

            'first_texture_input: for input in &graphics_pass.inputs {
              match input {
                InputAttachmentReference::Texture(input_texture_ref) => {
                  let input_attachment = info.attachments.get(&input_texture_ref.name).expect("Invalid attachment reference");
                  let texture_attachment = if let AttachmentInfo::Texture(texture_attachment) = input_attachment {
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

            VkRenderGraphTemplate::can_pass_be_merged(&pass, &info.attachments, width, height, size_class)
          },
          _ => {
            false
          }
        }
      } else {
        false
      };

      if can_be_merged {
        merged_pass.push(pass);
        pass_indices.push(pass_index);
      } else {
        if !merged_pass.is_empty() {
          let mut is_graphics_pass = merged_pass.len() > 1;
          if let PassInfo::Graphics(_) = merged_pass.first().expect("Invalid merged empty pass") {
            is_graphics_pass = true;
          }

          if is_graphics_pass {
            let graphics_passes = merged_pass.iter()
              .map(|p|
                if let PassInfo::Graphics(graphics_pass) = p { graphics_pass.clone() } else { unreachable!() }
              ).collect();

            // build subpasses, requires the attachment indices populated before
            let render_graph_pass = Self::build_render_pass(graphics_passes, device, &info.attachments, &mut layouts, &pass_indices, info.swapchain_format, info.swapchain_sample_count);
            did_render_to_backbuffer |= if let VkPassTemplate::Graphics { renders_to_swapchain, .. } = render_graph_pass { renders_to_swapchain } else { false };
            passes.push(render_graph_pass);
          } else {
            unimplemented!();
          }
        }

        merged_pass.clear();
        pass_indices.clear();

        merged_pass.push(pass);
        pass_indices.push(pass_index);
      }

      // insert last pass
      if !merged_pass.is_empty() {
        let mut is_graphics_pass = merged_pass.len() > 1;
        if let PassInfo::Graphics(_) = merged_pass.first().expect("Invalid merged empty pass") {
          is_graphics_pass = true;
        }

        if is_graphics_pass {
          let graphics_passes = merged_pass.iter()
            .map(|p|
              if let PassInfo::Graphics(graphics_pass) = p { graphics_pass.clone() } else { unreachable!() }
            ).collect();

          // build subpasses, requires the attachment indices populated before
          let render_graph_pass = Self::build_render_pass(graphics_passes, device, &info.attachments, &mut layouts, &pass_indices, info.swapchain_format, info.swapchain_sample_count);
          did_render_to_backbuffer |= if let VkPassTemplate::Graphics { renders_to_swapchain, .. } = render_graph_pass { renders_to_swapchain } else { false };
          passes.push(render_graph_pass);
        } else {
          unimplemented!();
        }
      }

      pass_opt = reordered_passes_queue.pop_front();
      pass_index += 1;
    }

    Self {
      device: device.clone(),
      passes,
      does_render_to_frame_buffer: did_render_to_backbuffer,
      attachments: info.attachments.clone()
    }
  }

  pub(crate) fn passes(&self) -> &[VkPassTemplate] {
    &self.passes
  }

  pub(crate) fn attachments(&self) -> &HashMap<String, AttachmentInfo> {
    &self.attachments
  }

  pub(crate) fn renders_to_swapchain(&self) -> bool {
    self.does_render_to_frame_buffer
  }

  fn reorder_passes(attachments: &HashMap<String, AttachmentInfo>, passes: &Vec<PassInfo>) -> Vec<PassInfo> {
    let mut passes_mut = passes.clone();
    let mut reordered_passes = vec![];

    while !passes_mut.is_empty() {
      let pass = VkRenderGraphTemplate::find_next_suitable_pass(attachments, &reordered_passes, &mut passes_mut);
      reordered_passes.push(pass);
    }
    return reordered_passes;
  }

  fn build_render_pass(passes: Vec<GraphicsPassInfo>,
                       device: &Arc<RawVkDevice>,
                       attachments: &HashMap<String, AttachmentInfo>,
                       layouts: &mut HashMap<String, vk::ImageLayout>,
                       pass_indices: &[u32],
                       swapchain_format: Format,
                       swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    let mut attachment_indices: HashMap<&str, u32> = HashMap::new();
    let mut used_attachments: Vec<String> = Vec::new();
    let mut pass_renders_to_backbuffer = false;
    let mut attachment_last_user_pass_index: HashMap<&str, u32> = HashMap::new();
    let mut attachment_producer_pass_index: HashMap<&str, u32> = HashMap::new();

    // Prepare attachments
    let mut pass_index = 0;
    for merged_pass in &passes {
      for output in &merged_pass.outputs {
        let index = render_pass_attachments.len() as u32;
        if &output.name == BACK_BUFFER_ATTACHMENT_NAME {
          if output.load_action == LoadAction::Load {
            panic!("cant load back buffer");
          }
          if output.store_action != StoreAction::Store {
            panic!("cant discard back buffer");
          }
          pass_renders_to_backbuffer = true;
          render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(swapchain_format),
              samples: samples_to_vk(swapchain_samples),
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
        } else {
          let attachment = attachments.get(&output.name).expect("Output not attachment not declared.");
          let texture_attachment = if let AttachmentInfo::Texture(attachment_texture) = &attachment { attachment_texture } else { unreachable!() };
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
        }

        used_attachments.push(output.name.clone());
        attachment_indices.insert(&output.name as &str, index);
        attachment_producer_pass_index.insert(&output.name as &str, pass_index);
        attachment_last_user_pass_index.entry(&output.name).and_modify(|attachment_pass_index| if pass_index > *attachment_pass_index {
          *attachment_pass_index = pass_index;
        }).or_insert(pass_index);
      }
      pass_index += 1;
    }

    let mut dependencies: Vec<vk::SubpassDependency> = Vec::new(); // todo
    let mut subpasses: Vec<vk::SubpassDescription> = Vec::new();
    let mut attachment_refs: Vec<vk::AttachmentReference> = Vec::new();
    let mut preserve_attachments: Vec<u32> = Vec::new();
    pass_index = 0;
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
            dependencies.push(vk::SubpassDependency {
              src_subpass: *(attachment_producer_pass_index.get(&texture_attachment.name as &str).unwrap()),
              dst_subpass: pass_index,
              src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
              dst_stage_mask: vk::PipelineStageFlags::TOP_OF_PIPE,
              src_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
              dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ,
              dependency_flags: if texture_attachment.is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
            });
          },
          _ => unimplemented!()
        }
      }

      let outputs_start = attachment_refs.len() as isize;
      let outputs_len = merged_pass.outputs.len() as u32;
      for output in &merged_pass.outputs {
        attachment_refs.push(vk::AttachmentReference {
          attachment: (*attachment_indices.get(&output.name as &str).expect(format!("Couldn't find index for {}", &output.name).as_str())),
          layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        });
      }

      for attachment in &used_attachments {
        if *(attachment_last_user_pass_index.get(attachment as &str).unwrap()) > pass_index {
          preserve_attachments.push(*(attachment_indices.get(&attachment as &str).expect(format!("Couldn't find index for {}", attachment).as_str())));
        }
      }

      unsafe {
        subpasses.push(vk::SubpassDescription {
          p_input_attachments: attachment_refs.as_ptr().offset(inputs_start),
          input_attachment_count: inputs_len,
          p_color_attachments: attachment_refs.as_ptr().offset(outputs_start),
          color_attachment_count: outputs_len,
          p_preserve_attachments: preserve_attachments.as_ptr(),
          preserve_attachment_count: preserve_attachments.len() as u32,
          ..Default::default()
        });
      }

      pass_index += 1;
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

    VkPassTemplate::Graphics {
      render_pass,
      renders_to_swapchain: pass_renders_to_backbuffer,
      attachments: used_attachments,
      pass_indices: pass_indices.iter().map(|i| *i).collect()
    }
  }

  fn can_pass_be_merged(pass: &PassInfo, attachments: &HashMap<String, AttachmentInfo>, base_width: f32, base_height: f32, base_size_class: AttachmentSizeClass) -> bool {
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

              can_be_merged &= texture_info.is_local && texture_attachment.size_class == base_size_class && (texture_attachment.width - base_width).abs() < 0.01f32 && (texture_attachment.height - base_height).abs() < 0.01f32;
            },
            _ => {}
          }
        }
        can_be_merged
      },
      _ => false
    }
  }

  fn find_next_suitable_pass(attachments: &HashMap<String, AttachmentInfo>, reordered_pass_infos: &[PassInfo], pass_infos: &mut Vec<PassInfo>) -> PassInfo {
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

impl RenderGraphTemplate for VkRenderGraphTemplate {
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
