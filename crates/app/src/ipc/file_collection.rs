//! The player's collection of PGN files and folders at their original locations.

use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Manager as _, Window};
use tauri_plugin_dialog::DialogExt as _;
use ts_rs::TS;

use super::{
    AccountRequest, EngineAction, PgnFile, PgnFileAnswer, RefusalReason, Request, Response,
    pgn_file,
};

/// The command routes game, file and account requests to their owners.
#[derive(Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum DesktopRequest {
    /// Requests answered by the games and PGN editor.
    Editor(Request),
    /// Requests about the player's collection or the system picker.
    Collection(CollectionRequest),
    /// Requests about the name saved on this device.
    Account(AccountRequest),
}

/// A request about a desktop collection or its file picker.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum CollectionRequest {
    /// Choose a local engine program.
    PickEnginePath,
    /// Read or change the registered local engines.
    EngineInventory {
        /// The requested inventory operation.
        action: EngineAction,
    },
    /// Choose an existing file, a folder, or a destination for a new PGN.
    PickPgnPath {
        /// Which dialog to show.
        choice: PickerChoice,
        /// The selected folder, when choosing a new file.
        folder: Option<PathBuf>,
    },
    /// Load the references the player has kept.
    GetFileCollection,
    /// Recover the collection after the player chooses its last good copy.
    RecoverFileCollection,
    /// Open or create the chosen item and keep it in the collection.
    AddPgnPath {
        /// Which picker supplied the path.
        choice: PickerChoice,
        /// The folder selected when the dialog opened.
        folder: Option<PathBuf>,
        /// Absolute path returned by the desktop picker.
        path: PathBuf,
    },
    /// Read the immediate children of a folder.
    ReadPgnFolder {
        /// The folder to list.
        path: PathBuf,
    },
}

/// The kind of system dialog the player asked for.
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, TS)]
pub enum PickerChoice {
    /// Open an existing PGN file.
    File,
    /// Open an existing folder.
    Folder,
    /// Choose a destination for a new PGN.
    New,
}

/// The kind of item shown in the file collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
pub enum EntryKind {
    /// A folder the player can expand.
    Folder,
    /// A PGN file the player can open.
    File,
}

/// A named item in the player's collection or in an expanded folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct CollectionEntry {
    kind: EntryKind,
    path: PathBuf,
    name: String,
}

/// The collection or the recovery options available for its current files.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum FileCollection {
    /// The collection was read successfully.
    Ready {
        /// The entries in their saved order.
        entries: Vec<CollectionEntry>,
    },
    /// The live collection could not be loaded or recovered.
    Unavailable {
        /// Describes the problem and what the player can do.
        message: String,
        /// The last good copy can be used to retry recovery.
        recoverable: bool,
    },
}

impl CollectionEntry {
    fn new(kind: EntryKind, path: PathBuf) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Choose an absolute path.",
            ));
        }
        if kind == EntryKind::File && !is_pgn(&path) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Choose a PGN file.",
            ));
        }
        let text = path.to_str().ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                "The path cannot be displayed as text.",
            )
        })?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(text)
            .to_owned();
        Ok(Self { kind, path, name })
    }
}

