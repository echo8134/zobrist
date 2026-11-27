//! Replace an app data file while holding the lock shared by all its writers.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub(super) struct LockedDataFile {
    path: PathBuf,
    _lock: File,
}

impl LockedDataFile {
    pub(super) fn open(directory: &Path, name: &str) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let path = directory.join(name);
        // The lock must outlive the rename, so it lives beside the file it protects.
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        lock.lock()?;
        Ok(Self { path, _lock: lock })
    }

    pub(super) fn write(&self, value: &impl Serialize) -> io::Result<()> {
        let staged = self.path.with_extension("tmp");
        let file = File::create(&staged)?;
        serde_json::to_writer(&file, value)?;
        file.sync_all()?;
        drop(file);
        fs::rename(staged, &self.path)
    }
}
