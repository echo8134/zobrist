//! Native databases that the player has registered on this device.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Manager as _, Window};
use ts_rs::TS;
use zobrist_store::{Database, ImportError, OpenError, PendingWrite, import_pgn_for_task};

use super::file_collection::{list_recovery, lock_collection, publish_collection, read_list};
use super::{DatabaseGameQuery, Response, RunningWrite};

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
    /// Browse the rows of one native collection.
    ListDatabaseGames {
        /// The collection on this device.
        path: PathBuf,
        /// The filters, ordering and page chosen by the player.
        query: DatabaseGameQuery,
    },
    /// Verify the chosen database and keep its location.
    AddDatabase {
        /// The absolute path chosen by the player.
        path: PathBuf,
    },
    /// Create a new native collection from one PGN and register its path.
    ImportDatabase {
        /// The PGN file chosen by the player.
        source: PathBuf,
        /// The new native database path.
        destination: PathBuf,
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
    pub fn answer(self, window: &Window, write: Option<&RunningWrite>) -> io::Result<Response> {
        let directory = window
            .path()
            .app_local_data_dir()
            .map_err(io::Error::other)?;
        self.apply(&directory, write.map(RunningWrite::permission))
    }

    fn apply(self, directory: &Path, write: Option<&PendingWrite>) -> io::Result<Response> {
        let catalog = directory.join("databases.json");
        match self {
            Self::ListDatabases => Ok(Response::ListDatabases {
                entries: entries(&paths(&catalog)?)?,
            }),
            Self::ListDatabaseGames { path, query } => query.list(&path),
            Self::AddDatabase { path } => {
                let games = game_count(&path)?;
                let entries =
                    CatalogEdit::open(directory, &catalog)?.update(|paths| paths.insert(path))?;
                Ok(Response::AddDatabase { entries, games })
            }
            Self::ImportDatabase {
                source,
                destination,
            } => {
                file_name(&destination)?;
                let mut first_problem = None;
                let standalone = PendingWrite::default();
                let (report, catalog) = import_pgn_for_task(
                    &source,
                    &destination,
                    write.unwrap_or(&standalone),
                    |line| {
                        if first_problem.is_none() {
                            first_problem = line;
                        }
                        Ok(())
                    },
                    || CatalogEdit::open(directory, &catalog),
                )
                .map_err(|error| match error {
                    ImportError::Read(error) => io::Error::new(
                        error.kind(),
                        format!("Could not read {}: {error}", source.display()),
                    ),
                    ImportError::Create(error) if error.kind() == ErrorKind::AlreadyExists => {
                        io::Error::new(
                            error.kind(),
                            format!(
                                "A file already exists at {}. Choose a new file.",
                                destination.display()
                            ),
                        )
                    }
                    ImportError::Create(error) => io::Error::new(
                        error.kind(),
                        format!("Could not create {}: {error}", destination.display()),
                    ),
                })?;
                let entries = catalog.and_then(|catalog| catalog.update(|paths| paths.insert(destination.clone())))
                    .map_err(|error| {
                        io::Error::new(
                            error.kind(),
                            format!(
                                "The collection was saved at {}. {error} Use “Add collection…” to register the saved file.",
                                destination.display()
                            ),
                        )
                    })?;
                Ok(Response::ImportDatabase {
                    entries,
                    path: destination,
                    games: report.games,
                    problems: report.problems,
                    first_problem,
                })
            }
            Self::InspectDatabase { path } => Ok(Response::InspectDatabase {
                games: game_count(&path)?,
            }),
            Self::RemoveDatabase { path } => {
                file_name(&path)?;
                Ok(Response::RemoveDatabase {
                    entries: CatalogEdit::open(directory, &catalog)?
                        .update(|paths| paths.remove(&path))?,
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
    Ok(open_database(path)?.game_count())
}

pub(super) fn open_database(path: &Path) -> io::Result<Database> {
    file_name(path)?;
    Database::open(path).map_err(database_error)
}

pub(super) fn database_error(error: OpenError) -> io::Error {
    match error {
        OpenError::NotDatabase => io::Error::new(ErrorKind::InvalidData, "Not a zobrist database"),
        OpenError::Read(error) => io::Error::new(error.kind(), "File unavailable"),
        error @ (OpenError::Damaged | OpenError::Version { .. }) => {
            io::Error::new(ErrorKind::InvalidData, error)
        }
    }
}

fn paths(catalog: &Path) -> io::Result<BTreeSet<PathBuf>> {
    read_list(
        catalog,
        "the database collection",
        &list_recovery("existing database files", "Databases"),
        |bytes| {
            let paths: BTreeSet<PathBuf> = serde_json::from_slice(bytes)?;
            for path in &paths {
                file_name(path)?;
            }
            Ok(paths)
        },
    )
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

struct CatalogEdit<'a> {
    _lock: File,
    file: &'a Path,
    paths: BTreeSet<PathBuf>,
}

impl<'a> CatalogEdit<'a> {
    fn open(directory: &Path, file: &'a Path) -> io::Result<Self> {
        let lock = lock_collection(directory, "databases.lock")?;
        Ok(Self {
            _lock: lock,
            file,
            paths: paths(file)?,
        })
    }

    fn update(
        mut self,
        change: impl FnOnce(&mut BTreeSet<PathBuf>) -> bool,
    ) -> io::Result<Vec<DatabaseEntry>> {
        let changed = change(&mut self.paths);
        let entries = entries(&self.paths)?;
        if changed {
            publish_collection(self.file, &self.paths)?;
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process, thread};

    use serde_json::json;
    use zobrist_store::DatabaseWriter;

    use super::*;

    #[test]
    fn an_import_can_be_registered_after_recovering_the_list() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-import-recovery-{}", process::id()));
        fs::create_dir(&directory)?;
        let source = directory.join("round.pgn");
        fs::write(&source, "[Result \"*\"]\n\n1. e4 *\n")?;
        let list = directory.join("databases.json");
        fs::write(&list, b"[")?;
        let destination = directory.join("Study.zobrist");
        let error = DatabaseRequest::ImportDatabase {
            source,
            destination: destination.clone(),
        }
        .apply(&directory, None)
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "The collection was saved at {}. Could not read the database collection at {}. Your existing database files are untouched. Restore the list from a backup or move it aside, then reopen Databases. Use “Add collection…” to register the saved file.",
                destination.display(),
                list.display()
            )
        );
        assert_eq!(fs::read(&list)?, b"[");
        fs::rename(&list, directory.join("damaged.json"))?;
        assert_eq!(
            json!(
                DatabaseRequest::AddDatabase {
                    path: destination.clone()
                }
                .apply(&directory, None)?
            ),
            json!({"type":"AddDatabase","entries":[{"name":"Study.zobrist","path":destination}],"games":1})
        );
        fs::remove_dir_all(directory)
    }

    #[test]
    fn import_counts_every_problem_and_returns_the_first_diagnostic() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-import-report-{}", process::id()));
        fs::create_dir(&directory)?;
        let source = directory.join("two.pgn");
        fs::write(
            &source,
            "[White \"A\"]\n[Black \"B\"]\n\n1. Ke3 *\n\n[White \"C\"]\n[Black \"D\"]\n\n1. e4 e5 2. Ke3 *\n",
        )?;
        let destination = directory.join("Two.zobrist");
        let answer = DatabaseRequest::ImportDatabase {
            source,
            destination: destination.clone(),
        }
        .apply(&directory, None)?;
        assert_eq!(
            json!(answer),
            json!({
                "type":"ImportDatabase", "entries":[{"name":"Two.zobrist","path":destination}],
                "path":destination, "games":2, "problems":2,
                "firstProblem":"game 1 (A - B): illegal move \"Ke3\" at White's move 1, 0 moves kept"
            })
        );
        fs::remove_dir_all(directory)
    }

    #[test]
    fn an_unreadable_database_list_identifies_the_file_and_leaves_both_files_untouched()
    -> io::Result<()> {
        let directory =
            env::temp_dir().join(format!("zobrist-damaged-database-list-{}", process::id()));
        fs::create_dir(&directory)?;
        let list = directory.join("databases.json");
        let database = directory.join("Masters.zobrist");
        DatabaseWriter::create(&database)?.finish()?;
        let original = fs::read(&database)?;
        let message = format!(
            "Could not read the database collection at {}. Your existing database files are untouched. Restore the list from a backup or move it aside, then reopen Databases.",
            list.display()
        );

        for damaged in [b"[".as_slice(), br#"["relative.zobrist"]"#] {
            fs::write(&list, damaged)?;
            for request in [
                DatabaseRequest::ListDatabases,
                DatabaseRequest::AddDatabase {
                    path: database.clone(),
                },
                DatabaseRequest::RemoveDatabase {
                    path: database.clone(),
                },
            ] {
                assert_eq!(
                    request.apply(&directory, None).unwrap_err().to_string(),
                    message
                );
                assert_eq!(fs::read(&list)?, damaged);
                assert_eq!(fs::read(&database)?, original);
            }
        }
        fs::remove_dir_all(directory)
    }

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
            .apply(&directory, None)
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
            json!(DatabaseRequest::ListDatabases.apply(&directory, None)?),
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
            .apply(&directory, None)
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
        .apply(&directory, None)?;
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
