//! The board session kept in the desktop app's local data directory.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::async_runtime::spawn_blocking;
use ts_rs::TS;
use zobrist_core::editor::{GameEditor, Place};
use zobrist_core::game::Game;

use super::{GamePicture, Games, InFile, OpenGame, Response, SharedGames};

/// An operation on the board tabs kept across normal quits.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum SessionRequest {
    /// Load the board session at startup.
    RestoreBoards,
    /// Save the boards before closing the window.
    QuitWithBoards {
        /// Open game numbers in tab order.
        boards: Vec<usize>,
        /// The selected open game, when a board is shown.
        selected: Option<usize>,
    },
}

/// A restored game and the picture at its saved place.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct BoardPicture {
    game: usize,
    picture: GamePicture,
}

#[derive(Serialize)]
struct BoardSession {
    boards: Vec<SavedBoard>,
    selected: Option<usize>,
}

#[derive(Deserialize, Serialize)]
struct SavedBoard {
    #[serde(with = "game_pgn")]
    game: Game,
    place: Vec<usize>,
    origin: Option<InFile>,
}

impl SharedGames {
    /// Whether the window has successfully loaded its boards and can save them on quit.
    #[must_use]
    pub fn is_restored(&self) -> bool {
        self.restored_selection.get().is_some()
    }

    /// Restores or saves a board session. PGN conversion and file I/O run outside the game lock.
    pub async fn answer_session(
        &self,
        request: SessionRequest,
        directory: PathBuf,
    ) -> io::Result<Response> {
        // A board snapshot needs the origin installed by any preceding Save.
        let mut pending_backup = self.one_save.lock().await;
        match request {
            SessionRequest::RestoreBoards => {
                if let Some(&selected) = self.restored_selection.get() {
                    return Ok(self.games.lock().await.board_pictures(selected, None));
                }
                let restored = spawn_blocking(move || ReopenedBoards::read(&directory))
                    .await
                    .map_err(io::Error::other)?;
                *pending_backup = restored.pending_backup;
                let mut games = self.games.lock().await;
                let mut selected_game = None;
                // A game opened or closed while the session was loading belongs to this run.
                if games.next_number == 1 {
                    for (index, open) in restored.boards {
                        let game = games.insert(open);
                        if restored.selected == Some(index) {
                            selected_game = Some(game);
                        }
                    }
                }
                self.restored_selection.get_or_init(|| selected_game);
                Ok(games.board_pictures(selected_game, restored.warning))
            }
            SessionRequest::QuitWithBoards { boards, selected } => {
                let snapshot = self.games.lock().await.snapshot(&boards, selected)?;
                if let Some(path) = pending_backup.clone() {
                    let result = spawn_blocking(move || preserve_original(&path))
                        .await
                        .map_err(io::Error::other)?;
                    match result {
                        Ok(_) => (),
                        Err(error) if error.kind() == ErrorKind::NotFound => (),
                        Err(error) => return Err(error),
                    }
                    *pending_backup = None;
                }
                spawn_blocking(move || snapshot.write(&directory))
                    .await
                    .map_err(io::Error::other)??;
                Ok(Response::QuitWithBoards)
            }
        }
    }
}

impl Games {
    fn board_pictures(&self, selected: Option<usize>, warning: Option<String>) -> Response {
        Response::RestoreBoards {
            boards: self
                .open_games
                .iter()
                .map(|(&game, open)| BoardPicture {
                    game,
                    picture: open.picture(),
                })
                .collect(),
            selected: selected.filter(|game| self.open_games.contains_key(game)),
            warning,
        }
    }

    fn snapshot(&self, boards: &[usize], selected: Option<usize>) -> io::Result<BoardSession> {
        let selected = selected
            .map(|game| {
                boards
                    .iter()
                    .position(|&number| number == game)
                    .ok_or_else(|| {
                        io::Error::new(ErrorKind::InvalidInput, "The selected board is not open.")
                    })
            })
            .transpose()?;
        if boards.len() != self.open_games.len() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "The tab list does not match the open boards.",
            ));
        }
        let mut seen = HashSet::new();
        let boards = boards
            .iter()
            .map(|game| {
                let open = self
                    .open_games
                    .get(game)
                    .filter(|_| seen.insert(game))
                    .ok_or_else(|| {
                        io::Error::new(
                            ErrorKind::InvalidInput,
                            "The tab list must name each open board once.",
                        )
                    })?;
                Ok(SavedBoard {
                    game: open.editor.game().clone(),
                    place: open.editor.place().0.clone(),
                    origin: open.file.clone(),
                })
            })
            .collect::<io::Result<_>>()?;
        Ok(BoardSession { boards, selected })
    }
}

#[derive(Deserialize)]
struct SessionRecord {
    boards: Vec<Value>,
    #[serde(default)]
    selected: Value,
}

#[derive(Default)]
struct ReopenedBoards {
    boards: Vec<(usize, OpenGame)>,
    selected: Option<usize>,
    warning: Option<String>,
    pending_backup: Option<PathBuf>,
}

