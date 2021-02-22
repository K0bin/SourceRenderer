use sourcerenderer_core::Platform;
use std::sync::Arc;
use sourcerenderer_core::platform::{Window, WindowState, InputState};
use std::error::Error;
use sourcerenderer_vulkan::{VkBackend, VkInstance, VkSurface, VkDevice, VkSwapchain};
use ndk::native_window::NativeWindow;
use ndk_sys::{ANativeWindow_release};

use ash::extensions::khr::AndroidSurface;
use ash::extensions::khr::Surface;
use ash::vk;
use std::os::raw::c_void;
use crate::io::AndroidIO;

pub struct AndroidPlatform {
  window: AndroidWindow,
  input_state: InputState,
}

impl AndroidPlatform {
  pub fn new(native_window: NativeWindow) -> Box<Self> {
    Box::new(Self {
      window: AndroidWindow::new(native_window),
      input_state: Default::default()
    })
  }

  pub(crate) fn input_state(&mut self) -> &mut InputState {
    &mut self.input_state
  }

  pub(crate) fn window_mut(&mut self) -> &mut AndroidWindow {
    &mut self.window
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

  fn input_state(&self) -> InputState {
    self.input_state.clone()
  }
}

pub struct AndroidWindow {
  native_window: NativeWindow,
  window_state: WindowState
}

impl AndroidWindow {
  pub fn new(native_window: NativeWindow) -> Self {
    let window_state = WindowState::FullScreen {
      width: native_window.width() as u32,
      height: native_window.height() as u32
    };
    Self {
      native_window,
      window_state
    }
  }

  pub(crate) fn state_mut(&mut self) -> &mut WindowState {
    &mut self.window_state
  }

  pub(crate) fn native_window(&self) -> &NativeWindow {
    &self.native_window
  }
}

impl Drop for AndroidWindow {
  fn drop(&mut self) {
    unsafe {
      ANativeWindow_release(self.native_window.ptr().as_ptr());
    }
  }
}

impl Window<AndroidPlatform> for AndroidWindow {
  fn create_surface(&self, graphics_instance: Arc<VkInstance>) -> Arc<VkSurface> {
    // thankfully, VkSurfaceKHR keeps a reference to the NativeWindow internally so I dont have to deal with that

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
    self.window_state.clone()
  }
}