impl CollectionRequest {
    /// Answers on a blocking thread. A dialog or a folder read never takes the game lock.
    pub fn answer(self, window: &Window) -> io::Result<Response> {
        match self {
            Self::PickEnginePath => {
                let path = window
                    .dialog()
                    .file()
                    .set_parent(window)
                    .blocking_pick_file()
                    .map(|file| file.into_path().map_err(io::Error::other))
                    .transpose()?;
                Ok(Response::PickEnginePath { path })
            }
            Self::EngineInventory { action } => {
                let directory = window
                    .path()
                    .app_local_data_dir()
                    .map_err(io::Error::other)?;
                Ok(Response::EngineInventory {
                    entries: action.apply(&directory)?,
                })
            }
            Self::PickPgnPath { choice, folder } => {
                let dialog = window.dialog().file().set_parent(window);
                let selected = match choice {
                    PickerChoice::Folder => dialog.blocking_pick_folder(),
                    PickerChoice::File => dialog
                        .add_filter("PGN files", &["pgn"])
                        .blocking_pick_file(),
                    PickerChoice::New => {
                        let mut dialog = dialog
                            .add_filter("PGN files", &["pgn"])
                            .set_file_name("Untitled.pgn");
                        if let Some(directory) = folder.or_else(|| {
                            suggested_folder(window.path().document_dir().ok().as_deref())
                        }) {
                            dialog = dialog.set_directory(directory);
                        }
                        dialog.blocking_save_file()
                    }
                };
                let path = selected
                    .map(|file| file.into_path().map_err(io::Error::other))
                    .transpose()?;
                if matches!(choice, PickerChoice::File)
                    && path.as_deref().is_some_and(|file| !is_pgn(file))
                {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "Choose a PGN file.",
                    ));
                }
                Ok(Response::PickPgnPath { path })
            }
            Self::GetFileCollection | Self::RecoverFileCollection => {
                let directory = window
                    .path()
                    .app_local_data_dir()
                    .map_err(io::Error::other)?;
                let recover = matches!(self, Self::RecoverFileCollection);
                let lock = lock_collection(&directory, "files.lock").ok();
                let collection = match read_collection(&directory, lock.as_ref(), recover) {
                    Ok(entries) => FileCollection::Ready { entries },
                    Err((error, recoverable)) => FileCollection::Unavailable {
                        message: error.to_string(),
                        recoverable,
                    },
                };
                Ok(if recover {
                    Response::RecoverFileCollection { collection }
                } else {
                    Response::GetFileCollection { collection }
                })
            }
            Self::AddPgnPath {
                choice,
                path,
                folder,
            } => {
                let directory = window
                    .path()
                    .app_local_data_dir()
                    .map_err(io::Error::other)?;
                Ok(Response::AddPgnPath {
                    answer: add(&directory, choice, &path, folder.as_deref())?,
                })
            }
            Self::ReadPgnFolder { path } => Ok(Response::ReadPgnFolder {
                entries: children(&path)?,
            }),
        }
    }
}

/// The selection and collection shown after opening or creating an item.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum AddedPgnPath {
    /// The selected PGN with its games.
    File {
        /// File name and location.
        entry: CollectionEntry,
        /// The root entries of the collection.
        entries: Vec<CollectionEntry>,
        /// The refreshed parent when the new file belongs to the selected folder.
        parent: Option<FolderListing>,
        /// The contents read from or written to the PGN file.
        file: PgnFile,
    },
    /// The selected folder with its immediate children.
    Folder {
        /// Folder name and location.
        entry: CollectionEntry,
        /// The root entries of the collection.
        entries: Vec<CollectionEntry>,
        /// The folders and PGNs directly below the selected folder.
        children: Vec<CollectionEntry>,
    },
    /// The file could not be opened or created.
    Refusal {
        /// The reason to show to the player.
        reason: RefusalReason,
        /// What the system said.
        system: Option<String>,
    },
}

/// The children of a folder after a new file was added.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct FolderListing {
    path: PathBuf,
    entries: Vec<CollectionEntry>,
}

