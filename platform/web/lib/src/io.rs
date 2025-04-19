use std::collections::HashMap;
use std::future::{poll_fn, ready, Future};
use std::io::{Error as IOError, ErrorKind, Result as IOResult, SeekFrom};
use std::marker::PhantomData;
use std::pin::{pin, Pin};
use std::path::Path;
use std::sync::atomic::AtomicU32;
use std::sync::LazyLock;
use std::task::{Context, Poll};

use async_task::{Runnable, Task};
use futures_lite::{AsyncRead, AsyncSeek, FutureExt};
use js_sys::Uint8Array;
use sourcerenderer_core::platform::{PlatformIO, FileWatcher};
use wasm_bindgen_futures::spawn_local;

pub struct WebFetchFile {
    length: u64,
    current_position: u64,
    path: Box<Path>,
    data: Option<Box<[u8]>>,
    task: Option<Task<IOResult<Box<[u8]>>>>,
}

const MAX_NON_RANGED_FETCH: usize = 2_000_000;

static FILE_LENGTH_CACHE: LazyLock<async_mutex::Mutex<HashMap<String, u64>>> = LazyLock::new(|| async_mutex::Mutex::new(HashMap::new()));

impl WebFetchFile {
    async fn new<P: AsRef<Path> + Send>(path: P) -> IOResult<Self> {
        let uri = path.as_ref().to_str().unwrap();
        let length = Self::fetch_file_length_cached(uri).await? as usize;

        let data = if length <= MAX_NON_RANGED_FETCH && length != 0 {
            let fetched_data = Self::fetch(uri).await?;
            assert_eq!(fetched_data.len(), length);
            Some(fetched_data)
        } else {
            None
        };

        Ok(Self {
            path: (path.as_ref() as &Path).into(),
            length: length as u64,
            current_position: 0,
            data,
            task: None,
        })
    }

    async fn fetch_file_length(uri: &str) -> IOResult<u64> {
        let future = crate::fetch_asset_head(uri);
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
        })?.as_f64().ok_or_else(|| IOError::new(ErrorKind::Other, "Wrong JS type"))?;
        Ok(length as u64)
    }

    async fn fetch(uri: &str) -> IOResult<Box<[u8]>> {
        log::trace!("Loading web file: {:?}", uri);
        let future = crate::fetch_asset(uri);
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
        let mut data = Vec::<u8>::with_capacity(len);
        unsafe { data.set_len(len); }
        buffer.copy_to(&mut data[..]);
        Ok(data.into_boxed_slice())
    }

    async fn fetch_range(uri: &str, offset: u64, length: u64) -> IOResult<Box<[u8]>> {
        log::trace!("Loading range of web file: {:?}, offet: {:?}, length: {:?}", uri, offset, length);

        if length < MAX_NON_RANGED_FETCH as u64{
            log::warn!("Doing a small read: Path: {:?}, Offset: {:?}, Length: {:?}", uri, offset, length);
        }

        let future = crate::fetch_asset_range(uri, offset as u32, length as u32);
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
        let mut data = Vec::<u8>::with_capacity(length as usize);
        let final_len = (length as usize).min(buffer.length() as usize);
        unsafe { data.set_len(final_len); }
        if final_len >= buffer.length() as usize {
            buffer.copy_to(&mut data[..final_len]);
        } else {
            let subarray = buffer.subarray(0, final_len as u32);
            subarray.copy_to(&mut data[..final_len]);
        }
        data.resize(length as usize, 0u8);
        Ok(data.into_boxed_slice())
    }

    async fn fetch_file_length_cached(uri: &str) -> IOResult<u64> {
        // Use global cache for file sizes to avoid redundant HEAD requests
        {
            let cache = FILE_LENGTH_CACHE.lock().await;
            if let Some(length) = cache.get(uri) {
                return Ok(*length);
            };
        }
        let result = Self::fetch_file_length(uri).await;
        match result {
            Ok(length) => {
                let mut cache = FILE_LENGTH_CACHE.lock().await;
                cache.insert(uri.to_string(), length);
                Ok(length)
            }
            Err(e) => Err(e)
        }
    }
}

impl AsyncRead for WebFetchFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<IOResult<usize>> {
        if self.current_position == self.length || buf.len() == 0 {
            return Poll::Ready(Ok(0usize));
        }

        if let Some(data) = self.data.as_ref() {
            let len = ((self.length - self.current_position) as usize).min(buf.len());
            let position = self.current_position as usize;
            buf[..len].copy_from_slice(&data[position..(position + len)]);
            self.current_position += len as u64;
            return Poll::Ready(Ok(len));
        }

        if let Some(task) = self.task.as_mut() {
            let data = std::task::ready!(task.poll(cx))?;
            let len = ((self.length - self.current_position) as usize).min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            self.current_position += len as u64;
            self.task = None;
            return Poll::Ready(Ok(len));
        }

        let position = self.current_position;
        let length = (self.length - position).min(buf.len() as u64);
        let uri = self.path.as_ref().to_string_lossy().to_string();

        let (runnable, mut task) = async_task::spawn_local(async move {
                Self::fetch_range(&uri, position, length).await
            },
            |runnable: Runnable| {
                spawn_local(async {
                    runnable.run();
            })
        });

        runnable.schedule();
        self.task = Some(task);

        if let Some(task) = self.task.as_mut() {
            let data = std::task::ready!(task.poll(cx))?;
            let len = ((self.length - self.current_position) as usize).min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            self.current_position += len as u64;
            self.task = None;
            return Poll::Ready(Ok(len));
        } else {
            unreachable!();
        }
    }
}

impl AsyncSeek for WebFetchFile {
    fn poll_seek(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                pos: std::io::SeekFrom,
            ) -> Poll<IOResult<u64>> {

        if let Some(task) = self.task.as_mut() {
            let data = std::task::ready!(task.poll(cx))?;
            let len = data.len() as u64;
            self.current_position = (self.current_position + len).min(self.length);
            self.task = None;
        }

        let new_pos: u64 = match pos {
            std::io::SeekFrom::Start(offset) => offset.min(self.length),
            std::io::SeekFrom::End(offset) => self.length - (offset.max(0i64) as u64).min(self.length),
            std::io::SeekFrom::Current(offset) => {
                let mut clamped_offset = offset.max(-(self.current_position as i64));
                clamped_offset = offset.min((self.length - self.current_position) as i64);
                let new_offset = (self.current_position as i64) + clamped_offset;
                new_offset as u64
            }
        };
        self.current_position = new_pos;
        Poll::Ready(Ok(new_pos))
    }
}

pub struct WebIO {}

impl PlatformIO for WebIO {
    type File = WebFetchFile;
    type FileWatcher = NopWatcher;

    async fn open_asset<P: AsRef<Path> + Send>(path: P) -> IOResult<Self::File> {
        WebFetchFile::new(path).await
    }

    async fn asset_exists<P: AsRef<Path> + Send>(path: P) -> bool {
        let uri = path.as_ref().to_str().unwrap();
        WebFetchFile::fetch_file_length_cached(&uri).await.is_ok()
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
