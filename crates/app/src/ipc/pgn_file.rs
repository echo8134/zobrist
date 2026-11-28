//! What the page Files asks for: the games of a PGN file, and a new, empty PGN file.

use std::env;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;
use zobrist_store::{PgnGameRow, list_pgn_games};

use super::{FirstStep, Games, Response};

pub(super) fn open(typed: String) -> FirstStep {
    FirstStep::blocking_work(move || {
        let answer = opened(&resolve(&typed));
        move |_: &mut Games| Response::OpenPgnFile { answer }
    })
}

pub(super) fn create(typed: String) -> FirstStep {
    FirstStep::blocking_work(move || {
        let answer = created(&resolve(&typed));
        move |_: &mut Games| Response::CreatePgnFile { answer }
    })
}

/// A PGN file with the list of its games, or why there is none.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum PgnFileAnswer {
    /// The file and the list of its games.
    File(PgnFile),
    /// Rust did not open or create the file.
    Refusal {
        /// Why.
        reason: RefusalReason,
        /// What the system said, when it said more than the reason.
        system: Option<String>,
    },
}

/// A PGN file and the rows shown in its game table.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct PgnFile {
    /// The path on disk.
    pub(super) path: String,
    rows: Vec<GameRow>,
    problems: usize,
    /// Binds rows eligible for deletion to the bytes read from their source.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    version: Option<String>,
}

/// Why Rust did not open or create a PGN file.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RefusalReason {
    /// The file cannot be read.
    CannotRead,
    /// Something is at the path already, so nothing was created.
    ExistsAlready,
    /// The file cannot be created.
    CannotCreate,
}

/// One game in the list of a file. A missing tag is the text `?`.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct GameRow {
    // JSON carries the number as a number, and no file of the maintainer has 2^53 games.
    #[ts(type = "number")]
    number: u64,
    white: String,
    black: String,
    result: String,
    date: String,
    event: String,
}

impl From<PgnGameRow> for GameRow {
    fn from(row: PgnGameRow) -> Self {
        Self {
            number: row.number,
            white: row.white,
            black: row.black,
            result: row.result,
            date: row.date,
            event: row.event,
        }
    }
}

pub(super) fn opened(path: &Path) -> PgnFileAnswer {
    match list_pgn_games(path) {
        Ok((rows, problems, version)) => PgnFileAnswer::File(PgnFile {
            path: path.to_string_lossy().into_owned(),
            version: (!rows.is_empty()).then_some(version),
            rows: rows.into_iter().map(GameRow::from).collect(),
            problems,
        }),
        Err(error) => PgnFileAnswer::Refusal {
            reason: RefusalReason::CannotRead,
            system: Some(error.to_string()),
        },
    }
}

pub(super) fn created(path: &Path) -> PgnFileAnswer {
    // `create_new` fails when anything is at the path, so it never replaces a file.
    match File::create_new(path) {
        Ok(_) => PgnFileAnswer::File(PgnFile {
            path: path.to_string_lossy().into_owned(),
            version: None,
            rows: Vec::new(),
            problems: 0,
        }),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => PgnFileAnswer::Refusal {
            reason: RefusalReason::ExistsAlready,
            system: None,
        },
        Err(error) => PgnFileAnswer::Refusal {
            reason: RefusalReason::CannotCreate,
            system: Some(error.to_string()),
        },
    }
}

/// The path as the maintainer typed it, without white space at its ends. `~/` at its start is
/// the home folder.
pub(super) fn resolve(typed: &str) -> PathBuf {
    let typed = typed.trim();
    match (typed.strip_prefix("~/"), env::home_dir()) {
        (Some(below_home), Some(home)) => home.join(below_home),
        (Some(_), None) | (None, _) => PathBuf::from(typed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_starts_with_a_tilde_and_a_slash_is_below_the_home_folder() {
        let home = env::home_dir().unwrap();
        assert_eq!(
            resolve(" ~/games/mine.pgn "),
            home.join("games").join("mine.pgn")
        );
        assert_eq!(resolve("~mine.pgn"), PathBuf::from("~mine.pgn"));
        assert_eq!(
            resolve("games/~/mine.pgn"),
            PathBuf::from("games/~/mine.pgn")
        );
    }
}
