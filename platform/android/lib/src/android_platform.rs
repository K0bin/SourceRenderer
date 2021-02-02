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
use crate::input::AndroidInput;

pub static mut BRIDGE: StaticMutex<AndroidBridge> = const_mutex(AndroidBridge {
  native_window: None,
  asset_manager: None,
  input: None
});

pub struct AndroidBridge {
  native_window: Option<NativeWindow>,
  asset_manager: Option<NonNull<AAssetManager>>,
  input: Option<Arc<AndroidInput>>
}

unsafe impl Send for AndroidBridge {}

impl AndroidBridge {
  pub fn native_window(&self) -> Option<&NativeWindow> {
    self.native_window.as_ref()
  }

  pub fn asset_manager(&self) -> Option<NonNull<AAssetManager>> {
    self.asset_manager
  }

  pub fn set_native_window(&mut self, window: Option<NativeWindow>) {
    self.native_window = window;
  }

  pub fn set_asset_manager(&mut self, asset_manager: Option<NonNull<AAssetManager>>) {
    self.asset_manager = asset_manager;
  }

  pub fn input(&mut self) -> &Arc<AndroidInput> {
    if self.input.is_none() {
      self.input = Some(Arc::new(AndroidInput::new()))
    }
    self.input.as_ref().unwrap()
  }

  pub fn clear_context_related(&mut self) {
    self.asset_manager = None;
    self.native_window = None;
  }
}

pub struct AndroidPlatform {
  input: Arc<AndroidInput>,
  window: AndroidWindow
}

impl AndroidPlatform {
  pub fn new() -> Box<Self> {
    let input = {
      let mut brige_guard = unsafe {
        BRIDGE.lock()
      };
      brige_guard.input().clone()
    };

    Box::new(Self {
      input,
      window: AndroidWindow::new()
    })
  }
}

impl Platform for AndroidPlatform {
  type GraphicsBackend = VkBackend;
  type Window = AndroidWindow;
  type Input = AndroidInput;
  type IO = AndroidIO;

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
}

impl AndroidWindow {
  pub fn new() -> Self {
    Self {
    }
  }
}

impl Window<AndroidPlatform> for AndroidWindow {
  fn create_surface(&self, graphics_instance: Arc<VkInstance>) -> Arc<VkSurface> {
    let window = unsafe {
      let bridge = BRIDGE.lock();
      bridge.native_window.clone()
    }.expect("Can not create a vulkan surface without an Android surface");

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
    let window = unsafe {
      let bridge = BRIDGE.lock();
      bridge.native_window.clone()
    }.expect("Can not create a vulkan surface without an Android surface");

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
