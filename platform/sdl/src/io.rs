use sourcerenderer_core::platform::io::IO;
use std::path::{Path, PathBuf};
use std::io::Result as IOResult;

pub struct StdIO {}

impl IO for StdIO {
  type File = std::fs::File;

  fn open_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    std::fs::File::open(path)
  }

  fn asset_exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
  }

  fn open_external_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File> {
    std::fs::File::open(path)
  }

  fn external_asset_exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
  }
}