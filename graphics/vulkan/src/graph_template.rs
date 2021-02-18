use sourcerenderer_core::graphics::{Output, RenderPassTextureExtent, ExternalOutput, ExternalProducerType, DepthStencil, PipelineStage};
use std::collections::{HashMap, VecDeque};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;

use sourcerenderer_core::graphics::{StoreAction, LoadAction, PassInfo, PassInput, RenderGraphTemplate, RenderGraphTemplateInfo, Format, SampleCount, GraphicsSubpassInfo, PassType, SubpassOutput};
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;
use crate::raw::RawVkDevice;

use crate::format::format_to_vk;
use crate::pipeline::samples_to_vk;
use crate::VkRenderPass;
use std::cmp::min;
use std::iter::FromIterator;
use ash::vk::{AttachmentStoreOp, AttachmentLoadOp};

const EMPTY_PIPELINE_STAGE_FLAGS: vk::PipelineStageFlags = vk::PipelineStageFlags::empty();

pub struct VkRenderGraphTemplate {
  pub device: Arc<RawVkDevice>,
  pub does_render_to_frame_buffer: bool,
  pub passes: Vec<VkPassTemplate>,
  pub resources: HashMap<String, ResourceMetadata>
}

pub struct VkPassTemplate {
  pub name: String,
  pub pass_type: VkPassType,
  pub renders_to_swapchain: bool,
  pub has_history_resources: bool,
  pub has_external_resources: bool,
  pub has_backbuffer: bool,
  pub resources: HashSet<String>,
}

#[derive(Clone)]
pub enum VkBarrierTemplate {
  Image {
    name: String,
    is_history: bool,
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
    is_history: bool,
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
    attachments: Vec<String>,
    barriers: Vec<VkBarrierTemplate>
  },
  ComputeCopy {
    barriers: Vec<VkBarrierTemplate>,
    is_compute: bool
  }
}

#[derive(Clone)]
pub enum VkResourceTemplate {
  Texture {
    name: String,
    format: Format,
    samples: SampleCount,
    extent: RenderPassTextureExtent,
    depth: u32,
    levels: u32,
    external: bool,
    load_action: LoadAction,
    store_action: StoreAction,
    stencil_load_action: LoadAction,
    stencil_store_action: StoreAction,
    is_backbuffer: bool
  },
  Buffer {
    name: String,
    format: Option<Format>,
    size: u32,
    clear: bool
  },
  ExternalBuffer,
  ExternalTexture {
    is_depth_stencil: bool
  },
}

#[derive(Clone)]
pub struct ResourceHistoryUsage {
  pub(super) first_used_in_pass_index: u32,
  pub(super) last_used_in_pass_index: u32
}

#[derive(Clone)]
pub struct ResourceMetadata {
  pub(super) template: VkResourceTemplate,
  pub(super) last_used_in_pass_index: u32,
  pub(super) history_usage: Option<ResourceHistoryUsage>,
  pub(super) produced_in_pass_index: u32,
  pub(super) layout: vk::ImageLayout,
  pub(super) used_in_stages: vk::PipelineStageFlags,
  is_dirty: bool,
  current_pipeline_stage: vk::PipelineStageFlags
}

impl ResourceMetadata {
  fn new(template: VkResourceTemplate) -> Self {
    ResourceMetadata {
      template,
      last_used_in_pass_index: 0,
      history_usage: None,
      produced_in_pass_index: 0,
      layout: vk::ImageLayout::UNDEFINED,
      used_in_stages: vk::PipelineStageFlags::empty(),
      is_dirty: true,
      current_pipeline_stage: vk::PipelineStageFlags::empty()
    }
  }
}

struct SubpassAttachmentMetadata {
  produced_in_subpass_index: u32,
  render_pass_attachment_index: u32,
  last_used_in_subpass_index: u32
}

impl Default for SubpassAttachmentMetadata {
  fn default() -> Self {
    Self {
      produced_in_subpass_index: vk::SUBPASS_EXTERNAL,
      render_pass_attachment_index: u32::MAX,
      last_used_in_subpass_index: 0
    }
  }
}

