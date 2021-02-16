extern crate ndk_sys;
extern crate jni;
extern crate sourcerenderer_core;
extern crate sourcerenderer_vulkan;
extern crate libc;
extern crate parking_lot;
#[macro_use]
extern crate lazy_static;

mod android_platform;
mod io;

use std::ffi::{CString, CStr};
use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject};
use jni::sys::{jstring, jlong, jint, jfloat};
use ndk_sys::{AAssetManager_fromJava, AInputQueue,
              android_LogPriority_ANDROID_LOG_DEBUG, __android_log_print};
use crate::android_platform::{AndroidPlatform, ASSET_MANAGER, AndroidWindow};
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
use std::cell::{RefCell, RefMut};
use std::borrow::BorrowMut;
use sourcerenderer_core::platform::{WindowState, InputState};
use sourcerenderer_core::{Vec2, Platform};

lazy_static! {
  static ref TAG: CString = {
    CString::new("SourceRenderer").unwrap()
  };
}

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
          unsafe {
            __android_log_print(android_LogPriority_ANDROID_LOG_DEBUG as i32, TAG.as_ptr(), msg.as_ptr());
          }
        }
      }
    }
  });
  println!("Logging set up.");
}

fn engine_from_long<'a>(engine_ptr: jlong) -> RefMut<'a, Engine<AndroidPlatform>> {
  assert_ne!(engine_ptr, 0);
  unsafe {
    let box_ptr = std::mem::transmute::<jlong, *mut RefCell<Engine<AndroidPlatform>>>(engine_ptr);
    let engine_box = Box::from_raw(box_ptr);
    let engine: RefMut<Engine<AndroidPlatform>> = (*engine_box).borrow_mut();
    let engine_ref = std::mem::transmute::<RefMut<Engine<AndroidPlatform>>, RefMut<'a, Engine<AndroidPlatform>>>(engine);
    std::mem::forget(engine_box);
    engine_ref
  }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_App_initNative(
  env: *mut jni::sys::JNIEnv,
  _class: JClass,
  asset_manager: JObject
) {
  setup_log();

  let asset_manager = unsafe { AAssetManager_fromJava(unsafe { std::mem::transmute(env) }, *asset_manager as *mut c_void) };
  unsafe {
    ASSET_MANAGER = asset_manager;
  }

  println!("Initialized application.");
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onDestroyNative(
  _env: JNIEnv,
  _class: JClass,
  engine_ptr: jlong
) {
  unsafe {
    let engine_ptr = std::mem::transmute::<jlong, *mut RefCell<Engine<AndroidPlatform>>>(engine_ptr);
    let mut engine_box = Box::from_raw(engine_ptr);
    {
      let mut engine_mut = (*engine_box).borrow_mut();
      *engine_mut.platform().window_mut().state_mut() = WindowState::Exited;
    }
    // engine box gets dropped
  }
  println!("Engine stopped");
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_startEngineNative(
  env: *mut jni::sys::JNIEnv,
  _class: JClass,
  surface: JObject
) -> jlong {
  let native_window_ptr = unsafe { ANativeWindow_fromSurface(std::mem::transmute(env), std::mem::transmute(*surface)) };
  let native_window_nonnull = NonNull::new(native_window_ptr).expect("Null surface provided");
  let native_window = unsafe { NativeWindow::from_ptr(native_window_nonnull) };
  let platform = AndroidPlatform::new(native_window);
  let mut engine = Box::new(RefCell::new(Engine::run(platform)));
  println!("Engine started");
  unsafe {
    std::mem::transmute(Box::into_raw(engine))
  }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onSurfaceChangedNative(
  env: *mut jni::sys::JNIEnv,
  _class: JClass,
  engine_ptr: jlong,
  surface: JObject
) {
  let mut engine = engine_from_long(engine_ptr);
  if surface.is_null() {
    return;
  } else {
    let native_window_ptr = unsafe { ANativeWindow_fromSurface(std::mem::transmute(env), std::mem::transmute(*surface)) };
    let native_window_nonnull = NonNull::new(native_window_ptr).expect("Null surface provided");
    let native_window = unsafe { NativeWindow::from_ptr(native_window_nonnull) };

    if &native_window != engine.platform().window().native_window() {
      *engine.platform().window_mut() = AndroidWindow::new(native_window);
    }
  }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onTouchInputNative(
  _env: *mut jni::sys::JNIEnv,
  _class: JClass,
  engine_ptr: jlong,
  x: jfloat,
  y: jfloat,
  finger_index: jint,
  event_type: jint
) {
  const ANDROID_EVENT_TYPE_POINTER_DOWN: i32 = 5;
  const ANDROID_EVENT_TYPE_POINTER_UP: i32 = 6;
  const ANDROID_EVENT_TYPE_DOWN: i32 = 0;
  const ANDROID_EVENT_TYPE_UP: i32 = 1;
  const ANDROID_EVENT_TYPE_MOVE: i32 = 2;

  let mut engine = engine_from_long(engine_ptr);

  {
    let mut input = engine.platform().input_state();
    match event_type {
      ANDROID_EVENT_TYPE_POINTER_DOWN |
      ANDROID_EVENT_TYPE_DOWN => {
        input.set_finger_position(finger_index as u32, Vec2::new(x, y));
        input.set_finger_down(finger_index as u32, true);
      }
      ANDROID_EVENT_TYPE_POINTER_UP |
      ANDROID_EVENT_TYPE_UP => {
        input.set_finger_position(finger_index as u32, Vec2::new(0f32, 0f32));
        input.set_finger_down(finger_index as u32, false);
      }
      ANDROID_EVENT_TYPE_MOVE => {
        input.set_finger_position(finger_index as u32, Vec2::new(x, y));
      }
      _ => {}
    }
  }

  engine.poll_platform();
}
