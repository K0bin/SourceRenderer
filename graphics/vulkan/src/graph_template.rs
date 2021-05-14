use sourcerenderer_core::graphics::{DepthStencil, ExternalOutput, ExternalProducerType, InputUsage, Output, PipelineStage, RenderPassTextureExtent};
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
const PASS_NAME_EXTERNAL: &'static str = "EXTERNAL";

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
  pub resources: HashSet<String>,
  pub barriers: Vec<VkBarrierTemplate>
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
    attachments: Vec<String>
  },
  ComputeCopy {
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

#[derive(Debug, Clone, Default)]
pub struct ResourcePassRange {
  pub(super) first_used_in_pass_index: u32,
  pub(super) last_used_in_pass_index: u32
}

#[derive(Clone)]
pub struct ResourceMetadata {
  pub(super) template: VkResourceTemplate,
  pub(super) pass_range: ResourcePassRange,
  pub(super) history: Option<HistoryResourceMetadata>,
  pass_accesses: HashMap<u32, ResourceAccess>,
  current_access: ResourceAccess
}

impl ResourceMetadata {
  fn new(template: VkResourceTemplate) -> Self {
    ResourceMetadata {
      template,
      pass_range: ResourcePassRange::default(),
      history: None,
      pass_accesses: HashMap::new(),
      current_access: ResourceAccess::default()
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct HistoryResourceMetadata {
  pub(super) pass_range: ResourcePassRange,
  pass_accesses: HashMap<u32, ResourceAccess>,
  current_access: ResourceAccess
}

#[derive(Debug, Clone, Default)]
struct ResourceAccess {
  stage: vk::PipelineStageFlags,
  access: vk::AccessFlags,
  layout: vk::ImageLayout,
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

    // TODO: more generic support for external images / one time rendering
    // TODO: (async) compute
    // TODO: barrier to init history resources
    // TODO: barrier to present after compute pass

    let mut attachment_metadata = HashMap::<String, ResourceMetadata>::new();
    let mut passes: Vec<VkPassTemplate> = Vec::new();
    let pass_infos = info.passes.clone();
    let reordered_passes = VkRenderGraphTemplate::reorder_passes(&pass_infos, &mut attachment_metadata, &info.external_resources, info.swapchain_format, info.swapchain_sample_count);
    // Prepare access for history resource barriers
    for resource in &mut attachment_metadata.values_mut() {
      if let Some(history) = resource.history.as_mut() {
        let (_, pass_access) = resource.pass_accesses.iter().max_by_key(|(pass_index, _)| **pass_index).unwrap();
        history.current_access = pass_access.clone();
      }
    }

    let mut reordered_passes_queue: VecDeque<PassInfo> = VecDeque::from_iter(reordered_passes);
    let mut pass_index: u32 = 0;
    let mut pass_opt = reordered_passes_queue.pop_front();
    while let Some(pass) = pass_opt {
      let render_graph_pass = match &pass.pass_type {
        PassType::Graphics {
          ref subpasses
        } => Self::build_render_pass(subpasses, &pass.name, device, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count),
        PassType::Compute {
          inputs, outputs
        } => Self::build_compute_copy_pass(inputs, outputs, &pass.name, device, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count, true),
        PassType::Copy {
          inputs, outputs
        } => Self::build_compute_copy_pass(inputs, outputs, &pass.name, device, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count, false),
      };
      did_render_to_backbuffer |= render_graph_pass.renders_to_swapchain;
      passes.push(render_graph_pass);
      pass_opt = reordered_passes_queue.pop_front();
      pass_index += 1;
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

  fn reorder_passes(passes: &[PassInfo],
                    metadata: &mut HashMap<String, ResourceMetadata>,
                    external_resources: &[ExternalOutput],
                    swapchain_format: Format,
                    swapchain_samples: SampleCount) -> Vec<PassInfo> {
    let mut passes_mut = passes.to_owned();
    let mut reordered_passes = vec![];

    for external in external_resources {
      match external {
        ExternalOutput::Buffer {
          name, producer_type
        } => {
          let access = ResourceAccess {
            stage: match producer_type {
              ExternalProducerType::Graphics => vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::VERTEX_SHADER,
              ExternalProducerType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              ExternalProducerType::Copy => vk::PipelineStageFlags::TRANSFER,
              ExternalProducerType::Host => vk::PipelineStageFlags::HOST
            },
            access: match producer_type {
              ExternalProducerType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
              ExternalProducerType::Compute => vk::AccessFlags::SHADER_WRITE,
              ExternalProducerType::Copy => vk::AccessFlags::TRANSFER_WRITE,
              ExternalProducerType::Host => vk::AccessFlags::HOST_WRITE
            },
            layout: vk::ImageLayout::UNDEFINED
          };
          let mut accesses = HashMap::new();
          accesses.insert(reordered_passes.len() as u32, access);
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalBuffer,
            pass_range: ResourcePassRange {
              first_used_in_pass_index: 0,
              last_used_in_pass_index: 0
            },
            history: None,
            current_access: ResourceAccess::default(),
            pass_accesses: accesses,
          });
        }
        ExternalOutput::RenderTarget {
          name, producer_type
        } => {
          let access = ResourceAccess {
            stage: match producer_type {
              ExternalProducerType::Graphics => vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
              ExternalProducerType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              ExternalProducerType::Copy => vk::PipelineStageFlags::TRANSFER,
              ExternalProducerType::Host => vk::PipelineStageFlags::HOST
            },
            access: match producer_type {
              ExternalProducerType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
              ExternalProducerType::Compute => vk::AccessFlags::SHADER_WRITE,
              ExternalProducerType::Copy => vk::AccessFlags::TRANSFER_WRITE,
              ExternalProducerType::Host => vk::AccessFlags::HOST_WRITE
            },
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
            },
          };
          let mut accesses = HashMap::new();
          accesses.insert(reordered_passes.len() as u32, access);
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalTexture { is_depth_stencil: false },
            pass_range: ResourcePassRange {
              first_used_in_pass_index: 0,
              last_used_in_pass_index: 0
            },
            history: None,
            pass_accesses: accesses,
            current_access: ResourceAccess::default(),
          });
        }
        ExternalOutput::DepthStencil {
          name, producer_type
        } => {
          let access = ResourceAccess {
            stage: match producer_type {
              ExternalProducerType::Graphics => vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
              ExternalProducerType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              ExternalProducerType::Copy => vk::PipelineStageFlags::TRANSFER,
              ExternalProducerType::Host => vk::PipelineStageFlags::HOST
            },
            access: match producer_type {
              ExternalProducerType::Graphics => vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
              ExternalProducerType::Compute => vk::AccessFlags::SHADER_WRITE,
              ExternalProducerType::Copy => vk::AccessFlags::TRANSFER_WRITE,
              ExternalProducerType::Host => vk::AccessFlags::HOST_WRITE
            },
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
            },
          };
          let mut accesses = HashMap::new();
          accesses.insert(reordered_passes.len() as u32, access);
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalTexture { is_depth_stencil: true },
            pass_range: ResourcePassRange {
              first_used_in_pass_index: 0,
              last_used_in_pass_index: 0
            },
            history: None,
            current_access: ResourceAccess::default(),
            pass_accesses: accesses,
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
                  let metadata_entry = metadata
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
                      }));
                    metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                    metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                      stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                      access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                      layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    });
                },
                SubpassOutput::Backbuffer {
                  clear: backbuffer_clear
                } => {
                  let metadata_entry = metadata
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
                      }));
                    metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                    metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                      stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                      access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                      layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    });
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
                let metadata_entry = metadata
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
                    }));
                  metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                  metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                    stage: vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                  });
              }
              DepthStencil::Input {
                name: ds_name, is_history, ..
              } => {
                let mut input_metadata = metadata.get_mut(ds_name).unwrap();
                if *is_history {
                  if let Some(history_usage) = input_metadata.history.as_mut() {
                    if history_usage.pass_range.first_used_in_pass_index > reordered_passes.len() as u32 {
                      history_usage.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                    }
                    if history_usage.pass_range.last_used_in_pass_index < reordered_passes.len() as u32 {
                      history_usage.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
                    }
                    let access = ResourceAccess {
                      stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                      access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
                      layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                    };
                    history_usage.pass_accesses.insert(reordered_passes.len() as u32, access);
                  } else {
                    let access = ResourceAccess {
                      stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                      access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
                      layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                    };
                    let mut accesses = HashMap::<u32, ResourceAccess>::new();
                    accesses.insert(reordered_passes.len() as u32, access);
                    input_metadata.history = Some(HistoryResourceMetadata {
                      pass_range: ResourcePassRange {
                        first_used_in_pass_index: reordered_passes.len() as u32,
                        last_used_in_pass_index: reordered_passes.len() as u32,
                      },
                      pass_accesses: accesses,
                      current_access: ResourceAccess::default(),
                    });
                  }
                } else {
                  input_metadata.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
                  input_metadata.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                    stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                    access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
                    layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                  });
                }
              }
              DepthStencil::None => {}
            }

            for input in &subpass.inputs {
              let mut input_metadata = metadata.get_mut(&input.name).unwrap();
              if input.is_history {
                if let Some(history_usage) = input_metadata.history.as_mut() {
                  if history_usage.pass_range.first_used_in_pass_index > reordered_passes.len() as u32 {
                    history_usage.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                  }
                  if history_usage.pass_range.last_used_in_pass_index < reordered_passes.len() as u32 {
                    history_usage.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
                  }
                  let access = ResourceAccess {
                    stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                    access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                  };
                  history_usage.pass_accesses.insert(reordered_passes.len() as u32, access);
                } else {
                  let access = ResourceAccess {
                    stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                    access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                  };
                  let mut accesses = HashMap::<u32, ResourceAccess>::new();
                  accesses.insert(reordered_passes.len() as u32, access);
                  input_metadata.history = Some(HistoryResourceMetadata {
                    pass_range: ResourcePassRange {
                      first_used_in_pass_index: reordered_passes.len() as u32,
                      last_used_in_pass_index: reordered_passes.len() as u32,
                    },
                    pass_accesses: accesses,
                    current_access: ResourceAccess::default(),
                  });
                }
              } else {
                input_metadata.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
                input_metadata.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: pipeline_stage_to_vk(input.stage),
                  access: vk::AccessFlags::SHADER_READ,
                  layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                });
              }
              assert_ne!(input.stage, PipelineStage::ComputeShader);
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
                metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                  access: vk::AccessFlags::SHADER_WRITE,
                  layout: vk::ImageLayout::GENERAL,
                });
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
                metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                  access: vk::AccessFlags::SHADER_WRITE,
                  layout: vk::ImageLayout::GENERAL,
                });
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
                metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                  access: vk::AccessFlags::SHADER_WRITE,
                  layout: vk::ImageLayout::UNDEFINED,
                });
              }
              _ => {}
            }
          }

          for input in inputs {
            let mut input_metadata = metadata.get_mut(&input.name).unwrap();
            let is_buffer = match &input_metadata.template {
              VkResourceTemplate::Buffer {..} => true,
              VkResourceTemplate::ExternalBuffer {..} => true,
              VkResourceTemplate::Texture {..} => false,
              VkResourceTemplate::ExternalTexture {..} => true,
            };
            if input.is_history {
              if let Some(history_usage) = input_metadata.history.as_mut() {
                if history_usage.pass_range.first_used_in_pass_index > reordered_passes.len() as u32 {
                  history_usage.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                }
                if history_usage.pass_range.last_used_in_pass_index < reordered_passes.len() as u32 {
                  history_usage.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
                }
                history_usage.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                  access: vk::AccessFlags::SHADER_READ,
                  layout: if is_buffer { vk::ImageLayout::UNDEFINED } else if input.usage == InputUsage::Storage { vk::ImageLayout::GENERAL } else { vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL },
                });
              } else {
                let mut accesses = HashMap::<u32, ResourceAccess>::new();
                accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                  access: vk::AccessFlags::SHADER_READ,
                  layout: if is_buffer { vk::ImageLayout::UNDEFINED } else if input.usage == InputUsage::Storage { vk::ImageLayout::GENERAL } else { vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL },
                });
                input_metadata.history = Some(HistoryResourceMetadata {
                  pass_range: ResourcePassRange {
                    first_used_in_pass_index: reordered_passes.len() as u32,
                    last_used_in_pass_index: reordered_passes.len() as u32,
                  },
                  current_access: Default::default(),
                  pass_accesses: accesses
                });
              }
            } else {
              input_metadata.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
              input_metadata.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                stage: vk::PipelineStageFlags::COMPUTE_SHADER,
                access: vk::AccessFlags::SHADER_READ,
                layout: if is_buffer { vk::ImageLayout::UNDEFINED } else if input.usage == InputUsage::Storage { vk::ImageLayout::GENERAL } else { vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL },
              });
            }
            assert_eq!(input.stage, PipelineStage::ComputeShader);
          }
        },
        PassType::Copy {
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
                metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::TRANSFER,
                  access: vk::AccessFlags::TRANSFER_WRITE,
                  layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                });
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
                metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::TRANSFER,
                  access: vk::AccessFlags::TRANSFER_WRITE,
                  layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                });
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
                metadata_entry.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                metadata_entry.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::TRANSFER,
                  access: vk::AccessFlags::TRANSFER_WRITE,
                  layout: vk::ImageLayout::UNDEFINED,
                });
              }
              _ => {}
            }
          }

          for input in inputs {
            let mut input_metadata = metadata.get_mut(&input.name).unwrap();
            let is_buffer = match &input_metadata.template {
              VkResourceTemplate::Buffer {..} => true,
              VkResourceTemplate::ExternalBuffer {..} => true,
              VkResourceTemplate::Texture {..} => false,
              VkResourceTemplate::ExternalTexture {..} => true,
            };
            if input.is_history {
              if let Some(history_usage) = input_metadata.history.as_mut() {
                if history_usage.pass_range.first_used_in_pass_index > reordered_passes.len() as u32 {
                  history_usage.pass_range.first_used_in_pass_index = reordered_passes.len() as u32;
                }
                if history_usage.pass_range.last_used_in_pass_index < reordered_passes.len() as u32 {
                  history_usage.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
                }
                history_usage.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::TRANSFER,
                  access: vk::AccessFlags::TRANSFER_READ,
                  layout: if is_buffer { vk::ImageLayout::UNDEFINED } else { vk::ImageLayout::TRANSFER_SRC_OPTIMAL },
                });
              } else {
                let mut accesses = HashMap::<u32, ResourceAccess>::new();
                accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                  stage: vk::PipelineStageFlags::TRANSFER,
                  access: vk::AccessFlags::TRANSFER_READ,
                  layout: if is_buffer { vk::ImageLayout::UNDEFINED } else { vk::ImageLayout::TRANSFER_SRC_OPTIMAL },
                });
                input_metadata.history = Some(HistoryResourceMetadata {
                  pass_range: ResourcePassRange {
                    first_used_in_pass_index: reordered_passes.len() as u32,
                    last_used_in_pass_index: reordered_passes.len() as u32,
                  },
                  current_access: Default::default(),
                  pass_accesses: accesses
                });
              }
            } else {
              input_metadata.pass_range.last_used_in_pass_index = reordered_passes.len() as u32;
              input_metadata.pass_accesses.insert(reordered_passes.len() as u32, ResourceAccess {
                stage: vk::PipelineStageFlags::TRANSFER,
                access: vk::AccessFlags::TRANSFER_READ,
                layout: if is_buffer { vk::ImageLayout::UNDEFINED } else { vk::ImageLayout::TRANSFER_SRC_OPTIMAL },
              });
            }
            assert_eq!(input.stage, PipelineStage::ComputeShader);
          }
        },
      }
      reordered_passes.push(pass);
    }

    reordered_passes
  }

  #[allow(unused_assignments, unused_variables)] // TODO
  fn build_render_pass(passes: &[GraphicsSubpassInfo],
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
            pass_has_history_resources |= metadata.history.is_some();
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
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::UNDEFINED,
                ..Default::default()
              }
            );
          },

          SubpassOutput::RenderTarget {
            name, format, samples, external, load_action, store_action, ..
          } => {
            let metadata = attachment_metadata.get(name.as_str()).unwrap();
            pass_has_history_resources |= metadata.history.is_some();
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
                initial_layout: metadata.current_access.layout,
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
          pass_has_history_resources |= metadata.history.is_some();
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
              initial_layout: metadata.current_access.layout,
              final_layout: vk::ImageLayout::UNDEFINED, // will be filled in later
              ..Default::default()
            }
          );
        }

        DepthStencil::Input {
          name: ds_name, ..
        } => {
          let metadata = attachment_metadata.get(ds_name.as_str()).expect("Can not find attachment.");
          pass_has_history_resources |= metadata.history.is_some();
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
              initial_layout: metadata.current_access.layout,
              final_layout: vk::ImageLayout::UNDEFINED, // will be filled in later
              ..Default::default()
            }
          );
        }

        DepthStencil::None => {}
      }

      for input in &pass.inputs {
        let metadata = attachment_metadata.get(input.name.as_str()).expect("Can not find attachment.");
        pass_has_history_resources |= metadata.history.is_some();
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
        use_external_subpass_dependencies &= is_buffer || !input.is_history && (subpass_metadata.produced_in_subpass_index != vk::SUBPASS_EXTERNAL || (metadata.current_access.layout == vk::ImageLayout::UNDEFINED || metadata.current_access.layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL));
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
                input.usage == InputUsage::Local,
                pass_index
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.current_access.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
              }
            }

            VkResourceTemplate::Buffer { .. } => {
              let dependency = Self::build_buffer_subpass_dependency(
                pass_index,
                subpass_index as u32,
                metadata,
                subpass_metadata,
                input.is_history
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
              }
            }

            VkResourceTemplate::ExternalTexture { .. } => {
              let dependency = Self::build_texture_subpass_dependency(
                subpass_index as u32,
                metadata,
                subpass_metadata,
                false,
                pass_index
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.current_access.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
              }
            }

            VkResourceTemplate::ExternalBuffer { .. } => {
              let dependency = Self::build_buffer_subpass_dependency(
                pass_index,
                subpass_index as u32,
                metadata,
                subpass_metadata,
                false
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
                input.is_history,
                pass_index
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
                pass_index
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
                pass_index
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
              }
            }
            VkResourceTemplate::ExternalTexture { .. } => {
              let barrier = Self::build_texture_barrier(
                &input.name,
                metadata,
                input.is_history,
                pass_index
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
                pass_index
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.current_access.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
              }
            } else {
              let barrier = Self::build_texture_barrier(
                rt_name,
                metadata,
                false,
                pass_index
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
                pass_index
              );
              if let Some(dependency) = dependency {
                dependencies.push(dependency);
                metadata.current_access.layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
              }
            } else {
              let barrier = Self::build_texture_barrier(
                BACK_BUFFER_ATTACHMENT_NAME,
                metadata,
                false,
                pass_index
              );
              if let Some(barrier) = barrier {
                barriers.push(barrier);
                vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].initial_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
              }
            }
            vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::PRESENT_SRC_KHR;
            metadata.current_access.layout = vk::ImageLayout::PRESENT_SRC_KHR;
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
              false,
              pass_index
            );
            if let Some(dependency) = dependency {
              dependencies.push(dependency);
              metadata.current_access.layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
            }
          } else {
            let barrier = Self::build_texture_barrier(
              ds_name,
              metadata,
              false,
              pass_index
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
              false,
              pass_index
            );
            if let Some(dependency) = dependency {
              dependencies.push(dependency);
              metadata.current_access.layout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
            }
          } else {
            let barrier = Self::build_texture_barrier(
              ds_name,
              metadata,
              false,
              pass_index
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
            && (metadata.pass_range.last_used_in_pass_index > pass_index || metadata.history.is_some() && metadata.history.as_ref().unwrap().pass_range.last_used_in_pass_index > pass_index || subpass_metadata.last_used_in_subpass_index > subpass_index as u32) {
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
      resources: used_resources,
      name: name.to_owned(),
      barriers,
      pass_type: VkPassType::Graphics {
        render_pass,
        attachments: used_attachments
      }
    }
  }

  fn build_texture_subpass_dependency(subpass_index: u32, resource_metadata: &mut ResourceMetadata, subpass_metadata: &SubpassAttachmentMetadata, is_local: bool, pass_index: u32) -> Option<vk::SubpassDependency> {
    let (pass_access, current_access) = {
      let pass_access = resource_metadata.pass_accesses.get(&pass_index).unwrap();
      let current_access = &mut resource_metadata.current_access;
      (pass_access, current_access)
    };

    // TODO: read after write?
    if current_access.access.is_empty() && current_access.layout == pass_access.layout {
      return None;
    }

    let mut src_stage = current_access.stage;
    let mut src_access = current_access.access;

    let mut dst_stage = vk::PipelineStageFlags::empty();
    let mut dst_access = vk::AccessFlags::empty();
    // Collect all future usages of the texture
    for access in resource_metadata.pass_accesses.values() {
      dst_stage |= access.stage;
      dst_access |= access.access;
    }

    let discard = current_access.layout == vk::ImageLayout::UNDEFINED;
    if discard {
      src_stage = dst_stage;
      src_access = vk::AccessFlags::empty();
      dst_access = vk::AccessFlags::empty();
    }

    current_access.access = vk::AccessFlags::empty();
    current_access.stage = pass_access.stage;
    current_access.layout = pass_access.layout;

    assert_ne!(dst_stage, vk::PipelineStageFlags::empty());
    assert_ne!(src_stage, vk::PipelineStageFlags::empty());

    Some(vk::SubpassDependency {
      src_subpass: subpass_metadata.produced_in_subpass_index,
      dst_subpass: subpass_index,
      src_stage_mask: src_stage,
      dst_stage_mask: dst_stage,
      src_access_mask: src_access,
      dst_access_mask: dst_access,
      dependency_flags: if is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
    })
  }

  fn build_buffer_subpass_dependency(pass_index: u32, subpass_index: u32, resource_metadata: &mut ResourceMetadata, subpass_metadata: &SubpassAttachmentMetadata, is_history: bool) -> Option<vk::SubpassDependency> {
    let (pass_access, current_access) = if !is_history {
      let pass_access = resource_metadata.pass_accesses.get(&pass_index).unwrap();
      let current_access = &mut resource_metadata.current_access;
      (pass_access, current_access)
    } else {
      let history = resource_metadata.history.as_mut().unwrap();
      let pass_access = history.pass_accesses.get(&pass_index).unwrap();
      let current_access = &mut history.current_access;
      (pass_access, current_access)
    };

    if current_access.access.is_empty() && current_access.layout == pass_access.layout {
      return None;
    }

    let mut src_stage = current_access.stage;
    let mut src_access = current_access.access;

    let mut dst_stage = vk::PipelineStageFlags::empty();
    let mut dst_access = vk::AccessFlags::empty();
    // Collect all future usages of the texture
    for access in resource_metadata.pass_accesses.values() {
      dst_stage |= access.stage;
      dst_access |= access.access;
    }

    let discard = current_access.layout == vk::ImageLayout::UNDEFINED;
    if discard {
      src_stage = dst_stage;
      src_access = vk::AccessFlags::empty();
      dst_access = vk::AccessFlags::empty();
    }

    current_access.access = vk::AccessFlags::empty();
    current_access.stage = pass_access.stage;
    current_access.layout = pass_access.layout;

    assert_ne!(dst_stage, vk::PipelineStageFlags::empty());
    assert_ne!(src_stage, vk::PipelineStageFlags::empty());

    Some(vk::SubpassDependency {
      src_subpass: subpass_metadata.produced_in_subpass_index,
      dst_subpass: subpass_index,
      src_stage_mask: src_stage,
      dst_stage_mask: dst_stage,
      src_access_mask: src_access,
      dst_access_mask: dst_access,
      dependency_flags: vk::DependencyFlags::empty()
    })
  }

  fn build_texture_barrier(
    name: &str,
    resource_metadata: &mut ResourceMetadata,
    is_history: bool,
    pass_index: u32
  ) -> Option<VkBarrierTemplate> {
    let (pass_access, current_access) = if !is_history {
      let pass_access = resource_metadata.pass_accesses.get(&pass_index).unwrap();
      let current_access = &mut resource_metadata.current_access;
      (pass_access, current_access)
    } else {
      let history = resource_metadata.history.as_mut().unwrap();
      let pass_access = history.pass_accesses.get(&pass_index).unwrap();
      let current_access = &mut history.current_access;
      (pass_access, current_access)
    };

    if current_access.access.is_empty() && current_access.layout == pass_access.layout {
      return None;
    }

    let mut src_stage = current_access.stage;
    let mut src_access = current_access.access;

    let mut dst_stage = vk::PipelineStageFlags::empty();
    let mut dst_access = vk::AccessFlags::empty();
    // Collect all future usages of the texture
    for access in resource_metadata.pass_accesses.values() {
      dst_stage |= access.stage;
      dst_access |= access.access;
    }

    let discard = current_access.layout == vk::ImageLayout::UNDEFINED;
    if discard {
      src_stage = dst_stage;
      src_access = vk::AccessFlags::empty();
      dst_access = vk::AccessFlags::empty();
    }

    let old_layout = current_access.layout;
    let new_layout = pass_access.layout;

    current_access.access = vk::AccessFlags::empty();
    current_access.stage = pass_access.stage;
    current_access.layout = pass_access.layout;

    assert_ne!(dst_stage, vk::PipelineStageFlags::empty());
    assert_ne!(src_stage, vk::PipelineStageFlags::empty());

    Some(VkBarrierTemplate::Image {
      name: name.to_string(),
      is_history,
      old_layout,
      new_layout,
      src_access_mask: src_access,
      dst_access_mask: dst_access,
      src_stage,
      dst_stage,
      src_queue_family_index: 0,
      dst_queue_family_index: 0
    })
  }

  fn build_buffer_barrier(
    name: &str,
    resource_metadata: &mut ResourceMetadata,
    is_history: bool,
    pass_index: u32
  ) -> Option<VkBarrierTemplate> {
    let (pass_access, current_access) = if !is_history {
      let pass_access = resource_metadata.pass_accesses.get(&pass_index).unwrap();
      let current_access = &mut resource_metadata.current_access;
      (pass_access, current_access)
    } else {
      let history = resource_metadata.history.as_mut().unwrap();
      let pass_access = history.pass_accesses.get(&pass_index).unwrap();
      let current_access = &mut history.current_access;
      (pass_access, current_access)
    };

    if current_access.access.is_empty() {
      return None;
    }

    let src_stage = current_access.stage;
    let src_access = current_access.access;

    let mut dst_stage = vk::PipelineStageFlags::empty();
    let mut dst_access = vk::AccessFlags::empty();
    // Collect all future usages of the texture
    for access in resource_metadata.pass_accesses.values() {
      dst_stage |= access.stage;
      dst_access |= access.access;
    }

    current_access.access = vk::AccessFlags::empty();
    current_access.stage = pass_access.stage;

    assert_ne!(dst_stage, vk::PipelineStageFlags::empty());
    assert_ne!(src_stage, vk::PipelineStageFlags::empty());

    Some(VkBarrierTemplate::Buffer {
      name: name.to_string(),
      is_history,
      src_access_mask: src_access,
      dst_access_mask: dst_access,
      src_stage,
      dst_stage,
      src_queue_family_index: 0,
      dst_queue_family_index: 0
    })
  }

  fn build_compute_copy_pass(
    inputs: &[PassInput],
    outputs: &[Output],
    name: &str,
    _device: &Arc<RawVkDevice>,
    pass_index: u32,
    attachment_metadata: &mut HashMap<String, ResourceMetadata>,
    _swapchain_format: Format,
    _swapchain_samples: SampleCount,
    is_compute: bool) -> VkPassTemplate {
    let mut used_resources = HashSet::<String>::new();
    let mut uses_history_resources = false;
    let mut uses_external_resources = false;
    let mut uses_backbuffer = false;

    let mut barriers = Vec::<VkBarrierTemplate>::new();
    for output in outputs {
      used_resources.insert(match output {
        Output::Buffer { name, .. } => {
          let metadata = attachment_metadata.get_mut(name).unwrap();
          let barrier = Self::build_buffer_barrier(
            name,
            metadata,
            false,
            pass_index
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
          uses_history_resources |= metadata.history.is_some();
          name.clone()
        },
        Output::Backbuffer { .. } => {
          let metadata = attachment_metadata.get_mut(BACK_BUFFER_ATTACHMENT_NAME).unwrap();
          uses_backbuffer = true;
          let barrier = Self::build_texture_barrier(
            BACK_BUFFER_ATTACHMENT_NAME,
            metadata,
            false,
            pass_index
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
          BACK_BUFFER_ATTACHMENT_NAME.to_string()
        },
        Output::DepthStencil { name, .. } => {
          let metadata = attachment_metadata.get_mut(name).unwrap();
          uses_history_resources |= metadata.history.is_some();
          let barrier = Self::build_texture_barrier(
            name,
            metadata,
            false,
            pass_index
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
          name.clone()
        },
        Output::RenderTarget { name, .. } => {
          let metadata = attachment_metadata.get_mut(name).unwrap();
          uses_history_resources |= metadata.history.is_some();
          let barrier = Self::build_texture_barrier(
            name,
            metadata,
            false,
            pass_index
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
          name.clone()
        }
      });
    }

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

      uses_history_resources |= metadata.history.is_some();
      match &metadata.template {
        VkResourceTemplate::Texture { is_backbuffer, .. } => {
          if *is_backbuffer {
            panic!("Using the backbuffer as a pass input is not allowed.");
          }
          let barrier = Self::build_texture_barrier(
            &input.name,
            metadata,
            input.is_history,
            pass_index
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        },
        VkResourceTemplate::Buffer { .. } => {
          let barrier = Self::build_buffer_barrier(&input.name, metadata, input.is_history, pass_index);
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        }
        VkResourceTemplate::ExternalBuffer => {
          let barrier = Self::build_buffer_barrier(&input.name, metadata, input.is_history, pass_index);
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        }
        VkResourceTemplate::ExternalTexture { .. } => {
          let barrier = Self::build_texture_barrier(
            &input.name,
            metadata,
            input.is_history,
            pass_index
          );
          if let Some(barrier) = barrier {
            barriers.push(barrier);
          }
        }
      }
    }

    VkPassTemplate {
      name: name.to_string(),
      barriers,
      pass_type: VkPassType::ComputeCopy {
        is_compute
      },
      renders_to_swapchain: uses_backbuffer,
      has_history_resources: uses_history_resources,
      has_external_resources: uses_external_resources,
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
                passes_since_ready = min(index.pass_range.first_used_in_pass_index, passes_since_ready);
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
              passes_since_ready = min(index.pass_range.first_used_in_pass_index, passes_since_ready);
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
        PassType::Copy {
          inputs, ..
        } => {
          for input in inputs {
            let index_opt = metadata.get(&input.name);
            if let Some(index) = index_opt {
              passes_since_ready = min(index.pass_range.first_used_in_pass_index, passes_since_ready);
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
