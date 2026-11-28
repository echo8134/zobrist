//! The engine programs registered on this computer.

use std::collections::HashSet;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::file_collection::{list_recovery, lock_collection, publish_collection, read_list};

/// A change to the ordered inventory, or a request to read it.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum EngineAction {
    /// List all registrations.
    List,
    /// Register an installed program selected by the player.
    Register {
        /// The absolute path from the file picker.
        path: PathBuf,
    },
    /// Choose a registered program as the default.
    SetDefault {
        /// The registered path.
        path: PathBuf,
    },
    /// Forget a registration, leaving its executable on disk.
    Remove {
        /// The registered path.
        path: PathBuf,
    },
}

/// The file name and resolved path shown in the inventory.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct EngineEntry {
    name: String,
    path: PathBuf,
}

/// The first registered executable is the default in the Engines page.
pub(super) fn default_program(directory: &Path) -> io::Result<PathBuf> {
    read(&directory.join("engines.json"))?
        .into_iter()
        .next()
        .map(|entry| entry.path)
        .ok_or_else(|| io::Error::other("Choose a default engine on the Engines page."))
}

impl EngineAction {
    pub(super) fn apply(self, directory: &Path) -> io::Result<Vec<EngineEntry>> {
        let inventory = directory.join("engines.json");
        if matches!(self, Self::List) {
            return read(&inventory);
        }
        let _lock = lock_collection(directory, "engines.lock")?;
        let mut entries = read(&inventory)?;
        match self {
            Self::Register { path } => {
                let path = executable(&path)?;
                if entries.iter().any(|entry| entry.path == path) {
                    return Ok(entries);
                }
                entries.push(entry(path)?);
            }
            Self::SetDefault { path } => {
                let index = registered(&entries, &path)?;
                if index == 0 {
                    return Ok(entries);
                }
                entries[..=index].rotate_right(1);
            }
            Self::Remove { path } => {
                let index = registered(&entries, &path)?;
                entries.remove(index);
            }
            Self::List => return Ok(entries),
        }
        let paths: Vec<_> = entries.iter().map(|entry| &entry.path).collect();
        publish_collection(&inventory, &paths)?;
        Ok(entries)
    }
}

fn registered(entries: &[EngineEntry], path: &Path) -> io::Result<usize> {
    entries
        .iter()
        .position(|entry| entry.path == path)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "This engine is no longer registered."))
}

fn executable(selected: &Path) -> io::Result<PathBuf> {
    let unavailable = || io::Error::new(ErrorKind::InvalidInput, "Engine file is unavailable.");
    if !selected.is_absolute() {
        return Err(unavailable());
    }
    let path = selected.canonicalize().map_err(|_| unavailable())?;
    let metadata = path.metadata().map_err(|_| unavailable())?;
    if !metadata.is_file() {
        return Err(unavailable());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(unavailable());
        }
    }
    #[cfg(windows)]
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["exe", "com", "cmd", "bat"]
                .iter()
                .any(|suffix| extension.eq_ignore_ascii_case(suffix))
        })
    {
        return Err(unavailable());
    }
    Ok(path)
}

fn entry(path: PathBuf) -> io::Result<EngineEntry> {
    if path.is_absolute()
        && path.to_str().is_some()
        && let Some(name) = path.file_name().and_then(|name| name.to_str())
    {
        return Ok(EngineEntry {
            name: name.to_owned(),
            path,
        });
    }
    Err(io::Error::new(
        ErrorKind::InvalidData,
        "Invalid path in the engine inventory.",
    ))
}

fn read(inventory: &Path) -> io::Result<Vec<EngineEntry>> {
    read_list(
        inventory,
        "the engine list",
        &list_recovery("engine programs", "Engines"),
        |bytes| {
            let paths: Vec<PathBuf> = serde_json::from_slice(bytes)?;
            let mut distinct = HashSet::new();
            if paths.iter().any(|path| !distinct.insert(path)) {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Duplicate path in the engine inventory.",
                ));
            }
            paths.into_iter().map(entry).collect()
        },
    )
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process, thread};

    use serde_json::json;

    use super::*;

    fn fixture(directory: &Path, name: &str) -> io::Result<PathBuf> {
        fs::create_dir_all(directory)?;
        let file = directory
            .join(name)
            .with_extension(env::consts::EXE_EXTENSION);
        fs::write(&file, [])?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700))?;
        }
        file.canonicalize()
    }

    #[test]
    fn remembers_two_programs_with_the_same_name_across_concurrent_registrations() -> io::Result<()>
    {
        let work = env::temp_dir().join(format!("zobrist-engine-list-{}", process::id()));
        let data = work.join("data");
        let first = fixture(&work.join("one"), "sample")?;
        let second = fixture(&work.join("two"), "sample")?;
        thread::scope(|scope| -> io::Result<()> {
            let request = EngineAction::Register {
                path: first.clone(),
            };
            let registering = scope.spawn(|| request.apply(&data));
            EngineAction::Register {
                path: second.clone(),
            }
            .apply(&data)?;
            registering.join().unwrap()?;
            Ok(())
        })?;
        EngineAction::SetDefault {
            path: second.clone(),
        }
        .apply(&data)?;
        #[cfg(unix)]
        {
            let alias = work.join("alias");
            std::os::unix::fs::symlink(&first, &alias)?;
            EngineAction::Register { path: alias }.apply(&data)?;
        }
        let paths: Vec<PathBuf> = serde_json::from_slice(&fs::read(data.join("engines.json"))?)?;
        assert_eq!(paths, [second.clone(), first.clone()]);
        let name = first.file_name().unwrap();
        let expected = json!([{"name": name.to_str().unwrap(), "path": second}, {"name": name.to_str().unwrap(), "path": first}]);
        assert_eq!(json!(EngineAction::List.apply(&data)?), expected);
        fs::remove_file(&second)?;
        assert_eq!(json!(EngineAction::List.apply(&data)?), expected);
        let remaining = EngineAction::Remove { path: second }.apply(&data)?;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, first);
        fs::remove_dir_all(work)
    }

    #[test]
    fn an_unavailable_file_or_a_failed_save_preserves_the_default() -> io::Result<()> {
        let work = env::temp_dir().join(format!("zobrist-engine-write-{}", process::id()));
        let data = work.join("data");
        let first = fixture(&work, "sample-a")?;
        let second = fixture(&work, "sample-b")?;
        let expected = EngineAction::Register { path: first }.apply(&data)?;
        let missing = EngineAction::Register {
            path: work.join("missing"),
        };
        assert_eq!(
            missing.apply(&data).unwrap_err().to_string(),
            "Engine file is unavailable."
        );
        fs::create_dir(data.join("engines.tmp"))?;
        assert!(
            EngineAction::Register { path: second }
                .apply(&data)
                .is_err()
        );
        assert_eq!(EngineAction::List.apply(&data)?, expected);
        fs::remove_dir_all(work)
    }
}
