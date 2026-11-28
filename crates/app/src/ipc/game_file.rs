//! A game of a PGN file as an open game of the app: opening it, and saving it into a file.

use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zobrist_core::editor::GameEditor;
use zobrist_core::game::{Game, MainLine};
use zobrist_store::{
    PgnHistory, PgnWriteConflict, add_pgn_game, delete_pgn_game, read_pgn_game, replace_pgn_game,
};

use super::{FirstStep, GamePicture, Games, Response, file_collection::history_at, pgn_file};

#[derive(Clone, Default)]
pub(super) enum SaveHistory {
    #[default]
    Disabled,
    Directory(PathBuf),
    Unavailable(String),
}

impl SaveHistory {
    fn capture(&self) -> Option<PgnHistory<'_>> {
        match self {
            Self::Directory(root) => Some(history_at(root)),
            Self::Unavailable(_) | Self::Disabled => None,
        }
    }
}

/// A game that is open in the app.
pub(super) struct OpenGame {
    pub(super) position: super::analysis::BoardPosition,
    pub(super) analysis: Arc<Mutex<super::analysis::BoardAnalysis>>,
    pub(super) editor: GameEditor,
    pub(super) file: GameSource,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub(super) enum GameSource {
    #[serde(rename = "deleted")]
    Deleted,
    // Existing board sessions store a file object or null in the origin field.
    #[serde(untagged)]
    Listed(Box<InFile>),
    #[default]
    #[serde(untagged)]
    New,
}

/// Where an open game came from, with the game as that file holds it.
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct InFile {
    /// The file with every symbolic link followed, so that two paths to one file are equal.
    pub(super) path: PathBuf,
    number: NonZeroU64,
    /// The game as it was read from the file, or as it was last written into it.
    #[serde(with = "super::session::game_pgn")]
    held: Game,
}

impl OpenGame {
    pub(super) fn new(editor: GameEditor, file: GameSource) -> Self {
        Self {
            position: super::analysis::BoardPosition::new(&editor),
            analysis: Arc::default(),
            editor,
            file,
        }
    }

