use std::io::{Error as IOError, ErrorKind, Result as IOResult, SeekFrom};
use std::path::Path;
use std::usize;

use bevy_tasks::futures_lite::io::Cursor;
use bevy_tasks::futures_lite::{AsyncReadExt, AsyncSeekExt};
use sourcerenderer_core::platform::{PlatformFile, PlatformIO};

use crate::asset::asset_manager::AssetFile;
use crate::asset::loaders::gltf::glb;
use crate::asset::AssetContainer;

pub struct GltfContainer<R: PlatformFile> {
    json_offset: u64,
    data_offset: u64,
    data_length: u64,
    file: Box<R>,
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

pub async fn load_file_gltf_container<IO: PlatformIO>(path: &str, external: bool) -> IOResult<GltfContainer<IO::File>> {
    let file = if external {
        IO::open_external_asset(path).await?
    } else {
        IO::open_asset(path).await?
    };
    GltfContainer::<IO::File>::new(path, file).await
}

impl<R: PlatformFile> GltfContainer<R> {
    pub async fn new(path: &str, mut file: R) -> IOResult<Self> {
        let header = glb::GlbHeader::read(&mut file).await?;

        let json_chunk_header = glb::GlbChunkHeader::read(&mut file).await?;
        let json_offset = file.seek(SeekFrom::Current(0)).await?;
        file.seek(SeekFrom::Current(json_chunk_header.length as i64)).await?;

        let data_chunk_header = glb::GlbChunkHeader::read(&mut file).await?;
        let data_offset = file.seek(SeekFrom::Current(0)).await?;

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
            file: Box::new(file),
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

impl<R: PlatformFile + 'static> AssetContainer for GltfContainer<R> {
    async fn contains(&self, path: &str) -> bool {
        log::trace!("Looking for file {:?} in GLTFContainer", path);
        path.starts_with(&self.scene_base_path)
            || path.starts_with(&self.texture_base_path)
            || path.starts_with(&self.buffer_base_path)
    }

    async fn load(&self, path: &str) -> Option<crate::asset::asset_manager::AssetFile> {
        log::trace!("Loading file: {:?} from GLTFContainer", path);
        if path.starts_with(&self.scene_base_path) {
            let length =
                (self.data_offset - self.json_offset - glb::GlbChunkHeader::size()) as usize;
            let mut buffer = Vec::with_capacity(length);
            unsafe {
                buffer.set_len(length);
            }
            let mut file = dyn_clone::clone_box(&*self.file);
            file.seek(SeekFrom::Start(self.json_offset)).await.ok()?;
            file.read_exact(&mut buffer).await.ok()?;
            return Some(AssetFile {
                path: path.to_string(),
                file: file,
            });
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
            let mut new_file = dyn_clone::clone_box(&*self.file);
            new_file
                .seek(SeekFrom::Start(self.data_offset + offset))
                .await
                .ok()?;
            new_file.read_exact(&mut buffer).await.ok()?;

            return Some(AssetFile {
                path: path.to_string(),
                file: new_file,
            });
        }

        None
    }
}
