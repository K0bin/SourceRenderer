use ndk_sys::{AAsset,
              AAsset_close,
              AAsset_read,
              AAsset_seek64,
              AAssetManager,
              AAssetManager_open};
use std::io::{Read, Result as IOResult, Error as IOError, ErrorKind, Seek, SeekFrom};
use std::os::raw::{c_void, c_int};
use libc::{SEEK_CUR, SEEK_END, SEEK_SET, O_RDONLY};
use std::path::Path;
use std::ffi::CString;
use sourcerenderer_core::platform::io::IO;
use jni::{JavaVM, JNIEnv};
use jni::objects::{JValue, JObject, JStaticMethodID, GlobalRef};
use std::fs::File;
use std::os::unix::io::FromRawFd;
use jni::signature::JavaType;
use ndk_sys::AAssetManager_fromJava;
use jni::signature::Primitive;
use std::mem::MaybeUninit;

static mut ASSET_MANAGER: *mut AAssetManager = std::ptr::null_mut();
static mut JVM: MaybeUninit<JavaVM> = MaybeUninit::uninit();
static mut IO_CLASS: MaybeUninit<GlobalRef> = MaybeUninit::uninit();
static mut IO_OPEN_FILE_METHOD: MaybeUninit<JStaticMethodID<'static>> = MaybeUninit::uninit();

pub fn initialize_globals(env: JNIEnv, asset_manager: JObject) {
  let asset_manager = unsafe { AAssetManager_fromJava(std::mem::transmute(env), *asset_manager as *mut c_void) };
  unsafe {
    ASSET_MANAGER = asset_manager;
    JVM = MaybeUninit::new(env.get_java_vm().unwrap());
    let io_class = env.find_class("de/kobin/sourcerenderer/IO").unwrap();
    let global_ref = env.new_global_ref(io_class).unwrap();
    IO_OPEN_FILE_METHOD = MaybeUninit::new(std::mem::transmute(env.get_static_method_id(&global_ref, "openFile", "(Ljava/lang/String;)I").unwrap()));
    IO_CLASS = MaybeUninit::new(global_ref);
    // retrieving those on a native thread doesn't work so the NDK docs recommend keeping a global reference
  }
}

pub struct AndroidIO {}

impl IO for AndroidIO {
  type File = AndroidFile;

  fn open_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    let asset_manager = unsafe {
      ASSET_MANAGER
    };

    AndroidFile::open_asset(asset_manager, path)
  }

  fn asset_exists<P: AsRef<Path>>(path: P) -> bool {
    Self::open_asset(path).is_ok()
  }

  fn open_external_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    let path = path.as_ref().to_str().unwrap();
    let start = path.find("document/").ok_or(IOError::new(ErrorKind::Other, "Failed to parse uri"))? + "document/".len();
    let actual_path = path[start ..].replace("/", "%2F");
    let fixed_path = path[.. start].to_string() + &actual_path;

    let (jvm, io_class, open_file_method) = unsafe {
      (
        JVM.as_ptr().as_ref().unwrap(),
        IO_CLASS.as_ptr().as_ref().unwrap(),
        IO_OPEN_FILE_METHOD.assume_init()
      )
    };
    let env = jvm.attach_current_thread().unwrap();
    let path_jstr = env.new_string(&fixed_path).unwrap();
    let result = env.call_static_method_unchecked(io_class, open_file_method, JavaType::Primitive(Primitive::Int), &[JValue::Object(*path_jstr)]).unwrap();
    let fd: c_int = if let JValue::Int(jint) = result {
      jint
    } else {
      panic!("Wrong return type")
    };

    match fd {
      -1 => {
        Err(IOError::new(ErrorKind::NotFound, "java.io.FileNotFoundException"))
      }
      -2 => {
        Err(IOError::new(ErrorKind::PermissionDenied, "java.lang.SecurityException"))
      }
      _ => {
        let file = unsafe { File::from_raw_fd(fd) };
        Ok(AndroidFile::Content(file))
      }
    }
  }

  fn external_asset_exists<P: AsRef<Path>>(path: P) -> bool {
    Self::open_external_asset(path).is_ok()
  }
}

pub enum AndroidFile {
  Asset(*mut AAsset),
  Content(File)
}

unsafe impl Send for AndroidFile {}

impl AndroidFile {
  pub fn open_asset<P: AsRef<Path>>(mgr: *mut AAssetManager, name: P) -> IOResult<Self> {
    let path_ref: &Path = name.as_ref();
    let name_c_str = CString::new(path_ref.to_str().unwrap()).unwrap();
    let asset = unsafe { AAssetManager_open(mgr, name_c_str.as_ptr(), O_RDONLY) };
    if asset == std::ptr::null_mut() {
      Err(IOError::new(ErrorKind::NotFound, "AAssetManager_open failed."))
    } else {
      Ok(Self::Asset(
        asset
      ))
    }
  }
}

impl Drop for AndroidFile {
  fn drop(&mut self) {
    match self {
      Self::Asset(asset) => {
        unsafe { AAsset_close(*asset); }
      }
      Self::Content(_file) => {}
    }
  }
}

impl Read for AndroidFile {
  fn read(&mut self, buf: &mut [u8]) ->IOResult<usize> {
    match self {
      Self::Asset(asset) => {
        let result = unsafe { AAsset_read(*asset, buf.as_mut_ptr() as *mut c_void, buf.len() as u64) };
        if result < 0 {
          Err(IOError::new(ErrorKind::Other, "Result is negative"))
        } else {
          Ok(result as usize)
        }
      }
      Self::Content(file) => {
        file.read(buf)
      }
    }
  }
}

impl Seek for AndroidFile {
  fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
    match self {
      Self::Asset(asset) => {
        let offset = match pos {
          SeekFrom::Start(offset) => {
            unsafe {
              AAsset_seek64(*asset, offset as i64, SEEK_SET)
            }
          }
          SeekFrom::End(offset_from_end) => {
            unsafe {
              AAsset_seek64(*asset, offset_from_end, SEEK_END)
            }
          }
          SeekFrom::Current(relative_offset) => {
            unsafe {
              AAsset_seek64(*asset, relative_offset, SEEK_CUR)
            }
          }
        };
        if offset < 0 {
          Err(IOError::new(ErrorKind::Other, "Offset is negative"))
        } else {
          Ok(offset as u64)
        }
      }
      Self::Content(file) => {
        file.seek(pos)
      }
    }
  }
}
