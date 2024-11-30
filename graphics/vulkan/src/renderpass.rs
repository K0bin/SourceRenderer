use std::{
    cmp::max,
    collections::HashMap,
    hash::{
        Hash,
        Hasher,
    },
    marker::PhantomData,
    sync::Arc,
    usize,
};

use ash::vk;
use smallvec::SmallVec;
use sourcerenderer_core::gpu::{self, ClearDepthStencilValue, LoadOpColor, LoadOpDepthStencil, StoreOp};

use super::*;

fn load_op_color_to_vk(load_op: gpu::LoadOpColor) -> vk::AttachmentLoadOp {
    match load_op {
        gpu::LoadOpColor::Load => vk::AttachmentLoadOp::LOAD,
        gpu::LoadOpColor::Clear(_) => vk::AttachmentLoadOp::CLEAR,
        gpu::LoadOpColor::DontCare => vk::AttachmentLoadOp::DONT_CARE,
    }
}
fn load_op_ds_to_vk(load_op: gpu::LoadOpDepthStencil) -> vk::AttachmentLoadOp {
    match load_op {
        gpu::LoadOpDepthStencil::Load => vk::AttachmentLoadOp::LOAD,
        gpu::LoadOpDepthStencil::Clear(_) => vk::AttachmentLoadOp::CLEAR,
        gpu::LoadOpDepthStencil::DontCare => vk::AttachmentLoadOp::DONT_CARE,
    }
}

fn store_op_to_vk(store_op: &gpu::StoreOp<VkBackend>) -> vk::AttachmentStoreOp {
    match store_op {
        gpu::StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
        gpu::StoreOp::Store => vk::AttachmentStoreOp::STORE,
        gpu::StoreOp::Resolve(_) => vk::AttachmentStoreOp::STORE,
    }
}

fn resolve_mode_to_vk(resolve_mode: gpu::ResolveMode) -> vk::ResolveModeFlags {
    match resolve_mode {
        gpu::ResolveMode::Average => vk::ResolveModeFlags::AVERAGE,
        gpu::ResolveMode::Min => vk::ResolveModeFlags::MIN,
        gpu::ResolveMode::Max => vk::ResolveModeFlags::MAX,
        gpu::ResolveMode::SampleZero => vk::ResolveModeFlags::SAMPLE_ZERO
    }
}

fn clear_color_to_vk(clear_color: gpu::ClearColor) -> vk::ClearValue {
    let mut val = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0f32; 4]
        }
    };
    unsafe {
        val.color.float32.clone_from_slice(clear_color.as_f32());
    }
    val
}

pub(crate) fn begin_render_pass(
    device: &RawVkDevice,
    command_buffer: vk::CommandBuffer,
    render_pass: &gpu::RenderPassBeginInfo<VkBackend>,
    recording_mode: gpu::RenderpassRecordingMode
) {

    let color_attachments: SmallVec<[vk::RenderingAttachmentInfo; 8]> = render_pass.render_targets
        .iter()
        .map(|rt| render_target_to_rendering_attachment_info(rt))
        .collect();

    let render_area = if let Some(rt) = render_pass.render_targets.first() {
        vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: rt.view.texture_info().width,
                height: rt.view.texture_info().height
            }
        }
    } else if let Some(dsv) = render_pass.depth_stencil.as_ref() {
        vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: dsv.view.texture_info().width,
                height: dsv.view.texture_info().height
            }
        }
    } else {
        panic!("Renderpass must have either color or depth stencil target")
    };

    let mut depth_stencil_attachment = vk::RenderingAttachmentInfo::default();

    if let Some(dsv) = render_pass.depth_stencil.as_ref() {
        let clear_value = if let LoadOpDepthStencil::Clear(clear_value) = dsv.load_op { clear_value } else { ClearDepthStencilValue::DEPTH_ONE };
        depth_stencil_attachment.clear_value = vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: clear_value.depth,
                stencil: clear_value.stencil
            }
        };
        depth_stencil_attachment.image_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        depth_stencil_attachment.image_view = dsv.view.view_handle();
        if let StoreOp::<VkBackend>::Resolve(resolve_view) = &dsv.store_op {
            depth_stencil_attachment.resolve_image_layout = vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;
            depth_stencil_attachment.resolve_image_view = resolve_view.view.view_handle();
            depth_stencil_attachment.resolve_mode = resolve_mode_to_vk(resolve_view.mode);
        }
        depth_stencil_attachment.load_op = load_op_ds_to_vk(dsv.load_op);
        depth_stencil_attachment.store_op = store_op_to_vk(&dsv.store_op);
    }

    let info = vk::RenderingInfo {
        flags: if recording_mode == gpu::RenderpassRecordingMode::CommandBuffers { vk::RenderingFlags::CONTENTS_SECONDARY_COMMAND_BUFFERS } else { vk::RenderingFlags::empty() },
        color_attachment_count: color_attachments.len() as u32,
        p_color_attachments: color_attachments.as_ptr(),
        view_mask: 0u32,
        layer_count: 1u32,
        render_area,
        p_depth_attachment: render_pass.depth_stencil.map(|_| &depth_stencil_attachment as *const vk::RenderingAttachmentInfo).unwrap_or(std::ptr::null()),
        p_stencil_attachment: render_pass.depth_stencil.map(|_| &depth_stencil_attachment as *const vk::RenderingAttachmentInfo).unwrap_or(std::ptr::null()),
        ..Default::default()
    };
    unsafe {
        device.cmd_begin_rendering(command_buffer, &info);
    }
}


pub(crate) fn render_target_to_rendering_attachment_info<'a>(render_target: &'a gpu::RenderTarget<'a, VkBackend>) -> vk::RenderingAttachmentInfo<'a> {
    let (resolve_view, resolve_layout, resolve_mode) = if let StoreOp::<VkBackend>::Resolve(resolve_attachment) = &render_target.store_op {
        (resolve_attachment.view.view_handle(), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, resolve_mode_to_vk(resolve_attachment.mode))
    } else { Default::default() };

    vk::RenderingAttachmentInfo {
        image_view: render_target.view.view_handle(),
        resolve_image_view: resolve_view,
        resolve_image_layout: resolve_layout,
        resolve_mode: resolve_mode,
        image_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        load_op: load_op_color_to_vk(render_target.load_op),
        store_op: store_op_to_vk(&render_target.store_op),
        clear_value: if let LoadOpColor::Clear(clear_color) = render_target.load_op { clear_color_to_vk(clear_color) } else { vk::ClearValue::default() },
        ..Default::default()
    }
}