impl ReopenedBoards {
    fn read(directory: &Path) -> Self {
        let path = directory.join("boards.json");
        let mut restored = Self::default();
        let record = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return restored,
            Ok(entry) if entry.is_file() || (entry.is_symlink() && path.is_file()) => {
                File::open(&path)
                    .ok()
                    .and_then(|file| serde_json::from_reader(io::BufReader::new(file)).ok())
            }
            _ => None,
        };
        let problem = match record {
            Some(record) => restored.reopen(record),
            None => Some("Could not read the board session.".to_owned()),
        };
        if let Some(problem) = problem {
            restored.warning = Some(if let Ok(backup) = preserve_original(&path) {
                format!(
                    "{problem} The original file was kept at {}.",
                    backup.display()
                )
            } else {
                let warning = format!(
                    "{problem} The original file could not be moved and is still at {}.",
                    path.display()
                );
                restored.pending_backup = Some(path);
                warning
            });
        }
        restored
    }

    fn reopen(&mut self, record: SessionRecord) -> Option<String> {
        let count = record.boards.len();
        self.selected = record
            .selected
            .as_u64()
            .and_then(|number| usize::try_from(number).ok())
            .filter(|&index| index < count);
        self.boards = record
            .boards
            .into_iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let board: SavedBoard = serde_json::from_value(value).ok()?;
                if board
                    .origin
                    .as_ref()
                    .is_some_and(|origin| !origin.path.is_absolute())
                {
                    return None;
                }
                let mut editor = GameEditor::new(board.game);
                editor.go_to(Place(board.place)).ok()?;
                Some((
                    index,
                    OpenGame {
                        editor,
                        file: board.origin,
                    },
                ))
            })
            .collect();
        match count - self.boards.len() {
            0 if !record.selected.is_null() && self.selected.is_none() => {
                Some("Could not restore the selected board.".to_owned())
            }
            0 => None,
            1 => Some("1 board could not be reopened.".to_owned()),
            lost => Some(format!("{lost} boards could not be reopened.")),
        }
    }
}

fn preserve_original(path: &Path) -> io::Result<PathBuf> {
    let kind = fs::symlink_metadata(path)?.file_type();
    if !kind.is_file() && !kind.is_symlink() {
        return Err(io::Error::other("The board session is not a regular file."));
    }
    let mut number = 1_u64;
    loop {
        let backup = path.with_file_name(format!("boards-recovery-{number}.json"));
        // Reserve the destination before rename, which would otherwise replace an earlier backup.
        match File::create_new(&backup) {
            Ok(reservation) => drop(reservation),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                number += 1;
                continue;
            }
            Err(error) => return Err(error),
        }
        if let Err(error) = fs::rename(path, &backup) {
            let _cleanup = fs::remove_file(backup);
            return Err(error);
        }
        return Ok(backup);
    }
}

impl BoardSession {
    fn write(&self, directory: &Path) -> io::Result<()> {
        fs::create_dir_all(directory)?;
        let contents = serde_json::to_vec(self)?;
        // Another process may be quitting, or a crash may have left its stage behind.
        let mut attempt = 0_u64;
        let (staged, mut file) = loop {
            let path = directory.join(format!("boards-{}-{attempt}.tmp", process::id()));
            match File::create_new(&path) {
                Ok(file) => break (path, file),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => attempt += 1,
                Err(error) => return Err(error),
            }
        };
        let written = io::Write::write_all(&mut file, &contents).and_then(|()| file.sync_all());
        drop(file);
        let published = written.and_then(|()| fs::rename(&staged, directory.join("boards.json")));
        if published.is_err() {
            // Preserve the write or rename error even if the stage cannot be removed.
            let _cleanup = fs::remove_file(staged);
        }
        published
    }
}

/// PGN keeps the game model independent of the session's JSON format.
pub(super) mod game_pgn {
    use std::io;

    use serde::{Deserialize, Deserializer, Serializer, de, ser};
    use zobrist_core::game::Game;
    use zobrist_core::pgn::{self, Entry};

    use crate::ipc::pgn_text;

    fn read(text: &str) -> io::Result<Game> {
        let mut entries = Vec::new();
        pgn::read_games(text.as_bytes(), |entry| entries.push(entry))?;
        if entries.len() == 1
            && let Some(Entry::Game { game, problem, .. }) = entries.pop()
            // The writer can preserve an invalid FEN in an empty game. Other damaged
            // move text would lose everything after its first problem on reopening.
            && (problem.is_none() || pgn_text(&game)? == text)
        {
            return Ok(*game);
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The board session contains an incomplete game.",
        ))
    }

    pub(in crate::ipc) fn serialize<S: Serializer>(
        game: &Game,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let pgn = pgn_text(game).map_err(ser::Error::custom)?;
        if read(&pgn).map_err(ser::Error::custom)? != *game {
            return Err(ser::Error::custom(
                "The game cannot be reopened from its PGN.",
            ));
        }
        serializer.serialize_str(&pgn)
    }

    pub(in crate::ipc) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Game, D::Error> {
        read(&String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