fn add(
    directory: &Path,
    choice: PickerChoice,
    path: &Path,
    folder: Option<&Path>,
) -> io::Result<AddedPgnPath> {
    let answer = match choice {
        PickerChoice::Folder => {
            let entry = CollectionEntry::new(EntryKind::Folder, path.to_owned())?;
            let children = children(path)?;
            let entries = remember(directory, &entry)?;
            return Ok(AddedPgnPath::Folder {
                entry,
                entries,
                children,
            });
        }
        PickerChoice::File => {
            CollectionEntry::new(EntryKind::File, path.to_owned())?;
            pgn_file::opened(path)
        }
        PickerChoice::New => {
            if !path.is_absolute() {
                return Ok(AddedPgnPath::Refusal {
                    reason: RefusalReason::CannotCreate,
                    system: Some("Choose a destination folder.".to_owned()),
                });
            }
            let mut path = path.to_owned();
            if !is_pgn(&path) {
                path.add_extension("pgn");
            }
            pgn_file::created(&path)
        }
    };
    let file = match answer {
        PgnFileAnswer::File(file) => file,
        PgnFileAnswer::Refusal { reason, system } => {
            return Ok(AddedPgnPath::Refusal { reason, system });
        }
    };
    let entry = CollectionEntry::new(EntryKind::File, PathBuf::from(&file.path))?;
    let parent =
        folder.filter(|folder| choice == PickerChoice::New && entry.path.parent() == Some(*folder));
    let result = match parent {
        Some(folder) => collection(directory).and_then(|entries| {
            Ok((
                entries,
                Some(FolderListing {
                    path: folder.to_owned(),
                    entries: children(folder)?,
                }),
            ))
        }),
        None => remember(directory, &entry).map(|entries| (entries, None)),
    };
    let (entries, parent) = result.map_err(|error| {
        if choice == PickerChoice::New {
            io::Error::new(
                error.kind(),
                format!(
                    "Created {}. The collection could not be updated. {error}",
                    file.path
                ),
            )
        } else {
            error
        }
    })?;
    Ok(AddedPgnPath::File {
        entry,
        entries,
        parent,
        file,
    })
}

fn suggested_folder(documents: Option<&Path>) -> Option<PathBuf> {
    let directory = documents?.join("zobrist");
    match fs::create_dir(&directory) {
        Ok(()) => Some(directory),
        Err(_) if directory.is_dir() => Some(directory),
        Err(_) => None,
    }
}

fn is_pgn(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pgn"))
}

fn children(path: &Path) -> io::Result<Vec<CollectionEntry>> {
    let mut entries = Vec::new();
    for child in fs::read_dir(path)? {
        let child = child?;
        let file_type = child.file_type()?;
        let path = child.path();
        let kind = if file_type.is_dir() {
            EntryKind::Folder
        } else if is_pgn(&path)
            && (file_type.is_file() || (file_type.is_symlink() && path.is_file()))
        {
            EntryKind::File
        } else {
            continue;
        };
        entries.push(CollectionEntry::new(kind, path)?);
    }
    entries.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
    Ok(entries)
}

fn collection(directory: &Path) -> io::Result<Vec<CollectionEntry>> {
    let lock = lock_collection(directory, "files.lock").ok();
    read_collection(directory, lock.as_ref(), false).map_err(|(error, _)| error)
}

fn parse_collection(bytes: &[u8]) -> io::Result<Vec<CollectionEntry>> {
    let paths: Vec<(EntryKind, PathBuf)> = serde_json::from_slice(bytes)?;
    paths
        .into_iter()
        .map(|(kind, path)| CollectionEntry::new(kind, path))
        .collect()
}

