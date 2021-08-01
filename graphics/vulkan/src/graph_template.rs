use sourcerenderer_core::graphics::{AttachmentInfo, AttachmentRef, BufferUsage, DepthStencil, DepthStencilAttachmentRef, ExternalOutput, ExternalProducerType, ExternalResource, InputUsage, LoadOp, Output, OutputAttachmentRef, PipelineStage, RenderPassInfo, RenderPassPipelineStage, RenderPassTextureExtent, StoreOp, SubpassInfo, TextureUsage};
use std::{cmp::max, collections::{HashMap, VecDeque}};
use std::collections::HashSet;
use std::sync::Arc;

use ash::vk;

use sourcerenderer_core::graphics::{PassInfo, PassInput, RenderGraphTemplate, RenderGraphTemplateInfo, Format, SampleCount, GraphicsSubpassInfo, PassType, SubpassOutput};
use sourcerenderer_core::graphics::BACK_BUFFER_ATTACHMENT_NAME;
use crate::{raw::RawVkDevice, VkThreadManager};

use crate::VkRenderPass;
use std::iter::FromIterator;

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
  current_access: ResourceAccess,
  pub(super) initial_layout: vk::ImageLayout
}

#[derive(Debug, Clone, Default)]
struct ResourceAccess {
  stage: vk::PipelineStageFlags,
  access: vk::AccessFlags,
  layout: vk::ImageLayout,
}

