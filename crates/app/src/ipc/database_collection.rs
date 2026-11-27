//! Native databases that the player has registered on this device.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Manager as _, Window};
use ts_rs::TS;
use zobrist_store::{Database, OpenError};

use super::Response;
use super::file_collection::{lock_collection, save_collection};

/// One local collection in the Databases list.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct DatabaseEntry {
    name: String,
    path: PathBuf,
}

/// An operation on the local database catalog.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum DatabaseRequest {
    /// List saved references, including files that have moved.
    ListDatabases,
    /// Verify the chosen database and keep its location.
    AddDatabase {
        /// The absolute path chosen by the player.
        path: PathBuf,
    },
    /// Read the current count from the selected database.
    InspectDatabase {
        /// The database to verify.
        path: PathBuf,
    },
    /// Forget a reference to a database.
    RemoveDatabase {
        /// The saved location to forget.
        path: PathBuf,
    },
}

impl DatabaseRequest {
    /// Answers on the blocking worker that owns this desktop request.
    pub fn answer(self, window: &Window) -> io::Result<Response> {
        let directory = window
            .path()
            .app_local_data_dir()
            .map_err(io::Error::other)?;
        self.apply(&directory)
    }

    fn apply(self, directory: &Path) -> io::Result<Response> {
        let catalog = directory.join("databases.json");
        match self {
            Self::ListDatabases => Ok(Response::ListDatabases {
                entries: entries(&paths(&catalog)?)?,
            }),
            Self::AddDatabase { path } => {
                let games = game_count(&path)?;
                let entries = update(directory, &catalog, |paths| paths.insert(path))?;
                Ok(Response::AddDatabase { entries, games })
            }
            Self::InspectDatabase { path } => Ok(Response::InspectDatabase {
                games: game_count(&path)?,
            }),
            Self::RemoveDatabase { path } => {
                file_name(&path)?;
                Ok(Response::RemoveDatabase {
                    entries: update(directory, &catalog, |paths| paths.remove(&path))?,
                })
            }
        }
    }
}

fn file_name(path: &Path) -> io::Result<&str> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Choose an absolute path.",
        ));
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Choose a database file."))
}

fn game_count(path: &Path) -> io::Result<u64> {
    file_name(path)?;
    let database = Database::open(path).map_err(|error| match error {
        OpenError::NotDatabase => io::Error::new(ErrorKind::InvalidData, "Not a zobrist database"),
        OpenError::Read(error) => io::Error::new(error.kind(), "File unavailable"),
        error @ (OpenError::Damaged | OpenError::Version { .. }) => {
            io::Error::new(ErrorKind::InvalidData, error)
        }
    })?;
    Ok(database.game_count())
}

fn paths(catalog: &Path) -> io::Result<BTreeSet<PathBuf>> {
    match fs::read(catalog) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error),
    }
}

fn entries(paths: &BTreeSet<PathBuf>) -> io::Result<Vec<DatabaseEntry>> {
    paths
        .iter()
        .map(|path| {
            Ok(DatabaseEntry {
                name: file_name(path)?.to_owned(),
                path: path.clone(),
            })
        })
        .collect()
}

fn update(
    directory: &Path,
    catalog: &Path,
    change: impl FnOnce(&mut BTreeSet<PathBuf>) -> bool,
) -> io::Result<Vec<DatabaseEntry>> {
    let _lock = lock_collection(directory, "databases.lock")?;
    let mut paths = paths(catalog)?;
    let changed = change(&mut paths);
    let entries = entries(&paths)?;
    if changed {
        save_collection(catalog, &paths)?;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::{env, process, thread};

    use serde_json::json;
    use zobrist_store::DatabaseWriter;

    use super::*;

    #[test]
    fn concurrent_registrations_keep_both_locations_and_removal_leaves_the_database()
    -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-database-catalog-{}", process::id()));
        let other = directory.join("other");
        fs::create_dir_all(&other)?;
        let first = directory.join("Masters.zobrist");
        let second = other.join("Masters.zobrist");
        DatabaseWriter::create(&first)?.finish()?;
        DatabaseWriter::create(&second)?.finish()?;
        let add = |path: &Path| {
            DatabaseRequest::AddDatabase {
                path: path.to_owned(),
            }
            .apply(&directory)
        };
        thread::scope(|scope| -> io::Result<()> {
            let saving = scope.spawn(|| add(&first));
            add(&second)?;
            saving.join().unwrap()?;
            Ok(())
        })?;
        let both = json!([
            {"name":"Masters.zobrist","path":first},
            {"name":"Masters.zobrist","path":second}
        ]);
        assert_eq!(
            json!(add(&first)?),
            json!({"type":"AddDatabase","entries":both,"games":0})
        );
        assert_eq!(
            json!(DatabaseRequest::ListDatabases.apply(&directory)?),
            json!({"type":"ListDatabases","entries":both})
        );
        let invalid = directory.join("Invalid.zobrist");
        fs::write(&invalid, "invalid")?;
        assert_eq!(
            add(&invalid).unwrap_err().to_string(),
            "Not a zobrist database"
        );
        let away = directory.join("away.zobrist");
        fs::rename(&first, &away)?;
        let inspect = || {
            DatabaseRequest::InspectDatabase {
                path: first.clone(),
            }
            .apply(&directory)
        };
        assert_eq!(inspect().unwrap_err().to_string(), "File unavailable");
        fs::rename(away, &first)?;
        assert_eq!(
            json!(inspect()?),
            json!({"type":"InspectDatabase","games":0})
        );
        let remaining = DatabaseRequest::RemoveDatabase {
            path: first.clone(),
        }
        .apply(&directory)?;
        assert_eq!(
            json!(remaining),
            json!({"type":"RemoveDatabase","entries":[
                {"name":"Masters.zobrist","path":second}
            ]})
        );
        assert_eq!(
            Database::open(&first)
                .map_err(io::Error::other)?
                .game_count(),
            0
        );
        fs::remove_dir_all(directory)
    }
}
