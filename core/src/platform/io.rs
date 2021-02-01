use std::io::{Read, Seek, Result as IOResult};
use std::path::Path;

pub trait IO {
  type File: Read + Seek;
  fn open_asset(&self, path: &Path) -> IOResult<Self::File>;
  fn open_external_asset(&self, path: &Path) -> IOResult<Self::File>;
}