impl VkRenderGraphTemplate {
  pub fn new(device: &Arc<RawVkDevice>,
             context: &Arc<VkThreadManager>,
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
        history.current_access.access &= write_access_mask();
        history.initial_layout = history.current_access.layout;
      }
    }

    let mut reordered_passes_queue: VecDeque<PassInfo> = VecDeque::from_iter(reordered_passes);
    let mut pass_index: u32 = 0;
    let mut pass_opt = reordered_passes_queue.pop_front();
    while let Some(pass) = pass_opt {
      let render_graph_pass = match &pass.pass_type {
        PassType::Graphics {
          ref subpasses
        } => Self::build_render_pass(subpasses, &pass.name, device, context, pass_index, &mut attachment_metadata, info.swapchain_format, info.swapchain_sample_count),
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
                  external: rt_external, ..
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
                  ..
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
                format: ds_format, ..
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
                      initial_layout: vk::ImageLayout::UNDEFINED
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
                    initial_layout: vk::ImageLayout::UNDEFINED
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
                name, format, samples, extent, depth, levels, external, ..
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
                ..
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
                  pass_accesses: accesses,
                  initial_layout: vk::ImageLayout::UNDEFINED
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
                name, format, samples, extent, depth, levels, external, ..
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
                ..
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
                  pass_accesses: accesses,
                  initial_layout: vk::ImageLayout::UNDEFINED
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




  fn build_render_pass(passes: &[GraphicsSubpassInfo],
                        name: &str,
                        device: &Arc<RawVkDevice>,
                        context: &Arc<VkThreadManager>,
                        pass_index: u32,
                        attachment_metadata: &mut HashMap<String, ResourceMetadata>,
                        swapchain_format: Format,
                        swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut barriers = Vec::<VkBarrierTemplate>::new();
    let mut rp_info = RenderPassInfo {
      attachments: vec![],
      subpasses: vec![]
    };

    let mut renders_to_swapchain = false;
    let mut has_history_resources = false;
    let mut has_external_resources = false;
    let mut input_resource_names = HashSet::<String>::new();
    let mut name_attachment_index_map = HashMap::<String, u32>::new();

    for pass in passes {
      let mut subpass_info = SubpassInfo {
        input_attachments: Vec::with_capacity(pass.inputs.len()),
        output_color_attachments: Vec::with_capacity(pass.outputs.len()),
        depth_stencil_attachment: None,
      };

      for output in &pass.outputs {
        match output {
          SubpassOutput::Backbuffer { clear } => {
            renders_to_swapchain = true;

            let resource_metadata = attachment_metadata.get_mut(BACK_BUFFER_ATTACHMENT_NAME).unwrap();
            if let Some(barrier) = VkRenderGraphTemplate::build_texture_barrier(BACK_BUFFER_ATTACHMENT_NAME, resource_metadata, false, pass_index) {
              barriers.push(barrier);
            }

            name_attachment_index_map.insert(BACK_BUFFER_ATTACHMENT_NAME.to_string(), rp_info.attachments.len() as u32);
            subpass_info.output_color_attachments.push(OutputAttachmentRef {
              index: rp_info.attachments.len() as u32,
              resolve_attachment_index: None
            });
            rp_info.attachments.push(AttachmentInfo {
              format: swapchain_format,
              samples: swapchain_samples,
              load_op: if *clear { LoadOp::Clear } else { LoadOp::DontCare },
              store_op: if resource_metadata.pass_range.last_used_in_pass_index == pass_index { StoreOp::DontCare } else { StoreOp::Store },
              stencil_load_op: LoadOp::DontCare,
              stencil_store_op: StoreOp::DontCare,
            });
          }
          SubpassOutput::RenderTarget { name, format, samples, clear, .. } => {
            let resource_metadata = attachment_metadata.get_mut(name).unwrap();
            if let Some(barrier) = VkRenderGraphTemplate::build_texture_barrier(name, resource_metadata, false, pass_index) {
              barriers.push(barrier);
            }

            name_attachment_index_map.insert(name.clone(), rp_info.attachments.len() as u32);
            subpass_info.output_color_attachments.push(OutputAttachmentRef {
              index: rp_info.attachments.len() as u32,
              resolve_attachment_index: None
            });
            rp_info.attachments.push(AttachmentInfo {
              format: *format,
              samples: *samples,
              load_op: if *clear { LoadOp::Clear } else { LoadOp::DontCare },
              store_op: if resource_metadata.pass_range.last_used_in_pass_index == pass_index { StoreOp::DontCare } else { StoreOp::Store },
              stencil_load_op: LoadOp::DontCare,
              stencil_store_op: StoreOp::DontCare,
            });
          },
        }
      }

      for input in &pass.inputs {
        let resource_metadata = attachment_metadata.get_mut(&input.name).unwrap();

        match &resource_metadata.template {
          VkResourceTemplate::Texture { .. } => {
            if let Some(barrier) = VkRenderGraphTemplate::build_texture_barrier(&input.name, resource_metadata, input.is_history, pass_index) {
              barriers.push(barrier);
            }
          },
          VkResourceTemplate::Buffer { .. } => {
            if let Some(barrier) = VkRenderGraphTemplate::build_buffer_barrier(&input.name, resource_metadata, input.is_history, pass_index) {
              barriers.push(barrier);
            }
          },
          VkResourceTemplate::ExternalTexture { .. } => {
            has_external_resources = true;
            if let Some(barrier) = VkRenderGraphTemplate::build_texture_barrier(&input.name, resource_metadata, input.is_history, pass_index) {
              barriers.push(barrier);
            }
          },
          VkResourceTemplate::ExternalBuffer => {
            has_external_resources = true;
            if let Some(barrier) = VkRenderGraphTemplate::build_buffer_barrier(&input.name, resource_metadata, input.is_history, pass_index) {
              barriers.push(barrier);
            }
          },
      }

        input_resource_names.insert(input.name.to_string());

        let index_opt = name_attachment_index_map.get(&input.name).map(|i| *i);
        if input.usage == InputUsage::Local && !input.is_history && index_opt.is_some() {
          let index = index_opt.unwrap();
          subpass_info.input_attachments.push(AttachmentRef {
            index: index,
            pipeline_stage: match input.stage {
              PipelineStage::GraphicsShaders => RenderPassPipelineStage::BOTH,
              PipelineStage::VertexShader => RenderPassPipelineStage::VERTEX,
              PipelineStage::FragmentShader => RenderPassPipelineStage::FRAGMENT,
              PipelineStage::ComputeShader => panic!("Illegal pipeline stage for a graphics pass input"),
              PipelineStage::Copy => panic!("Illegal pipeline stage for a graphics pass input"),
            }
          });
        } else if input.is_history {
          has_history_resources = true;
        }
      }

      match &pass.depth_stencil {
        DepthStencil::Output { name, format, samples, clear, .. } => {
          let resource_metadata = attachment_metadata.get_mut(name).unwrap();
          if let Some(barrier) = VkRenderGraphTemplate::build_texture_barrier(name, resource_metadata, false, pass_index) {
            barriers.push(barrier);
          }

          name_attachment_index_map.insert(name.clone(), rp_info.attachments.len() as u32);
          subpass_info.depth_stencil_attachment = Some(DepthStencilAttachmentRef {
            index: rp_info.attachments.len() as u32,
            read_only: false,
          });
          rp_info.attachments.push(AttachmentInfo {
            format: *format,
            samples: *samples,
            load_op: if *clear { LoadOp::Clear } else { LoadOp::DontCare },
            store_op: if resource_metadata.pass_range.last_used_in_pass_index == pass_index { StoreOp::DontCare } else { StoreOp::Store },
            stencil_load_op: LoadOp::DontCare,
            stencil_store_op: StoreOp::DontCare,
          });
        },
        DepthStencil::Input { name, is_history } => {
          let resource_metadata = attachment_metadata.get_mut(name).unwrap();
          if let Some(barrier) = VkRenderGraphTemplate::build_texture_barrier(name, resource_metadata, *is_history, pass_index) {
            barriers.push(barrier);
          }

          let existing_index = name_attachment_index_map.get(name).map(|i| *i);
          let index = existing_index.unwrap_or_else(|| {
            let new_index = rp_info.attachments.len() as u32;
            match &resource_metadata.template {
              VkResourceTemplate::Texture { format, samples, .. } => {
                rp_info.attachments.push(AttachmentInfo {
                  format: *format,
                  samples: *samples,
                  load_op: if resource_metadata.pass_range.first_used_in_pass_index == pass_index { LoadOp::Clear } else { LoadOp::Load },
                  store_op: if resource_metadata.pass_range.last_used_in_pass_index == pass_index { StoreOp::DontCare } else { StoreOp::Store },
                  stencil_load_op: LoadOp::DontCare,
                  stencil_store_op: StoreOp::DontCare,
                });
              },
              _ => panic!("Unsupported type of depth stencil input")
            };

            name_attachment_index_map.insert(name.clone(), new_index);
            new_index
          });

          subpass_info.depth_stencil_attachment = Some(DepthStencilAttachmentRef {
            index: index,
            read_only: true,
          });
        },
        DepthStencil::None => {},
      }

      rp_info.subpasses.push(subpass_info);
    }

    let shared = context.get_shared();
    let mut rp_opt = {
      let render_passes = shared.get_render_passes().read().unwrap();
      render_passes.get(&rp_info).map(|rp_ref| rp_ref.clone())
    };
    if rp_opt.is_none() {
      let rp = Arc::new(VkRenderPass::new(device, &rp_info));
      let mut render_passes = shared.get_render_passes().write().unwrap();
      render_passes.insert(rp_info.clone(), rp.clone());
      rp_opt = Some(rp);
    }
    let rp = rp_opt.unwrap();

    let mut ordered_attachment_names: Vec<(String, u32)> = name_attachment_index_map.into_iter().collect();
    ordered_attachment_names.sort_by_key(|(_, index)| *index);
    let rp_attachment_names = ordered_attachment_names.into_iter().map(|(name, _)| name).collect();

    VkPassTemplate {
      name: name.to_string(),
      renders_to_swapchain,
      has_history_resources,
      has_external_resources,
      resources: input_resource_names,
      barriers,
      pass_type: VkPassType::Graphics {
        render_pass: rp,
        attachments: rp_attachment_names
      }
    }
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

    let discard = current_access.layout == vk::ImageLayout::UNDEFINED;
    if discard {
      src_access = vk::AccessFlags::empty();
      dst_access = pass_access.access;
      dst_stage = pass_access.stage;
    } else if src_access != vk::AccessFlags::empty() {
      // Flush + invalidate caches
      // Collect all future usages of the texture
      for (resource_pass_index, access) in &resource_metadata.pass_accesses {
        if *resource_pass_index == pass_index {
          continue;
        }
        dst_stage |= access.stage;
        dst_access |= access.access & !current_access.access;
      }
      if dst_access.is_empty() {
        src_access = vk::AccessFlags::empty();
      }
    } else {
      // Only a layout transition
      dst_stage = pass_access.stage;
      dst_access = pass_access.access;
    }

    let old_layout = current_access.layout;
    let new_layout = pass_access.layout;

    current_access.access = pass_access.access & write_access_mask();
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
    let mut src_access = current_access.access;

    let mut dst_stage = vk::PipelineStageFlags::empty();
    let mut dst_access = vk::AccessFlags::empty();
    // Collect all future usages of the buffer
    for (resource_pass_index, access) in &resource_metadata.pass_accesses {
      if *resource_pass_index == pass_index {
        continue;
      }
      dst_stage |= access.stage;
      dst_access |= access.access & !current_access.access;
    }
    if dst_access.is_empty() {
      src_access = vk::AccessFlags::empty();
    }

    current_access.access = pass_access.access & write_access_mask();
    current_access.stage = pass_access.stage;

    assert_ne!(dst_stage, vk::PipelineStageFlags::empty());
    assert_ne!(src_stage, vk::PipelineStageFlags::empty());
    assert!((src_access == vk::AccessFlags::empty() && dst_access == vk::AccessFlags::empty()) || (src_access != vk::AccessFlags::empty() && dst_access != vk::AccessFlags::empty()));

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
      let mut youngest_dependency_pass = 0;

      match &pass.pass_type {
        PassType::Graphics {
          subpasses
        } => {
          let mut renders_to_backbuffer = false;
          for subpass in subpasses {
            for input in &subpass.inputs {
              let index_opt = metadata.get(&input.name);
              if let Some(index) = index_opt {
                youngest_dependency_pass = max(index.pass_range.first_used_in_pass_index, youngest_dependency_pass);
              } else {
                if input.is_history {
                  // history resource
                  youngest_dependency_pass = max(0, youngest_dependency_pass);
                } else {
                  is_ready = false;
                }
              }
            }
            for output in &subpass.outputs {
              match output {
                SubpassOutput::Backbuffer { .. } => { renders_to_backbuffer = true; }
                SubpassOutput::RenderTarget { .. } => {}
              }
            }

            if renders_to_backbuffer && is_ready && best_pass_index_score.is_none() {
              best_pass_index_score = Some((pass_index as u32, 0 as u32));
            } else if is_ready && (best_pass_index_score.is_none() || youngest_dependency_pass < best_pass_index_score.unwrap().1 as u32) {
              best_pass_index_score = Some((pass_index as u32, youngest_dependency_pass as u32));
            }
          }
        },
        PassType::Compute {
          inputs, outputs, ..
        } => {
          let mut renders_to_backbuffer = false;
          for input in inputs {
            let index_opt = metadata.get(&input.name);
            if let Some(index) = index_opt {
              youngest_dependency_pass = max(index.pass_range.first_used_in_pass_index, youngest_dependency_pass);
            } else {
              if input.is_history {
                // history resource
                youngest_dependency_pass = max(0, youngest_dependency_pass);
              } else {
                is_ready = false;
              }
            }
          }
          for output in outputs {
            match output {
              Output::Backbuffer { .. } => { renders_to_backbuffer = true; }
              _ => {}
            }
          }

          if renders_to_backbuffer && is_ready && best_pass_index_score.is_none() {
            best_pass_index_score = Some((pass_index as u32, 0 as u32));
          } else if is_ready && (best_pass_index_score.is_none() || youngest_dependency_pass < best_pass_index_score.unwrap().1 as u32) {
            best_pass_index_score = Some((pass_index as u32, youngest_dependency_pass as u32));
          }
        },
        PassType::Copy {
          inputs, outputs, ..
        } => {
          let mut renders_to_backbuffer = false;
          for input in inputs {
            let index_opt = metadata.get(&input.name);
            if let Some(index) = index_opt {
              youngest_dependency_pass = max(index.pass_range.first_used_in_pass_index, youngest_dependency_pass);
            } else {
              if input.is_history {
                // history resource
                youngest_dependency_pass = max(0, youngest_dependency_pass);
              } else {
                is_ready = false;
              }
            }
          }
          for output in outputs {
            match output {
              Output::Backbuffer { .. } => { renders_to_backbuffer = true; }
              _ => {}
            }
          }

          if renders_to_backbuffer && is_ready && best_pass_index_score.is_none() {
            best_pass_index_score = Some((pass_index as u32, 0 as u32));
          } else if is_ready && (best_pass_index_score.is_none() || youngest_dependency_pass < best_pass_index_score.unwrap().1 as u32) {
            best_pass_index_score = Some((pass_index as u32, youngest_dependency_pass as u32));
          }
        }
      }
    }
    pass_infos.remove(best_pass_index_score.expect("Invalid render graph").0 as usize)
  }
}

