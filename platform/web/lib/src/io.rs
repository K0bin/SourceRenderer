use std::io::{Read, Seek, SeekFrom, Result as IOResult};

use sourcerenderer_core::platform::io::IO;

pub struct WebIO {}

impl IO for WebIO {
  type File = WebFile;

  fn open_asset<P: AsRef<std::path::Path>>(path: P) -> IOResult<Self::File> {
    // IOResult::Err(std::io::Error::new(std::io::ErrorKind::Other, "stub"))
    todo!()
  }

  fn asset_exists<P: AsRef<std::path::Path>>(path: P) -> bool {
    false
  }

  fn open_external_asset<P: AsRef<std::path::Path>>(path: P) -> IOResult<Self::File> {
    todo!()
  }

  fn external_asset_exists<P: AsRef<std::path::Path>>(path: P) -> bool {
    false
  }
}

pub struct WebFile {

}

impl Read for WebFile {
  fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
    todo!()
  }
}

impl Seek for WebFile {
  fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
    todo!()
  }
}
