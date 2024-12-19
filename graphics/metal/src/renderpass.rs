use metal;

use sourcerenderer_core::gpu::{self, LoadOpColor, LoadOpDepthStencil, StoreOp, TextureView};

use super::*;

fn store_action_to_mtl(action: &gpu::StoreOp<MTLBackend>) -> metal::MTLStoreAction {
    match action {
        gpu::StoreOp::Store => metal::MTLStoreAction::Store,
        gpu::StoreOp::DontCare => metal::MTLStoreAction::DontCare,
        gpu::StoreOp::Resolve(_) => metal::MTLStoreAction::StoreAndMultisampleResolve
    }
}

fn load_action_color_to_mtl(action: gpu::LoadOpColor) -> metal::MTLLoadAction {
    match action {
        gpu::LoadOpColor::Load => metal::MTLLoadAction::Load,
        gpu::LoadOpColor::Clear(_) => metal::MTLLoadAction::Clear,
        gpu::LoadOpColor::DontCare => metal::MTLLoadAction::DontCare,
    }
}

fn load_action_depth_stencil_to_mtl(action: gpu::LoadOpDepthStencil) -> metal::MTLLoadAction {
    match action {
        gpu::LoadOpDepthStencil::Load => metal::MTLLoadAction::Load,
        gpu::LoadOpDepthStencil::Clear(_) => metal::MTLLoadAction::Clear,
        gpu::LoadOpDepthStencil::DontCare => metal::MTLLoadAction::DontCare,
    }
}

pub(crate) fn render_pass_to_descriptors(info: &gpu::RenderPassBeginInfo<MTLBackend>) -> metal::RenderPassDescriptor {
    let descriptor = metal::RenderPassDescriptor::new();
    for (index, rt) in info.render_targets.iter().enumerate() {
        let attachment_desc = descriptor.color_attachments().object_at(index as u64).unwrap();
        attachment_desc.set_load_action(load_action_color_to_mtl(rt.load_op));
        attachment_desc.set_store_action(store_action_to_mtl(&rt.store_op));
        attachment_desc.set_texture(Some(rt.view.handle()));
        attachment_desc.set_level(rt.view.info().base_mip_level as u64);
        attachment_desc.set_slice(rt.view.info().base_array_layer as u64);
        if let StoreOp::Resolve(resolve_view) = &rt.store_op {
            attachment_desc.set_texture(Some(resolve_view.view.handle()));
            attachment_desc.set_level(resolve_view.view.info().base_mip_level as u64);
            attachment_desc.set_slice(resolve_view.view.info().base_array_layer as u64);
        }
        if let LoadOpColor::Clear(color) = rt.load_op {
            attachment_desc.set_clear_color(metal::MTLClearColor::new(
                color.as_f32()[0] as f64,
                color.as_f32()[1] as f64,
                color.as_f32()[2] as f64,
                color.as_f32()[3] as f64,
            ));
        }
    }

    if let Some(dsv) = info.depth_stencil {
        let attachment_desc = descriptor.depth_attachment().unwrap();
        attachment_desc.set_load_action(load_action_depth_stencil_to_mtl(dsv.load_op));
        attachment_desc.set_store_action(store_action_to_mtl(&dsv.store_op));
        attachment_desc.set_texture(Some(dsv.view.handle()));
        attachment_desc.set_level(dsv.view.info().base_mip_level as u64);
        attachment_desc.set_slice(dsv.view.info().base_array_layer as u64);
        if let StoreOp::Resolve(resolve_view) = &dsv.store_op {
            attachment_desc.set_texture(Some(resolve_view.view.handle()));
            attachment_desc.set_level(resolve_view.view.info().base_mip_level as u64);
            attachment_desc.set_slice(resolve_view.view.info().base_array_layer as u64);
        }
        if let LoadOpDepthStencil::Clear(value) = dsv.load_op {
            attachment_desc.set_clear_depth(value.depth as f64);
        }
    }
    descriptor.to_owned()
}

