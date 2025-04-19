use std::io::{Error as IOError, ErrorKind, Result as IOResult, SeekFrom};
use std::path::Path;
use std::usize;

use bevy_tasks::futures_lite::io::{BufReader, Cursor};
use bevy_tasks::futures_lite::{AsyncReadExt, AsyncSeekExt};
use futures_io::{AsyncRead, AsyncSeek};
use sourcerenderer_core::platform::PlatformIO;

use crate::asset::asset_manager::AssetFile;
use crate::asset::loaders::gltf::glb;
use crate::asset::AssetContainer;

pub struct GltfContainer<R: AsyncRead + AsyncSeek + Unpin> {
    json_offset: u64,
    data_offset: u64,
    data_length: u64,
    reader: async_mutex::Mutex<R>,
    _base_path: String,
    scene_base_path: String,
    buffer_base_path: String,
    texture_base_path: String,
}

pub async fn load_memory_gltf_container<IO: PlatformIO>(path: &str, external: bool) -> IOResult<GltfContainer<Cursor<Box<[u8]>>>> {
    let mut file = if external {
        IO::open_external_asset(path).await?
    } else {
        IO::open_asset(path).await?
    };
    let mut data = Vec::<u8>::new();
    file.read_to_end(&mut data).await?;
    GltfContainer::<Cursor<Box<[u8]>>>::new(path, Cursor::new(data.into_boxed_slice())).await
}

pub async fn load_file_gltf_container<IO: PlatformIO>(path: &str, external: bool) -> IOResult<GltfContainer<BufReader<IO::File>>> {
    let file = BufReader::new(if external {
        IO::open_external_asset(path).await?
    } else {
        IO::open_asset(path).await?
    });
    GltfContainer::<BufReader<IO::File>>::new(path, file).await
}

impl<R: AsyncRead + AsyncSeek + Unpin> GltfContainer<R> {
    async fn new(path: &str, mut reader: R) -> IOResult<Self> {
        let header = glb::GlbHeader::read(&mut reader).await?;

        let json_chunk_header = glb::GlbChunkHeader::read(&mut reader).await?;
        let json_offset = reader.seek(SeekFrom::Current(0)).await?;
        reader.seek(SeekFrom::Current(json_chunk_header.length as i64)).await?;

        let data_chunk_header = glb::GlbChunkHeader::read(&mut reader).await?;
        let data_offset = reader.seek(SeekFrom::Current(0)).await?;

        if data_offset + data_chunk_header.length as u64 != header.length as u64 {
            log::error!("GLB file contains more than 3 chunks. This is currently unsupported.");
            return Err(IOError::new(
                ErrorKind::Other,
                "GLB file contains more than 3 chunks. This is currently unsupported.",
            ));
        }

        let file_name = Path::new(path)
            .file_name()
            .expect("Failed to read file name");
        let base_path = file_name.to_str().unwrap().to_string() + "/";

        let scene_base_path = base_path.clone() + "scene/";
        let buffer_base_path = base_path.clone() + "buffer/";
        let texture_base_path = base_path.clone() + "texture/";

        Ok(Self {
            reader: async_mutex::Mutex::new(reader),
            json_offset,
            data_offset,
            data_length: data_chunk_header.length as u64,
            _base_path: base_path,
            scene_base_path,
            texture_base_path,
            buffer_base_path,
        })
    }
}

impl<R: AsyncRead + AsyncSeek + Unpin + Send + Sync + 'static> AssetContainer for GltfContainer<R> {
    async fn contains(&self, path: &str) -> bool {
        log::trace!("Looking for file {:?} in GLTFContainer", path);
        path.starts_with(&self.scene_base_path)
            || path.starts_with(&self.texture_base_path)
            || path.starts_with(&self.buffer_base_path)
    }

    async fn load(&self, path: &str) -> Option<crate::asset::asset_manager::AssetFile> {
        log::trace!("Loading file: {:?} from GLTFContainer", path);
        let mut reader = self.reader.lock().await;
        if path.starts_with(&self.scene_base_path) {
            let length =
                (self.data_offset - self.json_offset - glb::GlbChunkHeader::size()) as usize;
            let mut buffer = Vec::with_capacity(length);
            unsafe {
                buffer.set_len(length);
            }
            reader.seek(SeekFrom::Start(self.json_offset)).await.ok()?;
            reader.read_exact(&mut buffer).await.ok()?;
            return Some(AssetFile::new_memory(path, buffer.into_boxed_slice()));
        }
        let is_texture = path.starts_with(&self.texture_base_path);
        let is_buffer = path.starts_with(&self.buffer_base_path);
        if is_buffer || is_texture {
            let base_path = if is_buffer {
                &self.buffer_base_path
            } else {
                &self.texture_base_path
            };
            let parts: Vec<&str> = path[base_path.len()..].split('-').collect();
            let offset: u64 = if !parts[0].is_empty() { parts[0].parse().unwrap() } else { 0 };
            let length: u64;
            if parts.len() > 1 {
                let mut end = parts[1];
                if let Some(pos) = end.find('.') {
                    end = &end[..pos];
                }
                length = end.parse().unwrap();
            } else {
                length = self.data_length - offset;
            };

            let mut buffer = Vec::with_capacity(length as usize);
            unsafe {
                buffer.set_len(length as usize);
            }
            reader
                .seek(SeekFrom::Start(self.data_offset + offset))
                .await
                .ok()?;
            reader.read_exact(&mut buffer).await.ok()?;

            return Some(AssetFile::new_memory(path, buffer.into_boxed_slice()));
        }

        None
    }
}
