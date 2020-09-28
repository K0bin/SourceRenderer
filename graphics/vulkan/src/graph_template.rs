use std::collections::{HashMap, VecDeque};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;
use ash::version::DeviceV1_0;

use sourcerenderer_core::graphics::{RenderGraph, AttachmentSizeClass, AttachmentInfo, StoreAction, LoadAction, PassInfo, InputAttachmentReference, RenderGraphTemplate, RenderGraphTemplateInfo, Format, SampleCount};
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

struct AttachmentPassIndices {
  last_used_in_pass_index: u32,
  produced_in_pass_index: u32
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

    let mut attachments: HashMap<&str, AttachmentPassIndices> = HashMap::new();
    for reordered_pass in &info.passes {
      match reordered_pass {
        PassInfo::Graphics {
          inputs, outputs, ..
        } => {
          for output in outputs {
            attachments.entry(output.name.as_str()).or_insert(AttachmentPassIndices {
              last_used_in_pass_index: 0,
              produced_in_pass_index: 0
            }).produced_in_pass_index = pass_index;
          }

          for input in inputs {
            match input {
              InputAttachmentReference::Texture {
                name, ..
              } => {
                attachments.entry(name.as_str()).or_insert(AttachmentPassIndices {
                  last_used_in_pass_index: 0,
                  produced_in_pass_index: 0
                }).last_used_in_pass_index = pass_index;
              },
              _ => unimplemented!()
            }
          }
        },
        _ => unimplemented!()
      }
      pass_index += 1;
    }

    let mut passes: Vec<VkPassTemplate> = Vec::new();
    let mut pass_infos = info.passes.clone();
    let mut reordered_passes = VkRenderGraphTemplate::reorder_passes(&info.attachments, &mut pass_infos);
    let mut reordered_passes_queue: VecDeque<PassInfo> = VecDeque::from_iter(reordered_passes);

