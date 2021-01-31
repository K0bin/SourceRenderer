use sourcerenderer_core::{Platform, Vec2I};
use std::sync::{Arc, Mutex};
use sourcerenderer_core::platform::{PlatformEvent, Window, WindowState, Input, Key};
use std::error::Error;
use sourcerenderer_vulkan::{VkBackend, VkInstance, VkSurface, VkDevice, VkSwapchain};
use sourcerenderer_core::graphics::Backend;
use ndk::native_window::NativeWindow;

use ash::extensions::khr::AndroidSurface;
use ash::extensions::khr::Surface;
use ash::vk;
use ash::vk::SurfaceKHR;
use std::os::raw::c_void;

pub struct AndroidPlatformBridge {
  native_window: Option<NativeWindow>
}

impl AndroidPlatformBridge {
  pub fn new() -> Arc<Mutex<Self>> {
    Arc::new(Mutex::new(Self {
      native_window: None
    }))
  }

  pub fn native_window(&self) -> Option<&NativeWindow> {
    self.native_window.as_ref()
  }

  pub fn change_native_window(&mut self, window: NativeWindow) {
    self.native_window = Some(window);
  }
}

pub struct AndroidPlatform {
  bridge: Arc<Mutex<AndroidPlatformBridge>>,
  input: Arc<AndroidInput>,
  window: AndroidWindow
}

impl AndroidPlatform {
  pub fn with_bridge(bridge: &Arc<Mutex<AndroidPlatformBridge>>) -> Box<Self> {
    Box::new(Self {
      bridge: bridge.clone(),
      window: AndroidWindow::new(bridge),
      input: Arc::new(AndroidInput {})
    })
  }
}

impl Platform for AndroidPlatform {
  type GraphicsBackend = VkBackend;
  type Window = AndroidWindow;
  type Input = AndroidInput;

  fn input(&self) -> &Arc<Self::Input> {
    &self.input
  }

  fn window(&mut self) -> &Self::Window {
    &self.window
  }

  fn handle_events(&mut self) -> PlatformEvent {
    // TODO
    PlatformEvent::Continue
  }

  fn create_graphics(&self, debug_layers: bool) -> Result<Arc<VkInstance>, Box<dyn Error>> {
    Ok(Arc::new(VkInstance::new(&["VK_KHR_surface", "VK_KHR_android_surface"], debug_layers)))
  }
}

pub struct AndroidWindow {
  bridge: Arc<Mutex<AndroidPlatformBridge>>
}

impl AndroidWindow {
  pub fn new(bridge: &Arc<Mutex<AndroidPlatformBridge>>) -> Self {
    Self {
      bridge: bridge.clone()
    }
  }
}

impl Window<AndroidPlatform> for AndroidWindow {
  fn create_surface(&self, graphics_instance: Arc<VkInstance>) -> Arc<VkSurface> {
    let bridge_guard = self.bridge.lock().unwrap();
    let window = bridge_guard.native_window.as_ref().expect("Can not create a vulkan surface without an Android surface");

    let instance_raw = graphics_instance.get_raw();
    let android_surface_loader = AndroidSurface::new(&instance_raw.entry, &instance_raw.instance);
    let surface = unsafe { android_surface_loader.create_android_surface(&vk::AndroidSurfaceCreateInfoKHR {
      flags: vk::AndroidSurfaceCreateFlagsKHR::empty(),
      window: window.ptr().as_ptr() as *mut c_void,
      ..Default::default()
    }, None).unwrap() };
    let surface_loader = Surface::new(&instance_raw.entry, &instance_raw.instance);
    Arc::new(VkSurface::new(instance_raw, surface, surface_loader))
  }

  fn create_swapchain(&self, vsync: bool, device: &VkDevice, surface: &Arc<VkSurface>) -> Arc<VkSwapchain> {
    let bridge_guard = self.bridge.lock().unwrap();
    let window = bridge_guard.native_window.as_ref().expect("Can not create a vulkan surface without an Android surface");

    let device_inner = device.get_inner();
    return VkSwapchain::new(vsync, window.width() as u32, window.height() as u32, device_inner, surface).unwrap();
  }

  fn state(&self) -> WindowState {
    WindowState::FullScreen {
      width: 1280,
      height: 720
    }
  }
}

pub struct AndroidInput {

}

impl Input for AndroidInput {
  fn is_key_down(&self, key: Key) -> bool {
    false
  }

  fn is_mouse_button_down(&self, button: u8) -> bool {
    false
  }

  fn mouse_position(&self) -> Vec2I {
    Vec2I::new(0, 0)
  }

  fn toggle_mouse_lock(&self, enabled: bool) {
  }
}