impl RenderGraphTemplate for VkRenderGraphTemplate {
}

fn store_action_to_vk(store_action: StoreOp) -> vk::AttachmentStoreOp {
  match store_action {
    StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
    StoreOp::Store => vk::AttachmentStoreOp::STORE
  }
}

fn load_action_to_vk(load_action: LoadOp) -> vk::AttachmentLoadOp {
  match load_action {
    LoadOp::DontCare => vk::AttachmentLoadOp::DONT_CARE,
    LoadOp::Load => vk::AttachmentLoadOp::LOAD,
    LoadOp::Clear => vk::AttachmentLoadOp::CLEAR
  }
}

fn pipeline_stage_to_vk(pipeline_stage: PipelineStage) -> vk::PipelineStageFlags {
  match pipeline_stage {
    PipelineStage::ComputeShader => vk::PipelineStageFlags::COMPUTE_SHADER,
    PipelineStage::VertexShader => vk::PipelineStageFlags::VERTEX_SHADER,
    PipelineStage::FragmentShader => vk::PipelineStageFlags::VERTEX_SHADER,
    PipelineStage::GraphicsShaders => vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
    PipelineStage::Copy => vk::PipelineStageFlags::TRANSFER,
  }
}

fn pipeline_stage_to_rp(pipeline_stage: PipelineStage) -> RenderPassPipelineStage {
  match pipeline_stage {
    PipelineStage::VertexShader => RenderPassPipelineStage::VERTEX,
    PipelineStage::FragmentShader => RenderPassPipelineStage::FRAGMENT,
    PipelineStage::GraphicsShaders => RenderPassPipelineStage::BOTH,
    PipelineStage::ComputeShader => panic!("Unsupported render pass pipeline stage"),
    PipelineStage::Copy => panic!("Unsupported render pass pipeline stage"),
  }
}

fn write_access_mask() -> vk::AccessFlags {
  // cant make that const :(
  vk::AccessFlags::HOST_WRITE | vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::TRANSFER_WRITE | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
}
