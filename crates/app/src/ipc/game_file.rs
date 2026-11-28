//! A game of a PGN file as an open game of the app: opening it, and saving it into a file.

use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zobrist_core::editor::GameEditor;
use zobrist_core::game::{Game, MainLine};
use zobrist_store::{add_pgn_game, read_pgn_game, replace_pgn_game};

use super::{FirstStep, GamePicture, Games, Response, pgn_file};

/// A game that is open in the app.
pub(super) struct OpenGame {
    pub(super) position: super::analysis::BoardPosition,
    pub(super) analysis: Arc<Mutex<super::analysis::BoardAnalysis>>,
    pub(super) editor: GameEditor,
    /// Where the game came from. A game that was never in a file has none.
    pub(super) file: Option<InFile>,
}

/// Where an open game came from, with the game as that file holds it.
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct InFile {
    /// The file with every symbolic link followed, so that two paths to one file are equal.
    pub(super) path: PathBuf,
    number: NonZeroU64,
    /// The game as it was read from the file, or as it was last written into it.
    #[serde(with = "super::session::game_pgn")]
    held: Game,
}

impl OpenGame {
    pub(super) fn new(editor: GameEditor, file: Option<InFile>) -> Self {
        Self {
            position: super::analysis::BoardPosition::new(&editor),
            analysis: Arc::default(),
            editor,
            file,
        }
    }

    pub(super) fn picture(&self) -> GamePicture {
        GamePicture::from_editor(&self.editor, self.file.as_ref(), self.position.revision)
    }

    pub(super) fn edit<T>(&mut self, change: impl FnOnce(&mut GameEditor) -> T) -> T {
        let result = change(&mut self.editor);
        self.position.follow(&self.editor);
        result
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
    FirstStep::blocking_work(move || {
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

    /// The last step of a save. The game as the file holds it becomes the game that was
    /// written, which is not the game of the editor when a request changed that one meanwhile.
    fn saved(&mut self, game: usize, written: Result<InFile, Refused>) -> Option<SaveGameAnswer> {
        let file = match written {
            Ok(file) => file,
            Err(Refused { reason, system }) => {
                return Some(SaveGameAnswer::Refusal { reason, system });
            }
        };
        let open = self.open_games.get_mut(&game)?;
        open.file = Some(file);
        Some(SaveGameAnswer::Saved {
            picture: Box::new(open.picture()),
        })
    }
}

/// The saved game, or why Rust did not save it.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum SaveGameAnswer {
    /// The file holds the game.
    Saved {
        /// The picture of the game, which names its file and its number there.
        picture: Box<GamePicture>,
    },
    /// Rust did not save the game.
    Refusal {
        /// Why.
        reason: GameFileRefusal,
        /// What the system said, when it said more than the reason.
        system: Option<String>,
    },
}

pub(super) fn save(games: &Games, game: usize) -> FirstStep {
    let Some(open) = games.open_games.get(&game) else {
        return FirstStep::Answer(Box::new(Response::SaveGame { answer: None }));
    };
    let Some(file) = &open.file else {
        return FirstStep::Answer(Box::new(Response::SaveGame {
            answer: Some(SaveGameAnswer::Refusal {
                reason: GameFileRefusal::NoFile,
                system: None,
            }),
        }));
    };
    let (path, number, held) = (file.path.clone(), file.number, file.held.clone());
    let to_write = open.editor.game().clone();
    FirstStep::blocking_work(move || {
        let written = write_in_place(&path, number, &held, &to_write).map(|()| InFile {
            path,
            number,
            held: to_write,
        });
        move |games: &mut Games| Response::SaveGame {
            answer: games.saved(game, written),
        }
    })
}

/// Replaces game `number` of the file, but only while it is the game that the app read there
/// or last wrote there. A file that another program changed would lose a game without a sign.
fn write_in_place(
    path: &Path,
    number: NonZeroU64,
    held: &Game,
    game: &Game,
) -> Result<(), Refused> {
    let in_file = read_pgn_game(path, number).map_err(|error| Refused::cannot_read(&error))?;
    if in_file.as_ref() != Some(held) {
        return Err(Refused::without_text(GameFileRefusal::FileChanged));
    }
    replace_pgn_game(path, number, game).map_err(|error| Refused {
        // The function refuses with this kind when it cannot place the game or when the
        // new file would not read back.
        reason: if error.kind() == io::ErrorKind::InvalidInput {
            GameFileRefusal::CannotReplace
        } else {
            GameFileRefusal::CannotWrite
        },
        system: Some(error.to_string()),
    })
}

pub(super) fn save_as(games: &Games, game: usize, typed: String) -> FirstStep {
    let Some(open) = games.open_games.get(&game) else {
        return FirstStep::Answer(Box::new(Response::SaveGameAs { answer: None }));
    };
    let to_write = open.editor.game().clone();
    FirstStep::blocking_work(move || {
        let written = write_as_last(&typed, &to_write).map(|(path, number)| InFile {
            path,
            number,
            held: to_write,
        });
        move |games: &mut Games| Response::SaveGameAs {
            answer: games.saved(game, written),
        }
    })
}

/// Adds the game as the last game of the file. Its number is the number of games that the
/// file then has.
fn write_as_last(typed: &str, game: &Game) -> Result<(PathBuf, NonZeroU64), Refused> {
    let path = pgn_file::resolve(typed);
    let number = add_pgn_game(&path, game).map_err(|error| Refused {
        reason: GameFileRefusal::CannotWrite,
        system: Some(error.to_string()),
    })?;
    let path = path
        .canonicalize()
        .map_err(|error| Refused::cannot_read(&error))?;
    Ok((path, number))
}
