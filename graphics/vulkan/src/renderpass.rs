use std::cmp::max;
use std::sync::Arc;
use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::usize;

use ash::vk;
use smallvec::SmallVec;
use sourcerenderer_core::graphics::{Format, LoadOp, RenderPassInfo, RenderPassPipelineStage, StoreOp};

use crate::format::format_to_vk;
use crate::pipeline::samples_to_vk;
use crate::{raw::RawVkDevice, texture::VkTextureView};

#[derive(Default)]
struct VkRenderPassAttachmentMetadata {
  pub produced_in_subpass: u32,
  pub last_used_in_subpass: u32,
  initial_layout: Option<vk::ImageLayout>,
  final_layout: Option<vk::ImageLayout>,
}

pub struct VkRenderPass {
  device: Arc<RawVkDevice>,
  render_pass: vk::RenderPass
}

impl VkRenderPass {
  pub fn new(device: &Arc<RawVkDevice>, info: &RenderPassInfo) -> Self {
    let mut attachment_references = Vec::<vk::AttachmentReference>::new();
    let mut subpass_infos = Vec::<vk::SubpassDescription>::with_capacity(info.subpasses.len());
    let mut dependencies = Vec::<vk::SubpassDependency>::new();
    let mut preserve_attachments = Vec::<u32>::new();

    let mut attachment_count = 0;
    for subpass in &info.subpasses {
      if subpass.depth_stencil_attachment.is_some() {
        attachment_count += 1;
      }

      attachment_count += subpass.input_attachments.len();
      attachment_count += subpass.output_color_attachments.len() * 2;
    }

    attachment_references.reserve_exact(attachment_count); // We must not allocate after this so the pointers stay valid
    preserve_attachments.reserve_exact(info.attachments.len() * info.subpasses.len());

    let mut attachment_metadata = HashMap::<u32, VkRenderPassAttachmentMetadata>::new();
    let mut subpass_attachment_bitmasks = Vec::<u64>::new();
    subpass_attachment_bitmasks.resize(info.subpasses.len(), 0);
    let subpass_dependencies_start = dependencies.len(); 
    for (subpass_index, subpass) in info.subpasses.iter().enumerate() {
      let subpass_attachment_bitmask: &mut u64 = subpass_attachment_bitmasks.get_mut(subpass_index).unwrap();

      for input_attachment in &subpass.input_attachments {
        let mut metadata = attachment_metadata.entry(input_attachment.index).or_default();
        let attachment_info = info.attachments.get(input_attachment.index as usize).unwrap();
        let mut dependency_opt = Some(build_dependency(subpass_index as u32, metadata.produced_in_subpass, attachment_info.format, input_attachment.pipeline_stage));
        dependency_opt = merge_dependencies(&mut dependencies[subpass_dependencies_start..], dependency_opt.unwrap());
        if let Some(dependency) = dependency_opt {
          dependencies.push(dependency);
        }

        if metadata.initial_layout.is_none() {
          metadata.initial_layout = Some(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        }
        metadata.last_used_in_subpass = max(metadata.last_used_in_subpass, subpass_index as u32);
        metadata.final_layout = Some(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        *subpass_attachment_bitmask |= 1 << input_attachment.index;
      }
      for color_attachment in &subpass.output_color_attachments {
        let mut metadata = attachment_metadata.entry(color_attachment.index).or_default();
        metadata.produced_in_subpass = max(metadata.produced_in_subpass, subpass_index as u32);
        if metadata.initial_layout.is_none() {
          metadata.initial_layout = Some(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        }
        metadata.final_layout = Some(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        *subpass_attachment_bitmask |= 1 << color_attachment.index;

        if let Some(resolve_attachment_index) = color_attachment.resolve_attachment_index {
          let mut resolve_metadata = attachment_metadata.entry(resolve_attachment_index).or_default();
          resolve_metadata.produced_in_subpass = max(resolve_metadata.produced_in_subpass, subpass_index as u32);
          if resolve_metadata.initial_layout.is_none() {
            resolve_metadata.initial_layout = Some(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
          }
          resolve_metadata.final_layout = Some(vk::ImageLayout::TRANSFER_DST_OPTIMAL);
          *subpass_attachment_bitmask |= 1 << resolve_attachment_index;
        }
      }
      if let Some(depth_stencil_attachment) = &subpass.depth_stencil_attachment {
        let mut depth_stencil_metadata = attachment_metadata.entry(depth_stencil_attachment.index).or_default();
        if depth_stencil_metadata.initial_layout.is_none() {
          depth_stencil_metadata.initial_layout = Some(if depth_stencil_attachment.read_only {
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
          } else {
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
          });
        } else {
          let mut dependency_opt = Some(build_depth_stencil_dependency(subpass_index as u32, depth_stencil_metadata.last_used_in_subpass, depth_stencil_metadata.initial_layout == Some(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)));
          dependency_opt = merge_dependencies(&mut dependencies[subpass_dependencies_start..], dependency_opt.unwrap());
          if let Some(dependency) = dependency_opt {
            dependencies.push(dependency);
          }
        }
        if depth_stencil_attachment.read_only { 
          depth_stencil_metadata.final_layout = Some(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL);
          depth_stencil_metadata.last_used_in_subpass = max(depth_stencil_metadata.last_used_in_subpass, subpass_index as u32);
        } else {
          depth_stencil_metadata.final_layout = Some(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
          depth_stencil_metadata.produced_in_subpass = max(depth_stencil_metadata.produced_in_subpass, subpass_index as u32);
        };
        *subpass_attachment_bitmask |= 1 << depth_stencil_attachment.index;
      }
    }
    for (index, attachment) in info.attachments.iter().enumerate() {
      if attachment.store_op == StoreOp::DontCare || attachment.stencil_store_op == StoreOp::DontCare {
        let mut metadata = attachment_metadata.entry(index as u32).or_default();
        metadata.last_used_in_subpass = vk::SUBPASS_EXTERNAL;
      }
    }

    for (subpass_index, subpass) in info.subpasses.iter().enumerate() {
      let p_input_attachments = unsafe { attachment_references.as_ptr().add(attachment_references.len()) };
      for input_attachment in &subpass.input_attachments {
        attachment_references.push(vk::AttachmentReference {
          attachment: input_attachment.index,
          layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        });
        debug_assert_eq!(attachment_references.capacity(), attachment_count);
      }

      let p_color_attachments = unsafe { attachment_references.as_ptr().add(attachment_references.len()) };
      for color_attachment in &subpass.output_color_attachments {
        attachment_references.push(vk::AttachmentReference {
          attachment: color_attachment.index,
          layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        });
        debug_assert_eq!(attachment_references.capacity(), attachment_count);
      }
      let p_resolve_attachments = unsafe { attachment_references.as_ptr().add(attachment_references.len()) };
      for color_attachment in &subpass.output_color_attachments {
        if let Some(resolve_attachment_index) = color_attachment.resolve_attachment_index {
          attachment_references.push(vk::AttachmentReference {
            attachment: resolve_attachment_index,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
          });
          debug_assert_eq!(attachment_references.capacity(), attachment_count);
        } else {
          attachment_references.push(vk::AttachmentReference {
            attachment: vk::ATTACHMENT_UNUSED,
            layout: vk::ImageLayout::UNDEFINED,
          });
          debug_assert_eq!(attachment_references.capacity(), attachment_count);
        }
      }

      let p_depth_stencil_attachment = if let Some(depth_stencil_attachment) = &subpass.depth_stencil_attachment {
        let p_depth_stencil_attachment = unsafe { attachment_references.as_ptr().add(attachment_references.len()) };
        attachment_references.push(vk::AttachmentReference {
          attachment: depth_stencil_attachment.index,
          layout: if depth_stencil_attachment.read_only {
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
          } else {
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
          },
        });
        debug_assert_eq!(attachment_references.capacity(), attachment_count);
        p_depth_stencil_attachment
      } else {
        std::ptr::null()
      };

      let p_preserve_attachments = unsafe { preserve_attachments.as_ptr().add(preserve_attachments.len()) };
      let preserve_attachments_offset = preserve_attachments.len();
      for (index, _) in info.attachments.iter().enumerate() {
        let subpass_attachment_bitmask = subpass_attachment_bitmasks.get(subpass_index).unwrap();
        let metadata = attachment_metadata.get(&(index as u32)).unwrap();
        if (subpass_attachment_bitmask & (1u64 << index as u64)) == 0 && metadata.last_used_in_subpass > (subpass_index as u32) {
          preserve_attachments.push(index as u32);
        }
      }

      subpass_infos.push(vk::SubpassDescription {
        flags: vk::SubpassDescriptionFlags::empty(),
        pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
        input_attachment_count: subpass.input_attachments.len() as u32,
        p_input_attachments,
        color_attachment_count: subpass.output_color_attachments.len() as u32,
        p_color_attachments,
        p_resolve_attachments,
        p_depth_stencil_attachment,
        preserve_attachment_count: (preserve_attachments.len() - preserve_attachments_offset) as u32,
        p_preserve_attachments,
      });
    }

    let attachments: Vec<vk::AttachmentDescription> = info.attachments.iter().enumerate().map(|(index, a)| {
      let metadata = attachment_metadata.get(&(index as u32)).unwrap();
      vk::AttachmentDescription {
        flags: vk::AttachmentDescriptionFlags::empty(),
        format: format_to_vk(a.format, device.supports_d24),
        samples: samples_to_vk(a.samples),
        load_op: load_op_to_vk(a.load_op),
        store_op: store_op_to_vk(a.store_op),
        stencil_load_op: load_op_to_vk(a.stencil_load_op),
        stencil_store_op: store_op_to_vk(a.stencil_store_op),
        initial_layout: metadata.initial_layout.unwrap(),
        final_layout: metadata.final_layout.unwrap(),
      }
    }).collect();

    let rp_info = vk::RenderPassCreateInfo {
        flags: vk::RenderPassCreateFlags::empty(),
        attachment_count: attachments.len() as u32,
        p_attachments: attachments.as_ptr(),
        subpass_count: subpass_infos.len() as u32,
        p_subpasses: subpass_infos.as_ptr(),
        dependency_count: dependencies.len() as u32,
        p_dependencies: dependencies.as_ptr(),
        ..Default::default()
    };
    Self {
      device: device.clone(),
      render_pass: unsafe { device.create_render_pass(&rp_info, None).unwrap() }
    }
  }

  pub fn handle(&self) -> &vk::RenderPass {
    &self.render_pass
  }
}

impl Drop for VkRenderPass {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_render_pass(self.render_pass, None);
    }
  }
}

impl Hash for VkRenderPass {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.render_pass.hash(state);
  }
}

impl PartialEq for VkRenderPass {
  fn eq(&self, other: &Self) -> bool {
    self.render_pass == other.render_pass
  }
}

impl Eq for VkRenderPass {}

pub struct VkFrameBuffer {
  device: Arc<RawVkDevice>,
  frame_buffer: vk::Framebuffer,
  width: u32,
  height: u32,
  render_pass: Arc<VkRenderPass>,
  attachments: SmallVec<[Arc<VkTextureView>; 8]>
}

impl VkFrameBuffer {
  pub(crate) fn new(device: &Arc<RawVkDevice>, width: u32, height: u32, render_pass: &Arc<VkRenderPass>, attachments: &[&Arc<VkTextureView>]) -> Self {
    let mut vk_attachments = SmallVec::<[vk::ImageView; 8]>::new();
    let mut attachment_refs = SmallVec::<[Arc<VkTextureView>; 8]>::new();
    for attachment in attachments {
      vk_attachments.push(*attachment.view_handle());
      attachment_refs.push((*attachment).clone());
    }

    Self {
      device: device.clone(),
      frame_buffer: unsafe { device.create_framebuffer(&vk::FramebufferCreateInfo {
          flags: vk::FramebufferCreateFlags::empty(),
          render_pass: *render_pass.handle(),
          attachment_count: vk_attachments.len() as u32,
          p_attachments: vk_attachments.as_ptr(),
          width,
          height,
          layers: 1,
          ..Default::default()
      }, None).unwrap() },
      width,
      height,
      attachments: attachment_refs,
      render_pass: render_pass.clone()
    }
  }

  pub(crate) fn handle(&self) -> &vk::Framebuffer {
    &self.frame_buffer
  }

  pub(crate) fn width(&self) -> u32 {
    self.width
  }

  pub(crate) fn height(&self) -> u32 {
    self.height
  }
}

impl Drop for VkFrameBuffer {
  fn drop(&mut self) {
    unsafe {
      self.device.destroy_framebuffer(self.frame_buffer, None);
    }
  }
}

fn load_op_to_vk(load_op: LoadOp) -> vk::AttachmentLoadOp {
  match load_op {
    LoadOp::Load => vk::AttachmentLoadOp::LOAD,
    LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
    LoadOp::DontCare => vk::AttachmentLoadOp::DONT_CARE,
  }
}

fn store_op_to_vk(store_op: StoreOp) -> vk::AttachmentStoreOp {
  match store_op {
    StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
    StoreOp::Store => vk::AttachmentStoreOp::STORE,
  }
}

fn build_dependency(subpass_index: u32, src_subpass: u32, format: Format, stage: RenderPassPipelineStage) -> vk::SubpassDependency {
  let mut vk_pipeline_stages = vk::PipelineStageFlags::empty();
  if stage.contains(RenderPassPipelineStage::FRAGMENT) {
    vk_pipeline_stages |= vk::PipelineStageFlags::FRAGMENT_SHADER;
  }
  if stage.contains(RenderPassPipelineStage::VERTEX) {
    vk_pipeline_stages |= vk::PipelineStageFlags::VERTEX_SHADER;
  }

  vk::SubpassDependency {
    src_subpass,
    dst_subpass: subpass_index,
    src_stage_mask: if format.is_depth() || format.is_stencil() {
      vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
    } else {
      vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
    },
    dst_stage_mask: vk_pipeline_stages,
    src_access_mask: if format.is_depth() || format.is_stencil() {
      vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
    } else {
      vk::AccessFlags::COLOR_ATTACHMENT_WRITE
    },
    dst_access_mask: vk::AccessFlags::SHADER_READ,
    dependency_flags: vk::DependencyFlags::BY_REGION,
  }
}

fn build_depth_stencil_dependency(subpass_index: u32, src_subpass: u32, memory_barrier: bool) -> vk::SubpassDependency {
  vk::SubpassDependency {
    src_subpass,
    dst_subpass: subpass_index,
    src_stage_mask: vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
    dst_stage_mask: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
    src_access_mask: if memory_barrier { vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE } else { vk::AccessFlags::empty() },
    dst_access_mask: if memory_barrier { vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE } else { vk::AccessFlags::empty() },
    dependency_flags: vk::DependencyFlags::BY_REGION,
  }
}

fn merge_dependencies(dependencies: &mut [vk::SubpassDependency], dependency: vk::SubpassDependency) -> Option<vk::SubpassDependency> {
  for existing_dependency in dependencies {
    if existing_dependency.src_subpass == dependency.src_subpass && existing_dependency.dst_subpass == dependency.dst_subpass {
      existing_dependency.src_access_mask |= dependency.src_access_mask;
      existing_dependency.dst_access_mask |= dependency.dst_access_mask;
      existing_dependency.src_stage_mask |= dependency.src_stage_mask;
      existing_dependency.dst_stage_mask |= dependency.dst_stage_mask;
      return None;
    }
  }
  Some(dependency)
}
