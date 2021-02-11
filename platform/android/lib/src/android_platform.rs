use sourcerenderer_core::{Platform, Vec2I};
use std::sync::{Arc, Mutex};
use parking_lot::{Mutex as StaticMutex, const_mutex};
use sourcerenderer_core::platform::{PlatformEvent, Window, WindowState, Input, Key};
use std::error::Error;
use sourcerenderer_vulkan::{VkBackend, VkInstance, VkSurface, VkDevice, VkSwapchain};
use sourcerenderer_core::graphics::Backend;
use ndk::native_window::NativeWindow;
use ndk_sys::{AAssetManager, AInputQueue};

use ash::extensions::khr::AndroidSurface;
use ash::extensions::khr::Surface;
use ash::vk;
use ash::vk::SurfaceKHR;
use std::os::raw::c_void;
use crate::io::AndroidIO;
use ndk::asset::AssetManager;
use std::ptr::NonNull;
use ndk::event::Keycode::{N, Mute};

pub static mut ASSET_MANAGER: *mut AAssetManager = std::ptr::null_mut();

pub struct AndroidPlatform {
  window: AndroidWindow
}

impl AndroidPlatform {
  pub fn new(native_window: NativeWindow) -> Box<Self> {
    Box::new(Self {
      window: AndroidWindow::new(native_window)
    })
  }
}

impl Platform for AndroidPlatform {
  type GraphicsBackend = VkBackend;
  type Window = AndroidWindow;
  type IO = AndroidIO;

  fn window(&self) -> &Self::Window {
    &self.window
  }

  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<VkInstance>, Box<dyn Error>> {
    Ok(Arc::new(VkInstance::new(&["VK_KHR_surface", "VK_KHR_android_surface"], debug_layers)))
  }
}

pub struct AndroidWindow {
  native_window: NativeWindow
}

impl AndroidWindow {
  pub fn new(native_window: NativeWindow) -> Self {
    Self {
      native_window
    }
  }
}

impl Window<AndroidPlatform> for AndroidWindow {
  fn create_surface(&self, graphics_instance: Arc<VkInstance>) -> Arc<VkSurface> {
    let instance_raw = graphics_instance.get_raw();
    let android_surface_loader = AndroidSurface::new(&instance_raw.entry, &instance_raw.instance);
    let surface = unsafe { android_surface_loader.create_android_surface(&vk::AndroidSurfaceCreateInfoKHR {
      flags: vk::AndroidSurfaceCreateFlagsKHR::empty(),
      window: self.native_window.ptr().as_ptr() as *mut c_void,
      ..Default::default()
    }, None).unwrap() };
    let surface_loader = Surface::new(&instance_raw.entry, &instance_raw.instance);
    Arc::new(VkSurface::new(instance_raw, surface, surface_loader))
  }

  fn create_swapchain(&self, vsync: bool, device: &VkDevice, surface: &Arc<VkSurface>) -> Arc<VkSwapchain> {
    let device_inner = device.get_inner();
    return VkSwapchain::new(vsync, self.native_window.width() as u32, self.native_window.height() as u32, device_inner, surface).unwrap();
  }

  fn state(&self) -> WindowState {
    WindowState::FullScreen {
      width: 1280,
      height: 720
    }
  }
}
