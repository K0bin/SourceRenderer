use sourcerenderer_core::platform::io::{IO, File};
use std::path::Path;
use std::io::Result as IOResult;

pub struct StdIO {}

impl IO for StdIO {
  type File = std::fs::File;

  fn open_asset(&self, path: &Path) -> IOResult<Self::File> {
    std::fs::File::open(path)
  }

  fn open_external_asset(&self, path: &Path) -> IOResult<Self::File> {
    unimplemented!()
  }
}