use std::{fs::File, io::{Cursor, Error as IOError, ErrorKind, Read, Result as IOResult}, path::Path, usize};
use gltf::{Document, Glb, Gltf, Node, Scene, buffer::Data as GltfBufferData, image::Data as GltfImageData, import, import_slice, json::serialize::to_string};
use legion::{World, WorldOptions};
use sourcerenderer_core::Platform;

use crate::asset::asset_manager::{AssetContainer, AssetFile, AssetFileData};

pub struct GltfContainer {
  gltf: Gltf,
  json_data: Box<[u8]>,
  buffers: Vec<GltfBufferData>,
  images: Vec<GltfImageData>,
  base_path: String
}

impl GltfContainer {
  pub fn load(path: &str, reader: &mut dyn Read) -> IOResult<Self> {
    let json_data = {
      let file = File::open(path)?;
      let glb = Glb::from_reader(file).map_err(|_e| IOError::new(ErrorKind::Other, "Failed to read Glb"))?;
      glb.json.into_owned().into_boxed_slice()
    };

    let (document, buffers, images) = import(path).unwrap();
    let gltf = Gltf {
      document, 
      blob: None
    };

    println!("GLTF: Found {} buffers & {} images", buffers.len(), images.len());

    let file_name = Path::new(path).file_name().expect("Failed to read file name");
    let base_path = file_name.to_str().unwrap().to_string() + "/";

    gltf.scenes().for_each(|s| println!("{:?}", s.name()));

    Ok(Self {
      gltf,
      json_data,
      buffers,
      images,
      base_path
    })
  }
}

impl<P: Platform> AssetContainer<P> for GltfContainer {
  fn load(&self, path: &str) -> Option<crate::asset::asset_manager::AssetFile<P>> {
    let scene_base_path = self.base_path.clone() + "scene/";
    if path.starts_with(&scene_base_path) {
      let scene_name = &path[scene_base_path.len()..];
      for scene in self.gltf.scenes() {
        if scene.name().map_or_else(|| scene.index().to_string(), |name| name.to_string()) == scene_name {
          // We just need the JSON data to load the scene
          return Some(AssetFile {
            path: path.to_string(),
            data: AssetFileData::Memory(Cursor::new(self.json_data.clone()))
          });
        }
      }
    }
    let buffer_base_path = self.base_path.clone() + "buffer/";
    if path.starts_with(&buffer_base_path) {
      let buffer_index: usize = Path::new(path).file_name().unwrap().to_str().unwrap().parse().unwrap();
      return Some(AssetFile {
        path: path.to_string(),
        data: AssetFileData::Memory(Cursor::new(self.buffers[buffer_index].0.clone().into_boxed_slice()))
      });
    }

    None
  }
}
