use std::collections::{HashMap, VecDeque};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{RenderGraph, AttachmentSizeClass, AttachmentInfo, StoreAction, LoadAction, PassInfo, InputAttachmentReference, RenderGraphTemplate, RenderGraphTemplateInfo, Format, SampleCount, GraphicsSubpassInfo, PassType, InputAttachmentReferenceType};
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
use sourcerenderer_core::job::JobScheduler;
use std::sync::atomic::Ordering;
use std::cmp::{max, min};
use std::iter::FromIterator;
use VkTexture;
use texture::VkTextureView;
use graph::VkRenderGraph;

pub struct VkRenderGraphTemplate {
  pub device: Arc<RawVkDevice>,
  pub does_render_to_frame_buffer: bool,
  pub passes: Vec<VkPassTemplate>,
  pub attachments: HashMap<String, AttachmentInfo>
}

pub struct VkPassTemplate {
  pub name: String,
  pub pass_type: VkPassType,
  pub renders_to_swapchain: bool,
  pub attachments: Vec<String>,
}

pub enum VkPassType {
  Graphics {
    render_pass: Arc<VkRenderPass>,
  },
  Compute,
  Copy
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct AttachmentMetadata {
  last_used_in_pass_index: u32,
  produced_in_pass_index: u32,
  producer_pass_type: AttachmentProducerPassType
}

impl Default for AttachmentMetadata {
  fn default() -> Self {
    Self {
      last_used_in_pass_index: 0,
      produced_in_pass_index: 0,
      producer_pass_type: AttachmentProducerPassType::Graphics
    }
  }
}

struct SubpassAttachmentMetadata {
  produced_in_subpass_index: u32,
  render_pass_attachment_index: u32,
  last_used_in_pass_index: u32,
  layout: vk::ImageLayout
}

impl Default for SubpassAttachmentMetadata {
  fn default() -> Self {
    Self {
      produced_in_subpass_index: vk::SUBPASS_EXTERNAL,
      render_pass_attachment_index: 0,
      last_used_in_pass_index: 0,
      layout: vk::ImageLayout::UNDEFINED
    }
  }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum AttachmentProducerPassType {
  Graphics,
  Compute,
  Copy
}

impl VkRenderGraphTemplate {
  pub fn new(device: &Arc<RawVkDevice>,
             info: &RenderGraphTemplateInfo) -> Self {

    let mut did_render_to_backbuffer = false;
    let mut layouts: HashMap<String, vk::ImageLayout> = HashMap::new();

    // TODO: figure out threading
    // TODO: more generic support for external images / one time rendering
    // TODO: (async) compute

    let mut passes: Vec<VkPassTemplate> = Vec::new();
    let mut pass_infos = info.passes.clone();
    let mut reordered_passes = VkRenderGraphTemplate::reorder_passes(&info.attachments, &mut pass_infos);

    let mut attachment_metadata: HashMap<&str, AttachmentMetadata> = HashMap::new();
    for (pass_index, reordered_pass) in info.passes.iter().enumerate() {
      match &reordered_pass.pass_type {
        PassType::Graphics {
          subpasses
        } => {
          for subpass in subpasses {
            for output in &subpass.outputs {
              attachment_metadata.entry(output.name.as_str())
                .or_default().produced_in_pass_index = pass_index as u32;
            }

            for input in &subpass.inputs {
              attachment_metadata.entry(input.name.as_str())
                .or_default().last_used_in_pass_index = pass_index as u32;
            }

            if let Some(depth_stencil) = &subpass.depth_stencil {
              attachment_metadata.entry(depth_stencil.name.as_str())
                .or_default().produced_in_pass_index = pass_index as u32;
            }
          }
        },
        _ => unimplemented!()
      }
    }

    let mut reordered_passes_queue: VecDeque<PassInfo> = VecDeque::from_iter(reordered_passes);
    let mut pass_index: u32 = 0;
    let mut pass_opt = reordered_passes_queue.pop_front();
    while pass_opt.is_some() {
      let pass = pass_opt.unwrap();

      match &pass.pass_type {
        PassType::Graphics {
          ref subpasses
        } => {
          // build subpasses, requires the attachment indices populated before
          let render_graph_pass = Self::build_render_pass(subpasses, &pass.name, device, &info.attachments, &mut layouts, pass_index, &attachment_metadata, info.swapchain_format, info.swapchain_sample_count);
          did_render_to_backbuffer |= render_graph_pass.renders_to_swapchain;
          passes.push(render_graph_pass);
        },
        _ => unimplemented!()
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

  fn build_render_pass(passes: &Vec<GraphicsSubpassInfo>,
                       name: &str,
                       device: &Arc<RawVkDevice>,
                       attachments: &HashMap<String, AttachmentInfo>,
                       layouts: &mut HashMap<String, vk::ImageLayout>,
                       pass_index: u32,
                       attachment_metadata: &HashMap<&str, AttachmentMetadata>,
                       swapchain_format: Format,
                       swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut vk_render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    let mut subpass_attachment_metadata: HashMap<&str, SubpassAttachmentMetadata> = HashMap::new();
    let mut pass_renders_to_backbuffer = false;

    // Prepare attachments
    for (subpass_index, pass) in passes.iter().enumerate() {
      for output in &pass.outputs {
        let mut metadata = subpass_attachment_metadata.entry(output.name.as_str())
          .or_default();
        metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
        metadata.produced_in_subpass_index = subpass_index as u32;

        if &output.name == BACK_BUFFER_ATTACHMENT_NAME {
          if output.load_action == LoadAction::Load {
            panic!("cant load back buffer");
          }
          if output.store_action != StoreAction::Store {
            panic!("cant discard back buffer");
          }
          pass_renders_to_backbuffer = true;
          vk_render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(swapchain_format),
              samples: samples_to_vk(swapchain_samples),
              load_op: load_action_to_vk(output.load_action),
              store_op: store_action_to_vk(output.store_action),
              stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
              stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
              initial_layout: metadata.layout,
              final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
              ..Default::default()
            }
          );

          metadata.layout = vk::ImageLayout::PRESENT_SRC_KHR;
        } else {
          let attachment = attachments.get(&output.name).expect("Output not attachment not declared.");
          match attachment {
            AttachmentInfo::Texture {
              format: texture_attachment_format, samples: texture_attachment_samples, ..
            } => {
              if texture_attachment_format.is_depth() {
                panic!("Output attachment must not have a depth stencil format");
              }

              vk_render_pass_attachments.push(
                vk::AttachmentDescription {
                  format: format_to_vk(*texture_attachment_format),
                  samples: samples_to_vk(*texture_attachment_samples),
                  load_op: load_action_to_vk(output.load_action),
                  store_op: store_action_to_vk(output.store_action),
                  stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                  stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                  initial_layout: metadata.layout,
                  final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                  ..Default::default()
                }
              );
            },
            _ => unreachable!()
          }

          metadata.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        }
      }

      if let Some(depth_stencil) = &pass.depth_stencil {
        let mut metadata = subpass_attachment_metadata.entry(depth_stencil.name.as_str())
          .or_default();
        metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
        metadata.produced_in_subpass_index = subpass_index as u32;

        let attachment = attachments.get(&depth_stencil.name).expect("DS  attachment not declared.");
        match attachment {
          AttachmentInfo::Texture {
            format: texture_attachment_format, samples: texture_attachment_samples, ..
          } => {
            if !texture_attachment_format.is_depth() {
              panic!("Depth stencil attachment must have a depth stencil format");
            }

            vk_render_pass_attachments.push(
              vk::AttachmentDescription {
                format: format_to_vk(*texture_attachment_format),
                samples: samples_to_vk(*texture_attachment_samples),
                load_op: load_action_to_vk(depth_stencil.load_action),
                store_op: store_action_to_vk(depth_stencil.store_action),
                stencil_load_op: load_action_to_vk(depth_stencil.load_action),
                stencil_store_op: store_action_to_vk(depth_stencil.store_action),
                initial_layout: metadata.layout,
                final_layout: vk::ImageLayout::DEPTH_READ_ONLY_STENCIL_ATTACHMENT_OPTIMAL,
                ..Default::default()
              }
            );
          },
          _ => unreachable!()
        }
        metadata.layout = vk::ImageLayout::DEPTH_READ_ONLY_STENCIL_ATTACHMENT_OPTIMAL;
      }

      for input in &pass.inputs {
        let mut metadata = subpass_attachment_metadata.entry(input.name.as_str())
          .or_default();
        if subpass_index as u32 > metadata.last_used_in_pass_index {
          metadata.last_used_in_pass_index = subpass_index as u32;
        }
      }
    }

    let mut dependencies: Vec<vk::SubpassDependency> = Vec::new(); // todo
    let mut subpasses: Vec<vk::SubpassDescription> = Vec::new();
    let mut attachment_refs: Vec<vk::AttachmentReference> = Vec::new();
    let mut preserve_attachments: Vec<u32> = Vec::new();
    for (subpass_index, pass) in passes.iter().enumerate() {
      let inputs_start = attachment_refs.len() as isize;
      let inputs_len = pass.inputs.len() as u32;
      for input in &pass.inputs {
        let metadata = &attachment_metadata[input.name.as_str()];
        let subpass_metadata = &subpass_attachment_metadata[input.name.as_str()];
        match &input.attachment_type {
          InputAttachmentReferenceType::Texture {
            is_local
          } => {
            if *is_local {
              attachment_refs.push(vk::AttachmentReference {
                attachment: subpass_metadata.render_pass_attachment_index,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              });
            }

            dependencies.push(vk::SubpassDependency {
              src_subpass: subpass_metadata.produced_in_subpass_index,
              dst_subpass: subpass_index as u32,
              src_stage_mask: match metadata.producer_pass_type {
                AttachmentProducerPassType::Graphics => vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                AttachmentProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                AttachmentProducerPassType::Copy => vk::PipelineStageFlags::TRANSFER,
              },
              dst_stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
              src_access_mask: match metadata.producer_pass_type {
                AttachmentProducerPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                AttachmentProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                AttachmentProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
              },
              dst_access_mask: if *is_local { vk::AccessFlags::COLOR_ATTACHMENT_READ } else { vk::AccessFlags::SHADER_READ },
              dependency_flags: if *is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
            });
          },
          _ => unimplemented!()
        }
      }

      let outputs_start = attachment_refs.len() as isize;
      let outputs_len = pass.outputs.len() as u32;
      let mut graph_pass_index = 0;
      for output in &pass.outputs {
        let metadata = &subpass_attachment_metadata[output.name.as_str()];
        graph_pass_index = metadata.produced_in_subpass_index;

        attachment_refs.push(vk::AttachmentReference {
          attachment: metadata.render_pass_attachment_index,
          layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        });
      }

      let depth_stencil_start = pass.depth_stencil.as_ref().map(|depth_stencil| {
        let metadata = &subpass_attachment_metadata[depth_stencil.name.as_str()];
        let index = attachment_refs.len();
        attachment_refs.push(vk::AttachmentReference {
          attachment: metadata.render_pass_attachment_index,
          layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        });
        index as isize
      });

      for (name, _) in attachments {
        let mut is_used = false;
        for input in &pass.inputs {
          is_used |= &input.name == name;
        }

        let metadata = attachment_metadata.get(&name.as_str()).unwrap();
        let subpass_metadata = subpass_attachment_metadata.get(&name.as_str()).unwrap();
        if !is_used && (metadata.last_used_in_pass_index > pass_index || subpass_metadata.last_used_in_pass_index > subpass_index as u32) {
          preserve_attachments.push(subpass_metadata.render_pass_attachment_index);
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
          p_depth_stencil_attachment: depth_stencil_start.map_or(std::ptr::null(), |start| attachment_refs.as_ptr().offset(start)),
          ..Default::default()
        });
      }
    }

    let render_pass_create_info = vk::RenderPassCreateInfo {
      p_attachments: vk_render_pass_attachments.as_ptr(),
      attachment_count: vk_render_pass_attachments.len() as u32,
      p_subpasses: subpasses.as_ptr(),
      subpass_count: subpasses.len() as u32,
      p_dependencies: dependencies.as_ptr(),
      dependency_count: dependencies.len() as u32,
      ..Default::default()
    };
    let render_pass = Arc::new(VkRenderPass::new(device, &render_pass_create_info));

    let mut sorted_metadata: Vec<(&&str, &SubpassAttachmentMetadata)> = subpass_attachment_metadata.iter().collect();
    sorted_metadata.sort_by_key(|(name, subpass_metadata)| subpass_metadata.render_pass_attachment_index);
    let used_attachments = sorted_metadata.iter().map(|(name, _)| (*name).to_string()).collect();

    VkPassTemplate {
      renders_to_swapchain: pass_renders_to_backbuffer,
      attachments: used_attachments,
      name: name.to_owned(),
      pass_type: VkPassType::Graphics {
        render_pass,
      }
    }
  }

