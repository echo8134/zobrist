//! PGN files opened on this device, in the order Home displays them.

use std::collections::HashSet;
use std::fs::TryLockError;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{
    CollectionEntry, EntryKind, collection_lock_file, list_recovery, publish_collection, read_list,
};

const LIMIT: usize = 10;
const LOCK_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) fn list(directory: &Path) -> io::Result<Vec<CollectionEntry>> {
    read_list(
        &directory.join("recent-files.json"),
        "the recent files list",
        &list_recovery("existing PGN files", "Home"),
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
    let Ok(lock) = collection_lock_file(directory, "recent-files.lock") else {
        return;
    };
    // A busy history may skip this open so the player can still see the games.
    let deadline = Instant::now() + LOCK_TIMEOUT;
    while let Err(error) = lock.try_lock() {
        if !matches!(error, TryLockError::WouldBlock) {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
        if Instant::now() >= deadline {
            return;
        }
    }
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

    use super::super::{AddedPgnPath, PickerChoice, add, lock_collection, open_listed};
    use super::*;
    use crate::ipc::{Games, PgnFileAnswer, Request, Response};

    #[test]
    fn opens_the_game_when_the_data_directory_cannot_be_resolved() -> io::Result<()> {
        let game =
            env::temp_dir().join(format!("zobrist-recent-no-directory-{}.pgn", process::id()));
        fs::write(&game, "[White \"Ada\"]\n[Black \"Bo\"]\n\n1. e4 1-0\n")?;
        let directory = Err(io::Error::other("No local data directory"));
        let answer = open_listed(directory, game.clone())?;
        assert_eq!(
            json!(answer),
            json!({"type":"File","path":game, "problems":0, "rows":[
                {"number":1,"white":"Ada","black":"Bo","result":"1-0","date":"?","event":"?"}
            ]})
        );
        fs::remove_file(game)
    }

    #[test]
    fn opens_the_game_while_another_handle_holds_the_history_lock() -> io::Result<()> {
        let work = env::temp_dir().join(format!("zobrist-recent-locked-{}", process::id()));
        fs::create_dir(&work)?;
        let first = work.join("first.pgn");
        let next = work.join("next.pgn");
        fs::write(&first, "")?;
        fs::write(&next, "[White \"Ada\"]\n[Black \"Bo\"]\n\n1. e4 1-0\n")?;
        open_listed(Ok(work.clone()), first.clone())?;

        let lock = lock_collection(&work, "recent-files.lock")?;
        let (send, receive) = std::sync::mpsc::channel();
        let answer = std::thread::scope(|scope| {
            scope.spawn(|| send.send(open_listed(Ok(work.clone()), next.clone())));
            // Allow the one-second lock wait plus time to read the fixture and schedule the thread.
            let result = receive.recv_timeout(std::time::Duration::from_millis(1250));
            drop(lock);
            result
        })
        .expect("opening a PGN must finish while the history lock is held")?;
        assert_eq!(
            json!(answer)["rows"],
            json!([{"number":1,"white":"Ada","black":"Bo","result":"1-0","date":"?","event":"?"}])
        );
        let first_entry = json!({"kind":"File","name":"first.pgn","path":first});
        let next_entry = json!({"kind":"File","name":"next.pgn","path":next});
        assert_eq!(json!(list(&work)?), json!([first_entry]));
        open_listed(Ok(work.clone()), next)?;
        assert_eq!(json!(list(&work)?), json!([next_entry, first_entry]));
        fs::remove_dir_all(work)
    }

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
        open_listed(Ok(data.clone()), child.clone())?;
        add(&data, PickerChoice::File, &picked, None)?;
        let picked_entry = json!({"kind": "File", "name": "Final.pgn", "path": picked});
        let child_entry = json!({"kind": "File", "name": "Final.pgn", "path": child});
        assert_eq!(json!(list(&data)?), json!([picked_entry, child_entry]));

        open_listed(Ok(data.clone()), child.clone())?;
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
            open_listed(Ok(data.clone()), picked.clone())?,
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
            open_listed(Ok(work.clone()), path)?;
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
                open_listed(Ok(work.clone()), path.clone())?,
                PgnFileAnswer::File(_)
            ));
            assert_eq!(fs::read(&history)?, bytes);
        }
        assert_eq!(fs::read(&path)?, b"1. e4 *\n");
        fs::remove_dir_all(work)
    }
}
