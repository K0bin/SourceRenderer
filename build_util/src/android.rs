/*
 * Originally from cargo-ndk
 *
Copyright (c) 2018-2023  Brendan Molloy <brendan@bbqsrc.net>

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
 *
 */

use std::{
    env,
    ffi::OsString,
    fs, io,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use cargo_metadata::semver::Version;

fn highest_version_ndk_in_path(ndk_dir: &Path) -> Option<PathBuf> {
    if ndk_dir.exists() {
        fs::read_dir(ndk_dir)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|x| {
                let path = x.path();
                path.components()
                    .last()
                    .and_then(|comp| comp.as_os_str().to_str())
                    .and_then(|name| Version::parse(name).ok())
                    .map(|version| (version, path))
            })
            .max_by(|(a, _), (b, _)| a.cmp(b))
            .map(|(_, path)| path)
    } else {
        None
    }
}

/// Return the name and value of the first environment variable that is set
///
/// Additionally checks that if any other variables are set then they should
/// be consistent with the first variable, otherwise a warning is printed.
fn find_first_consistent_var_set<'a>(vars: &'a [&str]) -> Option<(&'a str, OsString)> {
    let mut first_var_set = None;
    for var in vars {
        if let Some(path) = env::var_os(var) {
            if let Some((first_var, first_path)) = first_var_set.as_ref() {
                if *first_path != path {
                    log::warn!(
                        "Environment variable `{} = {:#?}` doesn't match `{} = {:#?}`",
                        first_var,
                        first_path,
                        var,
                        path
                    );
                }
                continue;
            }
            first_var_set = Some((*var, path));
        }
    }

    first_var_set
}

/// Return a path to a discovered NDK and string describing how it was found
pub fn derive_ndk_path() -> Option<(String, PathBuf)> {
    let ndk_vars = [
        "ANDROID_NDK_HOME",
        "ANDROID_NDK_ROOT",
        "ANDROID_NDK_PATH",
        "NDK_HOME",
    ];
    if let Some((var_name, path)) = find_first_consistent_var_set(&ndk_vars) {
        let path = PathBuf::from(path);
        return highest_version_ndk_in_path(&path)
            .or(Some(path))
            .map(|path| (var_name.to_string(), path));
    }

    let sdk_vars = ["ANDROID_HOME", "ANDROID_SDK_ROOT", "ANDROID_SDK_HOME"];
    if let Some((var_name, sdk_path)) = find_first_consistent_var_set(&sdk_vars) {
        let ndk_path = PathBuf::from(&sdk_path).join("ndk");
        if let Some(v) = highest_version_ndk_in_path(&ndk_path) {
            return Some((var_name.to_string(), v));
        }
    }

    // Check Android Studio installed directories
    let base_dir = find_base_dir();

    let ndk_dir = base_dir.join("Android").join("sdk").join("ndk");
    log::trace!("Default NDK dir: {:?}", &ndk_dir);
    highest_version_ndk_in_path(&ndk_dir).map(|path| ("Standard Location".to_string(), path))
}

fn find_base_dir() -> PathBuf {
    #[cfg(windows)]
    let base_dir = pathos::user::local_dir().unwrap().to_path_buf();
    #[cfg(target_os = "linux")]
    let base_dir = pathos::user::data_dir().unwrap().to_path_buf();
    #[cfg(target_os = "macos")]
    let base_dir = pathos::user::home_dir().unwrap().join("Library");

    base_dir
}

pub fn derive_ndk_version(path: &Path) -> Result<Version, io::Error> {
    let data = fs::read_to_string(path.join("source.properties"))?;
    for line in data.split('\n') {
        if line.starts_with("Pkg.Revision") {
            let mut chunks = line.split(" = ");
            let _ = chunks
                .next()
                .ok_or_else(|| io::Error::new(ErrorKind::Other, "No chunk"))?;
            let version = chunks
                .next()
                .ok_or_else(|| io::Error::new(ErrorKind::Other, "No chunk"))?;
            let version = Version::parse(version).map_err(|_e| {
                log::error!("Could not parse NDK version. Got: '{}'", version);
                io::Error::new(ErrorKind::Other, "Bad version")
            })?;
            return Ok(version);
        }
    }

    Err(io::Error::new(
        ErrorKind::Other,
        "Could not find Pkg.Revision in given path",
    ))
}

fn sysroot_suffix(arch: &str) -> PathBuf {
    ["toolchains", "llvm", "prebuilt", arch, "sysroot"]
        .iter()
        .collect()
}

pub fn sysroot() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    const ARCH: &str = "darwin-x86_64";
    #[cfg(target_os = "linux")]
    const ARCH: &str = "linux-x86_64";
    #[cfg(target_os = "windows")]
    const ARCH: &str = "windows-x86_64";
    let path = derive_ndk_path()?.1;
    Some(path.join(sysroot_suffix(ARCH)))
}
