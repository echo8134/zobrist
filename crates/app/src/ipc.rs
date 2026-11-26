//! Everything that crosses between the view and Rust.
//!
//! The view sends a [`Request`] and gets back the [`Response`] case with the same name. The
//! TypeScript types of the view are generated from these two types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, EnPassantMode, Outcome, Position, Role, Square, fen::Fen};
use ts_rs::TS;
use zobrist_core::editor::{GameEditor, Place, PlacedStep, board_target};
use zobrist_core::game::{GameBuilder, Move};

/// What the view can ask for.
#[derive(Clone, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum Request {
    /// The version of zobrist.
    Version,
    /// Open a game at the starting position.
    NewGame,
    /// Play a board move in an open game.
    PlayMove {
        /// The number from the new-game answer.
        game: usize,
        /// The square the piece moves from.
        #[serde(with = "square")]
        #[ts(as = "String")]
        from: Square,
        /// The square the piece moves to.
        #[serde(with = "square")]
        #[ts(as = "String")]
        to: Square,
        /// The new piece when a pawn promotes.
        promotion: Option<Promotion>,
    },
    /// Show the position reached by the sequence of choices.
    GoTo {
        /// The number from the new-game answer.
        game: usize,
        /// One choice per ply, starting from the initial position.
        place: Vec<usize>,
    },
    /// Close an open game.
    CloseGame {
        /// The number from the new-game answer.
        game: usize,
    },
}

mod square {
    use serde::{Deserialize, Deserializer, de};
    use shakmaty::Square;

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Square, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// The piece a pawn becomes on promotion.
#[derive(Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Promotion {
    /// Promote to a queen.
    Queen,
    /// Promote to a rook.
    Rook,
    /// Promote to a bishop.
    Bishop,
    /// Promote to a knight.
    Knight,
}

impl From<Promotion> for Role {
    fn from(promotion: Promotion) -> Self {
        match promotion {
            Promotion::Queen => Self::Queen,
            Promotion::Rook => Self::Rook,
            Promotion::Bishop => Self::Bishop,
            Promotion::Knight => Self::Knight,
        }
    }
}

/// What Rust answers. Each case has the name of its request.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum Response {
    /// The version of zobrist.
    Version {
        /// The version of this crate.
        version: String,
    },
    /// The newly opened game.
    NewGame {
        /// The game's number for this run of the app.
        game: usize,
        /// The picture at the starting position.
        picture: GamePicture,
    },
    /// The game after the editor accepts or refuses the move.
    PlayMove {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the editor accepts or refuses the place.
    GoTo {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game is closed.
    CloseGame,
}

/// Everything the view draws for one game, taken from the same editor position.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GamePicture {
    fen: String,
    turn: Side,
    in_check: bool,
    legal_moves: Vec<LegalMove>,
    last_move: Option<MoveSquares>,
    place: Vec<usize>,
    notation: Vec<NotationMove>,
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
enum Side {
    White,
    Black,
}

impl From<Color> for Side {
    fn from(color: Color) -> Self {
        match color {
            Color::White => Self::White,
            Color::Black => Self::Black,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct MoveSquares {
    from: String,
    to: String,
}

impl MoveSquares {
    fn from_move(chess_move: shakmaty::Move, turn: Color) -> Option<Self> {
        Some(Self {
            from: chess_move.from()?.to_string(),
            to: board_target(chess_move, turn).to_string(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct LegalMove {
    #[serde(flatten)]
    squares: MoveSquares,
    promotion: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct NotationMove {
    place: Vec<usize>,
    number: u32,
    side: Side,
    text: String,
    variations: Vec<Vec<Self>>,
}

impl NotationMove {
    fn from_step(
        placed: &PlacedStep<'_>,
        current: &Place,
        last_move: &mut Option<MoveSquares>,
    ) -> Self {
        let place = placed.place();
        let before = placed.step().position_before();
        if &place == current
            && let Move::Legal(chess_move) = placed.step().node().played()
        {
            *last_move = MoveSquares::from_move(chess_move, before.turn());
        }
        Self {
            place: place.0,
            number: before.fullmoves().get(),
            side: before.turn().into(),
            text: placed.step().notation().to_string(),
            variations: placed
                .variations()
                .map(|line| {
                    line.map(|step| Self::from_step(&step, current, last_move))
                        .collect()
                })
                .collect(),
        }
    }
}

impl GamePicture {
    fn from_editor(editor: &GameEditor) -> Self {
        let position = editor.position();
        let mut last_move = None;
        let notation = editor
            .walk()
            .map(|step| NotationMove::from_step(&step, editor.place(), &mut last_move))
            .collect();
        let legal_moves = position
            .legal_moves()
            .into_iter()
            // All four promotions have the same squares. Offer that pair once on the board.
            .filter(|chess_move| {
                chess_move
                    .promotion()
                    .is_none_or(|role| role == Role::Queen)
            })
            .filter_map(|chess_move| {
                Some(LegalMove {
                    squares: MoveSquares::from_move(chess_move, position.turn())?,
                    promotion: chess_move.is_promotion(),
                })
            })
            .collect();
        Self {
            fen: Fen::from_position(position, EnPassantMode::Legal).to_string(),
            turn: position.turn().into(),
            in_check: position.is_check(),
            legal_moves,
            last_move,
            place: editor.place().0.clone(),
            notation,
        }
    }
}

/// The open games of one run of the app.
pub struct Games {
    editors: HashMap<usize, GameEditor>,
    next_number: usize,
}

impl Default for Games {
    fn default() -> Self {
        Self {
            editors: HashMap::new(),
            next_number: 1,
        }
    }
}

impl Games {
    /// Applies the view's request and builds its answer before another request can edit the game.
    #[must_use]
    pub fn answer(&mut self, request: Request) -> Response {
        match request {
            Request::Version => Response::Version {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Request::NewGame => {
                let game = self.next_number;
                self.next_number = game.strict_add(1);
                let editor = GameEditor::new(
                    GameBuilder::new(Vec::new(), Chess::default()).finish(Outcome::Unknown),
                );
                let picture = GamePicture::from_editor(&editor);
                self.editors.insert(game, editor);
                Response::NewGame { game, picture }
            }
            Request::PlayMove {
                game,
                from,
                to,
                promotion,
            } => Response::PlayMove {
                picture: self.editors.get_mut(&game).map(|editor| {
                    let _played = editor.play(from, to, promotion.map(Into::into));
                    GamePicture::from_editor(editor)
                }),
            },
            Request::GoTo { game, place } => Response::GoTo {
                picture: self.editors.get_mut(&game).map(|editor| {
                    let _moved = editor.go_to(Place(place));
                    GamePicture::from_editor(editor)
                }),
            },
            Request::CloseGame { game } => {
                self.editors.remove(&game);
                Response::CloseGame
            }
        }
    }
}
