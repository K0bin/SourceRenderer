use std::collections::{HashMap, VecDeque};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;


use sourcerenderer_core::graphics::{PassOutput, StoreAction, LoadAction, PassInfo, PassInput, RenderGraphTemplate, RenderGraphTemplateInfo, Format, SampleCount, GraphicsSubpassInfo, PassType, SubpassOutput};
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;
use crate::raw::RawVkDevice;

use crate::format::format_to_vk;
use crate::pipeline::samples_to_vk;
use ::{VkRenderPass};
use std::cmp::{min};
use std::iter::FromIterator;

pub struct VkRenderGraphTemplate {
  pub device: Arc<RawVkDevice>,
  pub does_render_to_frame_buffer: bool,
  pub passes: Vec<VkPassTemplate>,
  pub attachments: HashMap<String, AttachmentMetadata>
}

pub struct VkPassTemplate {
  pub name: String,
  pub pass_type: VkPassType,
  pub renders_to_swapchain: bool,
  pub resources: HashSet<String>,
}

#[derive(Clone)]
pub enum VkBarrierTemplate {
  Image {
    name: String,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_access_mask: vk::AccessFlags,
    dst_access_mask: vk::AccessFlags,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_queue_family_index: u32,
    dst_queue_family_index: u32
  },
  Buffer {
    name: String,
    src_access_mask: vk::AccessFlags,
    dst_access_mask: vk::AccessFlags,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_queue_family_index: u32,
    dst_queue_family_index: u32
  }
}

pub enum VkPassType {
  Graphics {
    render_pass: Arc<VkRenderPass>,
    attachments: Vec<String>
  },
  Compute {
    barriers: Vec<VkBarrierTemplate>
  },
  Copy
}

#[derive(Clone)]
pub struct AttachmentMetadata {
  pub(super) output: PassOutput,
  pub(super) last_used_in_pass_index: u32,
  pub(super) produced_in_pass_index: u32,
  pub(super) producer_pass_type: AttachmentPassType,
  pub(super) layout: vk::ImageLayout
}

impl AttachmentMetadata {
  fn new(output: PassOutput) -> Self {
    Self {
      output,
      last_used_in_pass_index: 0,
      produced_in_pass_index: 0,
      producer_pass_type: AttachmentPassType::Graphics,
      layout: vk::ImageLayout::UNDEFINED
    }
  }
}

struct SubpassAttachmentMetadata {
  produced_in_subpass_index: u32,
  render_pass_attachment_index: u32,
  last_used_in_subpass_index: u32,
  layout: vk::ImageLayout
}

impl Default for SubpassAttachmentMetadata {
  fn default() -> Self {
    Self {
      produced_in_subpass_index: vk::SUBPASS_EXTERNAL,
      render_pass_attachment_index: 0,
      last_used_in_subpass_index: 0,
      layout: vk::ImageLayout::UNDEFINED
    }
  }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum AttachmentPassType {
  External,
  Graphics,
  Compute,
  Copy
}

impl VkRenderGraphTemplate {
  pub fn new(device: &Arc<RawVkDevice>,
             info: &RenderGraphTemplateInfo) -> Self {

    let mut did_render_to_backbuffer = false;

    // TODO: figure out threading
    // TODO: more generic support for external images / one time rendering
    // TODO: (async) compute

    let mut attachment_metadata = HashMap::<String, AttachmentMetadata>::new();

    /*for external in &info.external_resources {
      match external {
        PassOutput::Buffer(buffer) => {
          attachment_metadata.insert(buffer.name.clone(), AttachmentMetadata {
            output: external.clone(),
            last_used_in_pass_index: 0,
            produced_in_pass_index: 0,
            producer_pass_type: AttachmentProducerPassType::External,
            layout: vk::ImageLayout::UNDEFINED
          });

        }
      }
    }*/

    let mut passes: Vec<VkPassTemplate> = Vec::new();
    let mut pass_infos = info.passes.clone();
    let reordered_passes = VkRenderGraphTemplate::reorder_passes(&mut pass_infos, &mut attachment_metadata);

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
          let render_graph_pass = Self::build_render_pass(subpasses, &pass.name, device, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count);
          did_render_to_backbuffer |= render_graph_pass.renders_to_swapchain;
          passes.push(render_graph_pass);
        },
        PassType::Compute {
          inputs, outputs
        } => {
          let render_graph_pass = Self::build_compute_pass(inputs, outputs, &pass.name, device, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count);
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
      attachments: attachment_metadata
    }
  }

