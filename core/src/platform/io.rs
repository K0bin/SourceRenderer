use std::io::{Read, Seek, Result as IOResult};
use std::path::Path;

pub trait IO {
  type File: Read + Seek + Send;
  fn open_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File>;
  fn asset_exists<P: AsRef<Path>>(path: P) -> bool;
  fn open_external_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File>;
  fn external_asset_exists<P: AsRef<Path>>(path: P) -> bool;
}
