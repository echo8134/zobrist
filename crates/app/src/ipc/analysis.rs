//! Analysis of the position at the player's place in an open game.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove,
};
use tauri::async_runtime::spawn_blocking;
use ts_rs::TS;
use zobrist_core::{editor::GameEditor, game::Move};
use zobrist_engine::{Engine, Line, Position as EnginePosition, Score};

use super::{Response, SharedGames, engines::default_program};

/// A request about a board's engine, answered through the desktop command.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum AnalysisRequest {
    /// Control the search or read its current lines.
    Analysis {
        /// The open game that owns the analysis.
        game: usize,
        /// Identifies the run even when a stop overtakes its start.
        generation: usize,
        /// The operation for this run.
        action: AnalysisAction,
    },
}

/// An action for one run of the default engine.
#[derive(Deserialize, TS)]
pub enum AnalysisAction {
    /// Launch the default program.
    Start,
    /// Release the program and clear its lines.
    Stop,
    /// Read this run at the current position.
    Status,
}

/// The complete analysis answer for one board position and one run.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct AnalysisSnapshot {
    game: usize,
    revision: usize,
    generation: usize,
    state: AnalysisState,
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
enum AnalysisState {
    Stopped,
    Searching { lines: Vec<AnalysisLine> },
    Failed { message: String },
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct AnalysisLine {
    evaluation: String,
    notation: String,
}

impl AnalysisLine {
    fn at(line: Line, root: &Chess) -> Option<Self> {
        let evaluation = match line.score {
            Score::Centipawns(value) => {
                let white = root.turn().fold_wb(i64::from(value), -i64::from(value));
                let sign = if white < 0 { "−" } else { "+" };
                let magnitude = white.unsigned_abs();
                format!("{sign}{}.{:02}", magnitude / 100, magnitude % 100)
            }
            Score::Mate(value) => {
                let winner = if value > 0 { root.turn() } else { !root.turn() };
                let side = winner.fold_wb("White", "Black");
                format!("{side} mates in {}", value.unsigned_abs())
            }
        };
        let mut position = root.clone();
        let mut notation = String::new();
        for coordinates in line.moves {
            let chess_move = coordinates
                .parse::<UciMove>()
                .ok()?
                .to_move(&position)
                .ok()?;
            if !notation.is_empty() {
                notation.push(' ');
            }
            if position.turn() == Color::White {
                write!(notation, "{}. ", position.fullmoves()).ok()?;
            } else if notation.is_empty() {
                write!(notation, "{}... ", position.fullmoves()).ok()?;
            }
            write!(
                notation,
                "{}",
                SanPlus::from_move_and_play_unchecked(&mut position, chess_move)
            )
            .ok()?;
        }
        Some(Self {
            evaluation,
            notation,
        })
    }
}

pub(super) struct BoardPosition {
    pub(super) revision: usize,
    search: EnginePosition,
}

impl BoardPosition {
    pub(super) fn new(editor: &GameEditor) -> Self {
        Self {
            revision: 0,
            search: search_position(editor),
        }
    }

