use std::future::Future;
use std::io::{Result as IOResult, Error as IOError, ErrorKind};
use std::pin::Pin;
use std::path::Path;
use std::task::{Context, Poll};

use futures_lite::io::Cursor;

use futures_lite::{AsyncRead, AsyncReadExt, AsyncSeek};
use sourcerenderer_core::platform::{PlatformIO, FileWatcher};

pub struct WebFetchFile {
    length: usize,
    current_position: usize,
    path: Box<Path>,
}

impl WebFetchFile {
    async fn new<P: AsRef<Path> + Send>(path: P) -> IOResult<Self> {
        let path: &Path = path.as_ref();
        let length = Self::fetch_head(path).await?;
        Self {
            path: path.into(),
            length: length as usize,
            current_position: 0
        }
    }

    async fn fetch_head<P: AsRef<Path> + Send>(path: P) -> IOResult<usize> {
        let future = crate::fetch_asset_head(path.as_ref().to_str().unwrap());
        let length = future.await.map_err(|js_val| {
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
        Ok(length as usize)
    }

    async fn fetch<P: AsRef<Path> + Send>(path: P, buf: &mut [u8]) -> IOResult<usize> {
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
        let len = buffer.length() as usize;
        buffer.copy_to(&mut buf[..len]);
        Ok(len)
    }

    async fn fetch_range<P: AsRef<Path> + Send>(path: P, buf: &mut [u8], offset: usize) -> IOResult<usize> {
        log::trace!("Loading web file: {:?}", path.as_ref());
        let future = crate::fetch_asset_range(path.as_ref().to_str().unwrap(), offset as u32, buf.len() as u32);
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
        let len = buffer.length() as usize;
        buffer.copy_to(&mut buf[..len]);
        Ok(len)
    }
}

impl AsyncRead for WebFetchFile {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<IOResult<usize>> {
        let position = self.current_position;
        self.current_position = (self.current_position + buf.len()).min(self.length);
        let length = (self.length - position).min(buf.len());
        Self::fetch_range(&self.path, &mut buf[..length], position)
    }
}

impl AsyncSeek for WebFetchFile {
    fn poll_seek(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                pos: std::io::SeekFrom,
            ) -> Poll<IOResult<u64>> {
        let new_pos = match pos {
            std::io::SeekFrom::Start(offset) => offset.min(self.length),
            std::io::SeekFrom::End(offset) => self.length - (offset as usize).min(self.length),
            std::io::SeekFrom::Current(offset) => {
                let mut clamped_offset = offset.max(-(self.current_position as i64));
                clamped_offset = offset.min(-(self.length as i64));
                todo!();
                clamped_offset = (self.length - self.current_position).min((self.length as i64) - (clamped_offset));
                let new_offset = (self.current_position as i64) + clamped_offset;
                new_offset as usize
            }
        };
        self.current_position = new_pos;
        Poll::Ready(Ok(new_pos))
    }
}

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
        let future = crate::fetch_asset_head(path.as_ref().to_str().unwrap());
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
