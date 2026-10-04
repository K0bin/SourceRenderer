use std::collections::HashMap;
use std::io::{Error as IOError, ErrorKind, Result as IOResult};
use std::marker::PhantomData;
use std::path::Path;
use std::pin::Pin;
use std::sync::LazyLock;
use std::task::{Context, Poll};

use futures_lite::{AsyncRead, AsyncSeek, FutureExt};
use sourcerenderer_core::platform::{FileWatcher, PlatformIO};

type FetchFuture = Pin<Box<dyn Future<Output = IOResult<Box<[u8]>>>>>;

struct WebRequest {
    offset: usize,
    len: usize,
    task: FetchFuture,
}

pub struct WebFetchFile {
    length: usize,
    current_position: usize,
    path: Box<Path>,
    data: Option<Box<[u8]>>,
    request: Option<WebRequest>,
    _p: PhantomData<*const std::ffi::c_void>,
}

const MAX_NON_RANGED_FETCH: usize = 2_000_000;

static FILE_LENGTH_CACHE: LazyLock<async_lock::Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| async_lock::Mutex::new(HashMap::new()));

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
            length,
            current_position: 0,
            data,
            request: None,
            _p: PhantomData,
        })
    }

    async fn fetch_file_length(uri: &str) -> IOResult<usize> {
        let future = crate::fetch_asset_head(uri);
        let length = future
            .await
            .map_err(|js_val| {
                let response_code_opt = js_val.as_f64();
                if response_code_opt.is_none() {
                    IOError::new(ErrorKind::Other, format!("Response code: {:?}", js_val))
                } else {
                    let response_code = response_code_opt.unwrap() as u32;
                    match response_code {
                        404 => IOError::new(
                            ErrorKind::NotFound,
                            format!("Response code: {}", response_code),
                        ),
                        _ => IOError::new(
                            ErrorKind::Other,
                            format!("Response code: {}", response_code),
                        ),
                    }
                }
            })?
            .as_f64()
            .ok_or_else(|| IOError::new(ErrorKind::Other, "Wrong JS type"))?;
        Ok(length as usize)
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
                    404 => IOError::new(
                        ErrorKind::NotFound,
                        format!("Response code: {}", response_code),
                    ),
                    _ => IOError::new(
                        ErrorKind::Other,
                        format!("Response code: {}", response_code),
                    ),
                }
            }
        })?;
        let data = buffer.to_vec();
        Ok(data.into_boxed_slice())
    }

    async fn fetch_range(uri: &str, offset: u32, length: u32) -> IOResult<Box<[u8]>> {
        log::trace!(
            "Loading range of web file: {:?}, offet: {:?}, length: {:?}",
            uri,
            offset,
            length
        );

        let future = crate::fetch_asset_range(uri, offset, length);
        let buffer_res = future.await;
        let buffer = buffer_res.map_err(|js_val| {
            let response_code_opt = js_val.as_f64();
            if response_code_opt.is_none() {
                IOError::new(ErrorKind::Other, format!("Response code: {:?}", js_val))
            } else {
                let response_code = response_code_opt.unwrap() as u32;
                match response_code {
                    404 => IOError::new(
                        ErrorKind::NotFound,
                        format!("Response code: {}", response_code),
                    ),
                    _ => IOError::new(
                        ErrorKind::Other,
                        format!("Response code: {}", response_code),
                    ),
                }
            }
        })?;
        let mut data = Vec::<u8>::with_capacity(length as usize);
        let final_len = (length as usize).min(buffer.length() as usize);
        unsafe {
            data.set_len(final_len);
        }
        if final_len >= buffer.length() as usize {
            buffer.copy_to(&mut data[..final_len]);
        } else {
            let subarray = buffer.subarray(0, final_len as u32);
            subarray.copy_to(&mut data[..final_len]);
        }
        data.resize(length as usize, 0u8);
        Ok(data.into_boxed_slice())
    }

    async fn fetch_file_length_cached(uri: &str) -> IOResult<usize> {
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
            Err(e) => Err(e),
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
        let max_len = self.length - self.current_position;
        let position = self.current_position;
        let len = (self.length - self.current_position).min(buf.len());

        if let Some(data) = self.data.as_ref() {
            let len = max_len.min(buf.len());
            buf[..len].copy_from_slice(&data[position..(position + len)]);
            self.current_position += len;
            return Poll::Ready(Ok(len));
        }

        if let Some(request) = self.request.as_mut() {
            if request.offset != position || request.len != len {
                log::warn!("Cancelling existing request due to different read.");
                self.request = None;
            }
        }

        if self.request.is_none() {
            let uri = self.path.as_ref().to_string_lossy().to_string();
            self.request = Some(WebRequest {
                offset: position,
                len,
                task: Box::pin(async move {
                    let res = Self::fetch_range(&uri, position as u32, len as u32).await;
                    res
                })
            });
        }

        if let Some(mut request) = self.request.take() {
            let res = request.task.poll(cx);
            return match res {
                Poll::Pending => {
                    self.request = Some(request);
                    Poll::Pending
                }
                Poll::Ready(data_res) => {
                    match data_res {
                        Ok(data) => {
                            buf[..len].copy_from_slice(&data[..len]);
                            self.current_position += len;
                            Poll::Ready(Ok(len))
                        }
                        Err(e) => Poll::Ready(Err(e))
                    }
                }
            };
        }

        Poll::Pending
    }
}

impl AsyncSeek for WebFetchFile {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        pos: std::io::SeekFrom,
    ) -> Poll<IOResult<u64>> {
        if self.request.is_some() {
            log::warn!("Cancelling existing request due to seeking.");
        }
        self.request = None;

        let new_pos: usize = match pos {
            std::io::SeekFrom::Start(offset) => (offset as usize).min(self.length),
            std::io::SeekFrom::End(offset) => {
                self.length - (offset.max(0i64) as usize).min(self.length)
            }
            std::io::SeekFrom::Current(offset) => {
                let mut clamped_offset = offset.max(-(self.current_position as i64));
                clamped_offset = clamped_offset.min((self.length - self.current_position) as i64);
                let new_offset = (self.current_position as i64) + clamped_offset;
                new_offset as usize
            }
        };
        self.current_position = new_pos;
        Poll::Ready(Ok(new_pos as u64))
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