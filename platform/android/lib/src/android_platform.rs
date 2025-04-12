use sourcerenderer_core::Platform;
use sourcerenderer_core::platform::FileWatcher;
use std::sync::Arc;
use sourcerenderer_core::platform::{Window, ThreadHandle};
use std::error::Error;
use sourcerenderer_vulkan::{VkBackend, VkInstance, VkSurface, VkDevice, VkSwapchain};
use ndk::native_window::NativeWindow;
use ndk_sys::{ANativeWindow_release, ANativeWindow_getWidth, ANativeWindow_getHeight};

use ash::extensions::khr::AndroidSurface;
use ash::extensions::khr::Surface;
use ash::vk;
use std::os::raw::c_void;
use crate::io::AndroidIO;

pub struct AndroidPlatform {
  window: AndroidWindow
}

impl AndroidPlatform {
  pub fn new(native_window: NativeWindow) -> Self {
    Self {
      window: AndroidWindow::new(native_window)
    }
  }

  pub(crate) fn change_window(&mut self, window: AndroidWindow) {
    self.window = window;
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

pub struct AndroidFileWatcher {}
impl FileWatcher for AndroidFileWatcher {
  fn watch<P: AsRef<std::path::Path>>(&mut self, _path: P) {
    // It's probably possible to implement this, but not super useful.
  }

  fn unwatch<P: AsRef<std::path::Path>>(&mut self, _path: P) {
    // It's probably possible to implement this, but not super useful.
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

    let instance_raw = graphics_instance.raw();
    let android_surface_loader = AndroidSurface::new(&instance_raw.entry, &instance_raw.instance);
    let surface = unsafe { android_surface_loader.create_android_surface(&vk::AndroidSurfaceCreateInfoKHR {
      flags: vk::AndroidSurfaceCreateFlagsKHR::empty(),
      window: self.native_window.ptr().as_ptr() as *mut c_void,
      ..Default::default()
    }, None).unwrap() };
    let surface_loader = Surface::new(&instance_raw.entry, &instance_raw.instance);
    Arc::new(VkSurface::new(instance_raw, surface, surface_loader))
  }

  fn width(&self) -> u32 {
    unsafe {
      ANativeWindow_getWidth(self.native_window.ptr().as_ptr()) as u32
    }
  }

  fn height(&self) -> u32 {
    unsafe {
      ANativeWindow_getHeight(self.native_window.ptr().as_ptr()) as u32
    }
  }
}
