use std::ffi::CString;
use std::ops::Deref;
use std::sync::Arc;

use ash::VkResult;
use ash::vk;
use ash::vk::Handle;

use crate::raw::RawVkDevice;

pub struct RawVkCommandPool {
    pub pool: vk::CommandPool,
    pub device: Arc<RawVkDevice>,
}

impl RawVkCommandPool {
    pub fn new(
        device: &Arc<RawVkDevice>,
        create_info: &vk::CommandPoolCreateInfo,
        name: Option<&str>,
    ) -> VkResult<Self> {
        let pool = unsafe {
            device
                .create_command_pool(create_info, None)?
        };

        if let Some(name) = name {
            if let Some(debug_utils) = device.debug_utils.as_ref() {
                let name_cstring = CString::new(name).unwrap();
                unsafe {
                    debug_utils
                        .set_debug_utils_object_name(&vk::DebugUtilsObjectNameInfoEXT {
                            object_type: vk::ObjectType::COMMAND_POOL,
                            object_handle: pool.as_raw(),
                            p_object_name: name_cstring.as_ptr(),
                            ..Default::default()
                        })
                        .unwrap();
                }
            }
        }

        Ok(Self {
            pool,
            device: device.clone(),
        })
    }
}

impl Deref for RawVkCommandPool {
    type Target = vk::CommandPool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl Drop for RawVkCommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.device.destroy_command_pool(self.pool, None);
        }
    }
}
