//! Navigates and edits the moves of a game.

use std::{mem, ops::Deref};

use shakmaty::{Chess, Color, Position, Role, Square};

use crate::game::{
    AnnotationSymbol, Game, Line, MAX_VARIATION_DEPTH, MainLine, Move, Node, Step, Steps,
};
use crate::pgn::validate_comment;

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

/// A move in a game and the choices that reach it.
#[derive(Debug)]
pub struct PlacedStep<'game> {
    step: Step<'game>,
    parent: Place,
    choice: usize,
}

impl<'game> PlacedStep<'game> {
    /// The move, its notation, and the position it was played in.
    #[must_use]
    pub fn step(&self) -> &Step<'game> {
        &self.step
    }

    /// The place immediately after this move.
    #[must_use]
    pub fn place(&self) -> Place {
        let mut place = self.parent.clone();
        place.0.push(self.choice);
        place
    }

    /// Walks the lines that replace this move in their PGN order.
    pub fn variations(&self) -> impl Iterator<Item = impl Iterator<Item = PlacedStep<'game>>> {
        self.step
            .variations()
            .zip(variation_choices(self.step.node(), self.choice))
            .map(|(steps, choice)| walk_line(steps, self.parent.clone(), choice))
    }
}

fn walk_line(
    steps: Steps<'_>,
    mut parent: Place,
    mut first_choice: usize,
) -> impl Iterator<Item = PlacedStep<'_>> {
    steps.map(move |step| {
        let choice = mem::take(&mut first_choice);
        let placed = PlacedStep {
            step,
            parent: parent.clone(),
            choice,
        };
        parent.0.push(choice);
        placed
    })
}

/// The square targeted on the board. For castling, use the king's destination when it moves
/// two squares, and the rook's square for other starting arrangements.
#[must_use]
pub fn board_target(chess_move: shakmaty::Move, turn: Color) -> Square {
    if let (Some(from), Some(side)) = (chess_move.from(), chess_move.castling_side()) {
        let target = side.king_to(turn);
        if from.distance(target) == 2 {
            return target;
        }
    }
    chess_move.to()
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

/// A judgement of a move, numbered by its PGN annotation glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MoveSymbol {
    /// A good move (`!`).
    Good = 1,
    /// A mistake (`?`).
    Mistake = 2,
    /// A brilliant move (`!!`).
    Brilliant = 3,
    /// A blunder (`??`).
    Blunder = 4,
    /// An interesting move (`!?`).
    Interesting = 5,
    /// A dubious move (`?!`).
    Dubious = 6,
}

impl MoveSymbol {
    /// The move judgement represented by this glyph, if any.
    #[must_use]
    pub fn from_glyph(glyph: AnnotationSymbol) -> Option<Self> {
        match glyph.0 {
            1 => Some(Self::Good),
            2 => Some(Self::Mistake),
            3 => Some(Self::Brilliant),
            4 => Some(Self::Blunder),
            5 => Some(Self::Interesting),
            6 => Some(Self::Dubious),
            _ => None,
        }
    }
}

/// An assessment of a position, numbered by its PGN annotation glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PositionSymbol {
    /// An equal position.
    Equal = 10,
    /// An unclear position.
    Unclear = 13,
    /// White has a slight advantage.
    WhiteSlight = 14,
    /// Black has a slight advantage.
    BlackSlight = 15,
    /// White has a clear advantage.
    WhiteClear = 16,
    /// Black has a clear advantage.
    BlackClear = 17,
    /// White has a decisive advantage.
    WhiteDecisive = 18,
    /// Black has a decisive advantage.
    BlackDecisive = 19,
}

impl PositionSymbol {
    /// The position assessment represented by this glyph, if any.
    #[must_use]
    pub fn from_glyph(glyph: AnnotationSymbol) -> Option<Self> {
        match glyph.0 {
            10 => Some(Self::Equal),
            13 => Some(Self::Unclear),
            14 => Some(Self::WhiteSlight),
            15 => Some(Self::BlackSlight),
            16 => Some(Self::WhiteClear),
            17 => Some(Self::BlackClear),
            18 => Some(Self::WhiteDecisive),
            19 => Some(Self::BlackDecisive),
            _ => None,
        }
    }
}

/// Why the editor refused to change a comment or symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationError {
    /// There is no move to annotate at the start of the game.
    NoMove,
    /// The trimmed comment cannot be written as PGN.
    InvalidComment,
}

