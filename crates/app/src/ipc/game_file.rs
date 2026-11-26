//! A game of a PGN file as an open game of the app: opening it, and saving it into a file.

use std::io;
use std::num::NonZeroU64;
use std::path::PathBuf;

use serde::Serialize;
use ts_rs::TS;
use zobrist_core::editor::GameEditor;
use zobrist_core::game::{Game, MainLine};
use zobrist_store::read_pgn_game;

use super::{FirstStep, GamePicture, Games, Response, pgn_file};

/// A game that is open in the app.
pub(super) struct OpenGame {
    pub(super) editor: GameEditor,
    /// Where the game came from. A game that was never in a file has none.
    pub(super) file: Option<InFile>,
}

/// Where an open game came from, with the game as that file holds it.
pub(super) struct InFile {
    /// The file with every symbolic link followed, so that two paths to one file are equal.
    path: PathBuf,
    number: NonZeroU64,
    /// The game as it was read from the file, or as it was last written into it.
    held: Game,
}

impl OpenGame {
    pub(super) fn picture(&self) -> GamePicture {
        GamePicture::from_editor(&self.editor, self.file.as_ref())
    }
}

/// What the picture of a game says about its file.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub(super) struct SavedState {
    /// Where the game came from. A game that was never in a file has none.
    file: Option<GameFile>,
    /// Whether the game has changes that are not saved.
    unsaved: bool,
}

impl SavedState {
    /// Compares the game with the game as its file holds it. The picture gets this each time
    /// anew, so it cannot say "saved" while the file holds another game.
    pub(super) fn of(game: &Game, file: Option<&InFile>) -> Self {
        Self {
            file: file.map(|file| GameFile {
                path: file.path.to_string_lossy().into_owned(),
                number: file.number.get(),
            }),
            unsaved: match file {
                Some(file) => *game != file.held,
                None => {
                    !game.tags().is_empty()
                        || !matches!(
                            game.main_line(),
                            MainLine::Empty { comments } if comments.is_empty()
                        )
                }
            },
        }
    }
}

/// Where a game came from: a PGN file and the number of the game in it.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct GameFile {
    path: String,
    // JSON carries the number as a number, and no file of the maintainer has 2^53 games.
    #[ts(type = "number")]
    number: u64,
}

/// Why Rust did not open a game of a file or save a game into one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameFileRefusal {
    /// The file cannot be read.
    CannotRead,
    /// The file has no game with that number.
    NoSuchGame,
    /// The game has no file.
    NoFile,
    /// The file changed since the game was opened or last saved.
    FileChanged,
    /// The game in the file cannot be replaced.
    CannotReplace,
    /// The file cannot be written.
    CannotWrite,
}

struct Refused {
    reason: GameFileRefusal,
    system: Option<String>,
}

impl Refused {
    fn without_text(reason: GameFileRefusal) -> Self {
        Self {
            reason,
            system: None,
        }
    }

    fn cannot_read(error: &io::Error) -> Self {
        Self {
            reason: GameFileRefusal::CannotRead,
            system: Some(error.to_string()),
        }
    }
}

/// The open game, or why Rust did not open one.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum OpenGameAnswer {
    /// The game is open. It was open already when the app had opened it before.
    Game {
        /// The game's number for this run of the app.
        game: usize,
        /// The picture at the place where the game stands.
        picture: Box<GamePicture>,
    },
    /// Rust did not open the game.
    Refusal {
        /// Why.
        reason: GameFileRefusal,
        /// What the system said, when it said more than the reason.
        system: Option<String>,
    },
}

pub(super) fn open(typed: String, number: u64) -> FirstStep {
    FirstStep::file_work(move || {
        let read = read(&typed, number);
        move |games: &mut Games| Response::OpenGame {
            answer: match read {
                Ok(file) => games.open_from_file(file),
                Err(Refused { reason, system }) => OpenGameAnswer::Refusal { reason, system },
            },
        }
    })
}

fn read(typed: &str, number: u64) -> Result<InFile, Refused> {
    let path = pgn_file::resolve(typed)
        .canonicalize()
        .map_err(|error| Refused::cannot_read(&error))?;
    let no_such_game = || Refused::without_text(GameFileRefusal::NoSuchGame);
    let number = NonZeroU64::new(number).ok_or_else(no_such_game)?;
    let held = read_pgn_game(&path, number)
        .map_err(|error| Refused::cannot_read(&error))?
        .ok_or_else(no_such_game)?;
    Ok(InFile { path, number, held })
}

impl Games {
    /// Two open games for one game of a file would save over each other, so a game that is
    /// open already is the answer.
    fn open_from_file(&mut self, file: InFile) -> OpenGameAnswer {
        let open_already = self.open_games.iter().find(|(_, open)| {
            open.file
                .as_ref()
                .is_some_and(|other| other.path == file.path && other.number == file.number)
        });
        let (game, picture) = match open_already {
            Some((game, open)) => (*game, open.picture()),
            None => self.open(file.held.clone(), Some(file)),
        };
        OpenGameAnswer::Game {
            game,
            picture: Box::new(picture),
        }
    }
}
