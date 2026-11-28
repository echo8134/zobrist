use std::fmt;

use serde_json::Value;

use crate::{Endpoint, FetchError, REQUEST_TIMEOUT, read_response, request};

/// The result with best play for the side that was to move at lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TablebaseOutcome {
    /// A forced win.
    Win,
    /// A draw with best play.
    Draw,
    /// A forced loss.
    Loss,
    /// A win prevented by the 50-move rule.
    CursedWin,
    /// A loss prevented by the 50-move rule.
    BlessedLoss,
    /// The service cannot settle if the 50-move rule prevents a win.
    WinOrDraw,
    /// The service cannot settle if the 50-move rule prevents a loss.
    LossOrDraw,
    /// The result is unknown.
    Unknown,
}

impl TablebaseOutcome {
    fn from_category(value: &Value) -> Result<Self, TablebaseError> {
        match value.as_str() {
            Some("win") => Ok(Self::Win),
            Some("loss") => Ok(Self::Loss),
            Some("draw") => Ok(Self::Draw),
            Some("cursed-win") => Ok(Self::CursedWin),
            Some("blessed-loss") => Ok(Self::BlessedLoss),
            Some("syzygy-win" | "maybe-win") => Ok(Self::WinOrDraw),
            Some("syzygy-loss" | "maybe-loss") => Ok(Self::LossOrDraw),
            Some("unknown") => Ok(Self::Unknown),
            _ => Err(TablebaseError::Unavailable),
        }
    }

    fn opposite(self) -> Self {
        match self {
            Self::Win => Self::Loss,
            Self::Loss => Self::Win,
            Self::CursedWin => Self::BlessedLoss,
            Self::BlessedLoss => Self::CursedWin,
            Self::WinOrDraw => Self::LossOrDraw,
            Self::LossOrDraw => Self::WinOrDraw,
            Self::Draw | Self::Unknown => self,
        }
    }
}

impl fmt::Display for TablebaseOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Win => "Win",
            Self::Draw => "Draw",
            Self::Loss => "Loss",
            Self::CursedWin => "Draw (win without the 50-move rule)",
            Self::BlessedLoss => "Draw (loss without the 50-move rule)",
            Self::WinOrDraw => "Win or draw (50-move rule)",
            Self::LossOrDraw => "Loss or draw (50-move rule)",
            Self::Unknown => "Unknown",
        })
    }
}

/// A legal move and the result it achieves for its player.
#[derive(Debug, Eq, PartialEq)]
pub struct TablebaseMove {
    /// Standard algebraic notation as supplied by Lichess.
    pub san: String,
    /// Outcome for the player making this move.
    pub outcome: TablebaseOutcome,
}

/// The position outcome and its legal moves in tablebase order.
#[derive(Debug, Eq, PartialEq)]
pub struct TablebaseResult {
    /// Outcome for the side to move.
    pub outcome: TablebaseOutcome,
    /// Moves ordered best first.
    pub moves: Vec<TablebaseMove>,
}

/// Why the endgame lookup could not return a result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TablebaseError {
    /// Lichess asked for a pause between requests.
    RateLimited,
    /// The connection, service or returned position data was unavailable.
    Unavailable,
}

impl fmt::Display for TablebaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited => FetchError::RateLimited.fmt(formatter),
            Self::Unavailable => {
                formatter.write_str("Lichess tablebase is unavailable. Try again.")
            }
        }
    }
}

impl std::error::Error for TablebaseError {}

/// Look up a standard chess position by its full FEN.
pub fn lookup_tablebase(fen: &str) -> Result<TablebaseResult, TablebaseError> {
    let bytes = request(
        Endpoint::Tablebase(fen),
        "application/json",
        Some(REQUEST_TIMEOUT),
        read_response,
    )
    .map_err(|error| {
        if error == FetchError::RateLimited {
            TablebaseError::RateLimited
        } else {
            TablebaseError::Unavailable
        }
    })?;
    parse_tablebase(&bytes)
}

fn parse_tablebase(bytes: &[u8]) -> Result<TablebaseResult, TablebaseError> {
    let position: Value = serde_json::from_slice(bytes).map_err(|_| TablebaseError::Unavailable)?;
    let outcome = TablebaseOutcome::from_category(&position["category"])?;
    let mut moves = Vec::new();
    for entry in position["moves"]
        .as_array()
        .ok_or(TablebaseError::Unavailable)?
    {
        let san = entry["san"].as_str().ok_or(TablebaseError::Unavailable)?;
        // A move's category describes the resulting position with the opponent to move.
        moves.push(TablebaseMove {
            san: san.to_owned(),
            outcome: TablebaseOutcome::from_category(&entry["category"])?.opposite(),
        });
    }
    Ok(TablebaseResult { outcome, moves })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_can_take_the_queen_to_draw_but_loses_by_retreating() {
        let reply = br#"{"category":"draw","moves":[
            {"uci":"e8d7","san":"Kxd7","category":"draw"},
            {"uci":"e8f8","san":"Kf8","category":"win"}
        ]}"#;
        let result = parse_tablebase(reply).unwrap();
        assert_eq!(result.outcome.to_string(), "Draw");
        assert_eq!(
            result
                .moves
                .iter()
                .map(|entry| (entry.san.as_str(), entry.outcome.to_string()))
                .collect::<Vec<_>>(),
            [("Kxd7", "Draw".into()), ("Kf8", "Loss".into())]
        );
    }

    #[test]
    fn the_fifty_move_rule_and_unknown_results_apply_to_both_sides() {
        for (category, before, after) in [
            ("win", "Win", "Loss"),
            ("loss", "Loss", "Win"),
            ("draw", "Draw", "Draw"),
            (
                "cursed-win",
                "Draw (win without the 50-move rule)",
                "Draw (loss without the 50-move rule)",
            ),
            (
                "blessed-loss",
                "Draw (loss without the 50-move rule)",
                "Draw (win without the 50-move rule)",
            ),
            (
                "syzygy-win",
                "Win or draw (50-move rule)",
                "Loss or draw (50-move rule)",
            ),
            (
                "syzygy-loss",
                "Loss or draw (50-move rule)",
                "Win or draw (50-move rule)",
            ),
            (
                "maybe-win",
                "Win or draw (50-move rule)",
                "Loss or draw (50-move rule)",
            ),
            (
                "maybe-loss",
                "Loss or draw (50-move rule)",
                "Win or draw (50-move rule)",
            ),
            ("unknown", "Unknown", "Unknown"),
        ] {
            let reply = format!(
                r#"{{"category":"{category}","moves":[{{"san":"Kf8","category":"{category}"}}]}}"#
            );
            let result = parse_tablebase(reply.as_bytes()).unwrap();
            assert_eq!(result.outcome.to_string(), before);
            assert_eq!(result.moves[0].outcome.to_string(), after);
        }
    }

    #[test]
    fn incomplete_replies_do_not_return_partial_results() {
        for bytes in [
            r#"{"category":"win"}"#,
            r#"{"category":"draw","moves":[{"san":"Kxd7","category":"draw"},{"san":"Kf8"}]}"#,
            r#"{"category":"unknown","moves":[{"category":"loss"}]}"#,
            r#"{"category":"misspelled","moves":[]}"#,
            "{}",
        ] {
            assert_eq!(
                parse_tablebase(bytes.as_bytes()),
                Err(TablebaseError::Unavailable)
            );
        }
    }
}
