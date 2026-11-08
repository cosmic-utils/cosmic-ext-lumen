use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub fn config_path(relative: impl AsRef<Path>) -> io::Result<PathBuf> {
    directories::ProjectDirs::from("io", "github.cosmic-utils", "cosmic-ext-lemmy")
        .map(|dirs| dirs.config_dir().join(relative))
        .ok_or_else(|| io::Error::other("Could not locate the settings directory"))
}

pub fn read<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn write(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing settings directory"))?;
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options.open(&temporary).and_then(|mut file| {
        file.write_all(&bytes)?;
        drop(file);
        fs::rename(&temporary, path)
    });
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
