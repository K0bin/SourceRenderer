use sourcerenderer_core::graphics::{ComputeOutput, RenderPassTextureExtent, ExternalOutput, ExternalProducerType, DepthStencil, StoreOp};
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
    attachments: Vec<String>,
    barriers: Vec<VkBarrierTemplate>
  },
  Compute {
    barriers: Vec<VkBarrierTemplate>
  },
  Copy
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
pub struct ResourceMetadata {
  pub(super) template: VkResourceTemplate,
  pub(super) last_used_in_pass_index: u32,
  pub(super) produced_in_pass_index: u32,
  pub(super) producer_pass_type: VkResourceProducerPassType,
  pub(super) layout: vk::ImageLayout
}

impl ResourceMetadata {
  fn new(template: VkResourceTemplate) -> Self {
    ResourceMetadata {
      template,
      last_used_in_pass_index: 0,
      produced_in_pass_index: 0,
      producer_pass_type: VkResourceProducerPassType::Graphics,
      layout: vk::ImageLayout::UNDEFINED
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

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum VkResourceProducerPassType {
  Host,
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

    let mut attachment_metadata = HashMap::<String, ResourceMetadata>::new();
    let mut passes: Vec<VkPassTemplate> = Vec::new();
    let mut pass_infos = info.passes.clone();
    let reordered_passes = VkRenderGraphTemplate::reorder_passes(&mut pass_infos, &mut attachment_metadata, &info.external_resources, info.swapchain_format, info.swapchain_sample_count);

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
            produced_in_pass_index: 0,
            producer_pass_type: match producer_type {
              ExternalProducerType::Graphics => VkResourceProducerPassType::Graphics,
              ExternalProducerType::Compute => VkResourceProducerPassType::Compute,
              ExternalProducerType::Copy => VkResourceProducerPassType::Copy,
              ExternalProducerType::Host => VkResourceProducerPassType::Host
            },
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
            }
          });
        }
        ExternalOutput::RenderTarget {
          name, producer_type
        } => {
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalTexture { is_depth_stencil: false },
            last_used_in_pass_index: 0,
            produced_in_pass_index: 0,
            producer_pass_type: match producer_type {
              ExternalProducerType::Graphics => VkResourceProducerPassType::Graphics,
              ExternalProducerType::Compute => VkResourceProducerPassType::Compute,
              ExternalProducerType::Copy => VkResourceProducerPassType::Copy,
              ExternalProducerType::Host => VkResourceProducerPassType::Host
            },
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
            }
          });
        }
        ExternalOutput::DepthStencil {
          name, producer_type
        } => {
          metadata.insert(name.clone(), ResourceMetadata {
            template: VkResourceTemplate::ExternalTexture { is_depth_stencil: true },
            last_used_in_pass_index: 0,
            produced_in_pass_index: 0,
            producer_pass_type: match producer_type {
              ExternalProducerType::Graphics => VkResourceProducerPassType::Graphics,
              ExternalProducerType::Compute => VkResourceProducerPassType::Compute,
              ExternalProducerType::Copy => VkResourceProducerPassType::Copy,
              ExternalProducerType::Host => VkResourceProducerPassType::Host
            },
            layout: match producer_type {
              ExternalProducerType::Graphics => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
              ExternalProducerType::Compute => vk::ImageLayout::GENERAL,
              ExternalProducerType::Copy => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
              ExternalProducerType::Host => vk::ImageLayout::PREINITIALIZED
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
                },
                _ => {}
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
                name: ds_name, ..
              } => {
                let mut input_metadata = metadata.get_mut(ds_name).unwrap();
                input_metadata.last_used_in_pass_index = reordered_passes.len() as u32;
              }
              DepthStencil::None => {}
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
              ComputeOutput::RenderTarget {
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
                  metadata_entry.producer_pass_type = VkResourceProducerPassType::Compute;
              },
              ComputeOutput::Backbuffer {
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
                metadata_entry.producer_pass_type = VkResourceProducerPassType::Compute;
              },
              ComputeOutput::Buffer {
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
                  metadata_entry.producer_pass_type = VkResourceProducerPassType::Compute;
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
                       attachment_metadata: &mut HashMap<String, ResourceMetadata>,
                       swapchain_format: Format,
                       swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut vk_render_pass_attachments: Vec<vk::AttachmentDescription> = Vec::new();
    let mut subpass_attachment_metadata: HashMap<&str, SubpassAttachmentMetadata> = HashMap::new();
    let mut pass_renders_to_backbuffer = false;
    let mut barriers = Vec::<VkBarrierTemplate>::new();

    // Prepare attachments
    for (subpass_index, pass) in passes.iter().enumerate() {
      for output in &pass.outputs {
        match output {
          SubpassOutput::Backbuffer { clear } => {
            let metadata = attachment_metadata.get_mut(BACK_BUFFER_ATTACHMENT_NAME).unwrap();
            let mut subpass_metadata = subpass_attachment_metadata.entry(BACK_BUFFER_ATTACHMENT_NAME)
              .or_default();
            subpass_metadata.render_pass_attachment_index = vk_render_pass_attachments.len() as u32;
            subpass_metadata.produced_in_subpass_index = subpass_index as u32;

            pass_renders_to_backbuffer = true;
            vk_render_pass_attachments.push(
              vk::AttachmentDescription {
                format: format_to_vk(swapchain_format),
                samples: samples_to_vk(swapchain_samples),
                load_op: if *clear { vk::AttachmentLoadOp::CLEAR } else { vk::AttachmentLoadOp::DONT_CARE },
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

          SubpassOutput::RenderTarget {
            name, format, samples, extent, depth, levels, external, load_action, store_action
          } => {
            let metadata = attachment_metadata.get(name.as_str()).unwrap();
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
                final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, // will be filled in later
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
          let (format, samples) = match &metadata.template {
            VkResourceTemplate::Texture { format, samples, .. } => { (*format, *samples) }
            VkResourceTemplate::Buffer { .. } => { unreachable!() }
            VkResourceTemplate::ExternalBuffer => { unreachable!() }
            VkResourceTemplate::ExternalTexture { .. } => { unimplemented!() }
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
        let mut metadata = subpass_attachment_metadata.entry(input.name.as_str())
          .or_default();
        if subpass_index as u32 > metadata.last_used_in_subpass_index {
          metadata.last_used_in_subpass_index = subpass_index as u32;
        }
      }
      // TODO: transition non attachment images
    }

    let mut use_subpass_dependencies = true;
    for pass in passes.iter() {
      for input in &pass.inputs {
        let metadata = attachment_metadata.get(&input.name).unwrap();
        let is_buffer = match metadata.template {
          VkResourceTemplate::Buffer { .. } => true,
          VkResourceTemplate::ExternalBuffer { .. } => true,
          _ => false
        };
        let subpass_metadata = subpass_attachment_metadata.get(input.name.as_str()).unwrap();
        use_subpass_dependencies &= is_buffer || subpass_metadata.last_used_in_subpass_index != vk::SUBPASS_EXTERNAL;
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
        let metadata = attachment_metadata.get_mut(input.name.as_str()).unwrap();
        let subpass_metadata = &subpass_attachment_metadata[input.name.as_str()];

        if subpass_metadata.produced_in_subpass_index != vk::SUBPASS_EXTERNAL || use_subpass_dependencies {
          dependencies.push(match &metadata.template {
            VkResourceTemplate::Texture { format, is_backbuffer, .. } => {
              if *is_backbuffer {
                panic!("Using the backbuffer as a pass input is not allowed.");
              }
              let is_depth_stencil = format.is_depth() || format.is_stencil();
              vk::SubpassDependency {
                src_subpass: subpass_metadata.produced_in_subpass_index,
                dst_subpass: subpass_index as u32,
                src_stage_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::TRANSFER,
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST
                },
                dst_stage_mask: vk::PipelineStageFlags::ALL_GRAPHICS,
                src_access_mask: if is_depth_stencil {
                  vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                } else {
                  match metadata.producer_pass_type {
                    VkResourceProducerPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                    VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                    VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE
                  }
                },
                dst_access_mask: if input.is_local { vk::AccessFlags::COLOR_ATTACHMENT_READ } else { vk::AccessFlags::SHADER_READ },
                dependency_flags: if input.is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
              }
            }

            VkResourceTemplate::Buffer { .. } => {
              vk::SubpassDependency {
                src_subpass: subpass_metadata.produced_in_subpass_index,
                dst_subpass: subpass_index as u32,
                src_stage_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::TRANSFER,
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST
                },
                dst_stage_mask: vk::PipelineStageFlags::ALL_GRAPHICS,
                src_access_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                  VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE
                },
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                dependency_flags: vk::DependencyFlags::empty()
              }
            }

            VkResourceTemplate::ExternalTexture { is_depth_stencil, .. } => {
              vk::SubpassDependency {
                src_subpass: subpass_metadata.produced_in_subpass_index,
                dst_subpass: subpass_index as u32,
                src_stage_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::TRANSFER,
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST
                },
                dst_stage_mask: vk::PipelineStageFlags::ALL_GRAPHICS,
                src_access_mask: if *is_depth_stencil {
                  vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                } else {
                  match metadata.producer_pass_type {
                    VkResourceProducerPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                    VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                    VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE
                  }
                },
                dst_access_mask: if input.is_local { vk::AccessFlags::COLOR_ATTACHMENT_READ } else { vk::AccessFlags::SHADER_READ },
                dependency_flags: if input.is_local { vk::DependencyFlags::BY_REGION } else { vk::DependencyFlags::empty() }
              }
            }

            VkResourceTemplate::ExternalBuffer { .. } => {
              vk::SubpassDependency {
                src_subpass: subpass_metadata.produced_in_subpass_index,
                dst_subpass: subpass_index as u32,
                src_stage_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::TRANSFER,
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST
                },
                dst_stage_mask: vk::PipelineStageFlags::ALL_GRAPHICS,
                src_access_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                  VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE
                },
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                dependency_flags: vk::DependencyFlags::empty()
              }
            }
          });
        } else {
          barriers.push(match &metadata.template {
            VkResourceTemplate::Texture { format, is_backbuffer, .. } => {
              if *is_backbuffer {
                panic!("Using the backbuffer as a pass input is not allowed.");
              }
              let is_depth_stencil = format.is_depth() || format.is_stencil();
              let old_layout = std::mem::replace(&mut metadata.layout, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
              VkBarrierTemplate::Image {
                name: input.name.clone(),
                old_layout,
                new_layout: metadata.layout,
                src_access_mask: if is_depth_stencil {
                  vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                } else {
                  match metadata.producer_pass_type {
                    VkResourceProducerPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                    VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                    _ => unimplemented!()
                  }
                },
                dst_access_mask: vk::AccessFlags::SHADER_READ,
                src_stage: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST,
                },
                dst_stage: vk::PipelineStageFlags::TOP_OF_PIPE,
                src_queue_family_index: 0,
                dst_queue_family_index: 0
              }
            },
            VkResourceTemplate::Buffer { .. } => {
              VkBarrierTemplate::Buffer {
                name: input.name.clone(),
                src_access_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                  VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE,
                },
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                src_stage: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST,
                },
                dst_stage: vk::PipelineStageFlags::TOP_OF_PIPE,
                src_queue_family_index: 0,
                dst_queue_family_index: 0
              }
            }
            VkResourceTemplate::ExternalBuffer => {
              VkBarrierTemplate::Buffer {
                name: input.name.clone(),
                src_access_mask: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                  VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                  VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE,
                },
                dst_access_mask: vk::AccessFlags::MEMORY_READ,
                src_stage: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST,
                },
                dst_stage: vk::PipelineStageFlags::TOP_OF_PIPE,
                src_queue_family_index: 0,
                dst_queue_family_index: 0
              }
            }
            VkResourceTemplate::ExternalTexture { is_depth_stencil } => {
              let old_layout = std::mem::replace(&mut metadata.layout, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
              VkBarrierTemplate::Image {
                name: input.name.clone(),
                old_layout,
                new_layout: metadata.layout,
                src_access_mask: if *is_depth_stencil {
                  vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                } else {
                  match metadata.producer_pass_type {
                    VkResourceProducerPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                    VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                    _ => unimplemented!()
                  }
                },
                dst_access_mask: vk::AccessFlags::SHADER_READ,
                src_stage: match metadata.producer_pass_type {
                  VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
                  VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
                  VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
                  VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST,
                },
                dst_stage: vk::PipelineStageFlags::TOP_OF_PIPE,
                src_queue_family_index: 0,
                dst_queue_family_index: 0
              }
            }
          });
        }

        match &metadata.template {
          VkResourceTemplate::Texture { .. } => {}
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
          SubpassOutput::RenderTarget { name: rt_name, .. } => {
            let metadata = &subpass_attachment_metadata[rt_name.as_str()];
            attachment_refs.push(vk::AttachmentReference {
              attachment: metadata.render_pass_attachment_index,
              layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            });
          },
          SubpassOutput::Backbuffer {
            ..
          } => {
            let subpass_metadata = &subpass_attachment_metadata[BACK_BUFFER_ATTACHMENT_NAME];
            attachment_refs.push(vk::AttachmentReference {
              attachment: subpass_metadata.render_pass_attachment_index,
              layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            });
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
          vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
          metadata.layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
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
          vk_render_pass_attachments[subpass_metadata.render_pass_attachment_index as usize].final_layout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
          metadata.layout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
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
          && (metadata.last_used_in_pass_index > pass_index || subpass_metadata.last_used_in_subpass_index > subpass_index as u32) {
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
        attachments: used_attachments,
        barriers
      }
    }
  }

  fn build_compute_pass(inputs: &[PassInput],
                        outputs: &[ComputeOutput],
                        name: &str,
                        _device: &Arc<RawVkDevice>,
                        _pass_index: u32,
                        attachment_metadata: &mut HashMap<String, ResourceMetadata>,
                        _swapchain_format: Format,
                        _swapchain_samples: SampleCount) -> VkPassTemplate {
    let mut used_resources = HashSet::<String>::new();
    for output in outputs {
      used_resources.insert(match output {
        ComputeOutput::Buffer { name, .. } => name.clone(),
        ComputeOutput::Backbuffer { .. } => BACK_BUFFER_ATTACHMENT_NAME.to_string(),
        ComputeOutput::DepthStencil { name, .. } => name.clone(),
        ComputeOutput::RenderTarget { name, .. } => name.clone()
      });
    }

    for input in inputs {
      used_resources.insert(input.name.clone());
    }

    let barriers = inputs.iter().map(|input|{
      let metadata = attachment_metadata.get_mut(&input.name).unwrap();
      match &metadata.template {
        VkResourceTemplate::Texture { format, is_backbuffer, .. } => {
          if *is_backbuffer {
            panic!("Using the backbuffer as a pass input is not allowed.");
          }
          let is_depth_stencil = format.is_depth() || format.is_stencil();
          let old_layout = std::mem::replace(&mut metadata.layout, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
          VkBarrierTemplate::Image {
            name: input.name.clone(),
            old_layout,
            new_layout: metadata.layout,
            src_access_mask: match metadata.producer_pass_type {
                VkResourceProducerPassType::Graphics => if !is_depth_stencil {
                  vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                } else {
                  vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                },
                VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE,
            },
            dst_access_mask: vk::AccessFlags::SHADER_READ,
            src_stage: match metadata.producer_pass_type {
              VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
              VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST,
            },
            dst_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
            src_queue_family_index: 0,
            dst_queue_family_index: 0
          }
        },
        VkResourceTemplate::Buffer { .. } => {
          VkBarrierTemplate::Buffer {
            name: input.name.clone(),
            src_access_mask: match metadata.producer_pass_type {
              VkResourceProducerPassType::Graphics => vk::AccessFlags::SHADER_WRITE,
              VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
              VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
              VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE
            },
            dst_access_mask: vk::AccessFlags::MEMORY_READ,
            src_stage: match metadata.producer_pass_type {
              VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
              VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST
            },
            dst_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
            src_queue_family_index: 0,
            dst_queue_family_index: 0
          }
        }
        VkResourceTemplate::ExternalBuffer => {
          VkBarrierTemplate::Buffer {
            name: input.name.clone(),
            src_access_mask: match metadata.producer_pass_type {
              VkResourceProducerPassType::Graphics => vk::AccessFlags::SHADER_WRITE,
              VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
              VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
              VkResourceProducerPassType::Host => vk::AccessFlags::HOST_WRITE
            },
            dst_access_mask: vk::AccessFlags::MEMORY_READ,
            src_stage: match metadata.producer_pass_type {
              VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
              VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST,
              _ => unimplemented!()
            },
            dst_stage: vk::PipelineStageFlags::COMPUTE_SHADER,
            src_queue_family_index: 0,
            dst_queue_family_index: 0
          }
        }
        VkResourceTemplate::ExternalTexture { is_depth_stencil } => {
          let old_layout = std::mem::replace(&mut metadata.layout, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
          VkBarrierTemplate::Image {
            name: input.name.clone(),
            old_layout,
            new_layout: metadata.layout,
            src_access_mask: if *is_depth_stencil {
              vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            } else {
              match metadata.producer_pass_type {
                VkResourceProducerPassType::Graphics => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                VkResourceProducerPassType::Compute => vk::AccessFlags::SHADER_WRITE,
                VkResourceProducerPassType::Copy => vk::AccessFlags::TRANSFER_WRITE,
                _ => unimplemented!()
              }
            },
            dst_access_mask: vk::AccessFlags::SHADER_READ,
            src_stage: match metadata.producer_pass_type {
              VkResourceProducerPassType::Graphics => vk::PipelineStageFlags::ALL_GRAPHICS,
              VkResourceProducerPassType::Compute => vk::PipelineStageFlags::COMPUTE_SHADER,
              VkResourceProducerPassType::Copy => vk::PipelineStageFlags::empty(),
              VkResourceProducerPassType::Host => vk::PipelineStageFlags::HOST,
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
