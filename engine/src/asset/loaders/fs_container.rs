use std::path::PathBuf;
use std::sync::{
    Arc,
    Weak,
};
use crate::Mutex;
use std::thread;

use bevy_tasks::futures_lite::io::Cursor;
use bevy_tasks::futures_lite::AsyncReadExt;
use crossbeam_channel::{
    unbounded,
    Receiver,
};
use sourcerenderer_core::platform::{
    FileWatcher,
    IO,
};
use sourcerenderer_core::Platform;

use crate::asset::asset_manager::{
    AssetContainer,
    AssetFile,
};
use crate::asset::AssetManager;

pub struct FSContainer<P: Platform> {
    path: PathBuf,
    watcher: Option<Mutex<<P::IO as IO>::FileWatcher>>,
}

impl<P: Platform> AssetContainer for FSContainer<P> {
    // TODO: write path URI struct to handle getting the path without metadata more elegantly
    // TODO: replace / with platform specific separator

    async fn contains(&self, path: &str) -> bool {
        log::trace!("Looking for file {:?} in FSContainer", path);
        let path_without_metadata = if let Some(dot_pos) = path.rfind('.') {
            if let Some(first_slash_pos) = path[dot_pos..].find('/') {
                &path[..dot_pos + first_slash_pos]
            } else {
                path
            }
        } else {
            path
        };
        <P::IO as IO>::asset_exists(self.path.join(path_without_metadata)).await
    }

    async fn load(&self, path: &str) -> Option<AssetFile> {
        log::trace!("Loading file: {:?} from FSContainer", path);
        let path_without_metadata = if let Some(dot_pos) = path.rfind('.') {
            if let Some(first_slash_pos) = path[dot_pos..].find('/') {
                &path[..dot_pos + first_slash_pos]
            } else {
                path
            }
        } else {
            path
        };
        let final_path = self.path.join(path_without_metadata);
        let file_res = <P::IO as IO>::open_asset(final_path.clone()).await;
        if let Err(e) = file_res {
            log::error!("Failed to load file using platform API. Path: {}, Error: \n{:?}", path, e);
            return None;
        }
        let mut file = file_res.unwrap();
        if let Some(watcher) = self.watcher.as_ref() {
            log::trace!("Registering file for watcher: {} in FSContainer", path);
            let mut watcher_locked = watcher.lock().unwrap();
            watcher_locked.watch(final_path);
        }
        let mut buf = Vec::<u8>::new();
        file.read_to_end(&mut buf).await.ok()?;

        Some(AssetFile {
            path: path.to_string(),
            data: Cursor::new(buf.into_boxed_slice()),
        })
    }
}

impl<P: Platform> FSContainer<P> {
    pub fn new(asset_manager: &Arc<AssetManager>) -> Self {
        let (sender, receiver) = unbounded();
        let file_watcher = <P::IO as IO>::new_file_watcher(sender);
        let asset_mgr_weak = Arc::downgrade(asset_manager);

        if cfg!(feature = "threading") {
            let mut thread_builder = thread::Builder::new();
            thread_builder = thread_builder.name("AssetManagerWatchThread".to_string());
            let _ = thread_builder.spawn(move || {
                fs_container_watch_thread_fn::<P>(asset_mgr_weak, receiver)
            }).unwrap();
        }
        Self {
            path: PathBuf::from(""),
            watcher: Some(Mutex::new(file_watcher)),
        }
    }
}

fn fs_container_watch_thread_fn<P: Platform>(
    asset_manager: Weak<AssetManager>,
    receiver: Receiver<String>,
) {
    'watch_loop: loop {
        let changed = receiver.recv();
        match changed {
            Err(_) => {
                break 'watch_loop;
            }
            Ok(path) => {
                let mgr_opt = asset_manager.upgrade();
                if mgr_opt.is_none() {
                    break 'watch_loop;
                }
                let mgr = mgr_opt.unwrap();
                mgr.request_asset_update(&path);
            }
        }
    }
}
