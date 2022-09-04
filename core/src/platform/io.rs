use std::io::{Read, Seek, Result as IOResult};
use std::path::Path;

use crossbeam_channel::Sender;

pub trait FileWatcher {
  fn watch<P: AsRef<Path>>(&mut self, path: P);
  fn unwatch<P: AsRef<Path>>(&mut self, path: P);
}

pub trait IO {
  type File: Read + Seek + Send;
  type FileWatcher : FileWatcher + Send;
  fn open_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File>;
  fn asset_exists<P: AsRef<Path>>(path: P) -> bool;
  fn open_external_asset<P: AsRef<Path>>(path: P) -> IOResult<Self::File>;
  fn external_asset_exists<P: AsRef<Path>>(path: P) -> bool;
  fn new_file_watcher(sender: Sender<String>) -> Self::FileWatcher;
}
