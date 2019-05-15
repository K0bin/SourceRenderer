use std::sync::Arc;

use ash::vk;
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use sourcerenderer_core::graphics::{ Adapter, Device, AdapterType, Queue };
use crate::queue::VkQueue;

pub struct VkDevice {
  device: ash::Device,
  vk_graphics_queue: Arc<VkQueue>,
  vk_presentation_queue: Arc<VkQueue>,
  vk_compute_queue: Arc<VkQueue>,
  vk_transfer_queue: Arc<VkQueue>
}

impl VkDevice {
  pub fn new(device: ash::Device,
    graphics_queue: Arc<VkQueue>,
    presentation_queue: Arc<VkQueue>,
    compute_queue: Arc<VkQueue>,
    transfer_queue: Arc<VkQueue>) -> Self {


    return VkDevice {
      device: device,
      vk_graphics_queue: graphics_queue,
      vk_presentation_queue: presentation_queue,
      vk_compute_queue: compute_queue,
      vk_transfer_queue: transfer_queue
    };
  }
}

impl Device for VkDevice {
  fn graphics_queue(&self) -> Arc<dyn Queue> {
    return self.vk_graphics_queue.clone();
  }
  fn presentation_queue(&self) -> Arc<dyn Queue> {
    return self.vk_presentation_queue.clone();
  }
  fn compute_queue(&self) -> Arc<dyn Queue> {
    return self.vk_compute_queue.clone();
  }
  fn transfer_queue(&self) -> Arc<dyn Queue> {
    return self.vk_transfer_queue.clone();
  }
}