fn read_collection(
    directory: &Path,
    lock: Option<&File>,
    recover: bool,
) -> Result<Vec<CollectionEntry>, (io::Error, bool)> {
    let list = directory.join("files.json");
    let backup = directory.join("files.last-good.json");
    let live = fs::read(&list);
    let kind = match &live {
        Ok(bytes) => match parse_collection(bytes) {
            Ok(entries) => {
                if lock.is_some() {
                    refresh_backup(&backup, bytes);
                }
                return Ok(entries);
            }
            Err(error) => error.kind(),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => error.kind(),
    };
    let problem = format!(
        "Could not read the file collection at {}. Your existing PGN files are untouched.",
        list.display()
    );
    let (entries, bytes) = fs::read(&backup)
        .and_then(|bytes| Ok((parse_collection(&bytes)?, bytes)))
        .map_err(|error| {
            let manual = format!(
                "{problem} Restore the list from a backup or move it aside, then reopen Files."
            );
            let message = if error.kind() == ErrorKind::NotFound {
                manual
            } else {
                format!(
                    "{manual} The last good copy at {} could not be read.",
                    backup.display()
                )
            };
            (io::Error::new(kind, message), false)
        })?;
    if !recover {
        return Err((
            io::Error::new(
                kind,
                format!("{problem} Choose Recover file list to restore the last good copy."),
            ),
            true,
        ));
    }
    let recovery_error = |error: io::Error, action: &str| {
        (
            io::Error::new(
                error.kind(),
                format!(
                    "Could not {action} at {}. Your existing PGN files are untouched. Allow writing to the app data folder and try again.",
                    list.display()
                ),
            ),
            true,
        )
    };
    let preserve_error = |error| recovery_error(error, "preserve the damaged list");
    if lock.is_none() {
        return Err(preserve_error(ErrorKind::PermissionDenied.into()));
    }
    preserve_damaged(directory, &live.map_err(preserve_error)?).map_err(preserve_error)?;
    publish_bytes(&list, &bytes)
        .map_err(|error| recovery_error(error, "restore the file collection"))?;
    Ok(entries)
}

fn preserve_damaged(directory: &Path, bytes: &[u8]) -> io::Result<()> {
    for number in 1_u64.. {
        let path = directory.join(format!("files.damaged-{number}.json"));
        let mut file = match File::create_new(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        if result.is_err() {
            fs::remove_file(path).ok();
        }
        return result;
    }
    Err(io::Error::other("No unused damaged list name."))
}

fn refresh_backup(path: &Path, bytes: &[u8]) {
    if !fs::read(path).is_ok_and(|previous| previous == bytes) {
        // The valid live list stays usable even if the backup folder cannot be written.
        publish_bytes(path, bytes).ok();
    }
}

fn remember(directory: &Path, entry: &CollectionEntry) -> io::Result<Vec<CollectionEntry>> {
    let lock = lock_collection(directory, "files.lock")?;
    let mut entries = read_collection(directory, Some(&lock), false).map_err(|(error, _)| error)?;
    if let Some(previous) = entries
        .iter_mut()
        .find(|previous| previous.path == entry.path)
    {
        if previous.kind == entry.kind {
            return Ok(entries);
        }
        previous.clone_from(entry);
    } else {
        entries.push(entry.clone());
    }
    let paths: Vec<_> = entries
        .iter()
        .map(|entry| (entry.kind, &entry.path))
        .collect();
    let bytes = serde_json::to_vec(&paths)?;
    publish_bytes(&directory.join("files.json"), &bytes)?;
    refresh_backup(&directory.join("files.last-good.json"), &bytes);
    Ok(entries)
}

pub(super) fn lock_collection(directory: &Path, filename: &str) -> io::Result<File> {
    fs::create_dir_all(directory)?;
    // Separate app windows and processes may add to the same collection.
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(filename))?;
    lock.lock()?;
    Ok(lock)
}

pub(super) fn publish_collection(path: &Path, contents: &impl Serialize) -> io::Result<()> {
    publish_bytes(path, &serde_json::to_vec(contents)?)
}

fn publish_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let staged = path.with_extension("tmp");
    // Writers hold the collection lock. A leftover staging file may be a link.
    if let Err(error) = fs::remove_file(&staged)
        && error.kind() != ErrorKind::NotFound
    {
        return Err(error);
    }
    let mut file = File::create_new(&staged)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(staged, path)
}

#[cfg(test)]
mod tests {
    use std::{env, process, thread};

    use serde_json::json;

    use super::*;

    #[test]
    fn lists_the_folder_and_its_pgn_files_in_name_order() -> io::Result<()> {
        let folder = env::temp_dir().join(format!("zobrist-tournaments-{}", process::id()));
        fs::create_dir(&folder)?;
        fs::create_dir(folder.join("Archive"))?;
        fs::write(folder.join("round 2.PGN"), "")?;
        fs::write(folder.join("round 1.pgn"), "")?;
        fs::write(folder.join("schedule.txt"), "")?;
        fs::write(folder.join("Archive/final.pgn"), "")?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&folder, folder.join("loop"))?;

