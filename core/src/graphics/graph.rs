use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::cmp::Eq;
use std::ops::Fn;

use crate::graphics::{ Backend, VertexLayoutInfo, RasterizerInfo, DepthStencilInfo, BlendInfo, Format, SampleCount };

pub struct RenderGraphInfo {
  pub attachments: HashMap<String, RenderGraphAttachmentInfo>,
  pub passes: Vec<RenderPassInfo>
}

pub struct RenderPassInfo {
  pub outputs: Vec<OutputAttachmentReference>,
  pub inputs: Vec<InputAttachmentReference>,
  pub render: Box<dyn Fn(usize, bool) -> usize>
}

pub enum AttachmentSizeClass {
  Absolute,
  RelativeToSwapchain
}

pub struct RenderGraphAttachmentInfo {
  pub format: Format,
  pub size_class: AttachmentSizeClass,
  pub width: f32,
  pub height: f32,
  pub levels: u32,
  pub samples: SampleCount,
  pub external: bool
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct InputAttachmentReference {
  pub name: String
  /* maybe allow resolve/blit here */
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct OutputAttachmentReference {
  pub name: String
}

pub const BACK_BUFFER_ATTACHMENT_NAME: &str = "backbuffer";

pub trait RenderGraph {
}

/*pub struct RenderGraphNode<'a> {
  pub pass: &'a RenderPassInfo<'a>,
  pub parent: &'a RenderGraphNode<'a>
}

struct RenderGraphTree<'a> {
  pub nodes: Vec<RenderGraphNode<'a>>,
  pub root: &'a RenderGraphNode<'a>
}

struct RenderGraphAttachment<'a> {
  output_by: &'a RenderPassInfo<'a>,
  input_for: HashSet<&'a RenderPassInfo<'a>>
}

fn analyze_render_graph<B: Backend>(info: &RenderGraphInfo) {
  let mut attachments: HashMap<&str, RenderGraphAttachment> = HashMap::new();
  for &pass in info.passes {
    for output in pass.outputs {
      if attachments.contains_key(&output.name as &str) {
        panic!("reused"); // TODO: handle errors gracefully
      }
      if pass.inputs.iter().any(|i| i.name == output.name) {
        panic!("hazard");
      }
      attachments.insert(&output.name, RenderGraphAttachment {
        output_by: pass,
        input_for: HashSet::new()
      });
    }
  }
  for &pass in info.passes {
    for input in pass.inputs {
      let attachment = attachments.get_mut(&input.name as &str).expect("undeclared input");
      attachment.input_for.insert(pass);
    }
  }
}*/
