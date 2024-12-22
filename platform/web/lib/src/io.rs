use std::io::{Read, Seek, SeekFrom, Result as IOResult, Error as IOError, ErrorKind};
use std::sync::{Arc, Mutex, MutexGuard};
use std::path::Path;

use log::info;
use sourcerenderer_core::platform::{IO, FileWatcher};

use wasm_bindgen::JsCast;
use web_sys::DedicatedWorkerGlobalScope;
use web_sys::WorkerGlobalScope;
use async_channel::Sender;

//use crate::async_io_worker::{AsyncIOTask, AsyncIOTaskError};
//use crate::WorkerPool;

//static mut IO_SENDER: Option<Sender<Arc<AsyncIOTask>>> = None;

/*pub(super) fn init_global_io(worker_pool: &WorkerPool) {
  unsafe {
    IO_SENDER = Some(crate::async_io_worker::start_worker(&worker_pool));
  }
}*/

pub struct WebIO {}

impl IO for WebIO {
  type File = WebFile;
  type FileWatcher = NopWatcher;

  fn open_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    info!("Opening asset: {:?}", path.as_ref().to_str().unwrap());
    /*let task = AsyncIOTask::new(path.as_ref().to_str().unwrap());
    unsafe {
      IO_SENDER.as_ref().unwrap().try_send(
        task.clone()
      ).unwrap();
    }*/

    Ok(WebFile {
      //task,
      cursor_position: 0
    })
  }

  fn asset_exists<P: AsRef<Path>>(_path: P) -> bool {
    false
  }

  fn open_external_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    Self::open_asset(path)
  }

  fn external_asset_exists<P: AsRef<Path>>(_path: P) -> bool {
    false
  }

  fn new_file_watcher(_sender: crossbeam_channel::Sender<String>) -> Self::FileWatcher {
    NopWatcher {}
  }
}

pub struct NopWatcher {}
impl FileWatcher for NopWatcher {
  fn watch<P: AsRef<Path>>(&mut self, path: P) {}

  fn unwatch<P: AsRef<Path>>(&mut self, path: P) {}
}

pub struct WebFile {
  //task: Arc<AsyncIOTask>,
  cursor_position: u64
}

impl Read for WebFile {
  fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
    /*let data_guard = self.task.wait_for_result();
    let data = data_guard.as_ref().map_err(|e| {
      let msg = match e {
        AsyncIOTaskError::InProgress => unreachable!(),
        AsyncIOTaskError::Error(msg) => msg,
      };
      IOError::new(ErrorKind::NotFound, msg.as_str())
    })?;

    if self.cursor_position >= data.len() as u64 {
      return IOResult::Ok(0);
    }

    let read_length = buf.len().min(data.len() - self.cursor_position as usize);
    let read_start = self.cursor_position as usize;
    let read_end = read_start + read_length;
    buf[0 .. read_length].copy_from_slice(&data[read_start .. read_end]);
    self.cursor_position += read_length as u64;
    Ok(read_length)*/

    Ok(0)
  }
}

impl Seek for WebFile {
  fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
    /*let len = {
      let data_guard = self.task.wait_for_result();
      let data = data_guard.as_ref().map_err(|e| {
        let msg = match e {
          AsyncIOTaskError::InProgress => unreachable!(),
          AsyncIOTaskError::Error(msg) => msg,
        };
        IOError::new(ErrorKind::NotFound, msg.as_str())
      })?;
      data.len()
    };
    let new_pos = match pos {
      SeekFrom::Start(seek_pos) => seek_pos as i64,
      SeekFrom::End(seek_pos) => (len as i64) - seek_pos,
      SeekFrom::Current(seek_pos) => (self.cursor_position as i64 + seek_pos) as i64,
    };
    if new_pos > len as i64 || new_pos < 0 {
      IOResult::Err(IOError::new(ErrorKind::UnexpectedEof, format!("Can not perform seek: {:?}, calculated pos: {:?} bytes, total file length is {:?} bytes.", pos, new_pos, len)))
    } else {
      self.cursor_position = new_pos as u64;
      IOResult::Ok(self.cursor_position)
    }*/
    Ok(0)
  }
}
