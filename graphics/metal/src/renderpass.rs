use objc2::{ffi::NSUInteger, rc::Retained};
use objc2_metal;

use sourcerenderer_core::gpu::{self, TextureView as _};

use super::*;

fn store_action_to_mtl(action: &gpu::StoreOp<MTLBackend>) -> objc2_metal::MTLStoreAction {
    match action {
        gpu::StoreOp::Store => objc2_metal::MTLStoreAction::Store,
        gpu::StoreOp::DontCare => objc2_metal::MTLStoreAction::DontCare,
        gpu::StoreOp::Resolve(_) => objc2_metal::MTLStoreAction::StoreAndMultisampleResolve
    }
}

fn load_action_color_to_mtl(action: gpu::LoadOpColor) -> objc2_metal::MTLLoadAction {
    match action {
        gpu::LoadOpColor::Load => objc2_metal::MTLLoadAction::Load,
        gpu::LoadOpColor::Clear(_) => objc2_metal::MTLLoadAction::Clear,
        gpu::LoadOpColor::DontCare => objc2_metal::MTLLoadAction::DontCare,
    }
}

fn load_action_depth_stencil_to_mtl(action: gpu::LoadOpDepthStencil) -> objc2_metal::MTLLoadAction {
    match action {
        gpu::LoadOpDepthStencil::Load => objc2_metal::MTLLoadAction::Load,
        gpu::LoadOpDepthStencil::Clear(_) => objc2_metal::MTLLoadAction::Clear,
        gpu::LoadOpDepthStencil::DontCare => objc2_metal::MTLLoadAction::DontCare,
    }
}

pub(crate) unsafe fn render_pass_to_descriptors(info: &gpu::RenderPassBeginInfo<MTLBackend>) -> Retained<objc2_metal::MTLRenderPassDescriptor> {
    let descriptor = objc2_metal::MTLRenderPassDescriptor::new();
    for (index, rt) in info.render_targets.iter().enumerate() {
        let attachment_desc = objc2_metal::MTLRenderPassColorAttachmentDescriptor::new();
        attachment_desc.setLoadAction(load_action_color_to_mtl(rt.load_op));
        attachment_desc.setStoreAction(store_action_to_mtl(&rt.store_op));
        attachment_desc.setTexture(Some(rt.view.handle()));
        attachment_desc.setLevel(rt.view.info().base_array_layer as NSUInteger);
        attachment_desc.setSlice(rt.view.info().base_array_layer as NSUInteger);
        if let gpu::StoreOp::Resolve(resolve_view) = &rt.store_op {
            attachment_desc.setTexture(Some(resolve_view.view.handle()));
            attachment_desc.setLevel(resolve_view.view.info().base_mip_level as NSUInteger);
            attachment_desc.setSlice(resolve_view.view.info().base_array_layer as NSUInteger);
        }
        if let gpu::LoadOpColor::Clear(color) = rt.load_op {
            attachment_desc.setClearColor(objc2_metal::MTLClearColor {
                red: color.as_f32()[0] as f64,
                green: color.as_f32()[1] as f64,
                blue: color.as_f32()[2] as f64,
                alpha: color.as_f32()[3] as f64,
            });
        }
        descriptor.colorAttachments().setObject_atIndexedSubscript(Some(&attachment_desc), index as usize);
    }

    if let Some(dsv) = info.depth_stencil {
        let attachment_desc = descriptor.depthAttachment();
        attachment_desc.setLoadAction(load_action_depth_stencil_to_mtl(dsv.load_op));
        attachment_desc.setStoreAction(store_action_to_mtl(&dsv.store_op));
        attachment_desc.setTexture(Some(dsv.view.handle()));
        attachment_desc.setLevel(dsv.view.info().base_mip_level as NSUInteger);
        attachment_desc.setSlice(dsv.view.info().base_array_layer as NSUInteger);
        if let gpu::StoreOp::Resolve(resolve_view) = &dsv.store_op {
            attachment_desc.setTexture(Some(resolve_view.view.handle()));
            attachment_desc.setLevel(resolve_view.view.info().base_mip_level as NSUInteger);
            attachment_desc.setSlice(resolve_view.view.info().base_array_layer as NSUInteger);
        }
        if let gpu::LoadOpDepthStencil::Clear(value) = dsv.load_op {
            attachment_desc.setClearDepth(value.depth as f64);
        }
    }
    descriptor.to_owned()
}

