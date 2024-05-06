use std::collections::HashMap;

use metal;

use sourcerenderer_core::gpu::{self, TextureView};

use super::*;

fn store_action_to_mtl(action: gpu::StoreOp, resolve: bool) -> metal::MTLStoreAction {
    match action {
        gpu::StoreOp::Store => if resolve { metal::MTLStoreAction::StoreAndMultisampleResolve } else { metal::MTLStoreAction::Store },
        gpu::StoreOp::DontCare => metal::MTLStoreAction::DontCare,
    }
}

fn load_action_to_mtl(action: gpu::LoadOp) -> metal::MTLLoadAction {
    match action {
        gpu::LoadOp::Load => metal::MTLLoadAction::Load,
        gpu::LoadOp::Clear => metal::MTLLoadAction::Clear,
        gpu::LoadOp::DontCare => metal::MTLLoadAction::DontCare,
    }
}

pub(crate) fn render_pass_to_descriptors(info: &gpu::RenderPassBeginInfo<MTLBackend>) -> Vec<metal::RenderPassDescriptor> {
    let mut subpasses = Vec::<metal::RenderPassDescriptor>::with_capacity(info.subpasses.len());
    let mut first_and_last_used_in = HashMap::<u32, (u32, u32)>::new();
    for (subpass_index, subpass) in info.subpasses.iter().enumerate() {
        for output in subpass.output_color_attachments {
            let entry = first_and_last_used_in.entry(output.index).or_default();
            entry.0 = entry.0.min(subpass_index as u32);
            entry.1 = entry.0.max(subpass_index as u32);

            if let Some(resolve_index) = output.resolve_attachment_index {
                let entry = first_and_last_used_in.entry(resolve_index).or_default();
                entry.0 = entry.0.min(subpass_index as u32);
                entry.1 = entry.0.max(subpass_index as u32);
            }
        }
        if let Some(depth_stencil_index) = &subpass.depth_stencil_attachment {
            let entry = first_and_last_used_in.entry(depth_stencil_index.index).or_default();
            entry.0 = entry.0.min(subpass_index as u32);
            entry.1 = entry.0.max(subpass_index as u32);
        }
        for input in subpass.input_attachments {
            let entry = first_and_last_used_in.entry(input.index).or_default();
            entry.0 = entry.0.min(subpass_index as u32);
            entry.1 = entry.0.max(subpass_index as u32);
        }
    }

    for (subpass_index, subpass) in info.subpasses.iter().enumerate() {
        let descriptor = metal::RenderPassDescriptor::new();
        for (index, output) in subpass.output_color_attachments.iter().enumerate() {
            let attachment_desc = descriptor.color_attachments().object_at(index as u64).unwrap();
            let resource = info.attachments.get(output.index as usize).unwrap();
            let (first_used_in, last_used_in) = first_and_last_used_in.get(&output.index).unwrap();
            if *first_used_in == subpass_index as u32 {
                attachment_desc.set_load_action(load_action_to_mtl(resource.load_op));
            } else {
                attachment_desc.set_load_action(metal::MTLLoadAction::Load);
            }
            if *last_used_in == subpass_index as u32 {
                attachment_desc.set_store_action(store_action_to_mtl(resource.store_op, output.resolve_attachment_index.is_some()));
            } else {
                attachment_desc.set_store_action(metal::MTLStoreAction::Store);
            }
            match resource.view {
                gpu::RenderPassAttachmentView::RenderTarget(view) => {
                    attachment_desc.set_texture(Some(view.handle()));
                    attachment_desc.set_level(view.info().base_mip_level as u64);
                    attachment_desc.set_slice(view.info().base_array_layer as u64);
                },
                gpu::RenderPassAttachmentView::DepthStencil(_view) => unreachable!()
            }
            if let Some(view_index) = output.resolve_attachment_index {
                let resolve_resource = info.attachments.get(view_index as usize).unwrap();
                match resolve_resource.view {
                    gpu::RenderPassAttachmentView::RenderTarget(view) => {
                        attachment_desc.set_texture(Some(view.handle()));
                        attachment_desc.set_level(view.info().base_mip_level as u64);
                        attachment_desc.set_slice(view.info().base_array_layer as u64);
                    },
                    gpu::RenderPassAttachmentView::DepthStencil(_view) => unreachable!()
                }
            }
            attachment_desc.set_clear_color(metal::MTLClearColor::new(0f64, 0f64, 0f64, 1f64));
        }

        if let Some(depth_stencil) = subpass.depth_stencil_attachment.as_ref() {
            let attachment_desc = descriptor.depth_attachment().unwrap();
            let resource = info.attachments.get(depth_stencil.index as usize).unwrap();
            let (first_used_in, last_used_in) = first_and_last_used_in.get(&depth_stencil.index).unwrap();
            if *first_used_in == subpass_index as u32 {
                attachment_desc.set_load_action(load_action_to_mtl(resource.load_op));
            } else {
                attachment_desc.set_load_action(metal::MTLLoadAction::Load);
            }
            if *last_used_in == subpass_index as u32 {
                attachment_desc.set_store_action(store_action_to_mtl(resource.store_op, false));
            } else {
                attachment_desc.set_store_action(metal::MTLStoreAction::Store);
            }
            match resource.view {
                gpu::RenderPassAttachmentView::DepthStencil(view) => {
                    attachment_desc.set_texture(Some(view.handle()));
                    attachment_desc.set_level(view.info().base_mip_level as u64);
                    attachment_desc.set_slice(view.info().base_array_layer as u64);
                },
                gpu::RenderPassAttachmentView::RenderTarget(_view) => unreachable!()
            }
            attachment_desc.set_clear_depth(1f64);
        }
        if !subpass.input_attachments.is_empty() {
            todo!();
        }
        subpasses.push(descriptor.to_owned());
    }
    subpasses
}