#[derive(Clone, Debug)]
struct EditorState {
    game: Game,
    place: Place,
    position: Chess,
}

/// Owns a game, the current place, and the changes that can be undone or redone.
#[derive(Debug)]
pub struct GameEditor {
    current: EditorState,
    previous: Vec<EditorState>,
    undone: Vec<EditorState>,
}

impl GameEditor {
    /// Opens the game at its start position.
    #[must_use]
    pub fn new(game: Game) -> Self {
        Self {
            current: EditorState {
                position: game.start_position().clone(),
                game,
                place: Place::default(),
            },
            previous: Vec::new(),
            undone: Vec::new(),
        }
    }

    /// Whether a change is available to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.previous.is_empty()
    }

    /// Whether an undone change is available to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Restores the game and place before the last change. Returns whether a state was restored.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.previous.pop() else {
            return false;
        };
        self.undone.push(mem::replace(&mut self.current, previous));
        true
    }

    /// Restores the game and place saved by undo. Returns whether a state was restored.
    pub fn redo(&mut self) -> bool {
        let Some(undone) = self.undone.pop() else {
            return false;
        };
        self.previous.push(mem::replace(&mut self.current, undone));
        true
    }

    /// Whether the current move belongs to a variation.
    #[must_use]
    pub fn in_variation(&self) -> bool {
        self.current.place.variation_ply().is_ok()
    }

    /// Whether the current line continues after this place.
    #[must_use]
    pub fn has_next_move(&self) -> bool {
        let MainLine::Moves(line) = self.current.game.main_line() else {
            return false;
        };
        Continuation::at(line, &self.current.place.0, |_| {})
            .is_ok_and(|next| next.line.nodes().nth(next.index).is_some())
    }

    /// The game as it stands, including all edits.
    #[must_use]
    pub fn game(&self) -> &Game {
        &self.current.game
    }

    /// Takes the game out of the editor.
    #[must_use]
    pub fn into_game(self) -> Game {
        self.current.game
    }

    /// The position after the moves in the current place.
    #[must_use]
    pub fn position(&self) -> &Chess {
        &self.current.position
    }

    /// The current place in the game.
    #[must_use]
    pub fn place(&self) -> &Place {
        &self.current.place
    }

    /// Walks the main line from the start. Each move gives its place and nested variations.
    pub fn walk(&self) -> impl Iterator<Item = PlacedStep<'_>> {
        walk_line(self.current.game.steps(), Place::default(), 0)
    }

    /// Goes back one move. Returns whether the place changed.
    pub fn back(&mut self) -> bool {
        let Some((_, previous)) = self.current.place.0.split_last() else {
            return false;
        };
        self.go_to(Place(previous.to_vec())).is_ok()
    }

    /// Goes forward one move along the current line. Returns whether the place changed.
    pub fn forward(&mut self) -> bool {
        let mut place = self.current.place.clone();
        place.0.push(0);
        self.go_to(place).is_ok()
    }

    /// Goes to a place. A refusal leaves the editor unchanged.
    pub fn go_to(&mut self, place: Place) -> Result<(), NoSuchPlace> {
        let mut position = self.current.game.start_position().clone();
        if !place.0.is_empty() {
            let MainLine::Moves(line) = self.current.game.main_line() else {
                return Err(NoSuchPlace);
            };
            Continuation::at(line, &place.0, |node| node.replay(&mut position))?;
        }
        self.current.place = place;
        self.current.position = position;
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
        self.change(|state| state.play(from, to, promotion))
    }

    /// Replaces the comments at the current place with the trimmed text. Empty text clears them.
    pub fn set_comment(&mut self, text: &str) -> Result<(), AnnotationError> {
        self.change(|state| state.set_comment(text))
    }

    /// Replaces all move judgements at the current move, keeping the other glyphs in order.
    pub fn set_move_symbol(&mut self, symbol: Option<MoveSymbol>) -> Result<(), AnnotationError> {
        self.change(|state| state.set_move_symbol(symbol))
    }

    /// Replaces all position assessments at the current move, keeping the other glyphs in order.
    pub fn set_position_symbol(
        &mut self,
        symbol: Option<PositionSymbol>,
    ) -> Result<(), AnnotationError> {
        self.change(|state| state.set_position_symbol(symbol))
    }

    /// Promotes the current variation one level and stays on the same move.
    pub fn promote(&mut self) -> Result<(), EditVariationError> {
        self.change(EditorState::promote)
    }

    /// Deletes the current variation and returns to the position before its first move.
    pub fn delete_variation(&mut self) -> Result<(), EditVariationError> {
        self.change(EditorState::delete_variation)
    }

    /// Deletes every continuation after the current place.
    pub fn delete_rest(&mut self) {
        self.change(EditorState::delete_rest);
    }

    fn change<T>(&mut self, change: impl FnOnce(&mut EditorState) -> T) -> T {
        let before = self.current.clone();
        let result = change(&mut self.current);
        // Playing a move already in the game advances the place and preserves both histories.
        if self.current.game != before.game {
            self.previous.push(before);
            self.undone.clear();
        }
        result
    }
}

