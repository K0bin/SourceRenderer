use std::path::{Path, PathBuf};

pub mod android;
mod shader_compiler;
mod spirv_transformer;
pub use shader_compiler::*;
pub mod build_script_logger;

pub fn copy_directory_rec<F>(from: &Path, to: &Path, file_filter: &F)
where
    F: Fn(&Path) -> bool,
{
    for entry in std::fs::read_dir(from).unwrap() {
        println!("cargo:rerun-if-changed={}", from.to_str().unwrap());
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let mut from_buf = PathBuf::new();
            from_buf.push(from);
            from_buf.push(entry.file_name());
            let mut to_buf = PathBuf::new();
            to_buf.push(to);
            to_buf.push(entry.file_name());
            if !to_buf.exists() {
                std::fs::create_dir_all(&to_buf)
                    .unwrap_or_else(|_| panic!("Failed to create target directory {:?}.", to_buf));
            }
            copy_directory_rec(&from_buf, &to_buf, file_filter);
            continue;
        }

        if !(file_filter)(&entry.path()) {
            continue;
        }
        let mut dst_path = PathBuf::new();
        dst_path.push(to);
        dst_path.push(entry.file_name());
        println!("cargo:rerun-if-changed={}", entry.path().to_str().unwrap());
        std::fs::copy(&entry.path(), &dst_path).unwrap_or_else(|_| {
            panic!(
                "Failed to copy file over: {:?} to {:?}",
                entry.path(),
                &dst_path
            )
        });
    }
}

pub fn create_dir_if_necessary<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path: &Path = path.as_ref();

    let res = std::fs::create_dir_all(path);
    if let Err(e) = res {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            return Ok(());
        }
        return Err(e);
    }
    Ok(())
}

pub fn create_parent_dir_if_necessary<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path: &Path = path.as_ref();
    let mut buf = path.to_path_buf();
    if !buf.pop() {
        return Err(std::io::Error::other("Cannot make parent path"));
    }
    let res = std::fs::create_dir_all(&buf);
    if let Err(e) = res {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            return Ok(());
        }
        return Err(e);
    }
    Ok(())
}

pub fn symlink_dir<P: AsRef<Path>>(original: P, link: P) -> std::io::Result<()> {
    #[cfg(target_family = "windows")] {
        if (&original).is_file() {
            std::os::windows::fs::symlink_file(&original, &link)
        } else {
            std::os::windows::fs::symlink_dir(&original, &link)
        }
    }
    #[cfg(not(target_family = "windows"))] {
    std::os::unix::fs::symlink(&original, &link)
    }
}
