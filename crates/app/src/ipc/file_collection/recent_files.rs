//! PGN files opened on this device, in the order Home displays them.

use std::collections::HashSet;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::{CollectionEntry, EntryKind, lock_collection, publish_collection, read_or_default};

const LIMIT: usize = 10;

pub(super) fn list(directory: &Path) -> io::Result<Vec<CollectionEntry>> {
    read_or_default(
        &directory.join("recent-files.json"),
        "the recent files list",
        "Your existing PGN files are untouched. Restore the list from a backup or move it aside, then reopen Home.",
        |bytes| {
            let paths: Vec<PathBuf> = serde_json::from_slice(bytes)?;
            let mut seen = HashSet::new();
            if paths.len() > LIMIT || paths.iter().any(|path| !seen.insert(path)) {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Invalid recent files list.",
                ));
            }
            paths
                .into_iter()
                .map(|path| CollectionEntry::new(EntryKind::File, path))
                .collect()
        },
    )
}

pub(super) fn record(directory: &Path, path: &Path) {
    // Failure to save the history must not prevent the player from opening a PGN.
    let Ok(_lock) = lock_collection(directory, "recent-files.lock") else {
        return;
    };
    let Ok(entries) = list(directory) else {
        return;
    };
    if entries.first().is_some_and(|entry| entry.path == path) {
        return;
    }
    let mut paths: Vec<_> = entries.into_iter().map(|entry| entry.path).collect();
    paths.retain(|previous| previous != path);
    paths.insert(0, path.to_owned());
    paths.truncate(LIMIT);
    drop(publish_collection(
        &directory.join("recent-files.json"),
        &paths,
    ));
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use serde_json::json;

    use super::super::{AddedPgnPath, PickerChoice, add, open_listed};
    use super::*;
    use crate::ipc::{Games, PgnFileAnswer, Request, Response};

    #[test]
    fn only_successful_opens_change_the_recent_files_order() -> io::Result<()> {
        let work = env::temp_dir().join(format!("zobrist-recent-opens-{}", process::id()));
        let data = work.join("data");
        assert_eq!(json!(list(&data)?), json!([]));
        let folder = work.join("Club");
        fs::create_dir_all(&folder)?;
        let child = folder.join("Final.pgn");
        let picked = work.join("Final.pgn");
        fs::write(&child, b"1. e4 *\n")?;
        fs::write(&picked, b"1. d4 *\n")?;

        add(&data, PickerChoice::Folder, &folder, None)?;
        open_listed(&data, child.clone())?;
        add(&data, PickerChoice::File, &picked, None)?;
        let picked_entry = json!({"kind": "File", "name": "Final.pgn", "path": picked});
        let child_entry = json!({"kind": "File", "name": "Final.pgn", "path": child});
        assert_eq!(json!(list(&data)?), json!([picked_entry, child_entry]));

        open_listed(&data, child.clone())?;
        assert_eq!(json!(list(&data)?), json!([child_entry, picked_entry]));

        add(&data, PickerChoice::Folder, &folder, None)?;
        add(&data, PickerChoice::New, &work.join("New.pgn"), None)?;
        assert!(matches!(
            Games::default().answer(Request::OpenPgnFile {
                path: picked.to_string_lossy().into_owned(),
            }),
            Response::OpenPgnFile {
                answer: PgnFileAnswer::File(_)
            }
        ));
        fs::remove_file(&picked)?;
        assert!(matches!(
            open_listed(&data, picked.clone())?,
            PgnFileAnswer::Refusal { .. }
        ));
        assert!(matches!(
            add(&data, PickerChoice::File, &picked, None)?,
            AddedPgnPath::Refusal { .. }
        ));
        assert_eq!(json!(list(&data)?), json!([child_entry, picked_entry]));
        let saved = fs::read(data.join("recent-files.json"))?;
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&saved)?,
            json!([child, picked])
        );
        fs::remove_dir_all(work)
    }

    #[test]
    fn keeps_the_ten_latest_opened_paths() -> io::Result<()> {
        let work = env::temp_dir().join(format!("zobrist-recent-limit-{}", process::id()));
        fs::create_dir(&work)?;
        for number in 1..=11 {
            let path = work.join(format!("{number:02}.pgn"));
            fs::write(&path, b"")?;
            open_listed(&work, path)?;
        }
        let names: Vec<_> = list(&work)?.into_iter().map(|entry| entry.name).collect();
        assert_eq!(
            names,
            [
                "11.pgn", "10.pgn", "09.pgn", "08.pgn", "07.pgn", "06.pgn", "05.pgn", "04.pgn",
                "03.pgn", "02.pgn"
            ]
        );
        fs::remove_dir_all(work)
    }

    #[test]
    fn reports_damaged_history_and_keeps_it_intact_while_opening_pgns() -> io::Result<()> {
        let work = env::temp_dir().join(format!("zobrist-recent-damaged-{}", process::id()));
        fs::create_dir(&work)?;
        let history = work.join("recent-files.json");
        let path = work.join("game.pgn");
        fs::write(&path, b"1. e4 *\n")?;
        let message = format!(
            "Could not read the recent files list at {}. Your existing PGN files are untouched. Restore the list from a backup or move it aside, then reopen Home.",
            history.display()
        );

        let too_many: Vec<_> = (1..=11)
            .map(|number| work.join(format!("{number}.pgn")))
            .collect();
        for bytes in [
            b"{".to_vec(),
            serde_json::to_vec(&["relative.pgn"])?,
            serde_json::to_vec(&[work.join("games.txt")])?,
            serde_json::to_vec(&[&path, &path])?,
            serde_json::to_vec(&too_many)?,
        ] {
            fs::write(&history, &bytes)?;
            assert_eq!(list(&work).unwrap_err().to_string(), message);
            assert!(matches!(
                add(&work, PickerChoice::File, &path, None)?,
                AddedPgnPath::File { .. }
            ));
            assert!(matches!(
                open_listed(&work, path.clone())?,
                PgnFileAnswer::File(_)
            ));
            assert_eq!(fs::read(&history)?, bytes);
        }
        assert_eq!(fs::read(&path)?, b"1. e4 *\n");
        fs::remove_dir_all(work)
    }
}