impl EditorState {
    fn play(&mut self, from: Square, to: Square, promotion: Option<Role>) -> Result<(), PlayError> {
        let chess_move = self
            .position
            .legal_moves()
            .into_iter()
            .find(|candidate| {
                candidate.from() == Some(from)
                    && candidate.promotion() == promotion
                    && (candidate.to() == to
                        || board_target(*candidate, self.position.turn()) == to)
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

    fn set_comment(&mut self, text: &str) -> Result<(), AnnotationError> {
        let text = text.trim();
        validate_comment(text).map_err(|_| AnnotationError::InvalidComment)?;
        let comments = if self.place.0.is_empty() {
            match &mut self.game.main_line {
                MainLine::Empty { comments } => comments,
                MainLine::Moves(line) => &mut line.first.comments_before,
            }
        } else {
            &mut self.current_node()?.comments_after
        };
        comments.clear();
        if !text.is_empty() {
            comments.push(text.to_owned());
        }
        Ok(())
    }

    fn set_move_symbol(&mut self, symbol: Option<MoveSymbol>) -> Result<(), AnnotationError> {
        let glyphs = &mut self.current_node()?.annotation_symbols;
        glyphs.retain(|&glyph| MoveSymbol::from_glyph(glyph).is_none());
        if let Some(symbol) = symbol {
            glyphs.insert(0, AnnotationSymbol(symbol as u8));
        }
        Ok(())
    }

    fn set_position_symbol(
        &mut self,
        symbol: Option<PositionSymbol>,
    ) -> Result<(), AnnotationError> {
        let glyphs = &mut self.current_node()?.annotation_symbols;
        glyphs.retain(|&glyph| PositionSymbol::from_glyph(glyph).is_none());
        if let Some(symbol) = symbol {
            let after_moves = glyphs
                .iter()
                .rposition(|&glyph| MoveSymbol::from_glyph(glyph).is_some())
                .map_or(0, |last_move| last_move + 1);
            glyphs.insert(after_moves, AnnotationSymbol(symbol as u8));
        }
        Ok(())
    }

    fn current_node(&mut self) -> Result<&mut Node, AnnotationError> {
        let MainLine::Moves(line) = &mut self.game.main_line else {
            return Err(AnnotationError::NoMove);
        };
        let next = Continuation::at(line, &self.place.0, |_| {})
            .map_err(|NoSuchPlace| AnnotationError::NoMove)?;
        let index = next.index.checked_sub(1).ok_or(AnnotationError::NoMove)?;
        next.line
            .nodes_mut()
            .nth(index)
            .ok_or(AnnotationError::NoMove)
    }

    fn promote(&mut self) -> Result<(), EditVariationError> {
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

    fn delete_variation(&mut self) -> Result<(), EditVariationError> {
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

    fn delete_rest(&mut self) {
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

trait LineReference: Deref<Target = Line> + Sized {
    fn into_variation(self, index: usize, slot: usize) -> Option<Self>;
}

impl LineReference for &Line {
    fn into_variation(self, index: usize, slot: usize) -> Option<Self> {
        self.nodes().nth(index)?.variations.get(slot)
    }
}

impl LineReference for &mut Line {
    fn into_variation(self, index: usize, slot: usize) -> Option<Self> {
        self.nodes_mut().nth(index)?.variations.get_mut(slot)
    }
}

struct Continuation<L> {
    line: L,
    index: usize,
    depth: usize,
}

impl<'game> Continuation<&'game mut Line> {
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
}

impl<L: LineReference> Continuation<L> {
    fn at(line: L, choices: &[usize], mut visit: impl FnMut(&Node)) -> Result<Self, NoSuchPlace> {
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

    fn descend(mut self, path: &[(usize, usize)]) -> Option<Self> {
        for &(slot, _) in path {
            self.line = self.line.into_variation(self.index, slot)?;
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
        find_choice(node, &mut matches, 0, &mut path).then_some(path)
    }
}

// A place numbers first moves in PGN order, flattening nested alternatives at the same ply.
// Each step back to the tree records the variation's slot and the choice it replaces.
fn find_choice(
    node: &Node,
    matches: &mut impl FnMut(&Node, usize) -> bool,
    number: usize,
    path: &mut Vec<(usize, usize)>,
) -> bool {
    if matches(node, number) {
        return true;
    }
    for (slot, (variation, choice)) in node
        .variations
        .iter()
        .zip(variation_choices(node, number))
        .enumerate()
    {
        path.push((slot, number));
        if find_choice(&variation.first, matches, choice, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn variation_choices(node: &Node, parent: usize) -> impl Iterator<Item = usize> {
    // All first moves nested under the preceding variation also occupy choices at this ply.
    let offsets = std::iter::once(1).chain(
        node.variations
            .iter()
            .map(|line| first_move_count(&line.first)),
    );
    offsets
        .take(node.variations.len())
        .scan(parent, |choice, offset| {
            *choice += offset;
            Some(*choice)
        })
}

fn first_move_count(node: &Node) -> usize {
    1 + node
        .variations
        .iter()
        .map(|line| first_move_count(&line.first))
        .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use std::io;

    use shakmaty::{Color, Position, Square};

    use crate::game::Move;
    use crate::pgn::{self, Entry};

    use super::{GameEditor, Place, PlacedStep, board_target};

    type WalkedMove = (Vec<usize>, Place, String);

    fn collect_line<'game>(
        line: impl Iterator<Item = PlacedStep<'game>>,
        variations: &mut Vec<usize>,
        moves: &mut Vec<WalkedMove>,
        follower: &mut GameEditor,
    ) {
        for placed in line {
            let place = placed.place();
            follower.go_to(place.clone()).unwrap();
            let Move::Legal(played) = placed.step().node().played() else {
                unreachable!("these fixtures have only legal board moves")
            };
            let after = placed
                .step()
                .position_before()
                .clone()
                .play(played)
                .unwrap();
            assert_eq!(follower.position(), &after);
            assert!(follower.back());
            assert_eq!(follower.position(), placed.step().position_before());
            moves.push((
                variations.clone(),
                place,
                placed.step().notation().to_string(),
            ));
            for (slot, variation) in placed.variations().enumerate() {
                variations.push(slot);
                collect_line(variation, variations, moves, follower);
                variations.pop();
            }
        }
    }

    fn walk_games(source: &[u8]) -> io::Result<Vec<Vec<WalkedMove>>> {
        let mut games = Vec::new();
        pgn::read_games(source, |entry| {
            if let Entry::Game { game, problem, .. } = entry {
                assert_eq!(problem, None);
                let mut follower = GameEditor::new((*game).clone());
                let editor = GameEditor::new(*game);
                let mut moves = Vec::new();
                collect_line(editor.walk(), &mut Vec::new(), &mut moves, &mut follower);
                games.push(moves);
            }
        })?;
        Ok(games)
    }

    #[test]
    fn walks_first_move_variations_with_their_places_and_nesting() -> io::Result<()> {
        let games = walk_games(b"1. e4 (1. d4 (1. c4)) (1. Nf3) *")?;
        assert_eq!(
            games,
            [vec![
                (vec![], Place(vec![0]), "e4".to_owned()),
                (vec![0], Place(vec![1]), "d4".to_owned()),
                (vec![0, 0], Place(vec![2]), "c4".to_owned()),
                (vec![1], Place(vec![3]), "Nf3".to_owned()),
            ]]
        );
        Ok(())
    }

    #[test]
    fn follows_every_walked_place_of_the_clean_games() -> io::Result<()> {
        let games = walk_games(include_bytes!("../../cli/tests/fixtures/clean.pgn"))?;
        assert_eq!(games.iter().map(Vec::len).collect::<Vec<_>>(), [9, 4]);
        Ok(())
    }

    #[test]
    fn castling_targets_the_rook_when_the_king_moves_only_one_square() {
        assert_eq!(
            board_target(
                shakmaty::Move::Castle {
                    king: Square::F1,
                    rook: Square::H1
                },
                Color::White,
            ),
            Square::H1
        );
    }
}
