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

impl Place {
    fn variation_ply(&self) -> Result<usize, EditVariationError> {
        self.0
            .iter()
            .rposition(|&choice| choice != 0)
            .ok_or(EditVariationError::NoVariation)
    }
}

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

/// Why promoting or deleting the current variation was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditVariationError {
    /// The current place is at the start or in the main line.
    NoVariation,
    /// Demoting the outer line would exceed [`MAX_VARIATION_DEPTH`].
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
            Continuation::at(line, &place.0, |node| node.replay(&mut position))?;
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
                let next = Continuation::at(line, &self.place.0, |_| {})
                    .map_err(|NoSuchPlace| PlayError::IllegalMove)?;
                let mut choice = 0;
                let existing = next.find(|node, _| {
                    if node.played() == Move::Legal(chess_move) {
                        true
                    } else {
                        choice += 1;
                        false
                    }
                });
                if existing.is_none() {
                    let Continuation { line, index, depth } = next;
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

    /// Promotes the current variation one level and stays on the same move.
    pub fn promote(&mut self) -> Result<(), EditVariationError> {
        let ply = self.place.variation_ply()?;
        let (outer, slot, outer_choice) =
            Continuation::outer(&mut self.game.main_line, &self.place, ply, |_| {})?;
        // The replaced move's alternatives stay at this ply. Only later moves and their
        // variations gain a level of nesting.
        if outer.line.rest[outer.index..]
            .iter()
            .any(|node| reaches_depth_limit(node, outer.depth))
        {
            return Err(EditVariationError::TooDeep);
        }

        let old_rest = outer.line.rest.split_off(outer.index);
        let replaced = outer.line.last_mut();
        let mut alternatives = mem::take(&mut replaced.variations);
        let promoted = &mut alternatives[slot];
        mem::swap(replaced, &mut promoted.first);
        let new_rest = mem::replace(&mut promoted.rest, old_rest);
        let after_slot = slot + 1;
        alternatives.splice(after_slot..after_slot, mem::take(&mut replaced.variations));
        replaced.variations = alternatives;
        outer.line.rest.extend(new_rest);
        self.place.0[ply] = outer_choice;
        Ok(())
    }

    /// Deletes the current variation and returns to the position before its first move.
    pub fn delete_variation(&mut self) -> Result<(), EditVariationError> {
        let ply = self.place.variation_ply()?;
        let mut position = self.game.start_position().clone();
        let (outer, slot, _) =
            Continuation::outer(&mut self.game.main_line, &self.place, ply, |node| {
                node.replay(&mut position);
            })?;
        let replaced = outer
            .line
            .nodes_mut()
            .nth(outer.index)
            .ok_or(EditVariationError::NoVariation)?;
        replaced.variations.remove(slot);
        self.place.0.truncate(ply);
        self.position = position;
        Ok(())
    }

    /// Deletes every continuation after the current place.
    pub fn delete_rest(&mut self) {
        let MainLine::Moves(line) = &mut self.game.main_line else {
            return;
        };
        if self.place.0.is_empty() {
            self.game.main_line = MainLine::Empty {
                comments: Vec::new(),
            };
        } else if let Ok(next) = Continuation::at(line, &self.place.0, |_| {}) {
            next.line.rest.truncate(next.index - 1);
        }
    }
}

fn reaches_depth_limit(node: &Node, depth: usize) -> bool {
    depth == MAX_VARIATION_DEPTH
        || node.variations.iter().any(|variation| {
            variation
                .nodes()
                .any(|child| reaches_depth_limit(child, depth + 1))
        })
}

struct Continuation<'game> {
    line: &'game mut Line,
    index: usize,
    depth: usize,
}

impl<'game> Continuation<'game> {
    fn at(
        line: &'game mut Line,
        choices: &[usize],
        mut visit: impl FnMut(&Node),
    ) -> Result<Self, NoSuchPlace> {
        let mut next = Self {
            line,
            index: 0,
            depth: 0,
        };
        for &choice in choices {
            let path = next.path_to(choice).ok_or(NoSuchPlace)?;
            next = next.descend(&path).ok_or(NoSuchPlace)?;
            visit(next.line.nodes().nth(next.index).ok_or(NoSuchPlace)?);
            next.index += 1;
        }
        Ok(next)
    }

    fn outer(
        main: &'game mut MainLine,
        place: &Place,
        ply: usize,
        visit: impl FnMut(&Node),
    ) -> Result<(Self, usize, usize), EditVariationError> {
        let MainLine::Moves(line) = main else {
            return Err(EditVariationError::NoVariation);
        };
        let next = Self::at(line, &place.0[..ply], visit)
            .map_err(|NoSuchPlace| EditVariationError::NoVariation)?;
        let mut path = next
            .path_to(place.0[ply])
            .ok_or(EditVariationError::NoVariation)?;
        let (slot, parent_choice) = path.pop().ok_or(EditVariationError::NoVariation)?;
        let outer = next.descend(&path).ok_or(EditVariationError::NoVariation)?;
        Ok((outer, slot, parent_choice))
    }

    fn descend(mut self, path: &[(usize, usize)]) -> Option<Self> {
        for &(slot, _) in path {
            self.line = &mut self.line.nodes_mut().nth(self.index)?.variations[slot];
            self.index = 0;
            self.depth += 1;
        }
        Some(self)
    }

    fn path_to(&self, choice: usize) -> Option<Vec<(usize, usize)>> {
        self.find(|_, number| number == choice)
    }

    fn find(&self, mut matches: impl FnMut(&Node, usize) -> bool) -> Option<Vec<(usize, usize)>> {
        let mut path = Vec::new();
        let node = self.line.nodes().nth(self.index)?;
        find_choice(node, &mut matches, &mut 0, &mut path).then_some(path)
    }
}

// A place numbers first moves in PGN order, flattening nested alternatives at the same ply.
// Each step back to the tree records the variation's slot and the choice it replaces.
fn find_choice(
    node: &Node,
    matches: &mut impl FnMut(&Node, usize) -> bool,
    number: &mut usize,
    path: &mut Vec<(usize, usize)>,
) -> bool {
    if matches(node, *number) {
        return true;
    }
    let parent = *number;
    for (slot, variation) in node.variations.iter().enumerate() {
        *number += 1;
        path.push((slot, parent));
        if find_choice(&variation.first, matches, number, path) {
            return true;
        }
        path.pop();
    }
    false
}
