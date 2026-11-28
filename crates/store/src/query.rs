use std::cmp::Ordering;
use std::num::{NonZeroU64, NonZeroUsize};
use std::str::FromStr;

use zobrist_core::game::Game;

use crate::{Database, OpenError, PgnGameRow};

/// The column that orders a game list.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GameColumn {
    /// The original one-based game number.
    #[default]
    Number,
    /// The White player.
    White,
    /// The Black player.
    Black,
    /// The result marker.
    Result,
    /// The PGN date.
    Date,
    /// The event name.
    Event,
}

impl FromStr for GameColumn {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "number" => Ok(Self::Number),
            "white" => Ok(Self::White),
            "black" => Ok(Self::Black),
            "result" => Ok(Self::Result),
            "date" => Ok(Self::Date),
            "event" => Ok(Self::Event),
            _ => Err("expected number, white, black, result, date or event"),
        }
    }
}

/// All supplied filters must match the same game.
#[derive(Debug, Default)]
pub struct GameQuery {
    /// Case-insensitive substring of either player's name.
    pub player: Option<String>,
    /// Case-insensitive substring of the event.
    pub event: Option<String>,
    /// The exact four-digit year of the PGN date.
    pub year: Option<String>,
    /// The exact result marker.
    pub result: Option<String>,
    /// The column to order matches by.
    pub sort: GameColumn,
    /// Put larger values first. Ties always keep their original order.
    pub descending: bool,
}

impl GameQuery {
    fn compare(&self, left: &PgnGameRow, right: &PgnGameRow) -> Ordering {
        let order = match self.sort {
            GameColumn::Number => left.number.cmp(&right.number),
            GameColumn::White => left.white.cmp(&right.white),
            GameColumn::Black => left.black.cmp(&right.black),
            GameColumn::Result => left.result.cmp(&right.result),
            GameColumn::Date => left.date.cmp(&right.date),
            GameColumn::Event => left.event.cmp(&right.event),
        };
        let order = if self.descending {
            order.reverse()
        } else {
            order
        };
        order.then_with(|| left.number.cmp(&right.number))
    }
}

/// One page of a collection query, or all matches when no page was requested.
#[derive(Debug)]
pub struct GameList {
    /// Rows in the requested order, numbered as in the collection.
    pub rows: Vec<PgnGameRow>,
    /// The number of matches across all pages.
    pub total: usize,
}

impl Database {
    /// Searches headers across the collection. Pages contain up to 100 matches and count from 1.
    pub fn query(
        &mut self,
        query: &GameQuery,
        page: Option<NonZeroUsize>,
    ) -> Result<GameList, OpenError> {
        const PAGE_SIZE: usize = 100;
        let offset = page.map_or(0, |page| (page.get() - 1).saturating_mul(PAGE_SIZE));
        let end = page.map_or(usize::MAX, |_| offset.saturating_add(PAGE_SIZE));
        let stored_order = query.sort == GameColumn::Number && !query.descending;
        let player = query.player.as_deref().map(str::to_lowercase);
        let event = query.event.as_deref().map(str::to_lowercase);
        let matches = |row: &PgnGameRow| {
            player.as_ref().is_none_or(|name| {
                row.white.to_lowercase().contains(name) || row.black.to_lowercase().contains(name)
            }) && event
                .as_ref()
                .is_none_or(|text| row.event.to_lowercase().contains(text))
                && query.year.as_deref().is_none_or(|year| {
                    year.len() == 4
                        && year.bytes().all(|digit| digit.is_ascii_digit())
                        && row.date.split('.').next() == Some(year)
                })
                && query
                    .result
                    .as_ref()
                    .is_none_or(|result| row.result == *result)
        };
        let mut decoder = self.decoder()?;
        let mut rows = Vec::new();
        let mut total = 0;
        for number in 1.. {
            let Some(row) = decoder.next_row(number)? else {
                break;
            };
            if !matches(&row) {
                continue;
            }
            if !stored_order || (offset..end).contains(&total) {
                rows.push(row);
            }
            total += 1;
            // Only the best matches through the end of the requested page can still appear.
            if !stored_order && rows.len() / 2 >= end {
                rows.select_nth_unstable_by(end, |a, b| query.compare(a, b));
                rows.truncate(end);
            }
        }
        if !stored_order {
            rows.sort_unstable_by(|a, b| query.compare(a, b));
            rows.truncate(end);
            rows.drain(..offset.min(rows.len()));
        }
        Ok(GameList { rows, total })
    }

    /// Reads the complete game with this original number, or `None` past the last game.
    pub fn game(&mut self, number: NonZeroU64) -> Result<Option<Game>, OpenError> {
        for (index, game) in (1..).zip(self.games()?) {
            let game = game?;
            if index == number.get() {
                return Ok(Some(game));
            }
        }
        Ok(None)
    }
}
