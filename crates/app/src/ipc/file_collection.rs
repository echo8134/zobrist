//! The player's collection of PGN files and folders at their original locations.

use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Manager as _, Window};
use tauri_plugin_dialog::DialogExt as _;
use ts_rs::TS;

use super::{PgnFile, PgnFileAnswer, RefusalReason, Request, Response, pgn_file};

/// The command routes game requests and desktop file requests to their owners.
#[derive(Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum DesktopRequest {
    /// Requests answered by the games and PGN editor.
    Editor(Request),
    /// Requests about the player's collection or the system picker.
    Collection(CollectionRequest),
}

/// An operation on the Files page that needs the desktop environment.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum CollectionRequest {
    /// Choose an existing file, a folder, or a destination for a new PGN.
    PickPgnPath {
        /// Which dialog to show.
        choice: PickerChoice,
        /// The selected folder, when choosing a new file.
        folder: Option<PathBuf>,
    },
    /// Load the references the player has kept.
    GetFileCollection,
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
            Self::GetFileCollection => {
                let directory = window
                    .path()
                    .app_local_data_dir()
                    .map_err(io::Error::other)?;
                Ok(Response::GetFileCollection {
                    entries: collection(&directory)?,
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
    let list = directory.join("files.json");
    let entries = fs::read(&list).and_then(|bytes| {
        let paths: Vec<(EntryKind, PathBuf)> = serde_json::from_slice(&bytes)?;
        paths
            .into_iter()
            .map(|(kind, path)| CollectionEntry::new(kind, path))
            .collect()
    });
    match entries {
        Ok(entries) => Ok(entries),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!(
                "Could not read the file collection at {}. Your existing PGN files are untouched. Restore the list from a backup or move it aside, then reopen Files.",
                list.display()
            ),
        )),
    }
}

fn remember(directory: &Path, entry: &CollectionEntry) -> io::Result<Vec<CollectionEntry>> {
    fs::create_dir_all(directory)?;
    // Separate app windows and processes may add to the same collection.
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("files.lock"))?;
    lock.lock()?;
    let mut entries = collection(directory)?;
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
    let staged = directory.join("files.tmp");
    let mut file = File::create(&staged)?;
    let paths: Vec<_> = entries
        .iter()
        .map(|entry| (entry.kind, &entry.path))
        .collect();
    serde_json::to_writer(&mut file, &paths)?;
    file.sync_all()?;
    drop(file);
    fs::rename(staged, directory.join("files.json"))?;
    Ok(entries)
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
