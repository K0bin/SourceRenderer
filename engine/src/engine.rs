use sourcerenderer_core::platform::{Platform, PlatformEvent, GraphicsApi};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::*;
use sourcerenderer_core::graphics::SwapchainInfo;
use sourcerenderer_core::graphics::CommandBufferType;
use sourcerenderer_core::graphics::CommandBuffer;
use sourcerenderer_core::graphics::MemoryUsage;
use sourcerenderer_core::graphics::BufferUsage;
use sourcerenderer_core::Vec2;
use sourcerenderer_core::Vec2I;
use sourcerenderer_core::Vec2UI;
use sourcerenderer_core::Vec3;
use sourcerenderer_core::graphics::*;
use std::rc::Rc;
use std::path::Path;
use sourcerenderer_core::platform::Window;
use async_std::task;
use async_std::prelude::*;
use async_std::future;
use std::thread::{Thread};
use std::future::Future;
use async_std::task::JoinHandle;
use std::cell::RefCell;
use sourcerenderer_core::graphics::graph::{RenderGraph, RenderGraphInfo, RenderGraphAttachmentInfo, RenderPassInfo, BACK_BUFFER_ATTACHMENT_NAME, OutputAttachmentReference};
use std::collections::HashMap;
use image::{GenericImage, GenericImageView};
use nalgebra::{Matrix4, Point3, Vector3, Rotation3};
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicUsize;
use crate::RendererMessage;
use crate::renderer::Renderer;
use crate::msg::GameplayMessage;
use crate::scene::Scene;
use async_std::sync::{channel, Sender, Receiver};

pub struct Engine<P: Platform> {
  platform: Box<P>
}

impl<P: Platform> Engine<P> {
  pub fn new(platform: Box<P>) -> Arc<Engine<P>> {
    return Arc::new(Engine {
      platform
    });
  }

  pub fn run(&mut self) {
    let render_sender = Renderer::run(self.platform.as_mut());
    let gameplay_sender = Scene::run(render_sender.clone());
  }
}