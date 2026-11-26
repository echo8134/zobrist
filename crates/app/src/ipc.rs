//! Everything that crosses between the view and Rust.
//!
//! The view sends a [`Request`] and gets back the [`Response`] case with the same name. The
//! TypeScript types of the view are generated from these two types.

mod pgn_file;

pub use pgn_file::{GameRow, PgnFileAnswer, RefusalReason};

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, EnPassantMode, Outcome, Position, Role, Square, fen::Fen};
use ts_rs::TS;
use zobrist_core::editor::{self, GameEditor, Place, PlacedStep, board_target};
use zobrist_core::game::{GameBuilder, MainLine, Move};

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
    /// Replace the comment at the current place.
    SetComment {
        /// The number from the new-game answer.
        game: usize,
        /// The comment text. Empty text removes it.
        text: String,
    },
    /// Replace the symbol that annotates the current move.
    SetMoveSymbol {
        /// The number from the new-game answer.
        game: usize,
        /// The move symbol to keep, or none to remove it.
        symbol: Option<MoveSymbol>,
    },
    /// Replace the symbol that evaluates the current position.
    SetPositionSymbol {
        /// The number from the new-game answer.
        game: usize,
        /// The position symbol to keep, or none to remove it.
        symbol: Option<PositionSymbol>,
    },
    /// Take back the last change to the game.
    Undo {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Restore the last change taken back.
    Redo {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Promote the current variation one level.
    PromoteVariation {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Delete the current variation.
    DeleteVariation {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Delete the moves after the current place.
    DeleteRest {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Close an open game.
    CloseGame {
        /// The number from the new-game answer.
        game: usize,
    },
    /// List the games of a PGN file.
    OpenPgnFile {
        /// The path as it was typed.
        path: String,
    },
    /// Create a new, empty PGN file where nothing is yet.
    CreatePgnFile {
        /// The path as it was typed.
        path: String,
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

// The same spelling crosses the wire in a request, the current selection, and the notation.
macro_rules! wire_symbols {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
        pub enum $name {
            $(
                #[doc = $text]
                #[serde(rename = $text)]
                $variant,
            )+
        }

        impl $name {
            fn text(self) -> &'static str {
                match self { $(Self::$variant => $text,)+ }
            }
        }

        impl From<editor::$name> for $name {
            fn from(symbol: editor::$name) -> Self {
                match symbol { $(editor::$name::$variant => Self::$variant,)+ }
            }
        }

        impl From<$name> for editor::$name {
            fn from(symbol: $name) -> Self {
                match symbol { $($name::$variant => Self::$variant,)+ }
            }
        }
    };
}

wire_symbols! {
    /// The player's assessment of a move.
    MoveSymbol {
        Good => "!", Mistake => "?", Brilliant => "!!", Blunder => "??",
        Interesting => "!?", Dubious => "?!",
    }
}

wire_symbols! {
    /// The player's assessment of a position.
    PositionSymbol {
        Equal => "=", Unclear => "∞", WhiteSlight => "⩲", BlackSlight => "⩱",
        WhiteClear => "±", BlackClear => "∓", WhiteDecisive => "+-", BlackDecisive => "-+",
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
    /// The game after setting or refusing a comment.
    SetComment {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after setting or refusing a move symbol.
    SetMoveSymbol {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after setting or refusing a position symbol.
    SetPositionSymbol {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the undo request.
    Undo {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the redo request.
    Redo {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the request to promote the variation.
    PromoteVariation {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the request to delete the variation.
    DeleteVariation {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the request to delete the remaining moves.
    DeleteRest {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game is closed.
    CloseGame,
    /// The file with its games, or the refusal.
    OpenPgnFile {
        /// What the page Files shows next.
        answer: PgnFileAnswer,
    },
    /// The new file without games, or the refusal.
    CreatePgnFile {
        /// What the page Files shows next.
        answer: PgnFileAnswer,
    },
}

/// Everything the view draws for one game, taken from the same editor position.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GamePicture {
    fen: String,
    turn: Side,
    in_check: bool,
    legal_moves: Vec<LegalMove>,
    #[serde(flatten)]
    current: CurrentPlace,
    can_undo: bool,
    can_redo: bool,
    start_comments: Vec<String>,
    place: Vec<usize>,
    notation: Vec<NotationMove>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct CurrentPlace {
    in_variation: bool,
    has_next_move: bool,
    last_move: Option<MoveSquares>,
    move_symbol: Option<MoveSymbol>,
    position_symbol: Option<PositionSymbol>,
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
#[serde(rename_all = "camelCase")]
struct NotationMove {
    place: Vec<usize>,
    number: u32,
    side: Side,
    text: String,
    comments_before: Vec<String>,
    comments_after: Vec<String>,
    symbols: Vec<String>,
    variations: Vec<Vec<Self>>,
}

impl NotationMove {
    fn from_step(
        placed: &PlacedStep<'_>,
        current_place: &Place,
        current: &mut CurrentPlace,
    ) -> Self {
        let place = placed.place();
        let before = placed.step().position_before();
        let node = placed.step().node();
        let is_current = &place == current_place;
        if is_current && let Move::Legal(chess_move) = node.played() {
            current.last_move = MoveSquares::from_move(chess_move, before.turn());
        }
        let mut symbols = Vec::new();
        let mut position_symbols = Vec::new();
        let mut other_symbols = Vec::new();
        for &glyph in node.annotation_symbols() {
            if let Some(symbol) = editor::MoveSymbol::from_glyph(glyph).map(MoveSymbol::from) {
                symbols.push(symbol.text().to_owned());
                if is_current {
                    current.move_symbol.get_or_insert(symbol);
                }
            } else if let Some(symbol) =
                editor::PositionSymbol::from_glyph(glyph).map(PositionSymbol::from)
            {
                position_symbols.push(symbol.text().to_owned());
                if is_current {
                    current.position_symbol.get_or_insert(symbol);
                }
            } else {
                other_symbols.push(format!("${}", glyph.0));
            }
        }
        symbols.extend(position_symbols);
        symbols.extend(other_symbols);
        Self {
            place: place.0,
            number: before.fullmoves().get(),
            side: before.turn().into(),
            text: placed.step().notation().to_string(),
            comments_before: node.comments_before().to_vec(),
            comments_after: node.comments_after().to_vec(),
            symbols,
            variations: placed
                .variations()
                .map(|line| {
                    line.map(|step| Self::from_step(&step, current_place, current))
                        .collect()
                })
                .collect(),
        }
    }
}

impl GamePicture {
    fn from_editor(editor: &GameEditor) -> Self {
        let position = editor.position();
        let mut current = CurrentPlace {
            in_variation: editor.in_variation(),
            has_next_move: editor.has_next_move(),
            ..CurrentPlace::default()
        };
        let notation = editor
            .walk()
            .map(|step| NotationMove::from_step(&step, editor.place(), &mut current))
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
            current,
            can_undo: editor.can_undo(),
            can_redo: editor.can_redo(),
            start_comments: match editor.game().main_line() {
                MainLine::Empty { comments } => comments.clone(),
                MainLine::Moves(_) => Vec::new(),
            },
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
    fn edit<T>(
        &mut self,
        game: usize,
        change: impl FnOnce(&mut GameEditor) -> T,
    ) -> Option<GamePicture> {
        let editor = self.editors.get_mut(&game)?;
        // A refused edit leaves the editor at the position the view should draw.
        let _result = change(editor);
        Some(GamePicture::from_editor(editor))
    }

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
                picture: self.edit(game, |editor| {
                    editor.play(from, to, promotion.map(Into::into))
                }),
            },
            Request::GoTo { game, place } => Response::GoTo {
                picture: self.edit(game, |editor| editor.go_to(Place(place))),
            },
            Request::SetComment { game, text } => Response::SetComment {
                picture: self.edit(game, |editor| editor.set_comment(&text)),
            },
            Request::SetMoveSymbol { game, symbol } => Response::SetMoveSymbol {
                picture: self.edit(game, |editor| {
                    editor.set_move_symbol(symbol.map(Into::into))
                }),
            },
            Request::SetPositionSymbol { game, symbol } => Response::SetPositionSymbol {
                picture: self.edit(game, |editor| {
                    editor.set_position_symbol(symbol.map(Into::into))
                }),
            },
            Request::Undo { game } => Response::Undo {
                picture: self.edit(game, GameEditor::undo),
            },
            Request::Redo { game } => Response::Redo {
                picture: self.edit(game, GameEditor::redo),
            },
            Request::PromoteVariation { game } => Response::PromoteVariation {
                picture: self.edit(game, GameEditor::promote),
            },
            Request::DeleteVariation { game } => Response::DeleteVariation {
                picture: self.edit(game, GameEditor::delete_variation),
            },
            Request::DeleteRest { game } => Response::DeleteRest {
                picture: self.edit(game, GameEditor::delete_rest),
            },
            Request::CloseGame { game } => {
                self.editors.remove(&game);
                Response::CloseGame
            }
            Request::OpenPgnFile { path } => Response::OpenPgnFile {
                answer: pgn_file::open(&path),
            },
            Request::CreatePgnFile { path } => Response::CreatePgnFile {
                answer: pgn_file::create(&path),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use serde_json::json;
    use zobrist_core::pgn::{self, Entry};

    use super::*;

    #[test]
    fn pictures_group_imported_glyphs_and_select_the_first_of_each_kind() -> io::Result<()> {
        let source =
            b"{Start} 1. e4 $7 $10 $1 $16 $3 $13 $2 $14 $4 $15 $5 $17 $6 $18 $19 $36 {one} {two} *";
        let mut imported = None;
        pgn::read_games(source.as_slice(), |entry| {
            if let Entry::Game { game, problem, .. } = entry {
                assert_eq!(problem, None);
                imported = Some(*game);
            }
        })?;
        let mut editor = GameEditor::new(imported.unwrap());
        editor.go_to(Place(vec![0])).unwrap();
        let picture = serde_json::to_value(GamePicture::from_editor(&editor))?;
        assert_eq!(picture["moveSymbol"], "!");
        assert_eq!(picture["positionSymbol"], "=");
        assert_eq!(picture["startComments"], json!([]));
        assert_eq!(
            picture["notation"],
            json!([{
                "place": [0], "number": 1, "side": "white", "text": "e4",
                "commentsBefore": ["Start"], "commentsAfter": ["one", "two"],
                "symbols": ["!", "!!", "?", "??", "!?", "?!", "=", "±", "∞", "⩲", "⩱", "∓", "+-", "-+", "$7", "$36"],
                "variations": []
            }])
        );
        Ok(())
    }
}
