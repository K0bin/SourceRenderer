extern crate ndk_sys;
extern crate jni;

use std::ffi::CString;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use ndk_sys::__android_log_print;
use ndk_sys::android_LogPriority_ANDROID_LOG_VERBOSE;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_de_kobin_sourcerenderer_MainActivity_onCreateNative(env: JNIEnv, class: JClass) {
  let tag = CString::new("RS").unwrap();
  let msg = CString::new("Hello World").unwrap();
  unsafe {
    __android_log_print(android_LogPriority_ANDROID_LOG_VERBOSE as i32, tag.as_ptr(), msg.as_ptr());
  }
}
