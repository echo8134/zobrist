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

/// An operation on the board tabs kept across app restarts.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum SessionRequest {
    /// Load the board session at startup.
    RestoreBoards,
    /// Keep the settled boards for the next start.
    CheckpointBoards {
        /// Open game numbers in tab order.
        boards: Vec<usize>,
        /// The selected open game, when a board is shown.
        selected: Option<usize>,
    },
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

#[derive(PartialEq, Eq, Serialize)]
struct BoardSession {
    boards: Vec<SavedBoard>,
    selected: Option<usize>,
}

#[derive(PartialEq, Eq, Deserialize, Serialize)]
struct SavedBoard {
    #[serde(with = "game_pgn")]
    game: Game,
    place: Vec<usize>,
    origin: Option<InFile>,
}

impl From<&OpenGame> for SavedBoard {
    fn from(open: &OpenGame) -> Self {
        Self {
            game: open.editor.game().clone(),
            place: open.editor.place().0.clone(),
            origin: open.file.clone(),
        }
    }
}

#[derive(Default)]
pub(super) struct SessionFile {
    published: Option<BoardSession>,
    pending_backup: Option<PathBuf>,
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
        let mut file = self.one_save.lock().await;
        let quitting = matches!(request, SessionRequest::QuitWithBoards { .. });
        match request {
            SessionRequest::RestoreBoards => {
                if let Some(&selected) = self.restored_selection.get() {
                    return Ok(self.games.lock().await.board_pictures(selected, None));
                }
                let restored = spawn_blocking(move || ReopenedBoards::read(&directory))
                    .await
                    .map_err(io::Error::other)?;
                *file = restored.file;
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
            SessionRequest::CheckpointBoards { boards, selected }
            | SessionRequest::QuitWithBoards { boards, selected } => {
                let snapshot = self.games.lock().await.snapshot(&boards, selected)?;
                let response = if quitting {
                    Response::QuitWithBoards
                } else {
                    Response::CheckpointBoards
                };
                let unchanged = file.published.as_ref().map_or_else(
                    || !quitting && snapshot.boards.is_empty(),
                    |saved| *saved == snapshot,
                );
                if unchanged && file.pending_backup.is_none() {
                    return Ok(response);
                }
                if let Some(path) = file.pending_backup.clone() {
                    let result = spawn_blocking(move || preserve_original(&path))
                        .await
                        .map_err(io::Error::other)?;
                    match result {
                        Ok(_) => (),
                        Err(error) if error.kind() == ErrorKind::NotFound => (),
                        Err(error) => return Err(error),
                    }
                    file.pending_backup = None;
                }
                file.published = Some(
                    spawn_blocking(move || snapshot.write(&directory))
                        .await
                        .map_err(io::Error::other)??,
                );
                Ok(response)
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
                Ok(SavedBoard::from(open))
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
    file: SessionFile,
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
        let snapshot = BoardSession {
            boards: restored
                .boards
                .iter()
                .map(|(_, open)| SavedBoard::from(open))
                .collect(),
            selected: restored
                .boards
                .iter()
                .position(|(index, _)| Some(*index) == restored.selected),
        };
        if let Some(problem) = problem {
            let mut location = if let Ok(backup) = preserve_original(&path) {
                format!("The original file was kept at {}.", backup.display())
            } else {
                let location = format!(
                    "The original file could not be moved and is still at {}.",
                    path.display()
                );
                restored.file.pending_backup = Some(path);
                location
            };
            if restored.file.pending_backup.is_none() && !snapshot.boards.is_empty() {
                match snapshot.write(directory) {
                    Ok(saved) => restored.file.published = Some(saved),
                    Err(_) => {
                        location.push_str(" The reopened boards could not be saved for restart.");
                    }
                }
            }
            restored.warning = Some(format!(
                "{problem} {location} Your PGN files are untouched. You can open a board and keep playing."
            ));
        } else {
            restored.file.published = Some(snapshot);
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
                Some((index, OpenGame::new(editor, board.origin)))
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
    fn write(self, directory: &Path) -> io::Result<Self> {
        let staged = self.stage(directory)?;
        if let Err(error) = fs::rename(&staged, directory.join("boards.json")) {
            let _cleanup = fs::remove_file(staged);
            return Err(error);
        }
        Ok(self)
    }

    fn stage(&self, directory: &Path) -> io::Result<PathBuf> {
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
        if let Err(error) = written {
            let _cleanup = fs::remove_file(staged);
            return Err(error);
        }
        Ok(staged)
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
