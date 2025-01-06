use std::future::Future;
use std::io::Result as IOResult;
use std::path::Path;

use crossbeam_channel::Sender;
use futures_io::{AsyncRead, AsyncSeek};

pub trait FileWatcher {
  fn watch<P: AsRef<Path>>(&mut self, path: P);
  fn unwatch<P: AsRef<Path>>(&mut self, path: P);
}

pub trait IO {
  type File: AsyncRead + AsyncSeek + Send + Unpin;
  type FileWatcher : FileWatcher + Send;
  fn open_asset<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = IOResult<Self::File>> + Send;
  fn asset_exists<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = bool> + Send;
  fn open_external_asset<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = IOResult<Self::File>> + Send;
  fn external_asset_exists<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = bool> + Send;
  fn new_file_watcher(sender: Sender<String>) -> Self::FileWatcher;
}
