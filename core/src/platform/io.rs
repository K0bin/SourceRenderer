use std::future::Future;
use std::io::Result as IOResult;
use std::path::Path;

use crossbeam_channel::Sender;
use futures_io::{AsyncRead, AsyncSeek};

pub trait FileWatcher {
  fn watch<P: AsRef<Path>>(&mut self, path: P);
  fn unwatch<P: AsRef<Path>>(&mut self, path: P);
}


#[cfg(feature = "non_send_io")]
mod send_sync_bounds {
    #[allow(unused)]
    pub trait IOMaybeSend {}
    impl<T> IOMaybeSend for T {}

    #[allow(unused)]
    pub trait IOMaybeSync {}
    impl<T> IOMaybeSync for T {}
}

#[cfg(not(feature = "non_send_io"))]
mod send_sync_bounds {
    #[allow(unused)]
    pub trait IOMaybeSend: Send {}
    impl<T: Send> IOMaybeSend for T {}

    #[allow(unused)]
    pub trait IOMaybeSync: Sync {}
    impl<T: Sync> IOMaybeSync for T {}
}

pub use send_sync_bounds::*;

pub trait PlatformFile: AsyncRead + AsyncSeek + Send + Sync + Unpin {}
impl<T: AsyncRead + AsyncSeek + Sized + Send + Sync + Unpin> PlatformFile for T {}

pub trait PlatformIO: 'static + Send + Sync {
  type File: PlatformFile;
  type FileWatcher : FileWatcher + Send;
  fn open_asset<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = IOResult<Self::File>> + IOMaybeSend;
  fn asset_exists<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = bool> + IOMaybeSend;
  fn open_external_asset<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = IOResult<Self::File>> + IOMaybeSend;
  fn external_asset_exists<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = bool> + IOMaybeSend;
  fn new_file_watcher(sender: Sender<String>) -> Self::FileWatcher;
}
