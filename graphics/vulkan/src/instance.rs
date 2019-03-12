use std::ffi::{CStr, CString};
use std::cmp::Ordering;

use ash::vk;
use ash::{Entry, Instance};
use ash::extensions::khr;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use crate::queue::{QueueDesc, QueueFamily};
use crate::presenter::SWAPCHAIN_EXT_NAME;

pub struct PhysicalDeviceDesc {
  pub device: vk::PhysicalDevice,
  pub queue_families: Vec<QueueFamily>,
  pub graphics_queue: QueueDesc,
  pub compute_queue: QueueDesc,
  pub transfer_queue: QueueDesc,
  pub present_queue: Option<QueueDesc>
}

pub unsafe fn initialize_vulkan(instance_extensions: Vec<&str>, debug: bool) -> Result<(Entry, Instance), ash::InstanceError> {
  let entry: Entry = Entry::new().unwrap();

  let app_name = CString::new("CS:GO").unwrap();
  let app_name_ptr = app_name.as_ptr();
  let engine_name = CString::new("SourceRenderer").unwrap();
  let engine_name_ptr = engine_name.as_ptr();

  let mut layer_names_c: Vec<CString> = Vec::new();
  if debug {
    layer_names_c.push(CString::new("VK_LAYER_LUNARG_standard_validation").unwrap());
  }
  let layer_names_ptr: Vec<*const i8> = layer_names_c
    .iter()
    .map(|raw_name| raw_name.as_ptr())
    .collect();

  let extension_names_c: Vec<CString> = instance_extensions
    .iter()
    .map(|ext| CString::new(*ext).unwrap())
    .collect();
  let extension_names_ptr: Vec<*const i8> = extension_names_c
    .iter()
    .map(|ext_c| ext_c.as_ptr())
    .collect();

  let app_info = vk::ApplicationInfo {
    api_version: vk_make_version!(1, 0, 36),
    application_version: vk_make_version!(0, 0, 1),
    engine_version: vk_make_version!(0, 0, 1),
    p_application_name: app_name_ptr,
    p_engine_name: engine_name_ptr,
    ..Default::default()
  };

  let instance_create_info = vk::InstanceCreateInfo {
      p_application_info: &app_info,
      pp_enabled_layer_names: layer_names_ptr.as_ptr(),
      enabled_layer_count: layer_names_ptr.len() as u32,
      pp_enabled_extension_names: extension_names_ptr.as_ptr(),
      enabled_extension_count: extension_names_ptr.len() as u32,
      ..Default::default()
  };

  let instance = entry.create_instance(&instance_create_info, None).unwrap();

  return Ok((entry, instance));
}


pub unsafe fn pick_physical_device(instance: &Instance, surface_ext: &khr::Surface, surface: &vk::SurfaceKHR) -> PhysicalDeviceDesc {
  let mut physical_devices: Vec<vk::PhysicalDevice> = instance.enumerate_physical_devices().unwrap();
  physical_devices.sort_by(|a, b| {
      let props_a = instance.get_physical_device_properties(*a);
      let props_b = instance.get_physical_device_properties(*b);
      return if props_a.device_type == props_b.device_type {
        Ordering::Equal
      } else if props_a.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        Ordering::Less
      } else {
        Ordering::Greater
      }
    });
  return physical_devices
    .iter()
    .map(|device| get_device_description(instance, device, surface_ext, surface))
    .filter(|device_desc| {
        let extensions = instance.enumerate_device_extension_properties(device_desc.device).unwrap();
        if !extensions.iter().any(|ext_info| CStr::from_ptr(ext_info.extension_name.as_ptr()).to_str().unwrap() == SWAPCHAIN_EXT_NAME) {
          false
        } else {
          let desc = get_device_description(instance, &device_desc.device, surface_ext, surface);
          desc.present_queue.is_some()
        }
      }
    )
    .nth(0)
    .unwrap();
}