        assert_eq!(
            json!(children(&folder)?),
            json!([
                {"kind":"Folder", "name":"Archive", "path":folder.join("Archive")},
                {"kind":"File", "name":"round 1.pgn", "path":folder.join("round 1.pgn")},
                {"kind":"File", "name":"round 2.PGN", "path":folder.join("round 2.PGN")},
            ])
        );
        fs::remove_dir_all(folder)
    }

    #[test]
    fn a_new_pgn_appears_in_its_selected_folder_and_an_external_pgn_stays_at_its_location()
    -> io::Result<()> {
        let work = env::temp_dir().join(format!("zobrist-file-picker-{}", process::id()));
        let data = work.join("app");
        let folder = work.join("Tournaments");
        fs::create_dir_all(&folder)?;
        let expected_folder = json!({"kind":"Folder", "name":"Tournaments", "path":folder});
        assert_eq!(
            json!(add(&data, PickerChoice::Folder, &folder, None)?),
            json!({"type":"Folder", "entry":expected_folder, "entries":[expected_folder], "children":[]})
        );

        assert_eq!(
            json!(add(&data, PickerChoice::New, Path::new("test"), None)?),
            json!({"type":"Refusal","reason":"cannotCreate","system":"Choose a destination folder."})
        );
        let path = folder.join("test.pgn");
        let expected_file = json!({"kind":"File", "name":"test.pgn", "path":path});
        assert_eq!(
            json!(add(
                &data,
                PickerChoice::New,
                &folder.join("test"),
                Some(&folder)
            )?),
            json!({"type":"File", "entry":expected_file, "entries":[expected_folder],
                "parent":{"path":folder,"entries":[expected_file]},
                "file":{"path":path,"rows":[],"problems":0}})
        );
        fs::write(&path, "1. e4 *\n")?;
        assert_eq!(
            json!(add(&data, PickerChoice::New, &path, Some(&folder))?),
            json!({"type":"Refusal","reason":"existsAlready","system":null})
        );
        assert_eq!(fs::read(&path)?, b"1. e4 *\n");

        let repertoire = work.join("Repertoire.pgn");
        fs::write(&repertoire, b"")?;
        let expected_repertoire = json!({"kind":"File","name":"Repertoire.pgn","path":repertoire});
        assert_eq!(
            json!(add(&data, PickerChoice::File, &repertoire, Some(&folder))?),
            json!({"type":"File","entry":expected_repertoire,"entries":[expected_folder,expected_repertoire],
                "parent":null,"file":{"path":repertoire,"rows":[],"problems":0}})
        );
        assert_eq!(
            json!(collection(&data)?),
            json!([expected_folder, expected_repertoire])
        );
        fs::remove_dir_all(work)
    }

    #[test]
    fn keeps_concurrent_references_across_reads_and_remembers_each_only_once() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-collection-{}", process::id()));
        let folder = CollectionEntry::new(EntryKind::Folder, directory.join("Tournaments"))?;
        let file = CollectionEntry::new(EntryKind::File, directory.join("Repertoire.pgn"))?;
        assert_eq!(json!(collection(&directory)?), json!([]));

        thread::scope(|scope| -> io::Result<()> {
            let remembering_folder = scope.spawn(|| remember(&directory, &folder));
            remember(&directory, &file)?;
            remembering_folder.join().unwrap()?;
            Ok(())
        })?;
        remember(&directory, &folder)?;
        let mut reopened = collection(&directory)?;
        reopened.sort_by_key(|entry| entry.kind);
        assert_eq!(
            json!(reopened),
            json!([
                {"kind":"Folder", "name":"Tournaments", "path":directory.join("Tournaments")},
                {"kind":"File", "name":"Repertoire.pgn", "path":directory.join("Repertoire.pgn")},
            ])
        );

        let bad_collection = br#"[["File","relative.pgn"]]"#;
        fs::write(directory.join("files.json"), bad_collection)?;
        assert_eq!(
            remember(&directory, &file).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
        assert_eq!(fs::read(directory.join("files.json"))?, bad_collection);
        fs::remove_dir_all(directory)
    }

    #[test]
    fn identifies_an_unreadable_collection_when_loading_it_or_opening_a_pgn() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-damaged-list-{}", process::id()));
        fs::create_dir(&directory)?;
        let list = directory.join("files.json");
        let game = directory.join("game.pgn");
        fs::write(&list, b"[[")?;
        fs::write(&game, b"1. e4 *\n")?;

        let message = format!(
            "Could not read the file collection at {}. Your existing PGN files are untouched. Restore the list from a backup or move it aside, then reopen Files.",
            list.display()
        );
        assert_eq!(collection(&directory).unwrap_err().to_string(), message);
        assert_eq!(
            add(&directory, PickerChoice::File, &game, None)
                .unwrap_err()
                .to_string(),
            message
        );
        assert_eq!(fs::read(list)?, b"[[");
        assert_eq!(fs::read(game)?, b"1. e4 *\n");
        fs::remove_dir_all(directory)
    }

    #[test]
    fn a_missing_list_has_no_bytes_to_back_up_and_a_valid_empty_list_can_be_recovered()
    -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-empty-backup-{}", process::id()));
        let list = directory.join("files.json");
        let backup = directory.join("files.last-good.json");
        assert_eq!(json!(collection(&directory)?), json!([]));
        assert!(!backup.try_exists()?);
        fs::write(&backup, b"[ ]\n")?;
        assert_eq!(json!(collection(&directory)?), json!([]));
        assert_eq!(fs::read(&backup)?, b"[ ]\n");
        fs::write(&list, b"[[")?;

        let lock = lock_collection(&directory, "files.lock")?;
        assert_eq!(
            json!(read_collection(&directory, Some(&lock), true).unwrap()),
            json!([])
        );
        assert_eq!(fs::read(&list)?, b"[ ]\n");
        assert_eq!(fs::read(backup)?, b"[ ]\n");
        assert_eq!(fs::read(directory.join("files.damaged-1.json"))?, b"[[");
        drop(lock);
        fs::remove_dir_all(directory)
    }

    #[test]
    fn recovery_keeps_the_list_already_repaired_by_another_window() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-repaired-list-{}", process::id()));
        fs::create_dir(&directory)?;
        let list = directory.join("files.json");
        let backup = directory.join("files.last-good.json");
        fs::write(&list, b"[[")?;
        fs::write(&backup, b"[]")?;
        assert_eq!(
            collection(&directory).unwrap_err().to_string(),
            format!(
                "Could not read the file collection at {}. Your existing PGN files are untouched. Choose Recover file list to restore the last good copy.",
                list.display()
            )
        );

        let folder = directory.join("Club");
        let repaired = serde_json::to_string_pretty(&json!([["Folder", folder]]))?;
        fs::write(&list, &repaired)?;
        let lock = lock_collection(&directory, "files.lock")?;
        assert_eq!(
            json!(read_collection(&directory, Some(&lock), true).unwrap()),
            json!([{"kind":"Folder", "name":"Club", "path":folder}])
        );
        assert_eq!(fs::read_to_string(list)?, repaired);
        assert_eq!(fs::read_to_string(backup)?, repaired);
        assert!(!directory.join("files.damaged-1.json").try_exists()?);
        drop(lock);
        fs::remove_dir_all(directory)
    }

    #[test]
    fn a_failed_backup_refresh_keeps_the_previous_good_copy_and_the_live_list() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-failed-backup-{}", process::id()));
        fs::create_dir(&directory)?;
        let list = directory.join("files.json");
        let backup = directory.join("files.last-good.json");
        fs::write(&list, b"[ ]\n")?;
        fs::write(&backup, b"[]")?;
        fs::create_dir(directory.join("files.last-good.tmp"))?;

        assert_eq!(json!(collection(&directory)?), json!([]));
        assert_eq!(fs::read(list)?, b"[ ]\n");
        assert_eq!(fs::read(backup)?, b"[]");
        fs::remove_dir_all(directory)
    }

    #[test]
    fn suggests_the_app_folder_only_in_an_available_documents_directory() -> io::Result<()> {
        let documents = env::temp_dir().join(format!("zobrist-documents-{}", process::id()));
        assert_eq!(suggested_folder(None), None);
        assert_eq!(suggested_folder(Some(&documents)), None);
        fs::create_dir(&documents)?;
        let folder = documents.join("zobrist");
        assert_eq!(suggested_folder(Some(&documents)), Some(folder.clone()));
        assert!(folder.is_dir());
        assert_eq!(suggested_folder(Some(&documents)), Some(folder));
        fs::remove_dir_all(documents)
    }
}