    pub(super) fn picture(&self) -> GamePicture {
        GamePicture::from_editor(&self.editor, &self.file, self.position.revision)
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
    pub(super) fn of(game: &Game, source: &GameSource) -> Self {
        match source {
            GameSource::Listed(file) => Self {
                file: Some(GameFile::from(file.as_ref())),
                unsaved: *game != file.held,
            },
            GameSource::Deleted => Self {
                file: None,
                unsaved: true,
            },
            GameSource::New => Self {
                file: None,
                unsaved: !game.tags().is_empty()
                    || !matches!(
                        game.main_line(), MainLine::Empty { comments } if comments.is_empty()
                    ),
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

impl From<&InFile> for GameFile {
    fn from(file: &InFile) -> Self {
        Self {
            path: file.path.to_string_lossy().into_owned(),
            number: file.number.get(),
        }
    }
}

/// The result of the player's confirmed deletion.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum DeletePgnGameAnswer {
    /// The PGN and its earlier state have been published.
    Deleted {
        /// Open boards whose row changed or was removed.
        tabs: Vec<ChangedGameFile>,
    },
    /// The PGN was left alone.
    Refusal {
        /// Why the game could not be deleted.
        message: String,
    },
}

/// A tab affected by the renumbering of a PGN.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct ChangedGameFile {
    game: usize,
    previous: GameFile,
    #[serde(flatten)]
    saved: SavedState,
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
    /// Another change holds the history lock beyond the wait allowed for a save.
    ChangeInProgress,
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

    fn writing(error: &io::Error, replacing: Option<&Path>) -> Self {
        if let Some(conflict) = PgnWriteConflict::from_io_error(error) {
            return Self::without_text(match conflict {
                PgnWriteConflict::GameChanged => GameFileRefusal::FileChanged,
                PgnWriteConflict::HistoryLocked => GameFileRefusal::ChangeInProgress,
            });
        }
        let mut reason = GameFileRefusal::CannotWrite;
        if let Some(path) = replacing {
            if let Err(read_error) = std::fs::File::open(path) {
                return Self::cannot_read(&read_error);
            }
            if error.kind() == io::ErrorKind::InvalidInput {
                reason = GameFileRefusal::CannotReplace;
            }
        }
        Self {
            reason,
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
            matches!(&open.file, GameSource::Listed(other)
                 if other.path == file.path && other.number == file.number)
        });
        let (game, picture) = match open_already {
            Some((game, open)) => (*game, open.picture()),
            None => self.open(file.held.clone(), GameSource::Listed(Box::new(file))),
        };
        OpenGameAnswer::Game {
            game,
            picture: Box::new(picture),
        }
    }

    /// The last step of a save. The game as the file holds it becomes the game that was
    /// written, which is not the game of the editor when a request changed that one meanwhile.
    fn saved(
        &mut self,
        game: usize,
        written: Result<(InFile, Option<String>), Refused>,
        history: &SaveHistory,
    ) -> Option<SaveGameAnswer> {
        let (file, warning) = match written {
            Ok(saved) => saved,
            Err(Refused { reason, system }) => {
                return Some(SaveGameAnswer::Refusal { reason, system });
            }
        };
        let open = self.open_games.get_mut(&game)?;
        open.file = GameSource::Listed(Box::new(file));
        let picture = Box::new(open.picture());
        let warning = match history {
            SaveHistory::Unavailable(reason) => Some(format!(
                "Game saved. Could not keep the earlier state. {reason}"
            )),
            SaveHistory::Directory(_) | SaveHistory::Disabled => warning,
        };
        Some(match warning {
            Some(warning) => SaveGameAnswer::SavedWithWarning { picture, warning },
            None => SaveGameAnswer::Saved { picture },
        })
    }
}

/// The row the player selected in a particular version of a PGN file.
#[derive(Clone, Deserialize, TS)]
pub struct ListedPgnGame {
    path: String,
    #[ts(type = "number")]
    number: NonZeroU64,
    version: String,
}

pub(super) fn delete(listed: ListedPgnGame, history: &SaveHistory) -> FirstStep {
    let history = history.clone();
    let number = listed.number;
    FirstStep::blocking_work(move || {
        let target = match &history {
            SaveHistory::Directory(root) => delete_pgn_game(
                &pgn_file::resolve(&listed.path),
                number,
                &listed.version,
                history_at(root),
            ),
            SaveHistory::Unavailable(reason) => Err(io::Error::other(format!(
                "Could not delete game {number}. The current bytes could not be kept. {reason}"
            ))),
            SaveHistory::Disabled => Err(io::Error::other(format!(
                "Could not delete game {number}. The current bytes could not be kept. No app data folder is available."
            ))),
        };
        move |games: &mut Games| {
            let answer = match target {
                Ok(path) => {
                    let tabs = games
                        .open_games
                        .iter_mut()
                        .filter_map(|(&game, open)| {
                            let GameSource::Listed(file) = &mut open.file else {
                                return None;
                            };
                            if file.path != path || file.number < number {
                                return None;
                            }
                            let previous = GameFile::from(file.as_ref());
                            if file.number == number {
                                open.file = GameSource::Deleted;
                            } else {
                                file.number = NonZeroU64::new(file.number.get() - 1)?;
                            }
                            Some(ChangedGameFile {
                                game,
                                previous,
                                saved: SavedState::of(open.editor.game(), &open.file),
                            })
                        })
                        .collect();
                    DeletePgnGameAnswer::Deleted { tabs }
                }
                Err(error) => DeletePgnGameAnswer::Refusal {
                    message: error.to_string(),
                },
            };
            Response::DeletePgnGame { answer }
        }
    })
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
    /// The PGN was saved but its earlier state could not be kept.
    SavedWithWarning {
        /// The picture of the saved game.
        picture: Box<GamePicture>,
        /// The reason history was unavailable.
        warning: String,
    },
    /// Rust did not save the game.
    Refusal {
        /// Why.
        reason: GameFileRefusal,
        /// What the system said, when it said more than the reason.
        system: Option<String>,
    },
}

pub(super) fn save(games: &Games, game: usize, history: &SaveHistory) -> FirstStep {
    let Some(open) = games.open_games.get(&game) else {
        return FirstStep::Answer(Box::new(Response::SaveGame { answer: None }));
    };
    let GameSource::Listed(file) = &open.file else {
        return FirstStep::Answer(Box::new(Response::SaveGame {
            answer: Some(SaveGameAnswer::Refusal {
                reason: GameFileRefusal::NoFile,
                system: None,
            }),
        }));
    };
    let (path, number, held) = (file.path.clone(), file.number, file.held.clone());
    let to_write = open.editor.game().clone();
    let history = history.clone();
    FirstStep::blocking_work(move || {
        let written =
            write_in_place(&path, number, &held, &to_write, history.capture()).map(|warning| {
                (
                    InFile {
                        path,
                        number,
                        held: to_write,
                    },
                    warning,
                )
            });
        move |games: &mut Games| Response::SaveGame {
            answer: games.saved(game, written, &history),
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
    history: Option<PgnHistory<'_>>,
) -> Result<Option<String>, Refused> {
    replace_pgn_game(path, number, game, Some(held), history)
        .map_err(|error| Refused::writing(&error, Some(path)))
}

pub(super) fn save_as(
    games: &Games,
    game: usize,
    typed: String,
    history: &SaveHistory,
) -> FirstStep {
    let Some(open) = games.open_games.get(&game) else {
        return FirstStep::Answer(Box::new(Response::SaveGameAs { answer: None }));
    };
    let to_write = open.editor.game().clone();
    let history = history.clone();
    FirstStep::blocking_work(move || {
        let written = write_as_last(&typed, to_write, history.capture());
        move |games: &mut Games| Response::SaveGameAs {
            answer: games.saved(game, written, &history),
        }
    })
}

/// Adds the game as the last game of the file. Its number is the number of games that the
/// file then has.
fn write_as_last(
    typed: &str,
    game: Game,
    history: Option<PgnHistory<'_>>,
) -> Result<(InFile, Option<String>), Refused> {
    let path = pgn_file::resolve(typed);
    let (number, warning) =
        add_pgn_game(&path, &game, history).map_err(|error| Refused::writing(&error, None))?;
    let path = path
        .canonicalize()
        .map_err(|error| Refused::cannot_read(&error))?;
    Ok((
        InFile {
            path,
            number,
            held: game,
        },
        warning,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_busy_write_as_a_system_error() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let busy = io::Error::new(io::ErrorKind::ResourceBusy, "Device or resource busy");
        let refusal = Refused::writing(&busy, Some(&path));
        assert_eq!(refusal.reason, GameFileRefusal::CannotWrite);
        assert_eq!(refusal.system.as_deref(), Some("Device or resource busy"));
    }
}
