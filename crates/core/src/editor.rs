//! Navigates and edits the moves of a game.

use std::mem;

use shakmaty::{Chess, Position, Role, Square};

use crate::game::{Game, Line, MAX_VARIATION_DEPTH, MainLine, Move, Node};

/// Each choice plays one move from the start of the game. An empty place is the start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Place(
    /// Choices in PGN order, one per ply. The line's own move is `0`, followed by the
    /// variations of that move, including variations on their first moves.
    pub Vec<usize>,
);

/// The game has no move for at least one choice in the requested place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoSuchPlace;

/// Why [`GameEditor::play`] refused a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayError {
    /// No legal move matches the given squares and promotion.
    IllegalMove,
    /// A new variation here would exceed [`MAX_VARIATION_DEPTH`].
    TooDeep,
}

/// Owns a game and a current place in it.
#[derive(Debug)]
pub struct GameEditor {
    game: Game,
    place: Place,
    position: Chess,
}

impl GameEditor {
    /// Opens the game at its start position.
    #[must_use]
    pub fn new(game: Game) -> Self {
        Self {
            position: game.start_position().clone(),
            game,
            place: Place::default(),
        }
    }

    /// The game as it stands, including all edits.
    #[must_use]
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Takes the game out of the editor.
    #[must_use]
    pub fn into_game(self) -> Game {
        self.game
    }

    /// The position after the moves in the current place.
    #[must_use]
    pub fn position(&self) -> &Chess {
        &self.position
    }

    /// The current place in the game.
    #[must_use]
    pub fn place(&self) -> &Place {
        &self.place
    }

    /// Goes back one move. Returns whether the place changed.
    pub fn back(&mut self) -> bool {
        let Some((_, previous)) = self.place.0.split_last() else {
            return false;
        };
        self.go_to(Place(previous.to_vec())).is_ok()
    }

    /// Goes forward one move along the current line. Returns whether the place changed.
    pub fn forward(&mut self) -> bool {
        let mut place = self.place.clone();
        place.0.push(0);
        self.go_to(place).is_ok()
    }

    /// Goes to a place. A refusal leaves the editor unchanged.
    pub fn go_to(&mut self, place: Place) -> Result<(), NoSuchPlace> {
        let mut position = self.game.start_position().clone();
        if !place.0.is_empty() {
            let MainLine::Moves(line) = &mut self.game.main_line else {
                return Err(NoSuchPlace);
            };
            Continuation::at(line, &place, |node| node.replay(&mut position))?;
        }
        self.place = place;
        self.position = position;
        Ok(())
    }

    /// Plays a legal move and advances to it. Castling accepts a king move of two squares to
    /// its castled square or a king move onto its rook. A refusal leaves the editor unchanged.
    pub fn play(
        &mut self,
        from: Square,
        to: Square,
        promotion: Option<Role>,
    ) -> Result<(), PlayError> {
        let chess_move = self
            .position
            .legal_moves()
            .into_iter()
            .find(|candidate| {
                candidate.from() == Some(from)
                    && candidate.promotion() == promotion
                    && (candidate.to() == to
                        || (from.distance(to) == 2
                            && candidate
                                .castling_side()
                                .is_some_and(|side| side.king_to(self.position.turn()) == to)))
            })
            .ok_or(PlayError::IllegalMove)?;

        let choice = match &mut self.game.main_line {
            MainLine::Empty { comments } => {
                let node = Node::legal(chess_move, mem::take(comments));
                self.game.main_line = MainLine::Moves(Line::new(node));
                0
            }
            MainLine::Moves(line) => {
                let Continuation { line, index, depth } =
                    Continuation::at(line, &self.place, |_| {})
                        .map_err(|NoSuchPlace| PlayError::IllegalMove)?;
                let mut choice = 0;
                let existing = Continuation::select(line, index, depth, &mut |node| {
                    if node.played() == Move::Legal(chess_move) {
                        true
                    } else {
                        choice += 1;
                        false
                    }
                });
                if existing.is_none() {
                    let node = Node::legal(chess_move, Vec::new());
                    if let Some(replaced) = line.nodes_mut().nth(index) {
                        if depth == MAX_VARIATION_DEPTH {
                            return Err(PlayError::TooDeep);
                        }
                        replaced.variations.push(Line::new(node));
                    } else {
                        line.push(node);
                    }
                }
                choice
            }
        };

        self.place.0.push(choice);
        self.position.play_unchecked(chess_move);
        Ok(())
    }
}

struct Continuation<'game> {
    line: &'game mut Line,
    index: usize,
    depth: usize,
}

impl<'game> Continuation<'game> {
    fn at(
        line: &'game mut Line,
        place: &Place,
        mut visit: impl FnMut(&Node),
    ) -> Result<Self, NoSuchPlace> {
        let mut next = Self {
            line,
            index: 0,
            depth: 0,
        };
        for &choice in &place.0 {
            let mut choices = 0..;
            next = Self::select(next.line, next.index, next.depth, &mut |node| {
                if choices.next() != Some(choice) {
                    return false;
                }
                visit(node);
                true
            })
            .ok_or(NoSuchPlace)?;
            next.index += 1;
        }
        Ok(next)
    }

    fn select(
        line: &'game mut Line,
        index: usize,
        depth: usize,
        matches: &mut impl FnMut(&Node) -> bool,
    ) -> Option<Self> {
        if matches(line.nodes().nth(index)?) {
            return Some(Self { line, index, depth });
        }
        for variation in &mut line.nodes_mut().nth(index)?.variations {
            if let Some(selected) = Self::select(variation, 0, depth + 1, matches) {
                return Some(selected);
            }
        }
        None
    }
}