  fn find_next_suitable_pass(attachments: &HashMap<String, AttachmentInfo>, reordered_pass_infos: &[PassInfo], pass_infos: &mut Vec<PassInfo>) -> PassInfo {
    let mut attachment_indices: HashMap<String, usize> = HashMap::new();
    for (index, pass) in reordered_pass_infos.iter().enumerate() {
      match &pass.pass_type {
        PassType::Graphics {
          subpasses
        } => {
          for subpass in subpasses {
            for output in &subpass.outputs {
              attachment_indices.insert(output.name.clone(), index);
            }
          }
        },
        _ => unimplemented!()
      }
    }

    let mut best_pass_index_score: Option<(usize, usize)> = None;
    for (pass_index, pass) in pass_infos.iter().enumerate() {
      let mut is_ready = true;
      let mut passes_since_ready = usize::MAX;

      match &pass.pass_type {
        PassType::Graphics {
          subpasses
        } => {
          for subpass in subpasses {
            for input in &subpass.inputs {
              let index_opt = attachment_indices.get(&input.name);
              if let Some(index) = index_opt {
                passes_since_ready = min(*index, passes_since_ready);
              } else {
                is_ready = false;
              }
            }

            if is_ready && (best_pass_index_score.is_none() || passes_since_ready > best_pass_index_score.unwrap().1) {
              best_pass_index_score = Some((pass_index, passes_since_ready));
            }
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