    let mut pass_opt = reordered_passes_queue.pop_front();
    let mut merged_pass: Vec<PassInfo> = Vec::new();
    pass_index = 0;
    while pass_opt.is_some() {
      let pass = pass_opt.unwrap();
      let previous_pass = merged_pass.last();
      let can_be_merged = if let Some(previous_pass) = previous_pass {
        match previous_pass {
          PassInfo::Graphics {
            outputs: _, inputs
          } => {
            let mut width = 0.0f32;
            let mut height = 0.0f32;
            let mut size_class = AttachmentSizeClass::RelativeToSwapchain;

            'first_texture_input: for input in inputs {
              match input {
                InputAttachmentReference::Texture {
                  name, is_local
                } => {
                  let input_attachment = info.attachments.get(name).expect("Invalid attachment reference");
                  match input_attachment {
                    AttachmentInfo::Texture {
                      width: texture_width, height: texture_height, size_class: texture_size_class, ..
                    } => {
                      width = *texture_width;
                      height = *texture_height;
                      size_class = *texture_size_class;
                      break 'first_texture_input;
                    },
                    _ => unreachable!("Attachment type does not match reference type")
                  }
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
      } else {
        if !merged_pass.is_empty() {
          match merged_pass.first().unwrap() {
            PassInfo::Graphics {
              ..
            } => {
              // build subpasses, requires the attachment indices populated before
              let render_graph_pass = Self::build_render_pass(&merged_pass, device, &info.attachments, &mut layouts, &attachments, info.swapchain_format, info.swapchain_sample_count);
              did_render_to_backbuffer |= if let VkPassTemplate::Graphics { renders_to_swapchain, .. } = render_graph_pass { renders_to_swapchain } else { false };
              passes.push(render_graph_pass);
            },
            _ => unimplemented!()
          }

          merged_pass.clear();
        }
        merged_pass.push(pass);
      }
      pass_opt = reordered_passes_queue.pop_front();
    }

    // insert last pass
    if !merged_pass.is_empty() {
      match &merged_pass.first().unwrap() {
        PassInfo::Graphics {
          ..
        } => {

          // build subpasses, requires the attachment indices populated before
          let render_graph_pass = Self::build_render_pass(&merged_pass, device, &info.attachments, &mut layouts, &attachments, info.swapchain_format, info.swapchain_sample_count);
          did_render_to_backbuffer |= if let VkPassTemplate::Graphics { renders_to_swapchain, .. } = render_graph_pass { renders_to_swapchain } else { false };
          passes.push(render_graph_pass);
        },
        _ => unimplemented!()
      }
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

  fn build_render_pass(passes: &Vec<PassInfo>,
                       device: &Arc<RawVkDevice>,
                       attachments: &HashMap<String, AttachmentInfo>,
                       layouts: &mut HashMap<String, vk::ImageLayout>,
                       attachment_pass_indices: &HashMap<&str, AttachmentPassIndices>,
                       swapchain_format: Format,
                       swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut vk_render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    let mut vk_attachment_indices: HashMap<&str, u32> = HashMap::new();
    let mut used_attachments: Vec<String> = Vec::new();
    let mut pass_renders_to_backbuffer = false;
    let mut graph_pass_indices: Vec<u32> = Vec::new(); // pass indices in the original unordered render graph description
    let mut attachment_producer_subpass_index: HashMap<&str, u32> = HashMap::new();

    // Prepare attachments
    let mut pass_index = 0;
    for merged_pass in passes {
      let mut graph_pass_index = 0;
      match merged_pass {
        PassInfo::Graphics {
          outputs, ..
        } => {
          for output in outputs {
            let index = vk_render_pass_attachments.len() as u32;
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
                  initial_layout: *(layouts.get(&output.name).unwrap_or(&vk::ImageLayout::UNDEFINED)),
                  final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                  ..Default::default()
                }
              );
              layouts.insert(output.name.clone(), vk::ImageLayout::PRESENT_SRC_KHR);
            } else {
              let attachment = attachments.get(&output.name).expect("Output not attachment not declared.");
              match attachment {
                AttachmentInfo::Texture {
                  format: texture_attachment_format, samples: texture_attachment_samples, ..
                } => {
                  vk_render_pass_attachments.push(
                    vk::AttachmentDescription {
                      format: format_to_vk(*texture_attachment_format),
                      samples: samples_to_vk(*texture_attachment_samples),
                      load_op: load_action_to_vk(output.load_action),
                      store_op: store_action_to_vk(output.store_action),
                      stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                      stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                      initial_layout: *layouts.get(&output.name as &str).unwrap_or(&vk::ImageLayout::UNDEFINED),
                      final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                      ..Default::default()
                    }
                  );
                  layouts.insert(output.name.clone(), vk::ImageLayout::PRESENT_SRC_KHR);
                },
                _ => unreachable!()
              }
            }

            used_attachments.push(output.name.clone());
            vk_attachment_indices.insert(&output.name as &str, index);

            graph_pass_index = attachment_pass_indices.get(output.name.as_str()).unwrap().produced_in_pass_index;
            attachment_producer_subpass_index.insert(&output.name as &str, pass_index);
          }
        },
        _ => unreachable!()
      }

      graph_pass_indices.push(graph_pass_index);
      pass_index += 1;
    }

    let mut dependencies: Vec<vk::SubpassDependency> = Vec::new(); // todo
    let mut subpasses: Vec<vk::SubpassDescription> = Vec::new();
    let mut attachment_refs: Vec<vk::AttachmentReference> = Vec::new();
    let mut preserve_attachments: Vec<u32> = Vec::new();
    pass_index = 0;
    for merged_pass in passes {
      let mut graph_pass_index = 0;

      match merged_pass {
        PassInfo::Graphics {
          inputs, outputs
        } => {
          let inputs_start = attachment_refs.len() as isize;
          let inputs_len = inputs.len() as u32;
          for input in inputs {
            match input {
              InputAttachmentReference::Texture {
                is_local, name
              } => {
                attachment_refs.push(vk::AttachmentReference {
                  attachment: (*vk_attachment_indices.get(name as &str).expect(format!("Couldn't find index for {}", name).as_str())) as u32,
                  layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                });
                dependencies.push(vk::SubpassDependency {
                  src_subpass: *(attachment_producer_subpass_index.get(name as &str).unwrap()),
                  dst_subpass: pass_index,
                  src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                  dst_stage_mask: vk::PipelineStageFlags::TOP_OF_PIPE,
                  src_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                  dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ,
                  dependency_flags: if *is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
                });
              },
              _ => unimplemented!()
            }
          }

          let outputs_start = attachment_refs.len() as isize;
          let outputs_len = outputs.len() as u32;
          for output in outputs {
            graph_pass_index = attachment_pass_indices.get(output.name.as_str()).unwrap().produced_in_pass_index;

            attachment_refs.push(vk::AttachmentReference {
              attachment: (*vk_attachment_indices.get(&output.name as &str).expect(format!("Couldn't find index for {}", &output.name).as_str())),
              layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            });
          }

          for attachment in &used_attachments {
            if attachment_pass_indices.get(attachment.as_str()).unwrap().last_used_in_pass_index > graph_pass_index {
              preserve_attachments.push(*(vk_attachment_indices.get(attachment.as_str()).expect(format!("Couldn't find index for {}", attachment).as_str())));
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
        },
        _ => unreachable!()
      }
      pass_index += 1;
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

    VkPassTemplate::Graphics {
      render_pass,
      renders_to_swapchain: pass_renders_to_backbuffer,
      attachments: used_attachments,
      pass_indices: graph_pass_indices
    }
  }

  fn can_pass_be_merged(pass: &PassInfo, attachments: &HashMap<String, AttachmentInfo>, base_width: f32, base_height: f32, base_size_class: AttachmentSizeClass) -> bool {
    match pass {
      PassInfo::Graphics {
        inputs, ..
      } => {
        let mut can_be_merged = true;
        for input in inputs {
          match input {
            InputAttachmentReference::Texture {
              is_local, name
            } => {
              let input_attachment = attachments.get(name).expect("Invalid attachment reference");
              match input_attachment {
                AttachmentInfo::Texture {
                  size_class, width, height, ..
                } => {
                  can_be_merged &= *is_local && *size_class == base_size_class && (*width - base_width).abs() < 0.01f32 && (*height - base_height).abs() < 0.01f32;
                },
                _ => panic!("Attachment type does not match reference type")
              }
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
        PassInfo::Graphics{
          outputs, ..
        } => {
          for output in outputs {
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
        PassInfo::Graphics {
          outputs: last_graphics_pass_outputs, ..
        } => {
          let last_pass_output = last_graphics_pass_outputs.first().expect("Pass has no outputs");
          if &last_pass_output.name != &BACK_BUFFER_ATTACHMENT_NAME {
            let attachment = attachments.get(&last_pass_output.name).expect("Invalid attachment reference");

            match attachment {
              AttachmentInfo::Texture {
                width: texture_attachment_width, height: texture_attachment_height, size_class: texture_attachment_size_class, ..
              } => {
                width = *texture_attachment_width;
                height = *texture_attachment_height;
                size_class = *texture_attachment_size_class;
              }
              _ => unreachable!()
            }
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
        PassInfo::Graphics {
          inputs, outputs, ..
        } => {
          for input in inputs {
            match input {
              InputAttachmentReference::Texture {
                is_local, name
              } => {
                let input_attachment = attachments.get(name).expect("Invalid attachment reference");
                match input_attachment {
                  AttachmentInfo::Texture {
                    size_class: texture_attachment_size_class, width: texture_attachment_width, height: texture_attachment_height, ..
                  } => {
                    can_be_merged &= *is_local && *texture_attachment_size_class == size_class && (*texture_attachment_width - width).abs() < 0.01f32 && (*texture_attachment_height - height).abs() < 0.01f32;
                    let index_opt = attachment_indices.get(name);
                    if let Some(index) = index_opt {
                      passes_since_ready = min(*index, passes_since_ready);
                    } else {
                      is_ready = false;
                    }
                  },
                  _ => panic!("Mismatching attachment types")
                }
              },
              _ => {
                can_be_merged = false;
              }
            }
          }

          let first_output = outputs.first().expect("Pass has no outputs");
          let (output_width, output_height, output_size_class) = if &first_output.name != &BACK_BUFFER_ATTACHMENT_NAME {
            let first_output_attachment = attachments.get(&first_output.name).expect("Invalid attachment reference");
            match first_output_attachment {
              AttachmentInfo::Texture {
                width: first_output_texture_width, height: first_output_texture_height, size_class: first_output_texture_size_class, ..
              } => {
                (*first_output_texture_width, *first_output_texture_height, *first_output_texture_size_class)
              },
              _ => unreachable!()
            }
          } else {
            (1.0f32, 1.0f32, AttachmentSizeClass::RelativeToSwapchain)
          };

          for output in outputs {
            let (width, height, size_class) = if &output.name == &BACK_BUFFER_ATTACHMENT_NAME {
              (1.0f32, 1.0f32, AttachmentSizeClass::RelativeToSwapchain)
            } else {
              let attachment = attachments.get(&output.name).expect("Invalid attachment reference");
              match attachment {
                AttachmentInfo::Texture {
                  width: output_texture_width, height: output_texture_height, size_class: output_texture_size_class, ..
                } => {
                  (*output_texture_width, *output_texture_height, *output_texture_size_class)
                },
                _ => unreachable!()
              }
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
