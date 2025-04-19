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
    pub trait IOFutureMaybeSend {}
    impl<T> IOFutureMaybeSend for T {}

    #[allow(unused)]
    pub trait IOFutureMaybeSync {}
    impl<T> IOFutureMaybeSync for T {}
}

#[cfg(not(feature = "non_send_io"))]
mod send_sync_bounds {
    #[allow(unused)]
    pub trait IOFutureMaybeSend: Send {}
    impl<T: Send> IOFutureMaybeSend for T {}

    #[allow(unused)]
    pub trait IOFutureMaybeSync: Sync {}
    impl<T: Sync> IOFutureMaybeSync for T {}
}

pub use send_sync_bounds::*;

pub trait PlatformFile: AsyncRead + AsyncSeek + Send + Sync + Unpin {}
impl<T: AsyncRead + AsyncSeek + Sized + Send + Sync + Unpin> PlatformFile for T {}

pub trait PlatformIO: 'static + Send + Sync {
  type File: PlatformFile;
  type FileWatcher : FileWatcher + Send;
  fn open_asset<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = IOResult<Self::File>> + IOFutureMaybeSend;
  fn asset_exists<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = bool> + IOFutureMaybeSend;
  fn open_external_asset<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = IOResult<Self::File>> + IOFutureMaybeSend;
  fn external_asset_exists<P: AsRef<Path> + Send>(path: P) -> impl Future<Output = bool> + IOFutureMaybeSend;
  fn new_file_watcher(sender: Sender<String>) -> Self::FileWatcher;
}