    pub(super) fn follow(&mut self, editor: &GameEditor) {
        let search = search_position(editor);
        if self.search != search {
            self.revision = self.revision.strict_add(1);
            self.search = search;
        }
    }
}

fn search_position(editor: &GameEditor) -> EnginePosition {
    let start = editor.game().start_position();
    let mode = start.castles().mode();
    let fen = Fen::from_position(start, EnPassantMode::Always);
    let mut command = if fen == Fen::default() {
        "position startpos".to_owned()
    } else {
        format!("position fen {fen}")
    };
    let moves = editor.moves_to_place();
    if !moves.is_empty() {
        command.push_str(" moves");
    }
    for played in moves {
        let Move::Legal(chess_move) = played else {
            // PGN threat annotations can pass a turn. UCI game histories contain legal moves.
            command = format!(
                "position fen {}",
                Fen::from_position(editor.position(), EnPassantMode::Always)
            );
            break;
        };
        command.push(' ');
        command.push_str(&UciMove::from_move(chess_move, mode).to_string());
    }
    EnginePosition {
        command,
        chess960: mode == CastlingMode::Chess960,
    }
}

#[derive(Default)]
pub(super) struct BoardAnalysis {
    generation: usize,
    run: Option<Result<Engine, String>>,
}

pub(super) fn close(analysis: &Mutex<BoardAnalysis>) {
    let mut analysis = analysis
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // An already dispatched Start can still hold a reference to this closed board.
    analysis.generation = usize::MAX;
    analysis.run = None;
}

impl AnalysisRequest {
    /// Snapshot the editor before doing file or process work on a blocking thread.
    pub async fn answer(self, games: &SharedGames, directory: PathBuf) -> tauri::Result<Response> {
        let Self::Analysis {
            game,
            generation,
            action,
        } = self;
        let target = games.games.lock().await.open_games.get(&game).map(|open| {
            (
                open.analysis.clone(),
                open.position.revision,
                open.position.search.clone(),
                open.editor.position().clone(),
            )
        });
        let Some((owner, revision, command, position)) = target else {
            return Ok(Response::Analysis {
                snapshot: AnalysisSnapshot {
                    game,
                    revision: 0,
                    generation,
                    state: AnalysisState::Stopped,
                },
            });
        };
        let state = spawn_blocking(move || {
            let mut analysis = owner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match action {
                AnalysisAction::Start if generation > analysis.generation => {
                    analysis.generation = generation;
                    analysis.run = None;
                    analysis.run = Some(
                        default_program(&directory)
                            .and_then(|path| Engine::start(&path, revision, command.clone()))
                            .map_err(|error| error.to_string()),
                    );
                }
                AnalysisAction::Stop if generation >= analysis.generation => {
                    analysis.generation = generation;
                    analysis.run = None;
                }
                AnalysisAction::Start | AnalysisAction::Stop | AnalysisAction::Status => {}
            }
            if generation != analysis.generation {
                return AnalysisState::Stopped;
            }
            match &mut analysis.run {
                None => AnalysisState::Stopped,
                Some(Err(message)) => AnalysisState::Failed {
                    message: message.clone(),
                },
                Some(Ok(engine)) => {
                    engine.set_position(revision, command);
                    match engine.snapshot() {
                        Ok((seen, lines)) => AnalysisState::Searching {
                            lines: if seen == revision {
                                lines
                                    .into_iter()
                                    .filter_map(|line| AnalysisLine::at(line, &position))
                                    .collect()
                            } else {
                                Vec::new()
                            },
                        },
                        Err(message) => {
                            analysis.run = Some(Err(message.clone()));
                            AnalysisState::Failed { message }
                        }
                    }
                }
            }
        })
        .await?;
        Ok(Response::Analysis {
            snapshot: AnalysisSnapshot {
                game,
                revision,
                generation,
                state,
            },
        })
    }
}

impl SharedGames {
    /// Stop and reap any active engines before the desktop event loop exits the process.
    pub fn stop_engines(&self) {
        let searches: Vec<_> = self
            .games
            .blocking_lock()
            .open_games
            .values()
            .map(|open| open.analysis.clone())
            .collect();
        for search in searches {
            close(&search);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{Games, Request};

    #[test]
    fn a_variation_supplies_its_own_history_and_a_revisit_gets_a_new_revision() {
        let mut games = Games::default();
        let opened = games.open_pgn("1. e4 (1. d4 d5) e5 *").unwrap();
        let game = opened.game;
        assert_eq!(opened.picture.revision, 0);
        for (place, revision, command) in [
            (vec![1, 0], 1, "position startpos moves d2d4 d7d5"),
            (vec![0, 0], 2, "position startpos moves e2e4 e7e5"),
            (vec![1, 0], 3, "position startpos moves d2d4 d7d5"),
            (vec![1, 0], 3, "position startpos moves d2d4 d7d5"),
        ] {
            let Response::GoTo {
                picture: Some(picture),
            } = games.answer(Request::GoTo { game, place })
            else {
                unreachable!("The registered game is open");
            };
            assert_eq!(picture.revision, revision);
            assert_eq!(games.open_games[&game].position.search.command, command);
        }
    }
}
