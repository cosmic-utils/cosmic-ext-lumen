//! Bounded, best-effort storage for disposable cache entries.
use std::{
    fs::{self, File, OpenOptions},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};

static DISK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
pub enum Cache {
    Images,
    Reading,
    Navigation,
}

impl Cache {
    fn directory(self) -> Option<PathBuf> {
        let dirs = super::local::project_dirs()?;
        Some(dirs.cache_dir().join(match self {
            Self::Images => "images-v1",
            Self::Reading => "reading-v1",
            Self::Navigation => "navigation-v1",
        }))
    }

    fn limits(self) -> (usize, u64, usize) {
        match self {
            Self::Images => (12 * 1024 * 1024, 256 * 1024 * 1024, 2048),
            Self::Reading => (4 * 1024 * 1024, 48 * 1024 * 1024, 512),
            Self::Navigation => (4 * 1024 * 1024, 8 * 1024 * 1024, 128),
        }
    }

    pub fn read(self, key: &str, max_age: Duration) -> Option<Vec<u8>> {
        let file = File::open(self.directory()?.join(key)).ok()?;
        let metadata = file.metadata().ok()?;
        let limit = self.limits().0 as u64;
        if metadata.len() > limit || metadata.modified().ok()?.elapsed().ok()? > max_age {
            return None;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(limit + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() as u64 <= limit).then_some(bytes)
    }

    pub fn write(self, key: &str, bytes: &[u8]) {
        if bytes.len() > self.limits().0 {
            return;
        }
        let Some(directory) = self.directory() else {
            return;
        };
        let Ok(_guard) = DISK.lock() else { return };

        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        if builder.create(&directory).is_err() {
            return;
        }

        let temporary = directory.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = options.open(&temporary).and_then(|mut file| {
            file.write_all(bytes)?;
            drop(file);
            fs::rename(&temporary, directory.join(key))
        });
        if result.is_err() {
            let _ = fs::remove_file(temporary);
            return;
        }
        self.trim(directory);
    }

    pub fn invalidate(self, prefix: &str) {
        let Some(directory) = self.directory() else {
            return;
        };
        let Ok(_guard) = DISK.lock() else { return };
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(prefix) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    fn trim(self, directory: PathBuf) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        let mut files: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata
                    .is_file()
                    .then(|| (entry.path(), metadata.len(), metadata.modified().ok()))
            })
            .collect();
        let (_, budget, max_entries) = self.limits();
        let mut size: u64 = files.iter().map(|(_, size, _)| *size).sum();
        let mut count = files.len();
        if size <= budget && count <= max_entries {
            return;
        }
        files.sort_unstable_by_key(|(_, _, time)| *time);
        for (path, length, _) in files {
            if size <= budget && count <= max_entries {
                break;
            }
            if fs::remove_file(path).is_ok() {
                size -= length;
                count -= 1;
            }
        }
    }
}

pub fn key(value: impl Hash) -> String {
    let mut hash = DefaultHasher::new();
    value.hash(&mut hash);
    format!("{:016x}", hash.finish())
}