impl VkRenderGraphTemplate {
  pub fn new(device: &Arc<RawVkDevice>,
             info: &RenderGraphTemplateInfo) -> Self {

    let mut did_render_to_backbuffer = false;

    // TODO: figure out threading
    // TODO: more generic support for external images / one time rendering
    // TODO: (async) compute

    let mut attachment_metadata = HashMap::<String, ResourceMetadata>::new();
    let mut passes: Vec<VkPassTemplate> = Vec::new();
    let mut pass_infos = info.passes.clone();
    let reordered_passes = VkRenderGraphTemplate::reorder_passes(&mut pass_infos, &mut attachment_metadata, &info.external_resources, info.swapchain_format, info.swapchain_sample_count);

    let mut reordered_passes_queue: VecDeque<PassInfo> = VecDeque::from_iter(reordered_passes);
    let mut pass_index: u32 = 0;
    let mut pass_opt = reordered_passes_queue.pop_front();
    while let Some(pass) = pass_opt {
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
          let render_graph_pass = Self::build_compute_copy_pass(inputs, outputs, &pass.name, device, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count, true);
          passes.push(render_graph_pass);
        },
        PassType::Copy {
          inputs, outputs
        } => {
          let render_graph_pass = Self::build_compute_copy_pass(inputs, outputs, &pass.name, device, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count, false);
          passes.push(render_graph_pass);
        }
      }
      pass_opt = reordered_passes_queue.pop_front();
      pass_index += 1;
    }

    for pass in &mut passes {
      let barriers = match &mut pass.pass_type {
        VkPassType::Graphics {
          barriers, ..
        } => barriers,
        VkPassType::ComputeCopy {
          barriers, is_compute: _
        } => barriers
      };
      for barrier in barriers {
        match barrier {
          VkBarrierTemplate::Image {
            name, is_history, old_layout, ..
          } => {
            let metadata = attachment_metadata.get(name.as_str()).unwrap();
            if *is_history {
              *old_layout = metadata.layout;
            }
          }
          _ => {}
        }
      }
    }

    Self {
      device: device.clone(),
      passes,
      does_render_to_frame_buffer: did_render_to_backbuffer,
      resources: attachment_metadata
    }
  }

  pub(crate) fn passes(&self) -> &[VkPassTemplate] {
    &self.passes
  }

  pub(crate) fn resources(&self) -> &HashMap<String, ResourceMetadata> {
    &self.resources
  }

  pub(crate) fn renders_to_swapchain(&self) -> bool {
    self.does_render_to_frame_buffer
  }

  fn reorder_passes(passes: &Vec<PassInfo>,
                    metadata: &mut HashMap<String, ResourceMetadata>,
                    external_resources: &Vec<ExternalOutput>,
                    swapchain_format: Format,
                    swapchain_samples: SampleCount) -> Vec<PassInfo> {
    let mut passes_mut = passes.clone();
    let mut reordered_passes = vec![];

    for external in external_resources {
      match external {
        ExternalOutput::Buffer {
          name, producer_type
        } => {
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalBuffer,
            last_used_in_pass_index: 0,
            history_usage: None,
            produced_in_pass_index: 0,
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
            },
            used_in_stages: vk::PipelineStageFlags::empty(),
            is_dirty: true,
            current_pipeline_stage: match producer_type {
              ExternalProducerType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              ExternalProducerType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              ExternalProducerType::Copy => vk::PipelineStageFlags::TRANSFER,
              ExternalProducerType::Host => vk::PipelineStageFlags::HOST
            }
          });
        }
        ExternalOutput::RenderTarget {
          name, producer_type
        } => {
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalTexture { is_depth_stencil: false },
            last_used_in_pass_index: 0,
            history_usage: None,
            produced_in_pass_index: 0,
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
            },
            used_in_stages: vk::PipelineStageFlags::empty(),
            is_dirty: true,
            current_pipeline_stage: match producer_type {
              ExternalProducerType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              ExternalProducerType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              ExternalProducerType::Copy => vk::PipelineStageFlags::TRANSFER,
              ExternalProducerType::Host => vk::PipelineStageFlags::HOST
            }
          });
        }
        ExternalOutput::DepthStencil {
          name, producer_type
        } => {
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalTexture { is_depth_stencil: true },
            last_used_in_pass_index: 0,
            history_usage: None,
            produced_in_pass_index: 0,
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
            },
            used_in_stages: vk::PipelineStageFlags::empty(),
            is_dirty: true,
            current_pipeline_stage: match producer_type {
              ExternalProducerType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              ExternalProducerType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              ExternalProducerType::Copy => vk::PipelineStageFlags::TRANSFER,
              ExternalProducerType::Host => vk::PipelineStageFlags::HOST
            }
          });
        }
      }
    }

    while !passes_mut.is_empty() {
      let pass = VkRenderGraphTemplate::find_next_suitable_pass(&mut passes_mut, &metadata);
      match &pass.pass_type {
        PassType::Graphics {
          subpasses
        } => {
          for subpass in subpasses {
            for output in &subpass.outputs {
              match output {
                SubpassOutput::RenderTarget {
                  name: rt_name, format: rt_format, samples: rt_samples,
                  extent: rt_extent, depth: rt_depth, levels: rt_levels,
                  load_action: rt_load_action, store_action: rt_store_action, external: rt_external
                } => {
                  metadata
                      .entry(rt_name.to_string())
                      .or_insert_with(|| ResourceMetadata::new(VkResourceTemplate::Texture {
                        name: rt_name.clone(),
                        format: *rt_format,
                        samples: *rt_samples,
                        extent: rt_extent.clone(),
                        depth: *rt_depth,
                        levels: *rt_levels,
                        external: *rt_external,
                        load_action: *rt_load_action,
                        store_action: *rt_store_action,
                        stencil_load_action: LoadAction::DontCare,
                        stencil_store_action: StoreAction::DontCare,
                        is_backbuffer: false
                      }))
                      .produced_in_pass_index = reordered_passes.len() as u32;
                },
                SubpassOutput::Backbuffer {
                  clear: backbuffer_clear
                } => {
                  metadata
                      .entry(BACK_BUFFER_ATTACHMENT_NAME.to_string())
                      .or_insert_with(|| ResourceMetadata::new(VkResourceTemplate::Texture {
                        name: BACK_BUFFER_ATTACHMENT_NAME.to_string(),
                        format: swapchain_format,
                        samples: swapchain_samples,
                        extent: RenderPassTextureExtent::RelativeToSwapchain { width: 1.0f32, height: 1.0f32 },
                        depth: 1,
                        levels: 1,
                        external: false,
                        load_action: if *backbuffer_clear { LoadAction::Clear } else { LoadAction::DontCare },
                        store_action: StoreAction::Store,
                        stencil_load_action: LoadAction::DontCare,
                        stencil_store_action: StoreAction::DontCare,
                        is_backbuffer: true
                      }))
                      .produced_in_pass_index = reordered_passes.len() as u32;
                }
              }
            }

            match &subpass.depth_stencil {
              DepthStencil::Output {
                name: ds_name,
                samples: ds_samples,
                extent: ds_extent,
                format: ds_format,
                depth_load_action,
                depth_store_action,
                stencil_load_action,
                stencil_store_action
              } => {
                metadata
                    .entry(ds_name.to_string())
                    .or_insert_with(|| ResourceMetadata::new(VkResourceTemplate::Texture {
                      name: ds_name.clone(),
                      format: *ds_format,
                      samples: *ds_samples,
                      extent: ds_extent.clone(),
                      depth: 1,
                      levels: 1,
                      external: false,
                      load_action: *depth_load_action,
                      store_action: *depth_store_action,
                      stencil_load_action: *stencil_load_action,
                      stencil_store_action: *stencil_store_action,
                      is_backbuffer: false
                    }))
                    .produced_in_pass_index = reordered_passes.len() as u32;
              }
              DepthStencil::Input {
                name: ds_name, is_history, ..
              } => {
                let mut input_metadata = metadata.get_mut(ds_name).unwrap();
                if *is_history {
                  if let Some(history_usage) = input_metadata.history_usage.as_mut() {
                    if history_usage.first_used_in_pass_index > reordered_passes.len() as u32 {
                      history_usage.first_used_in_pass_index = reordered_passes.len() as u32;
                    }
                    if history_usage.last_used_in_pass_index < reordered_passes.len() as u32 {
                      history_usage.last_used_in_pass_index = reordered_passes.len() as u32;
                    }
                  } else {
                    input_metadata.history_usage = Some(ResourceHistoryUsage {
                      first_used_in_pass_index: reordered_passes.len() as u32,
                      last_used_in_pass_index: reordered_passes.len() as u32,
                    });
                  }
                } else {
                  input_metadata.last_used_in_pass_index = reordered_passes.len() as u32;
                }
                input_metadata.used_in_stages |= vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS;
              }
              DepthStencil::None => {}
            }

            for input in &subpass.inputs {
              let mut input_metadata = metadata.get_mut(&input.name).unwrap();
              if input.is_history {
                if let Some(history_usage) = input_metadata.history_usage.as_mut() {
                  if history_usage.first_used_in_pass_index > reordered_passes.len() as u32 {
                    history_usage.first_used_in_pass_index = reordered_passes.len() as u32;
                  }
                  if history_usage.last_used_in_pass_index < reordered_passes.len() as u32 {
                    history_usage.last_used_in_pass_index = reordered_passes.len() as u32;
                  }
                } else {
                  input_metadata.history_usage = Some(ResourceHistoryUsage {
                    first_used_in_pass_index: reordered_passes.len() as u32,
                    last_used_in_pass_index: reordered_passes.len() as u32,
                  });
                }
              } else {
                input_metadata.last_used_in_pass_index = reordered_passes.len() as u32;
              }
              assert_ne!(input.stage, PipelineStage::ComputeShader);
              input_metadata.used_in_stages |= pipeline_stage_to_vk(input.stage);
            }
          }
        },
        PassType::Compute {
          inputs, outputs
        } => {
          for output in outputs {
            match output {
              Output::RenderTarget {
                name, format, samples, extent, depth, levels, external, clear
              } => {
                let metadata_entry = metadata
                    .entry(name.clone())
                    .or_insert_with(|| ResourceMetadata::new(VkResourceTemplate::Texture {
                      name: name.clone(),
                      format: *format,
                      samples: *samples,
                      extent: extent.clone(),
                      depth: *depth,
                      levels: *levels,
                      external: *external,
                      load_action: if *clear { LoadAction::Clear } else { LoadAction::DontCare },
                      store_action: StoreAction::Store,
                      stencil_load_action: LoadAction::DontCare,
                      stencil_store_action: StoreAction::DontCare,
                      is_backbuffer: false
                    }));
                metadata_entry.produced_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.current_pipeline_stage = vk::PipelineStageFlags::COMPUTE_SHADER;
              },
              Output::Backbuffer {
                clear
              } => {
                let metadata_entry = metadata
                    .entry(BACK_BUFFER_ATTACHMENT_NAME.to_string())
                    .or_insert_with(|| ResourceMetadata::new(VkResourceTemplate::Texture {
                      name: BACK_BUFFER_ATTACHMENT_NAME.to_string(),
                      format: Format::Unknown,
                      samples: SampleCount::Samples1,
                      extent: RenderPassTextureExtent::RelativeToSwapchain { width: 1.0f32, height: 1.0f32 },
                      depth: 1,
                      levels: 1,
                      external: false,
                      load_action: if *clear { LoadAction::Load } else { LoadAction::Clear },
                      store_action: StoreAction::Store,
                      stencil_load_action: LoadAction::DontCare,
                      stencil_store_action: StoreAction::DontCare,
                      is_backbuffer: true
                    }));
                metadata_entry.produced_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.current_pipeline_stage = vk::PipelineStageFlags::COMPUTE_SHADER;
              },
              Output::Buffer {
                name, format, size, clear
              } => {
                let metadata_entry = metadata
                    .entry(name.to_string())
                    .or_insert_with(|| ResourceMetadata::new(VkResourceTemplate::Buffer {
                      name: name.clone(),
                      format: *format,
                      size: *size,
                      clear: *clear
                    }));
                metadata_entry.produced_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.current_pipeline_stage = vk::PipelineStageFlags::COMPUTE_SHADER;
              }
              _ => {}
            }
          }

          for input in inputs {
            let mut input_metadata = metadata.get_mut(&input.name).unwrap();
            if input.is_history {
              if let Some(history_usage) = input_metadata.history_usage.as_mut() {
                if history_usage.first_used_in_pass_index > reordered_passes.len() as u32 {
                  history_usage.first_used_in_pass_index = reordered_passes.len() as u32;
                }
                if history_usage.last_used_in_pass_index < reordered_passes.len() as u32 {
                  history_usage.last_used_in_pass_index = reordered_passes.len() as u32;
                }
              } else {
                input_metadata.history_usage = Some(ResourceHistoryUsage {
                  first_used_in_pass_index: reordered_passes.len() as u32,
                  last_used_in_pass_index: reordered_passes.len() as u32,
                });
              }
            } else {
              input_metadata.last_used_in_pass_index = reordered_passes.len() as u32;
            }
            assert_eq!(input.stage, PipelineStage::ComputeShader);
            input_metadata.used_in_stages |= vk::PipelineStageFlags::COMPUTE_SHADER;
          }
        },
        _ => unimplemented!()
      }
      reordered_passes.push(pass);
    }
    return reordered_passes;
  }

  #[allow(unused_assignments, unused_variables)] // TODO
  fn build_render_pass(passes: &Vec<GraphicsSubpassInfo>,
                       name: &str,
                       device: &Arc<RawVkDevice>,
                       pass_index: u32,
                       attachment_metadata: &mut HashMap<String, ResourceMetadata>,
                       swapchain_format: Format,
                       swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut vk_render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    let mut subpass_attachment_metadata: HashMap<&str, SubpassAttachmentMetadata> = HashMap::new();
    let mut pass_renders_to_backbuffer = false;
    let mut pass_has_history_resources = false;
    let mut pass_has_external_resources = false;
    let mut barriers = Vec::<VkBarrierTemplate>::new();
    let mut use_external_subpass_dependencies = false;

    // Prepare attachments
    // build list of VkAttachmentDescriptions and collect metadata like produced_in_subpass_index
    for (subpass_index, pass) in passes.iter().enumerate() {
      for output in &pass.outputs {
        match output {
          SubpassOutput::Backbuffer { clear } => {
            let metadata = attachment_metadata.get_mut(BACK_BUFFER_ATTACHMENT_NAME).unwrap();
            pass_has_history_resources |= metadata.history_usage.is_some();
            let mut subpass_metadata = subpass_attachment_metadata.entry(BACK_BUFFER_ATTACHMENT_NAME)
                .or_default();
            subpass_metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
            subpass_metadata.produced_in_subpass_index = subpass_index as u32;

            pass_renders_to_backbuffer = true;
            pass_has_external_resources |= true;
            vk_render_pass_attachments.push(
              vk::AttachmentDescription {
                format: format_to_vk(swapchain_format),
                samples: samples_to_vk(swapchain_samples),
                load_op: if *clear { vk::AttachmentLoadOp::CLEAR } else { vk::AttachmentLoadOp::DONT_CARE },
                store_op: vk::AttachmentStoreOp::STORE,
                stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: metadata.layout,
                final_layout: vk::ImageLayout::UNDEFINED,
                ..Default::default()
              }
            );
          },

          SubpassOutput::RenderTarget {
            name, format, samples, external, load_action, store_action, ..
          } => {
            let metadata = attachment_metadata.get(name.as_str()).unwrap();
            pass_has_history_resources |= metadata.history_usage.is_some();
            pass_has_external_resources |= *external;
            let mut subpass_metadata = subpass_attachment_metadata.entry(name.as_str())
                .or_default();
            if format.is_depth() {
              panic!("Output attachment must not have a depth stencil format");
            }
            subpass_metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
            subpass_metadata.produced_in_subpass_index = subpass_index as u32;

            vk_render_pass_attachments.push(
              vk::AttachmentDescription {
                format: format_to_vk(*format),
                samples: samples_to_vk(*samples),
                load_op: load_action_to_vk(*load_action),
                store_op: store_action_to_vk(*store_action),
                stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: metadata.layout,
                final_layout: vk::ImageLayout::UNDEFINED, // will be filled in later
                ..Default::default()
              }
            );
          }
        }
      }

      match &pass.depth_stencil {
        DepthStencil::Output {
          name: ds_name,
          format: ds_format,
          samples: ds_samples,
          depth_load_action,
          depth_store_action,
          stencil_load_action,
          stencil_store_action,
          ..
        } => {
          let metadata = attachment_metadata.get(ds_name.as_str()).unwrap();
          pass_has_history_resources |= metadata.history_usage.is_some();
          let mut subpass_metadata = subpass_attachment_metadata.entry(ds_name.as_str())
              .or_default();
          subpass_metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
          subpass_metadata.produced_in_subpass_index = subpass_index as u32;

          if !ds_format.is_depth() {
            panic!("Depth stencil attachment must have a depth stencil format");
          }

          vk_render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(*ds_format),
              samples: samples_to_vk(*ds_samples),
              load_op: load_action_to_vk(*depth_load_action),
              store_op: store_action_to_vk(*depth_store_action),
              stencil_load_op: load_action_to_vk(*stencil_load_action),
              stencil_store_op: store_action_to_vk(*stencil_store_action),
              initial_layout: metadata.layout,
              final_layout: vk::ImageLayout::UNDEFINED, // will be filled in later
              ..Default::default()
            }
          );
        }

        DepthStencil::Input {
          name: ds_name, ..
        } => {
          let metadata = attachment_metadata.get(ds_name.as_str()).expect("Can not find attachment.");
          pass_has_history_resources |= metadata.history_usage.is_some();
          let (format, samples) = match &metadata.template {
            VkResourceTemplate::Texture { format, samples, .. } => { (*format, *samples) }
            VkResourceTemplate::Buffer { .. } => { unreachable!() }
            VkResourceTemplate::ExternalBuffer => { unreachable!() }
            VkResourceTemplate::ExternalTexture { .. } => {
              pass_has_external_resources |= true;
              unimplemented!()
            }
          };
          let mut subpass_metadata = subpass_attachment_metadata.entry(ds_name.as_str())
              .or_default();
          subpass_metadata.last_used_in_subpass_index = subpass_index as u32;
          subpass_metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;

          if !format.is_depth() {
            panic!("Depth stencil attachment must have a depth stencil format");
          }

          vk_render_pass_attachments.push(
            vk::AttachmentDescription {
              format: format_to_vk(format),
              samples: samples_to_vk(samples),
              load_op: AttachmentLoadOp::LOAD,
              store_op: AttachmentStoreOp::STORE,
              stencil_load_op: AttachmentLoadOp::LOAD,
              stencil_store_op: AttachmentStoreOp::STORE,
              initial_layout: metadata.layout,
              final_layout: vk::ImageLayout::UNDEFINED, // will be filled in later
              ..Default::default()
            }
          );
        }

        DepthStencil::None => {}
      }

      for input in &pass.inputs {
        let metadata = attachment_metadata.get(input.name.as_str()).expect("Can not find attachment.");
        pass_has_history_resources |= metadata.history_usage.is_some();
        let mut subpass_metadata = subpass_attachment_metadata.entry(input.name.as_str())
          .or_default();
        if subpass_index as u32 > subpass_metadata.last_used_in_subpass_index {
          subpass_metadata.last_used_in_subpass_index = subpass_index as u32;
        }

        let mut is_buffer = false;
        match metadata.template {
          VkResourceTemplate::ExternalBuffer { .. } => {
            is_buffer = true;
            pass_has_external_resources |= true;
          }
          VkResourceTemplate::ExternalTexture { .. } => {
            pass_has_external_resources |= true;
          }
          VkResourceTemplate::Buffer { .. } => {
            is_buffer = true;
          }
          _ => {}
        }
        use_external_subpass_dependencies &= !input.is_history && (is_buffer || subpass_metadata.produced_in_subpass_index != vk::SUBPASS_EXTERNAL || (metadata.layout == vk::ImageLayout::UNDEFINED || metadata.layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL));
      }
    }

    // Prepare barriers using previously set up metadata
    let mut dependencies: Vec<vk::SubpassDependency> = Vec::new(); // todo
    let mut subpasses: Vec<vk::SubpassDescription> = Vec::new();
    let mut attachment_refs: Vec<vk::AttachmentReference> = Vec::new();
    let mut preserve_attachments: Vec<u32> = Vec::new();
    for (subpass_index, pass) in passes.iter().enumerate() {
      let inputs_start = attachment_refs.len() as isize;
      let mut inputs_len = 0;
      for input in &pass.inputs {
        let metadata = attachment_metadata.get_mut(input.name.as_str()).unwrap();
        let subpass_metadata = &subpass_attachment_metadata[input.name.as_str()];

        if subpass_metadata.produced_in_subpass_index != vk::SUBPASS_EXTERNAL || use_external_subpass_dependencies {
          match &metadata.template {
            VkResourceTemplate::Texture { format, is_backbuffer, .. } => {
              if *is_backbuffer {
                panic!("Using the backbuffer as a pass input is not allowed.");
              }
              let is_depth_stencil = format.is_depth() || format.is_stencil();
              let dependency = Self::build_texture_subpass_dependency(
                subpass_index as u32,
                metadata,
                subpass_metadata,
                is_depth_stencil,
                input.is_local
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
              }
            }

            VkResourceTemplate::Buffer { .. } => {
              let dependency = Self::build_buffer_subpass_dependency(
                subpass_index as u32,
                metadata,
                subpass_metadata
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
              }
            }

            VkResourceTemplate::ExternalTexture { is_depth_stencil, .. } => {
              let is_depth_stencil = *is_depth_stencil;
              let dependency = Self::build_texture_subpass_dependency(
                subpass_index as u32,
                metadata,
                subpass_metadata,
                is_depth_stencil,
                false
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
              }
            }

            VkResourceTemplate::ExternalBuffer { .. } => {
              let dependency = Self::build_buffer_subpass_dependency(
                subpass_index as u32,
                metadata,
                subpass_metadata
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
              }
            }
          }
        } else {
          match &metadata.template {
            VkResourceTemplate::Texture { format, is_backbuffer, .. } => {
              if *is_backbuffer {
                panic!("Using the backbuffer as a pass input is not allowed.");
              }
              let is_depth_stencil = format.is_depth() || format.is_stencil();
              let barrier = Self::build_texture_barrier(
                &input.name,
                metadata,
                is_depth_stencil,
                input.is_history,
                vk::PipelineStageFlags::ALL_GRAPHICS,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
                vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].initial_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
              }
            },
            VkResourceTemplate::Buffer { .. } => {
              let barrier = Self::build_buffer_barrier(
                &input.name,
                metadata,
                input.is_history,
                vk::PipelineStageFlags::ALL_GRAPHICS
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
              }
            }
            VkResourceTemplate::ExternalBuffer => {
              let barrier = Self::build_buffer_barrier(
                &input.name,
                metadata,
                input.is_history,
                vk::PipelineStageFlags::ALL_GRAPHICS
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
              }
            }
            VkResourceTemplate::ExternalTexture { is_depth_stencil } => {
              let is_depth_stencil = *is_depth_stencil;
              let barrier = Self::build_texture_barrier(
                &input.name,
                metadata,
                is_depth_stencil,
                input.is_history,
                vk::PipelineStageFlags::ALL_GRAPHICS,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
                vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].initial_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
              }
            }
          }
        }

        match &metadata.template {
          VkResourceTemplate::Texture { .. } => {}
          _ => { continue; }
        }
        attachment_refs.push(vk::AttachmentReference {
          attachment: subpass_metadata.render_pass_attachment_index,
          layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        });
        vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        inputs_len += 1;
      }

      let outputs_start = attachment_refs.len() as isize;
      let outputs_len = pass.outputs.len() as u32;
      for output in &pass.outputs {
        match output {
          SubpassOutput::RenderTarget { name: rt_name, .. } => {
            let metadata = attachment_metadata.get_mut(rt_name.as_str()).unwrap();
            let subpass_metadata = &subpass_attachment_metadata[rt_name.as_str()];
            attachment_refs.push(vk::AttachmentReference {
              attachment: subpass_metadata.render_pass_attachment_index,
              layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            });

            if use_external_subpass_dependencies {
              let dependency = Self::build_texture_subpass_dependency(
                subpass_index as u32,
                metadata,
                subpass_metadata,
                false,
                false
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
              }
            } else {
              let barrier = Self::build_texture_barrier(
                rt_name,
                metadata,
                false,
                false,
                vk::PipelineStageFlags::ALL_GRAPHICS,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
                vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].initial_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
              }
            }

            vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
          },
          SubpassOutput::Backbuffer {
            ..
          } => {
            let metadata = attachment_metadata.get_mut(BACK_BUFFER_ATTACHMENT_NAME).unwrap();
            let subpass_metadata = &subpass_attachment_metadata[BACK_BUFFER_ATTACHMENT_NAME];
            attachment_refs.push(vk::AttachmentReference {
              attachment: subpass_metadata.render_pass_attachment_index,
              layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            });

            if use_external_subpass_dependencies {
              let dependency = Self::build_texture_subpass_dependency(
                subpass_index as u32,
                metadata,
                subpass_metadata,
                false,
                false
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
              }
            } else {
              let barrier = Self::build_texture_barrier(
                BACK_BUFFER_ATTACHMENT_NAME,
                metadata,
                false,
                false,
                vk::PipelineStageFlags::ALL_GRAPHICS,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
                vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].initial_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
              }
            }
            vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::PRESENT_SRC_KHR;
            metadata.layout = vk::ImageLayout::PRESENT_SRC_KHR;
            metadata.current_pipeline_stage = vk::PipelineStageFlags::ALL_GRAPHICS;
          }
        }
      }

      let depth_stencil_start = match &pass.depth_stencil {
        DepthStencil::Output {
          name: ds_name,
          ..
        } => {
          let metadata = attachment_metadata.get_mut(ds_name.as_str()).unwrap();
          let subpass_metadata = &subpass_attachment_metadata[ds_name.as_str()];
          let index = attachment_refs.len();
          attachment_refs.push(vk::AttachmentReference {
            attachment: subpass_metadata.render_pass_attachment_index,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
          });
          if use_external_subpass_dependencies {
            let dependency = Self::build_texture_subpass_dependency(
              subpass_index as u32,
              metadata,
              subpass_metadata,
              true,
              false
            );
            if let Some(dependency) = dependency {
              dependencies.push(dependency);
              metadata.layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
            }
          } else {
            let barrier = Self::build_texture_barrier(
              ds_name,
              metadata,
              true,
              false,
              vk::PipelineStageFlags::ALL_GRAPHICS,
              vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
            );
            if let Some(barrier) = barrier {
              barriers.push(barrier);
              vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].initial_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
            }
          }
          vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
          Some(index as isize)
        }
        DepthStencil::Input {
          name: ds_name, ..
        } => {
          let metadata = attachment_metadata.get_mut(ds_name.as_str()).unwrap();
          let subpass_metadata = &subpass_attachment_metadata[ds_name.as_str()];
          let index = attachment_refs.len();
          attachment_refs.push(vk::AttachmentReference {
            attachment: subpass_metadata.render_pass_attachment_index,
            layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
          });
          if use_external_subpass_dependencies {
            let dependency = Self::build_texture_subpass_dependency(
              subpass_index as u32,
              metadata,
              subpass_metadata,
              true,
              false
            );
            if let Some(dependency) = dependency {
              dependencies.push(dependency);
              metadata.layout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
            }
          } else {
            let barrier = Self::build_texture_barrier(
              ds_name,
              metadata,
              true,
              false,
              vk::PipelineStageFlags::ALL_GRAPHICS,
              vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
            );
            if let Some(barrier) = barrier {
              barriers.push(barrier);
              vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].initial_layout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
            }
          }
          vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
          Some(index as isize)
        }
        DepthStencil::None => {
          None
        }
      };

      for (name, subpass_metadata) in subpass_attachment_metadata.iter() {
        let mut is_used = false;
        for input in &pass.inputs {
          is_used |= input.name.as_str() == *name;
        }

        let metadata = attachment_metadata.get(*name).unwrap();
        if !is_used && subpass_metadata.produced_in_subpass_index < subpass_index as u32
            && (metadata.last_used_in_pass_index > pass_index || metadata.history_usage.is_some() && metadata.history_usage.as_ref().unwrap().last_used_in_pass_index > pass_index || subpass_metadata.last_used_in_subpass_index > subpass_index as u32) {
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
      has_history_resources: pass_has_history_resources,
      has_external_resources: pass_has_external_resources,
      has_backbuffer: false,
      resources: used_resources,
      name: name.to_owned(),
      pass_type: VkPassType::Graphics {
        render_pass,
        attachments: used_attachments,
        barriers
      }
    }
  }

  fn build_texture_subpass_dependency(subpass_index: u32, resource_metadata: &mut ResourceMetadata, subpass_metadata: &SubpassAttachmentMetadata, is_depth_stencil_format: bool, is_local: bool) -> Option<vk::SubpassDependency> {
    let old_stage = std::mem::replace(&mut resource_metadata.current_pipeline_stage, vk::PipelineStageFlags::ALL_GRAPHICS);
    let dirty = std::mem::replace(&mut resource_metadata.is_dirty, false);
    let discard = resource_metadata.layout == vk::ImageLayout::UNDEFINED;
    if discard {
      return None;
    }
    Some(vk::SubpassDependency {
      src_subpass: subpass_metadata.produced_in_subpass_index,
      dst_subpass: subpass_index,
      src_stage_mask: old_stage,
      dst_stage_mask: if dirty {
        resource_metadata.used_in_stages
      } else {
        resource_metadata.current_pipeline_stage
      },
      src_access_mask: if !dirty {
        vk::AccessFlags::empty()
      } else {
        match old_stage {
          EMPTY_PIPELINE_STAGE_FLAGS => vk::AccessFlags::empty(),
          vk::PipelineStageFlags::ALL_GRAPHICS => if is_depth_stencil_format {
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
          } else {
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
          },
          vk::PipelineStageFlags::COMPUTE_SHADER => vk::AccessFlags::SHADER_WRITE,
          vk::PipelineStageFlags::TRANSFER => vk::AccessFlags::TRANSFER_WRITE,
          vk::PipelineStageFlags::HOST => vk::AccessFlags::HOST_WRITE,
          vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS => vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
          _ => panic!("Unsupported value for current_pipeline_stage")
        }
      },
      dst_access_mask: if !dirty {
        vk::AccessFlags::empty()
      } else if is_local {
        vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
      } else {
        vk::AccessFlags::SHADER_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
      }
      ,
      dependency_flags: if is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
    })
  }

  fn build_buffer_subpass_dependency(subpass_index: u32, resource_metadata: &mut ResourceMetadata, subpass_metadata: &SubpassAttachmentMetadata) -> Option<vk::SubpassDependency> {
    let old_stage = std::mem::replace(&mut resource_metadata.current_pipeline_stage, vk::PipelineStageFlags::ALL_GRAPHICS);
    if subpass_metadata.produced_in_subpass_index != vk::SUBPASS_EXTERNAL && resource_metadata.is_dirty {
      return None;
    }
    Some(vk::SubpassDependency {
      src_subpass: subpass_metadata.produced_in_subpass_index,
      dst_subpass: subpass_index,
      src_stage_mask: old_stage,
      dst_stage_mask: resource_metadata.used_in_stages,
      src_access_mask: match old_stage {
        EMPTY_PIPELINE_STAGE_FLAGS => vk::AccessFlags::empty(),
        vk::PipelineStageFlags::ALL_GRAPHICS => vk::AccessFlags::SHADER_WRITE,
        vk::PipelineStageFlags::COMPUTE_SHADER => vk::AccessFlags::SHADER_WRITE,
        vk::PipelineStageFlags::TRANSFER => vk::AccessFlags::TRANSFER_WRITE,
        vk::PipelineStageFlags::HOST => vk::AccessFlags::HOST_WRITE,
        vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS => vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        _ => panic!("Unsupported value for current_pipeline_stage")
      },
      dst_access_mask: vk::AccessFlags::MEMORY_READ,
      dependency_flags: vk::DependencyFlags::empty()
    })
  }

  fn build_texture_barrier(
    name: &str,
    resource_metadata: &mut ResourceMetadata,
    is_depth_stencil_format: bool,
    is_history: bool,
    pipeline_stage: vk::PipelineStageFlags,
    layout: vk::ImageLayout
  ) -> Option<VkBarrierTemplate> {
    let old_stage = std::mem::replace(&mut resource_metadata.current_pipeline_stage, pipeline_stage);
    let old_layout = std::mem::replace(&mut resource_metadata.layout, layout);
    let discard = old_layout == vk::ImageLayout::UNDEFINED;
    let dirty = std::mem::replace(&mut resource_metadata.is_dirty, false);
    if !dirty && resource_metadata.layout == old_layout {
      return None;
    }
    Some(VkBarrierTemplate::Image {
      name: name.to_string(),
      is_history,
      old_layout,
      new_layout: resource_metadata.layout,
      src_access_mask: if !dirty || discard {
        vk::AccessFlags::empty()
      } else {
        match old_stage {
          EMPTY_PIPELINE_STAGE_FLAGS => vk::AccessFlags::empty(),
          vk::PipelineStageFlags::ALL_GRAPHICS => if is_depth_stencil_format {
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
          } else {
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
          },
          vk::PipelineStageFlags::COMPUTE_SHADER => vk::AccessFlags::SHADER_WRITE,
          vk::PipelineStageFlags::TRANSFER => vk::AccessFlags::TRANSFER_WRITE,
          vk::PipelineStageFlags::HOST => vk::AccessFlags::HOST_WRITE,
          vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS => vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
          _ => panic!("Unsupported value for current_pipeline_stage: {:?}", old_stage)
        }
      },
      dst_access_mask: if !dirty || discard {
        vk::AccessFlags::empty()
      } else {
        let mut flags = vk::AccessFlags::empty();
        if resource_metadata.used_in_stages.contains(vk::PipelineStageFlags::ALL_GRAPHICS) || resource_metadata.used_in_stages.contains(vk::PipelineStageFlags::COMPUTE_SHADER) {
          flags |= vk::AccessFlags::SHADER_READ;
        }
        if resource_metadata.used_in_stages.contains(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS) {
          flags |= vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
        }
        if resource_metadata.used_in_stages.contains(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS) {
          flags |= vk::AccessFlags::TRANSFER_READ;
        }
        flags
      },
      src_stage: old_stage,
      dst_stage: if discard {
        vk::PipelineStageFlags::empty()
      } else if dirty {
        resource_metadata.used_in_stages
      } else {
        resource_metadata.current_pipeline_stage
      },
      src_queue_family_index: 0,
      dst_queue_family_index: 0
    })
  }

  fn build_buffer_barrier(
    name: &str,
    resource_metadata: &mut ResourceMetadata,
    is_history: bool,
    pipeline_stage: vk::PipelineStageFlags,
  ) -> Option<VkBarrierTemplate> {
    let old_stage = std::mem::replace(&mut resource_metadata.current_pipeline_stage, pipeline_stage);
    let dirty = std::mem::replace(&mut resource_metadata.is_dirty, false);
    if !dirty {
      return None;
    }
    Some(VkBarrierTemplate::Buffer {
      name: name.to_string(),
      is_history,
      src_access_mask: match old_stage {
        EMPTY_PIPELINE_STAGE_FLAGS => vk::AccessFlags::empty(),
        vk::PipelineStageFlags::ALL_GRAPHICS => vk::AccessFlags::SHADER_WRITE,
        vk::PipelineStageFlags::COMPUTE_SHADER => vk::AccessFlags::SHADER_WRITE,
        vk::PipelineStageFlags::TRANSFER => vk::AccessFlags::TRANSFER_WRITE,
        vk::PipelineStageFlags::HOST => vk::AccessFlags::HOST_WRITE,
        vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS => vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        _ => panic!("Unsupported value for current_pipeline_stage")
      },
      dst_access_mask: vk::AccessFlags::MEMORY_READ,
      src_stage: old_stage,
      dst_stage: resource_metadata.used_in_stages,
      src_queue_family_index: 0,
      dst_queue_family_index: 0
    })
  }

  fn build_compute_copy_pass(
    inputs: &[PassInput],
    outputs: &[Output],
    name: &str,
    _device: &Arc<RawVkDevice>,
    _pass_index: u32,
    attachment_metadata: &mut HashMap<String, ResourceMetadata>,
    _swapchain_format: Format,
    _swapchain_samples: SampleCount,
    is_compute: bool) -> VkPassTemplate {
    let mut used_resources = HashSet::<String>::new();
    let mut uses_history_resources = false;
    let mut uses_external_resources = false;
    let mut uses_backbuffer = false;

    let (stage, layout) = if is_compute {
      (vk::PipelineStageFlags::COMPUTE_SHADER, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    } else {
      (vk::PipelineStageFlags::TRANSFER, vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
    };

    for output in outputs {
      used_resources.insert(match output {
        Output::Buffer { name, .. } => {
          let metadata = attachment_metadata.get_mut(name).unwrap();
          uses_history_resources |= metadata.history_usage.is_some();
          name.clone()
        },
        Output::Backbuffer { .. } => {
          uses_backbuffer = true;
          BACK_BUFFER_ATTACHMENT_NAME.to_string()
        },
        Output::DepthStencil { name, .. } => {
          let metadata = attachment_metadata.get_mut(name).unwrap();
          uses_history_resources |= metadata.history_usage.is_some();
          name.clone()
        },
        Output::RenderTarget { name, .. } => {
          let metadata = attachment_metadata.get_mut(name).unwrap();
          uses_history_resources |= metadata.history_usage.is_some();
          name.clone()
        }
      });
    }

    let mut barriers = Vec::<VkBarrierTemplate>::new();
    for input in inputs {
      used_resources.insert(input.name.clone());
      let metadata = attachment_metadata.get_mut(&input.name).unwrap();
      match metadata.template {
        VkResourceTemplate::ExternalBuffer { .. } => {
          uses_external_resources |= true;
        }
        VkResourceTemplate::ExternalTexture { .. } => {
          uses_external_resources |= true;
        }
        _ => {}
      }

      uses_history_resources |= metadata.history_usage.is_some();
      match &metadata.template {
        VkResourceTemplate::Texture { format, is_backbuffer, .. } => {
          if *is_backbuffer {
            panic!("Using the backbuffer as a pass input is not allowed.");
          }
          let is_depth_stencil = format.is_depth() || format.is_stencil();
          let barrier = Self::build_texture_barrier(
            &input.name,
            metadata,
            is_depth_stencil,
            input.is_history,
            stage,
            layout
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        },
        VkResourceTemplate::Buffer { .. } => {
          let barrier = Self::build_buffer_barrier(&input.name, metadata, input.is_history, stage);
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        }
        VkResourceTemplate::ExternalBuffer => {
          let barrier = Self::build_buffer_barrier(&input.name, metadata, input.is_history, stage);
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        }
        VkResourceTemplate::ExternalTexture { is_depth_stencil } => {
          let is_depth_stencil = *is_depth_stencil;
          let barrier = Self::build_texture_barrier(
            &input.name,
            metadata,
            is_depth_stencil,
            input.is_history,
            stage,
            layout
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        }
      }
    }

    /*
    let mut post_barriers = Vec::<VkBarrierTemplate>::new();
    if uses_backbuffer {
    }
    TODO
    */

    VkPassTemplate {
      name: name.to_string(),
      pass_type: VkPassType::ComputeCopy {
        barriers,
        is_compute
      },
      renders_to_swapchain: false,
      has_history_resources: uses_history_resources,
      has_external_resources: uses_external_resources,
      has_backbuffer: uses_backbuffer,
      resources: used_resources
    }
  }

  fn find_next_suitable_pass(pass_infos: &mut Vec<PassInfo>, metadata: &HashMap<String, ResourceMetadata>) -> PassInfo {
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
                if input.is_history {
                  // history resource
                  passes_since_ready = min(0, passes_since_ready);
                } else {
                  is_ready = false;
                }
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
              if input.is_history {
                // history resource
                passes_since_ready = min(0, passes_since_ready);
              } else {
                is_ready = false;
              }
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

fn pipeline_stage_to_vk(pipeline_stage: PipelineStage) -> vk::PipelineStageFlags {
  match pipeline_stage {
    PipelineStage::ComputeShader => vk::PipelineStageFlags::COMPUTE_SHADER,
    PipelineStage::VertexShader => vk::PipelineStageFlags::VERTEX_SHADER,
    PipelineStage::FragmentShader => vk::PipelineStageFlags::VERTEX_SHADER,
    PipelineStage::Copy => vk::PipelineStageFlags::TRANSFER,
  }
}
