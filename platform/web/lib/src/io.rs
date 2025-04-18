use std::future::Future;
use std::io::{Result as IOResult, Error as IOError, ErrorKind};
use std::pin::Pin;
use std::path::Path;
use std::task::{Context, Poll};

use futures_lite::io::Cursor;

use sourcerenderer_core::platform::{PlatformIO, FileWatcher};

pub struct WebIO {}

impl PlatformIO for WebIO {
    type File = Cursor<Box<[u8]>>;
    type FileWatcher = NopWatcher;

    async fn open_asset<P: AsRef<Path> + Send>(path: P) -> IOResult<Self::File> {
        log::trace!("Loading web file: {:?}", path.as_ref());
        let future = crate::fetch_asset(path.as_ref().to_str().unwrap());
        let buffer_res = future.await;
        let buffer = buffer_res.map_err(|js_val| {
            let response_code_opt = js_val.as_f64();
            if response_code_opt.is_none() {
                IOError::new(ErrorKind::Other, format!("Response code: {:?}", js_val))
            } else {
                let response_code = response_code_opt.unwrap() as u32;
                match response_code {
                    404 => IOError::new(ErrorKind::NotFound, format!("Response code: {}", response_code)),
                    _ => IOError::new(ErrorKind::Other, format!("Response code: {}", response_code)),
                }
            }
        })?;
        let mut wasm_copy = Vec::<u8>::with_capacity(buffer.length() as usize);
        unsafe { wasm_copy.set_len(buffer.length() as usize); }
        buffer.copy_to(&mut wasm_copy[..]);
        Ok(Cursor::new(wasm_copy.into_boxed_slice()))
    }

    async fn asset_exists<P: AsRef<Path> + Send>(path: P) -> bool {
        // There is no smarter solution for this as far as I'm aware. Hope the caching work at least...
        let future = crate::fetch_asset(path.as_ref().to_str().unwrap());
        let result = future.await;
        result.is_ok()
    }

    async fn open_external_asset<P: AsRef<Path> + Send>(path: P) -> IOResult<Self::File> {
        Self::open_asset(path).await
    }

    async fn external_asset_exists<P: AsRef<Path> + Send>(path: P) -> bool {
        Self::asset_exists(path).await
    }

    fn new_file_watcher(_sender: crossbeam_channel::Sender<String>) -> Self::FileWatcher {
        NopWatcher {}
    }
}

pub struct NopWatcher {}
impl FileWatcher for NopWatcher {
    fn watch<P: AsRef<Path>>(&mut self, _path: P) {}

    fn unwatch<P: AsRef<Path>>(&mut self, _path: P) {}
}
