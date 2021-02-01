extern crate ndk_sys;
extern crate jni;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate libc;
extern crate parking_lot;

mod android_platform;
mod io;

use std::ffi::{CString, CStr};
use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject};
use jni::sys::{jstring, jlong, jint};
use ndk_sys::__android_log_print;
use ndk_sys::android_LogPriority_ANDROID_LOG_VERBOSE;
use ndk_sys::android_LogPriority_ANDROID_LOG_ERROR;
use ndk_sys::AAssetManager_fromJava;
use crate::android_platform::{AndroidPlatform, AndroidBridge, BRIDGE};
use sourcerenderer_engine::Engine;
use std::sync::{Arc, Mutex};
use std::os::raw::c_void;
use ndk_sys::ANativeWindow_fromSurface;
use std::ptr::NonNull;
use ndk::native_window::NativeWindow;
use std::io::{BufReader, BufRead};
use std::fs::File;
use std::os::unix::io::FromRawFd;
use std::os::unix::prelude::RawFd;

fn setup_log() {
  let mut pipe: [RawFd; 2] = Default::default();
  unsafe {
    libc::pipe(pipe.as_mut_ptr());
    libc::dup2(pipe[1], libc::STDOUT_FILENO);
    libc::dup2(pipe[1], libc::STDERR_FILENO);
  }

  std::thread::spawn(move || {
    let file = unsafe { File::from_raw_fd(pipe[0]) };
    let mut reader = BufReader::new(file);
    let mut buffer = String::new();
    loop {
      buffer.clear();
      if let Ok(len) = reader.read_line(&mut buffer) {
        if len == 0 {
          break;
        } else if let Ok(msg) = CString::new(buffer.clone()) {
          let tag = CString::new("SourceRenderer").unwrap();
          unsafe {
            __android_log_print(android_LogPriority_ANDROID_LOG_VERBOSE as i32, tag.as_ptr(), msg.as_ptr());
          }
        }
      }
    }
  });
  println!("Logging set up");
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onCreateNative(env: *mut jni::sys::JNIEnv, class: JClass, asset_manager: JObject) {
  setup_log();

  let tag = CString::new("RS").unwrap();
  let msg = CString::new("Hello World").unwrap();
  unsafe {
    __android_log_print(android_LogPriority_ANDROID_LOG_VERBOSE as i32, tag.as_ptr(), msg.as_ptr());
  }

  let asset_manager = unsafe { AAssetManager_fromJava(unsafe { std::mem::transmute(env) }, *asset_manager as *mut c_void) };
  unsafe {
    let mut bridge = BRIDGE.lock();
    bridge.set_asset_manager(NonNull::new(asset_manager).expect("Passed AssetManager is null."));
  }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onDestroyNative(env: JNIEnv, class: JClass) {
  unsafe {
    let mut bridge = BRIDGE.lock();
    bridge.clear();
  }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onSurfaceChangedNative(env: *mut jni::sys::JNIEnv, class: JClass, surface: JObject) {
  let mut is_engine_running = true;
  unsafe {
    let mut bridge_guard = BRIDGE.lock();
    is_engine_running = bridge_guard.native_window().is_some();
    let native_window_ptr = unsafe { ANativeWindow_fromSurface(std::mem::transmute(env), std::mem::transmute(*surface)) };
    let native_window_nonnull = NonNull::new(native_window_ptr).expect("Null surface provided");
    let native_window = unsafe { NativeWindow::from_ptr(native_window_nonnull) };
    bridge_guard.set_native_window(native_window);
  }

  if !is_engine_running {
    let platform = AndroidPlatform::new();
    std::thread::spawn(move || {
      let mut engine = Engine::new(platform);
      engine.run();
    });
  }
}
