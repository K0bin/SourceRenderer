use crate::queue::Queue;
use crate::presenter::Presenter;
use crate::presenter::SWAPCHAIN_EXT_NAME;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::sync::Arc;
use ash::{Entry, Instance, Device, vk };
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::extensions::khr::Surface;
use ash::vk::SurfaceKHR;
use sourcerenderer_core::renderer::{Mesh, Texture, Material};
use sourcerenderer_core::renderer::Renderer as AbstractRenderer;
use vk_mem::{Allocator, AllocatorCreateInfo, AllocatorCreateFlags};
use crate::resource::VkMesh;
use ash::extensions::khr;
use crate::instance;

pub struct Renderer {
  entry: Entry,
  instance: Instance,
  physical_device: vk::PhysicalDevice,
  device: Arc<Device>,
  graphics_queue: Queue,
  transfer_queue: Queue,
  compute_queue: Queue,
  present_queue: Queue,
  allocator: Allocator,
  presenter: Presenter
}

impl Renderer {
  pub fn new(entry: Entry, instance: Instance, surface: SurfaceKHR) -> Result<Box<Renderer>, Box<Error>> {
    unsafe {
      let surface_ext = Surface::new(&entry, &instance);
      let physical_device_desc = instance::pick_physical_device(&instance, &surface_ext, &surface);
      let physical_device = physical_device_desc.device;

      // Create queues
      let queue_create_descs: Vec<vk::DeviceQueueCreateInfo> = physical_device_desc.queue_families
        .iter()
        .map(|queue_desc|
          vk::DeviceQueueCreateInfo {
            queue_family_index: queue_desc.queue_family_index,
            queue_count: queue_desc.queue_count,
            p_queue_priorities: queue_desc.queue_priorities.as_ptr(),
            ..Default::default()
          }
        )
        .collect();

      let enabled_features: vk::PhysicalDeviceFeatures = Default::default();
      let extension_names: Vec<&str> = vec!(SWAPCHAIN_EXT_NAME);
      let extension_names_c: Vec<CString> = extension_names
        .iter()
        .map(|ext| CString::new(*ext).unwrap())
        .collect();
      let extension_names_ptr: Vec<*const i8> = extension_names_c
        .iter()
        .map(|ext_c| ext_c.as_ptr())
        .collect();

      let device_create_info = vk::DeviceCreateInfo {
        p_queue_create_infos: queue_create_descs.as_ptr(),
        queue_create_info_count: queue_create_descs.len() as u32,
        p_enabled_features: &enabled_features,
        pp_enabled_extension_names: extension_names_ptr.as_ptr(),
        enabled_extension_count: extension_names_c.len() as u32,
        ..Default::default()
      };

      let device = Arc::new(instance.create_device(physical_device, &device_create_info, None).unwrap());

      let graphics_queue_desc = physical_device_desc.graphics_queue;
      let graphics_queue = Queue::new(device.get_device_queue(graphics_queue_desc.queue_family_index, graphics_queue_desc.queue_index), graphics_queue_desc);
      let compute_queue_desc = physical_device_desc.compute_queue;
      let compute_queue = Queue::new(device.get_device_queue(compute_queue_desc.queue_family_index, compute_queue_desc.queue_index), compute_queue_desc);
      let transfer_queue_desc = physical_device_desc.transfer_queue;
      let transfer_queue = Queue::new(device.get_device_queue(transfer_queue_desc.queue_family_index, transfer_queue_desc.queue_index), transfer_queue_desc);
      let present_queue_desc = physical_device_desc.present_queue.expect("No present queue");
      let present_queue = Queue::new(device.get_device_queue(present_queue_desc.queue_family_index, present_queue_desc.queue_index), present_queue_desc);

      let allocator_info = AllocatorCreateInfo {
        physical_device: physical_device.clone(),
        device: (*device).clone(),
        instance: instance.clone(),
        flags: AllocatorCreateFlags::NONE,
        preferred_large_heap_block_size: 0,
        frame_in_use_count: 5,
        heap_size_limits: None
      };
      let allocator = Allocator::new(&allocator_info).unwrap();

      let swapchain_ext = khr::Swapchain::new(&instance, &*device);
      let presenter = Presenter::new(&physical_device, device.clone(), surface_ext, surface, swapchain_ext);

      return Ok(Box::new(Renderer {
        entry: entry,
        instance: instance,
        physical_device: physical_device,
        device: device,
        graphics_queue: graphics_queue,
        compute_queue: compute_queue,
        transfer_queue: transfer_queue,
        present_queue: present_queue,
        allocator: allocator,
        presenter: presenter
      }));
    }
  }
}

impl AbstractRenderer for Renderer {
  fn create_texture(&mut self) -> Box<Texture> {
    unimplemented!();
  }

  fn create_mesh(&mut self, vertex_size: u64, index_size: u64) -> Box<Mesh> {
    unimplemented!();
  }

  fn create_material(&mut self) -> Box<Material> {
    unimplemented!();
  }

  fn render(&mut self) {
  }
}

