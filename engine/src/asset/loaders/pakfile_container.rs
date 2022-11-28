use std::io::Cursor;
use std::sync::Mutex;

use sourcerenderer_bsp::PakFile;

use crate::asset::asset_manager::{
    AssetContainer,
    AssetFile,
};

pub struct PakFileContainer {
    pakfile: Mutex<PakFile>,
}

impl PakFileContainer {
    pub fn new(pakfile: PakFile) -> Self {
        Self {
            pakfile: Mutex::new(pakfile),
        }
    }
}

impl AssetContainer for PakFileContainer {
    fn contains(&self, path: &str) -> bool {
        let mut guard = self.pakfile.lock().unwrap();
        guard.contains_entry(path)
    }

    fn load(&self, path: &str) -> Option<AssetFile> {
        let mut guard = self.pakfile.lock().unwrap();
        let data = guard.read_entry(path)?;
        Some(AssetFile {
            path: path.to_string(),
            data: Cursor::new(data),
        })
    }
}