unsafe fn get_device_description(instance: &Instance, physical_device: &vk::PhysicalDevice, surface_ext: &khr::Surface, surface: &vk::SurfaceKHR) -> PhysicalDeviceDesc {
  let queue_properties = instance.get_physical_device_queue_family_properties(*physical_device);

  let graphics_queue_family_props = queue_properties
    .iter()
    .enumerate()
    .find(|(_, queue_props)|
      queue_props.queue_count > 0
      && queue_props.queue_flags & vk::QueueFlags::GRAPHICS == vk::QueueFlags::GRAPHICS
    )
    .expect("Vulkan device has no graphics queue");

  let compute_queue_family_props = queue_properties
    .iter()
    .enumerate()
    .find(|(index, queue_props)|
      queue_props.queue_count > 0
      && queue_props.queue_flags & vk::QueueFlags::COMPUTE == vk::QueueFlags::COMPUTE
      && *index as u32 != graphics_queue_family_props.0 as u32
    );

  let transfer_queue_family_props = queue_properties
    .iter()
    .enumerate()
    .find(|(index, queue_props)|
      queue_props.queue_count > 0
      && queue_props.queue_flags & vk::QueueFlags::TRANSFER == vk::QueueFlags::TRANSFER
      && *index as u32 != graphics_queue_family_props.0 as u32
      && (compute_queue_family_props.is_none() || *index as u32 != compute_queue_family_props.unwrap().0 as u32)
    );

  let present_queue_family_props = queue_properties
    .iter()
    .enumerate()
    .find(|(index, queue_props)|
      queue_props.queue_count > 0
      && surface_ext.get_physical_device_surface_support(*physical_device, *index as u32, *surface)
      && *index as u32 != graphics_queue_family_props.0 as u32
      && (compute_queue_family_props.is_none() || *index as u32 != compute_queue_family_props.unwrap().0 as u32)
      && (transfer_queue_family_props.is_none() || *index as u32 != transfer_queue_family_props.unwrap().0 as u32)
    );

  let mut graphics_queue_priorities: Vec<f32> = vec!();
  let graphics_queue_info = QueueDesc {
    queue_family_index: graphics_queue_family_props.0 as u32,
    queue_index: graphics_queue_priorities.len() as u32
  };
  graphics_queue_priorities.push(1f32);

  let mut compute_queue_priorities: Vec<f32> = vec!();
  let compute_queue_info = compute_queue_family_props.map_or_else(||
    if graphics_queue_family_props.1.queue_flags & vk::QueueFlags::COMPUTE == vk::QueueFlags::COMPUTE {
      if graphics_queue_family_props.1.queue_count > graphics_queue_priorities.len() as u32 {
        //Use additional graphics queue
        let result = QueueDesc {
          queue_family_index: graphics_queue_family_props.0 as u32,
          queue_index: graphics_queue_priorities.len() as u32
        };
        graphics_queue_priorities.push(0.9f32);
        result
      } else {
        //Use last graphics queue
        QueueDesc {
          queue_family_index: graphics_queue_family_props.0 as u32,
          queue_index: graphics_queue_priorities.len() as u32 - 1
        }
      }
    } else {
      //No compute queue
      unreachable!("Vulkan device has no compute queue")
    },
    |(index, _)| {
      //There is a separate queue family specifically for compute
      let result = QueueDesc {
        queue_family_index: index as u32,
        queue_index: compute_queue_priorities.len() as u32
      };
      compute_queue_priorities.push(1.0f32);
      result
    }
  );

  let mut transfer_queue_priorities: Vec<f32> = vec!();
  let transfer_queue_info = transfer_queue_family_props.map_or_else(||
    //queues have to support transfer operations if they support either graphics or compute
    if compute_queue_family_props.is_some()
      && compute_queue_family_props.unwrap().1.queue_count > 1 {
      //Use additional compute queue
      let result = QueueDesc {
        queue_family_index: compute_queue_family_props.unwrap().0 as u32,
        queue_index: compute_queue_priorities.len() as u32
      };
      compute_queue_priorities.push(0.6f32);
      result
    } else if graphics_queue_family_props.1.queue_count > graphics_queue_priorities.len() as u32 {
      //Use additional graphics queue
      let result = QueueDesc {
        queue_family_index: graphics_queue_family_props.0 as u32,
        queue_index: graphics_queue_priorities.len() as u32
      };
      graphics_queue_priorities.push(0.6f32);
      result
    } else if compute_queue_family_props.is_some() {
      //Use last compute queue
      QueueDesc {
        queue_family_index: compute_queue_family_props.unwrap().0 as u32,
        queue_index: compute_queue_priorities.len() as u32 - 1
      }
    } else {
      //Use last graphics queue
      QueueDesc {
        queue_family_index: graphics_queue_family_props.0 as u32,
        queue_index: graphics_queue_priorities.len() as u32 - 1
      }
    },
    |(index, _)| {
      //There is a separate queue family specifically for transfers
      let result = QueueDesc {
        queue_family_index: index as u32,
        queue_index: 0
      };
      transfer_queue_priorities.push(1.0f32);
      result
    }
  );

  let mut present_queue_priorities: Vec<f32> = vec!();
  let present_queue_info = present_queue_family_props.map_or_else(||
    //queues have to support transfer operations if they support either graphics or compute, no need to check it
    if compute_queue_family_props.is_some()
      && surface_ext.get_physical_device_surface_support(*physical_device, compute_queue_family_props.unwrap().0 as u32, *surface) {
      if compute_queue_family_props.unwrap().1.queue_count > 1 {
        //Use additional compute queue
        let result = Some(QueueDesc {
          queue_family_index: compute_queue_family_props.unwrap().0 as u32,
          queue_index: compute_queue_priorities.len() as u32
        });
        compute_queue_priorities.push(1.0f32);
        result
      } else {
        //Use last compute queue
        Some(QueueDesc {
          queue_family_index: compute_queue_family_props.unwrap().0 as u32,
          queue_index: compute_queue_priorities.len() as u32 - 1
        })
      }
    } else if surface_ext.get_physical_device_surface_support(*physical_device, graphics_queue_family_props.0 as u32, *surface) {
      if graphics_queue_family_props.1.queue_count > graphics_queue_priorities.len() as u32 {
        //Use additional graphics queue
        let result = Some(QueueDesc {
          queue_family_index: graphics_queue_family_props.0 as u32,
          queue_index: graphics_queue_priorities.len() as u32
        });
        graphics_queue_priorities.push(1.0f32);
        result
      } else {
        //Use last graphics queue
        Some(QueueDesc {
          queue_family_index: graphics_queue_family_props.0 as u32,
          queue_index: graphics_queue_priorities.len() as u32 - 1
        })
      }
    } else {
      None
    },
    |(index, _)| {
      //There is a separate queue family specifically for transfers
      let result = Some(QueueDesc {
        queue_family_index: index as u32,
        queue_index: 0
      });
      present_queue_priorities.push(1.0f32);
      result
    }
  );

  let mut queue_families: Vec<QueueFamily> = vec!();
  queue_families.push(QueueFamily {
    queue_family_index: graphics_queue_family_props.0 as u32,
    queue_count: graphics_queue_priorities.len() as u32,
    queue_priorities: graphics_queue_priorities
  });
  if compute_queue_family_props.is_some() {
    queue_families.push(QueueFamily {
      queue_family_index: compute_queue_family_props.unwrap().0 as u32,
      queue_count: compute_queue_priorities.len() as u32,
      queue_priorities: compute_queue_priorities
    });
  }
  if transfer_queue_family_props.is_some() {
    queue_families.push(QueueFamily {
      queue_family_index: transfer_queue_family_props.unwrap().0 as u32,
      queue_count: transfer_queue_priorities.len() as u32,
      queue_priorities: transfer_queue_priorities
    });
  }
  if present_queue_family_props.is_some() {
    queue_families.push(QueueFamily {
      queue_family_index: present_queue_family_props.unwrap().0 as u32,
      queue_count: present_queue_priorities.len() as u32,
      queue_priorities: present_queue_priorities
    });
  }

  return PhysicalDeviceDesc {
    device: *physical_device,
    queue_families: queue_families,
    graphics_queue: graphics_queue_info,
    compute_queue: compute_queue_info,
    transfer_queue: transfer_queue_info,
    present_queue: present_queue_info
  };
}
