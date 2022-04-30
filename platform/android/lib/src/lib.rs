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

use std::ffi::CString;
use jni::JNIEnv;
use jni::objects::{JClass, JObject};
use jni::sys::{jlong, jint, jfloat};
use ndk_sys::{android_LogPriority_ANDROID_LOG_INFO, android_LogPriority_ANDROID_LOG_ERROR, __android_log_print, android_LogPriority};
use sourcerenderer_core::Vec2UI;
use sourcerenderer_core::platform::Window;
use crate::android_platform::{AndroidPlatform, AndroidWindow};
use sourcerenderer_engine::Engine;
use ndk_sys::ANativeWindow_fromSurface;
use std::ptr::NonNull;
use ndk::native_window::NativeWindow;
use std::io::{BufReader, BufRead};
use std::fs::File;
use std::os::unix::io::FromRawFd;
use std::os::unix::prelude::RawFd;
use std::cell::{RefCell, RefMut};
use sourcerenderer_core::{Vec2, Platform, platform::Event};

lazy_static! {
  static ref TAG: CString = {
    CString::new("SourceRenderer").unwrap()
  };
}

fn setup_log(fd: libc::c_int, severity: android_LogPriority) {
  let mut pipe: [RawFd; 2] = Default::default();
  unsafe {
    libc::pipe(pipe.as_mut_ptr());
    libc::dup2(pipe[1], fd);
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
        } else {
          const MAX_LOGCAT_LENGTH: usize = 512;
          let mut start_index = 0usize;
          let mut remaining_buffer = &buffer[start_index..];
          while !remaining_buffer.is_empty() {
            let logcat_slice = if remaining_buffer.len() > MAX_LOGCAT_LENGTH {
              let maxlength_slice = &remaining_buffer[..MAX_LOGCAT_LENGTH];
              let last_whitespace_index = maxlength_slice.rfind(char::is_whitespace);
              if let Some(last_whitespace_index) = last_whitespace_index {
                &maxlength_slice[..last_whitespace_index]
              } else {
                maxlength_slice
              }
            } else {
              remaining_buffer
            };
            if let Ok(msg) = CString::new(if start_index > 0 { "... ".to_string() } else { "".to_string() } + logcat_slice.trim()) {
              unsafe {
                __android_log_print(severity as i32, TAG.as_ptr(), msg.as_ptr());
              }
            }
            start_index += logcat_slice.len();
            remaining_buffer = &buffer[start_index..];
          }
        }
      }
    }
  });
  println!("Logging set up.");
}

fn enable_backtrace() {
  use std::env;
  const KEY: &'static str = "RUST_BACKTRACE";
  env::set_var(KEY, "1");
}

fn engine_from_long<'a>(engine_ptr: jlong) -> RefMut<'a, EngineWrapper> {
  assert_ne!(engine_ptr, 0);
  unsafe {
    let ptr = std::mem::transmute::<jlong, *mut RefCell<EngineWrapper>>(engine_ptr);
    let engine: RefMut<EngineWrapper> = (*ptr).borrow_mut();
    let engine_ref = std::mem::transmute::<RefMut<EngineWrapper>, RefMut<'a, EngineWrapper>>(engine);
    engine_ref
  }
}

struct EngineWrapper {
  engine: Engine<AndroidPlatform>,
  platform: AndroidPlatform
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_App_initNative(
  env: jni::JNIEnv,
  _class: JClass,
  asset_manager: JObject
) {
  enable_backtrace();
  setup_log(libc::STDOUT_FILENO, android_LogPriority_ANDROID_LOG_INFO);
  setup_log(libc::STDERR_FILENO, android_LogPriority_ANDROID_LOG_ERROR);
  io::initialize_globals(env, asset_manager);
  Engine::<AndroidPlatform>::initialize_global();

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
    let engine_ptr = std::mem::transmute::<jlong, *mut RefCell<EngineWrapper>>(engine_ptr);
    let engine_box = Box::from_raw(engine_ptr);
    {
      let engine_mut = (*engine_box).borrow_mut();
      engine_mut.engine.dispatch_event(Event::Quit);
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
  assert!(!surface.is_null());
  let native_window_ptr = unsafe { ANativeWindow_fromSurface(std::mem::transmute(env), std::mem::transmute(*surface)) };
  let native_window_nonnull = NonNull::new(native_window_ptr).expect("Null surface provided");
  let native_window = unsafe { NativeWindow::from_ptr(native_window_nonnull) };
  let platform = AndroidPlatform::new(native_window);
  let engine = Box::new(RefCell::new(EngineWrapper {
    engine: Engine::run(&platform),
    platform
  }));
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
  let mut wrapper = engine_from_long(engine_ptr);
  if surface.is_null() {
    return;
  } else {
    let native_window_ptr = unsafe { ANativeWindow_fromSurface(std::mem::transmute(env), std::mem::transmute(*surface)) };
    let native_window_nonnull = NonNull::new(native_window_ptr).expect("Null surface provided");
    let native_window = unsafe { NativeWindow::from_ptr(native_window_nonnull) };

    if &native_window != wrapper.platform.window().native_window() {
      wrapper.platform.change_window(AndroidWindow::new(native_window));
      wrapper.engine.dispatch_event(Event::SurfaceChanged(wrapper.platform.window().create_surface(wrapper.engine.instance().clone())));
      wrapper.engine.dispatch_event(Event::WindowSizeChanged(Vec2UI::new(wrapper.platform.window().width(), wrapper.platform.window().height())));
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

  let wrapper = engine_from_long(engine_ptr);
  let engine = &wrapper.engine;

  {
    match event_type {
      ANDROID_EVENT_TYPE_POINTER_DOWN |
      ANDROID_EVENT_TYPE_DOWN => {
        engine.dispatch_event(Event::FingerDown(finger_index as u32));
        engine.dispatch_event(Event::FingerMoved {
          index: finger_index as u32,
          position: Vec2::new(x, y)
        });
      }
      ANDROID_EVENT_TYPE_POINTER_UP |
      ANDROID_EVENT_TYPE_UP => {
        engine.dispatch_event(Event::FingerMoved {
          index: finger_index as u32,
          position: Vec2::new(x, y)
        });
        engine.dispatch_event(Event::FingerUp(finger_index as u32));
      }
      ANDROID_EVENT_TYPE_MOVE => {
        engine.dispatch_event(Event::FingerMoved {
          index: finger_index as u32,
          position: Vec2::new(x, y)
        });
      }
      _ => {}
    }
  }
}
