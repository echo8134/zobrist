//! The board session kept in the desktop app's local data directory.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
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

#[derive(Deserialize, Serialize)]
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
        let _one_save = self.one_save.lock().await;
        match request {
            SessionRequest::RestoreBoards => {
                if let Some(current) = self
                    .games
                    .lock()
                    .await
                    .live_boards(&self.restored_selection)
                {
                    return Ok(current);
                }
                let (boards, selected) = spawn_blocking(move || BoardSession::read(&directory))
                    .await
                    .map_err(io::Error::other)??;
                let mut games = self.games.lock().await;
                if let Some(current) = games.live_boards(&self.restored_selection) {
                    return Ok(current);
                }
                let mut selected_game = None;
                for (index, open) in boards.into_iter().enumerate() {
                    let game = games.insert(open);
                    if selected == Some(index) {
                        selected_game = Some(game);
                    }
                }
                self.restored_selection.get_or_init(|| selected_game);
                Ok(games.board_pictures(selected_game))
            }
            SessionRequest::QuitWithBoards { boards, selected } => {
                let snapshot = self.games.lock().await.snapshot(&boards, selected)?;
                spawn_blocking(move || snapshot.write(&directory))
                    .await
                    .map_err(io::Error::other)??;
                Ok(Response::QuitWithBoards)
            }
        }
    }
}

impl Games {
    fn live_boards(&self, restored_selection: &OnceLock<Option<usize>>) -> Option<Response> {
        if let Some(&selected) = restored_selection.get() {
            Some(self.board_pictures(selected))
        } else if self.next_number > 1 {
            // A game opened or closed while the session was loading belongs to this run.
            restored_selection.get_or_init(|| None);
            Some(self.board_pictures(None))
        } else {
            None
        }
    }

    fn board_pictures(&self, selected: Option<usize>) -> Response {
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

impl BoardSession {
    fn read(directory: &Path) -> io::Result<(Vec<OpenGame>, Option<usize>)> {
        let path = directory.join("boards.json");
        let contents = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok((Vec::new(), None)),
            Err(error) => return Err(error),
        };
        let session: Self = serde_json::from_reader(io::BufReader::new(contents))?;
        if session
            .selected
            .is_some_and(|index| index >= session.boards.len())
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "The saved board selection is out of range.",
            ));
        }
        let mut origins = HashSet::new();
        for origin in session
            .boards
            .iter()
            .filter_map(|board| board.origin.as_ref())
        {
            if !origin.path.is_absolute() || !origins.insert((&origin.path, origin.number)) {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Invalid file origin in the board session.",
                ));
            }
        }
        let mut boards = Vec::with_capacity(session.boards.len());
        for board in session.boards {
            let mut editor = GameEditor::new(board.game);
            editor.go_to(Place(board.place)).map_err(|_| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    "The saved move does not exist in the game.",
                )
            })?;
            boards.push(OpenGame {
                editor,
                file: board.origin,
            });
        }
        Ok((boards, session.selected))
    }

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
