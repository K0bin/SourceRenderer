use std::{path::{Path, PathBuf}, sync::{Weak, Mutex, Arc}};

use crossbeam_channel::{unbounded, Receiver};
use sourcerenderer_core::{Platform, platform::{IO, FileWatcher}};

use crate::asset::{asset_manager::{AssetContainer, AssetFile, AssetFileData}, AssetManager};

pub struct FSContainer<P: Platform> {
  path: PathBuf,
  external: bool,
  watcher: Option<Mutex<<P::IO as IO>::FileWatcher>>
}

impl<P: Platform> AssetContainer<P> for FSContainer<P> {
  // TODO: write path URI struct to handle getting the path without metadata more elegantly
  // TODO: replace / with platform specific separator

  fn contains(&self, path: &str) -> bool {
    let path_without_metadata = if let Some(dot_pos) = path.rfind('.') {
      if let Some(first_slash_pos) = path[dot_pos..].find('/') {
        &path[..dot_pos + first_slash_pos]
      } else {
        path
      }
    } else {
      path
    };
    if !self.external {
      <P::IO as IO>::asset_exists(self.path.join(path_without_metadata))
    } else {
      <P::IO as IO>::external_asset_exists(self.path.join(path_without_metadata))
    }
  }
  fn load(&self, path: &str) -> Option<AssetFile<P>> {
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
    let file = if !self.external {
      <P::IO as IO>::open_asset(final_path.clone()).ok()?
    } else {
      <P::IO as IO>::open_external_asset(final_path.clone()).ok()?
    };
    if let Some(watcher) = self.watcher.as_ref() {
      let mut watcher_locked = watcher.lock().unwrap();
      watcher_locked.watch(final_path);
    }
    Some(AssetFile::<P> {
      path: path.to_string(),
      data: AssetFileData::File(file)
    })
  }
}

impl<P: Platform> FSContainer<P> {
  pub fn new(platform: &P, asset_manager: &Arc<AssetManager<P>>) -> Self {
    let (sender, receiver) = unbounded();
    let file_watcher = <P::IO as IO>::new_file_watcher(sender);
    let asset_mgr_weak = Arc::downgrade(asset_manager);
    let _thread_handle = platform.start_thread("AssetManagerWatchThread", move || fs_container_watch_thread_fn(asset_mgr_weak, receiver));
    Self {
      path: PathBuf::from(""),
      external: false,
      watcher: Some(Mutex::new(file_watcher))
    }
  }

  fn new_external(base_path: &str) -> Self {
    let path: PathBuf = Path::new(base_path).to_path_buf();
    Self {
      path,
      external: true,
      watcher: None
    }
  }
}

fn fs_container_watch_thread_fn<P: Platform>(asset_manager: Weak<AssetManager<P>>, receiver: Receiver<String>) {
  'watch_loop: loop {
    let changed = receiver.recv();
    match changed {
      Err(_) => { break 'watch_loop; }
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
