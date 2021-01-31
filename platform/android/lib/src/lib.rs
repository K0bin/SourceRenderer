extern crate ndk_sys;
extern crate jni;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;

mod android_platform;

use std::ffi::CString;
use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject};
use jni::sys::{jstring, jlong, jint};
use ndk_sys::__android_log_print;
use ndk_sys::android_LogPriority_ANDROID_LOG_VERBOSE;
use crate::android_platform::{AndroidPlatform, AndroidPlatformBridge};
use sourcerenderer_engine::Engine;
use std::sync::{Arc, Mutex};
use std::os::raw::c_void;
use ndk_sys::ANativeWindow_fromSurface;
use std::ptr::NonNull;
use ndk::native_window::NativeWindow;

fn get_bridge(bridge_ptr: jlong) -> Arc<Mutex<AndroidPlatformBridge>> {
  assert_ne!(bridge_ptr, 0);
  let brige_ptr = unsafe { std::mem::transmute::<jlong, *const Mutex<AndroidPlatformBridge>>(bridge_ptr) };
  unsafe { Arc::from_raw(brige_ptr) }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onCreateNative(env: JNIEnv, class: JClass) -> jlong {
  let tag = CString::new("RS").unwrap();
  let msg = CString::new("Hello World").unwrap();
  unsafe {
    __android_log_print(android_LogPriority_ANDROID_LOG_VERBOSE as i32, tag.as_ptr(), msg.as_ptr());
  }

  let bridge = AndroidPlatformBridge::new();
  let ptr = Arc::into_raw(bridge);
  unsafe { std::mem::transmute::<*const Mutex<AndroidPlatformBridge>, jlong>(ptr) }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onDestroyNative(env: JNIEnv, class: JClass, bridge: jlong) {
  get_bridge(bridge);
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onSurfaceChangedNative(env: *mut jni::sys::JNIEnv, class: JClass, bridge: jlong, surface: JObject) {
  let bridge = get_bridge(bridge);
  let mut is_engine_running = true;
  {
    let mut bridge_guard = bridge.lock().unwrap();
    is_engine_running = bridge_guard.native_window().is_some();
    let native_window_ptr = unsafe { ANativeWindow_fromSurface(std::mem::transmute(env), std::mem::transmute(*surface)) };
    let native_window_nonnull = NonNull::new(native_window_ptr).expect("Null surface provided");
    let native_window = unsafe { NativeWindow::from_ptr(native_window_nonnull) };
    bridge_guard.change_native_window(native_window);
  }

  if !is_engine_running {
    let platform = AndroidPlatform::with_bridge(&bridge);
    std::thread::spawn(move || {
      let mut engine = Engine::new(platform);
      engine.run();
    });
  }
  std::mem::forget(bridge);
}
