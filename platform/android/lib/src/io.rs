use ndk_sys::{AAsset,
              AAsset_close,
              AAsset_read,
              AAsset_seek64,
              AAssetManager,
              AAssetManager_open};
use std::io::{Read, Result as IOResult, Error as IOError, ErrorKind, Seek, SeekFrom};
use std::os::raw::c_void;
use libc::{SEEK_CUR, SEEK_END, SEEK_SET, O_RDONLY};
use std::path::Path;
use std::ffi::CString;
use sourcerenderer_core::platform::io::IO;
use crate::android_platform::ASSET_MANAGER;
use std::sync::{Mutex, Arc};

pub struct AndroidIO {}

impl IO for AndroidIO {
  type File = AndroidAsset;

  fn open_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    let asset_manager = unsafe {
      ASSET_MANAGER
    };

    AndroidAsset::open(asset_manager, path)
  }

  fn open_external_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    unimplemented!()
  }
}

pub struct AndroidAsset {
  asset: *mut AAsset
}

impl AndroidAsset {
  pub fn open<P: AsRef<Path>>(mgr: *mut AAssetManager, name: P) -> IOResult<Self> {
    let path_ref: &Path = name.as_ref();
    let name_c_str = CString::new(path_ref.to_str().unwrap()).unwrap();
    let asset = unsafe { AAssetManager_open(mgr, name_c_str.as_ptr(), O_RDONLY) };
    if asset == std::ptr::null_mut() {
      Err(IOError::new(ErrorKind::NotFound, "AAssetManager_open failed."))
    } else {
      Ok(Self {
        asset
      })
    }
  }
}

impl Drop for AndroidAsset {
  fn drop(&mut self) {
    unsafe { AAsset_close(self.asset); }
  }
}

impl Read for AndroidAsset {
  fn read(&mut self, buf: &mut [u8]) ->IOResult<usize> {
    let result = unsafe { AAsset_read(self.asset, buf.as_mut_ptr() as *mut c_void, buf.len() as u64) };
    if result < 0 {
      Err(IOError::new(ErrorKind::Other, "Result is negative"))
    } else {
      Ok(result as usize)
    }
  }
}

impl Seek for AndroidAsset {
  fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
    let offset = match pos {
      SeekFrom::Start(offset) => {
        unsafe {
          AAsset_seek64(self.asset, offset as i64, SEEK_SET)
        }
      }
      SeekFrom::End(offset_from_end) => {
        unsafe {
          AAsset_seek64(self.asset, offset_from_end, SEEK_END)
        }
      }
      SeekFrom::Current(relative_offset) => {
        unsafe {
          AAsset_seek64(self.asset, relative_offset, SEEK_CUR)
        }
      }
    };
    if offset < 0 {
      Err(IOError::new(ErrorKind::Other, "Offset is negative"))
    } else {
      Ok(offset as u64)
    }
  }
}
