use serde::Serialize;
use shakmaty::{Chess, EnPassantMode, Position, fen::Fen};
use ts_rs::TS;

use super::{FirstStep, Games, Response};

const MAX_PIECES: usize = 7;

/// The tablebase result for one legal move, from the moving side's perspective.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct TablebaseMove {
    san: String,
    outcome: String,
}

/// What the board shows after an explicit tablebase lookup.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum TablebaseAnswer {
    /// The position and legal moves, best moves first.
    Evaluated {
        /// The result for the side to move.
        outcome: String,
        /// The result for that same side after each move.
        moves: Vec<TablebaseMove>,
    },
    /// The position was ineligible or the site could not provide a result.
    Refused {
        /// The message to show on the board.
        reason: String,
    },
}

pub(super) fn unavailable_reason(position: &Chess) -> Option<String> {
    let count = position.board().occupied().count();
    (count > MAX_PIECES).then(|| {
        format!("This lookup supports at most {MAX_PIECES} pieces ({count} on the board).")
    })
}

impl Games {
    pub(super) fn tablebase(&self, game: usize, revision: usize) -> FirstStep {
        let refused = |reason| {
            FirstStep::Answer(Box::new(Response::LookupTablebase {
                answer: TablebaseAnswer::Refused { reason },
            }))
        };
        let Some(open) = self
            .open_games
            .get(&game)
            .filter(|open| open.position.revision == revision)
        else {
            return refused("The board position has changed.".to_owned());
        };
        let position = open.editor.position();
        if let Some(reason) = unavailable_reason(position) {
            return refused(reason);
        }
        let fen = Fen::from_position(position, EnPassantMode::Legal).to_string();
        FirstStep::blocking_work(move || {
            let answer = match zobrist_online::lookup_tablebase(&fen) {
                Ok(result) => TablebaseAnswer::Evaluated {
                    outcome: result.outcome.to_string(),
                    moves: result
                        .moves
                        .into_iter()
                        .map(|row| TablebaseMove {
                            san: row.san,
                            outcome: row.outcome.to_string(),
                        })
                        .collect(),
                },
                Err(error) => TablebaseAnswer::Refused {
                    reason: error.to_string(),
                },
            };
            move |_: &mut Games| Response::LookupTablebase { answer }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::Request;

    #[test]
    fn only_the_clicked_revision_with_at_most_seven_pieces_can_start_a_request() {
        let mut games = Games::default();
        for (fen, count) in [
            ("4k3/ppp5/8/8/8/8/PP6/4K3 w - - 0 1", 7),
            ("4k3/ppp5/8/8/8/8/PPP5/4K3 w - - 0 1", 8),
        ] {
            let opened = games
                .open_pgn(&format!("[SetUp \"1\"]\n[FEN \"{fen}\"]\n\n*"))
                .unwrap();
            let game = opened.game;
            let revision = opened.picture.revision;
            let request = Request::LookupTablebase { game, revision };
            if count == 7 {
                assert!(matches!(
                    games.begin(request, &crate::ipc::SaveHistory::Disabled),
                    FirstStep::BlockingWork(_)
                ));
            } else {
                assert_eq!(
                    games.answer(request),
                    Response::LookupTablebase {
                        answer: TablebaseAnswer::Refused {
                            reason: "This lookup supports at most 7 pieces (8 on the board)."
                                .to_owned()
                        }
                    }
                );
            }
            assert_eq!(
                games.answer(Request::LookupTablebase {
                    game,
                    revision: revision + 1
                }),
                Response::LookupTablebase {
                    answer: TablebaseAnswer::Refused {
                        reason: "The board position has changed.".to_owned()
                    }
                }
            );
        }
    }
}
