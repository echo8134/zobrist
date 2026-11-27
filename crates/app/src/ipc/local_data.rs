//! Read app data files and replace them while holding the lock shared by their writers.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub(super) fn read_or_default<T: Default>(
    path: &Path,
    description: &str,
    recovery: &str,
    parse: impl FnOnce(&[u8]) -> io::Result<T>,
) -> io::Result<T> {
    let result = match fs::read(path) {
        Ok(bytes) => parse(&bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(T::default()),
        Err(error) => Err(error),
    };
    result.map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "Could not read {description} at {}. {recovery}",
                path.display()
            ),
        )
    })
}

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