  pub(crate) fn passes(&self) -> &[VkPassTemplate] {
    &self.passes
  }

  pub(crate) fn attachments(&self) -> &HashMap<String, AttachmentMetadata> {
    &self.attachments
  }

  pub(crate) fn renders_to_swapchain(&self) -> bool {
    self.does_render_to_frame_buffer
  }

  fn reorder_passes(passes: &Vec<PassInfo>, metadata: &mut HashMap<String, AttachmentMetadata>) -> Vec<PassInfo> {
    let mut passes_mut = passes.clone();
    let mut reordered_passes = vec![];

    while !passes_mut.is_empty() {
      let pass = VkRenderGraphTemplate::find_next_suitable_pass(&mut passes_mut, &metadata);
      match &pass.pass_type {
        PassType::Graphics {
          subpasses
        } => {
          for subpass in subpasses {
            for output in &subpass.outputs {
              match output {
                SubpassOutput::RenderTarget(render_target_output) => {
                  metadata
                    .entry(render_target_output.name.to_string())
                    .or_insert_with(|| AttachmentMetadata::new(PassOutput::RenderTarget(render_target_output.clone())))
                    .produced_in_pass_index = reordered_passes.len() as u32;
                },
                SubpassOutput::Backbuffer(backbuffer_output) => {
                  metadata
                    .entry(BACK_BUFFER_ATTACHMENT_NAME.to_string())
                    .or_insert_with(|| AttachmentMetadata::new(PassOutput::Backbuffer(backbuffer_output.clone())))
                    .produced_in_pass_index = reordered_passes.len() as u32;
                },
                _ => {}
              }
            }

            if let Some(depth_stencil) = &subpass.depth_stencil {
              metadata
                .entry(depth_stencil.name.to_string())
                .or_insert_with(|| AttachmentMetadata::new(PassOutput::DepthStencil(depth_stencil.clone())))
                .produced_in_pass_index = reordered_passes.len() as u32;
            }

            for input in &subpass.inputs {
              let mut input_metadata = metadata.get_mut(&input.name).unwrap();
              input_metadata.last_used_in_pass_index = reordered_passes.len() as u32;
            }
          }
        },
        PassType::Compute {
          inputs, outputs
        } => {
          for output in outputs {
            match output {
              PassOutput::RenderTarget(render_target_output) => {
                let metadata_entry = metadata
                  .entry(render_target_output.name.to_string())
                  .or_insert_with(|| AttachmentMetadata::new(PassOutput::RenderTarget(render_target_output.clone())));
                  metadata_entry.produced_in_pass_index = reordered_passes.len() as u32;
                  metadata_entry.producer_pass_type = AttachmentPassType::Compute;
              },
              PassOutput::Backbuffer(backbuffer_output) => {
                let metadata_entry = metadata
                  .entry(BACK_BUFFER_ATTACHMENT_NAME.to_string())
                  .or_insert_with(|| AttachmentMetadata::new(PassOutput::Backbuffer(backbuffer_output.clone())));
                metadata_entry.produced_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.producer_pass_type = AttachmentPassType::Compute;
              },
              PassOutput::Buffer(buffer_output) => {
                let metadata_entry = metadata
                  .entry(buffer_output.name.to_string())
                  .or_insert_with(|| AttachmentMetadata::new(PassOutput::Buffer(buffer_output.clone())));
                  metadata_entry.produced_in_pass_index = reordered_passes.len() as u32;
                  metadata_entry.producer_pass_type = AttachmentPassType::Compute;
              }
              _ => {}
            }
          }

          for input in inputs {
            let mut input_metadata = metadata.get_mut(&input.name).unwrap();
            input_metadata.last_used_in_pass_index = reordered_passes.len() as u32;
          }
        },
        _ => unimplemented!()
      }
      reordered_passes.push(pass);
    }
    return reordered_passes;
  }

