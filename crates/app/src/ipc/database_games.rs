//! Browsing and opening games from a native collection on this device.

use std::io;
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use ts_rs::TS;
use zobrist_store::{GameColumn, GameQuery};

use super::database_collection::{database_error, open_database};
use super::{FirstStep, GameFileRefusal, Games, OpenGameAnswer, Response};

/// A filtered and sorted page of a collection, numbered from one.
#[derive(Deserialize, TS)]
pub struct DatabaseGameQuery {
    player: Option<String>,
    event: Option<String>,
    year: Option<String>,
    result: Option<String>,
    sort: DatabaseGameColumn,
    descending: bool,
    #[ts(type = "number")]
    page: NonZeroUsize,
}

#[derive(Deserialize, TS)]
enum DatabaseGameColumn {
    Number,
    White,
    Black,
    Result,
    Date,
    Event,
}

impl DatabaseGameQuery {
    pub(super) fn list(self, path: &Path) -> io::Result<Response> {
        let query = GameQuery {
            player: self.player,
            event: self.event,
            year: self.year,
            result: self.result,
            sort: match self.sort {
                DatabaseGameColumn::Number => GameColumn::Number,
                DatabaseGameColumn::White => GameColumn::White,
                DatabaseGameColumn::Black => GameColumn::Black,
                DatabaseGameColumn::Result => GameColumn::Result,
                DatabaseGameColumn::Date => GameColumn::Date,
                DatabaseGameColumn::Event => GameColumn::Event,
            },
            descending: self.descending,
        };
        let list = open_database(path)?
            .query(&query, Some(self.page))
            .map_err(database_error)?;
        Ok(Response::ListDatabaseGames {
            rows: list.rows.into_iter().map(Into::into).collect(),
            total: list.total,
        })
    }
}

pub(super) fn open_database_game(path: PathBuf, number: NonZeroU64) -> FirstStep {
    FirstStep::blocking_work(move || {
        let read = open_database(&path)
            .and_then(|mut database| database.game(number).map_err(database_error));
        move |games: &mut Games| Response::OpenDatabaseGame {
            answer: match read {
                Ok(Some(stored)) => {
                    let (game, picture) = games.open(stored, None);
                    OpenGameAnswer::Game {
                        game,
                        picture: Box::new(picture),
                    }
                }
                Ok(None) => OpenGameAnswer::Refusal {
                    reason: GameFileRefusal::NoSuchGame,
                    system: None,
                },
                Err(error) => OpenGameAnswer::Refusal {
                    reason: GameFileRefusal::CannotRead,
                    system: Some(error.to_string()),
                },
            },
        }
    })
}