  fn build_render_pass(passes: &Vec<GraphicsSubpassInfo>,
                       name: &str,
                       device: &Arc<RawVkDevice>,
                       pass_index: u32,
                       attachment_metadata: &mut HashMap<String, AttachmentMetadata>,
                       swapchain_format: Format,
                       swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut vk_render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    let mut subpass_attachment_metadata: HashMap<&str, SubpassAttachmentMetadata> = HashMap::new();
    let mut pass_renders_to_backbuffer = false;

    // Prepare attachments
    for (subpass_index, pass) in passes.iter().enumerate() {
      for output in &pass.outputs {
        match output {
          SubpassOutput::Backbuffer(backbuffer_output) => {
            let mut metadata = subpass_attachment_metadata.entry(BACK_BUFFER_ATTACHMENT_NAME)
              .or_default();
            metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
            metadata.produced_in_subpass_index = subpass_index as u32;

            pass_renders_to_backbuffer = true;
            vk_render_pass_attachments.push(
              vk::AttachmentDescription {
                format: format_to_vk(swapchain_format),
                samples: samples_to_vk(swapchain_samples),
                load_op: if backbuffer_output.clear { vk::AttachmentLoadOp::CLEAR } else { vk::AttachmentLoadOp::DONT_CARE },
                store_op: vk::AttachmentStoreOp::STORE,
                stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: metadata.layout,
                final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                ..Default::default()
              }
            );
            metadata.layout = vk::ImageLayout::PRESENT_SRC_KHR;
          },

          SubpassOutput::RenderTarget(render_target_output) => {
            let mut metadata = subpass_attachment_metadata.entry(render_target_output.name.as_str())
              .or_default();
            if render_target_output.format.is_depth() {
              panic!("Output attachment must not have a depth stencil format");
            }

            vk_render_pass_attachments.push(
              vk::AttachmentDescription {
                format: format_to_vk(render_target_output.format),
                samples: samples_to_vk(render_target_output.samples),
                load_op: load_action_to_vk(render_target_output.load_action),
                store_op: store_action_to_vk(render_target_output.store_action),
                stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: metadata.layout,
                final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ..Default::default()
              }
            );

            metadata.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
          }
        }
      }

      if let Some(depth_stencil) = &pass.depth_stencil {
        let mut metadata = subpass_attachment_metadata.entry(depth_stencil.name.as_str())
          .or_default();
        metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
        metadata.produced_in_subpass_index = subpass_index as u32;

        if !depth_stencil.format.is_depth() {
          panic!("Depth stencil attachment must have a depth stencil format");
        }

        vk_render_pass_attachments.push(
          vk::AttachmentDescription {
            format: format_to_vk(depth_stencil.format),
            samples: samples_to_vk(depth_stencil.samples),
            load_op: load_action_to_vk(depth_stencil.load_action),
            store_op: store_action_to_vk(depth_stencil.store_action),
            stencil_load_op: load_action_to_vk(depth_stencil.load_action),
            stencil_store_op: store_action_to_vk(depth_stencil.store_action),
            initial_layout: metadata.layout,
            final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ..Default::default()
          }
        );
        metadata.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
      }

      for input in &pass.inputs {
        let mut metadata = subpass_attachment_metadata.entry(input.name.as_str())
          .or_default();
        if subpass_index as u32 > metadata.last_used_in_subpass_index {
          metadata.last_used_in_subpass_index = subpass_index as u32;
        }
      }
    }

    let mut dependencies: Vec<vk::SubpassDependency> = Vec::new(); // todo
    let mut subpasses: Vec<vk::SubpassDescription> = Vec::new();
    let mut attachment_refs: Vec<vk::AttachmentReference> = Vec::new();
    let mut preserve_attachments: Vec<u32> = Vec::new();
    for (subpass_index, pass) in passes.iter().enumerate() {
      let inputs_start = attachment_refs.len() as isize;
      let mut inputs_len = 0;
      for input in &pass.inputs {
        let metadata = attachment_metadata.get(input.name.as_str()).unwrap();
        let subpass_metadata = &subpass_attachment_metadata[input.name.as_str()];

        dependencies.push(vk::SubpassDependency {
          src_subpass: subpass_metadata.produced_in_subpass_index,
          dst_subpass: subpass_index as u32,
          src_stage_mask: match metadata.producer_pass_type {
            AttachmentPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
            AttachmentPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
            AttachmentPassType::Copy => vk::PipelineStageFlags::TRANSFER,
            AttachmentPassType::External => unimplemented!()
          },
          dst_stage_mask: vk::PipelineStageFlags::ALL_GRAPHICS,
          src_access_mask: match metadata.producer_pass_type {
            AttachmentPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            AttachmentPassType::Compute => vk::AccessFlags::SHADER_WRITE,
            AttachmentPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
            AttachmentPassType::External => unimplemented!()
          },
          dst_access_mask: if input.is_local { vk::AccessFlags::COLOR_ATTACHMENT_READ } else { vk::AccessFlags::SHADER_READ },
          dependency_flags: if input.is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
        });

        match &metadata.output {
          PassOutput::RenderTarget(_) => {}
          PassOutput::Backbuffer(_) => {}
          _ => { continue; }
        }
        attachment_refs.push(vk::AttachmentReference {
          attachment: subpass_metadata.render_pass_attachment_index,
          layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        });
        inputs_len += 1;
      }

      let outputs_start = attachment_refs.len() as isize;
      let outputs_len = pass.outputs.len() as u32;
      for output in &pass.outputs {
        match output {
          SubpassOutput::RenderTarget(render_target_output) => {
            let metadata = &subpass_attachment_metadata[render_target_output.name.as_str()];
            attachment_refs.push(vk::AttachmentReference {
              attachment: metadata.render_pass_attachment_index,
              layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            });
          },
          SubpassOutput::Backbuffer {
            ..
          } => {
            let metadata = &subpass_attachment_metadata[BACK_BUFFER_ATTACHMENT_NAME];
            attachment_refs.push(vk::AttachmentReference {
              attachment: metadata.render_pass_attachment_index,
              layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            });
          }
      }
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

      for (name, _) in attachment_metadata.iter() {
        let mut is_used = false;
        for input in &pass.inputs {
          is_used |= input.name.as_str() == *name;
        }

        let metadata = attachment_metadata.get(name).unwrap();
        let subpass_metadata = subpass_attachment_metadata.get(name.as_str()).unwrap();
        if !is_used && (metadata.last_used_in_pass_index > pass_index || subpass_metadata.last_used_in_subpass_index > subpass_index as u32) {
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

    let mut sorted_metadata: Vec<(&&str, &SubpassAttachmentMetadata)> = subpass_attachment_metadata.iter()
      .filter(|(_, subpass_metadata)| subpass_metadata.render_pass_attachment_index != u32::MAX)
      .collect();
    sorted_metadata.sort_by_key(|(_, subpass_metadata)| subpass_metadata.render_pass_attachment_index);
    let used_attachments = sorted_metadata.iter().map(|(name, _)| (*name).to_string()).collect();

    let used_resources = subpass_attachment_metadata.iter()
    .map(|(name, _)| (*name).to_string())
    .collect();

    VkPassTemplate {
      renders_to_swapchain: pass_renders_to_backbuffer,
      resources: used_resources,
      name: name.to_owned(),
      pass_type: VkPassType::Graphics {
        render_pass,
        attachments: used_attachments
      }
    }
  }

  fn build_compute_pass(inputs: &[PassInput],
                        outputs: &[PassOutput],
                        name: &str,
                        _device: &Arc<RawVkDevice>,
                        _pass_index: u32,
                        attachment_metadata: &mut HashMap<String, AttachmentMetadata>,
                        _swapchain_format: Format,
                        _swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut used_resources = HashSet::<String>::new();
    for output in outputs {
      used_resources.insert(match output {
        PassOutput::Buffer(buffer_output) => buffer_output.name.clone(),
        PassOutput::Backbuffer(_) => BACK_BUFFER_ATTACHMENT_NAME.to_string(),
        PassOutput::DepthStencil(ds_output) => ds_output.name.clone(),
        PassOutput::RenderTarget(rt_output) => rt_output.name.clone()
      });
    }

    for input in inputs {
      used_resources.insert(input.name.clone());
    }

    let barriers = inputs.iter().map(|input|{
      let metadata = attachment_metadata.get(&input.name).unwrap();
      match &metadata.output {
        PassOutput::RenderTarget(_rt_output) => {
          VkBarrierTemplate::Image {
            name: input.name.clone(),
            old_layout: metadata.layout,
            new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            src_access_mask: match metadata.producer_pass_type {
              AttachmentPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
              AttachmentPassType::Compute => vk::AccessFlags::SHADER_WRITE,
              AttachmentPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
              _ => unimplemented!()
            },
            dst_access_mask: vk::AccessFlags::SHADER_READ,
            src_stage: match metadata.producer_pass_type {
              AttachmentPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              AttachmentPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              AttachmentPassType::Copy => vk::PipelineStageFlags::empty(),
              _ => unimplemented!()
            },
            dst_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
            src_queue_family_index: 0,
            dst_queue_family_index: 0
          }
        },
        PassOutput::DepthStencil(_ds_output) => {
          VkBarrierTemplate::Image {
            name: input.name.clone(),
            old_layout: metadata.layout,
            new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            src_access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            dst_access_mask: vk::AccessFlags::SHADER_READ,
            src_stage: match metadata.producer_pass_type {
              AttachmentPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              AttachmentPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              AttachmentPassType::Copy => vk::PipelineStageFlags::empty(),
              _ => unimplemented!()
            },
            dst_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
            src_queue_family_index: 0,
            dst_queue_family_index: 0
          }
        },
        PassOutput::Backbuffer(_) => unreachable!(),
        PassOutput::Buffer(_buffer_output) => {
          VkBarrierTemplate::Buffer {
            name: input.name.clone(),
            src_access_mask: vk::AccessFlags::SHADER_WRITE,
            dst_access_mask: vk::AccessFlags::SHADER_READ,
            src_stage: match metadata.producer_pass_type {
              AttachmentPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              AttachmentPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              AttachmentPassType::Copy => vk::PipelineStageFlags::empty(),
              _ => unimplemented!()
            },
            dst_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
            src_queue_family_index: 0,
            dst_queue_family_index: 0
          }
        }
      }
    }).collect();

    VkPassTemplate {
      name: name.to_string(),
      pass_type: VkPassType::Compute {
        barriers
      },
      renders_to_swapchain: false,
      resources: used_resources
    }
  }

  fn find_next_suitable_pass(pass_infos: &mut Vec<PassInfo>, metadata: &HashMap<String, AttachmentMetadata>) -> PassInfo {
    let mut best_pass_index_score: Option<(u32, u32)> = None;
    for (pass_index, pass) in pass_infos.iter().enumerate() {
      let mut is_ready = true;
      let mut passes_since_ready = u32::MAX;

      match &pass.pass_type {
        PassType::Graphics {
          subpasses
        } => {
          for subpass in subpasses {
            for input in &subpass.inputs {
              let index_opt = metadata.get(&input.name);
              if let Some(index) = index_opt {
                passes_since_ready = min(index.produced_in_pass_index, passes_since_ready);
              } else {
                is_ready = false;
              }
            }

            if is_ready && (best_pass_index_score.is_none() || passes_since_ready > best_pass_index_score.unwrap().1 as u32) {
              best_pass_index_score = Some((pass_index as u32, passes_since_ready as u32));
            }
          }
        },
        PassType::Compute {
          inputs, ..
        } => {
          for input in inputs {
            let index_opt = metadata.get(&input.name);
            if let Some(index) = index_opt {
              passes_since_ready = min(index.produced_in_pass_index, passes_since_ready);
            } else {
              is_ready = false;
            }
          }

          if is_ready && (best_pass_index_score.is_none() || passes_since_ready > best_pass_index_score.unwrap().1 as u32) {
            best_pass_index_score = Some((pass_index as u32, passes_since_ready as u32));
          }
        },
        _ => unimplemented!()
      }
    }
    pass_infos.remove(best_pass_index_score.expect("Invalid render graph").0 as usize)
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
