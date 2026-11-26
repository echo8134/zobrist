//! The game model: tags, start position, a tree of legal moves, and the result.
//!
//! A [`GameBuilder`] makes a game, and a [`GameEditor`](crate::editor::GameEditor) can edit it.
//! Both add moves by playing them on the position before them. Every variation starts from the
//! position before the move it replaces, and no line is empty. [`Game::steps`] replays the moves.

use std::{iter, mem, slice};

use shakmaty::san::{San, SanError, SanPlus, Suffix};
use shakmaty::uci::UciMove;
use shakmaty::{Chess, Color, EnPassantMode, FromSetup, Outcome, Position};

/// One tag pair, like `[White "Alpha"]`, decoded to text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// The tag name without the spaces around it, like `White`.
    pub name: String,
    /// The tag value without quotes and escapes, like `Alpha`.
    pub value: String,
}

/// The most variations that can be open inside each other. No [`Game`] exceeds it.
pub const MAX_VARIATION_DEPTH: usize = 255;

/// A numeric annotation glyph as stored in PGN, such as `$1` for a good move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnnotationSymbol(pub u8);

/// A move in a game: a legal chess move, or a null move that passes the turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Move {
    /// A legal chess move, including Chess960 castling.
    Legal(shakmaty::Move),
    /// A null move, written `--` or `Z0`. Annotators use it to show a threat.
    Null,
}

/// A game read from PGN or built move by move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    tags: Vec<Tag>,
    start: Chess,
    pub(super) main_line: MainLine,
    result: Outcome,
}

/// What [`Game::counts`] adds up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// The moves of the main line, without the moves of variations.
    pub main_line_moves: usize,
    /// The variations at every depth.
    pub variations: usize,
    /// Every comment before or after a move at every depth, and the comments of a game without
    /// moves.
    pub comments: usize,
}

/// The moves of a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainLine {
    /// The game has no moves.
    Empty {
        /// The comments of the move text, in file order.
        comments: Vec<String>,
    },
    /// The game has at least one move.
    Moves(Line),
}

/// A sequence of at least one node. The main line and every variation are lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub(super) first: Node,
    pub(super) rest: Vec<Node>,
}

/// One move with its annotation symbols, its comments, and the variations that replace it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    played: Played,
    pub(super) annotation_symbols: Vec<AnnotationSymbol>,
    pub(super) comments_before: Vec<String>,
    pub(super) comments_after: Vec<String>,
    pub(super) variations: Vec<Line>,
}

/// shakmaty has no null move that cannot fail, so a null move keeps the position after it and a
/// replay never has to compute one.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Played {
    Legal(shakmaty::Move),
    Null { after: Box<Chess> },
}

/// A line move by move with positions.
///
/// Made only by [`Game::steps`] and [`Step::variations`], so a walk always starts from the
/// position its line was played in.
#[derive(Clone, Debug)]
pub struct Steps<'game> {
    first: Option<&'game Node>,
    rest: slice::Iter<'game, Node>,
    position: Chess,
}

/// One node of a [`Steps`] walk with the position it was played in.
#[derive(Clone, Debug)]
pub struct Step<'game> {
    node: &'game Node,
    before: Chess,
}

/// Why [`GameBuilder::play`] refused a move. The builder is unchanged after a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveError {
    /// The move is not legal in the current position. A null move is illegal in check.
    IllegalMove,
    /// The notation matches more than one legal move.
    AmbiguousMove,
}

/// The innermost open line has no move yet, so there is nothing to annotate. The builder is
/// unchanged after a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoMove;

/// Why [`GameBuilder::begin_variation`] refused to open a variation. The builder is unchanged
/// after a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariationError {
    /// The innermost open line has no move yet, so there is nothing for a variation to replace.
    NoMove,
    /// [`MAX_VARIATION_DEPTH`] variations are open already.
    TooDeep,
}

/// The only way to make a [`Game`]. Takes the parts of a game in PGN order: moves, annotation
/// symbols, comments, and variations.
#[derive(Clone, Debug)]
pub struct GameBuilder {
    tags: Vec<Tag>,
    start: Chess,
    main: OpenLine,
    variations: Vec<OpenLine>,
}

#[derive(Clone, Debug)]
struct OpenLine {
    position: Chess,
    waiting_comments: Vec<String>,
    moves: Option<(Line, Chess)>,
}

impl Game {
    /// The tags in file order, including any that zobrist does not know.
    #[must_use]
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// The value of the first tag with this name, like `game.tag("White")`.
    #[must_use]
    pub fn tag(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|tag| tag.name == name)
            .map(|tag| tag.value.as_str())
    }

    /// The position before the first move: the standard position, or the one from the `FEN`
    /// tag with its castling rules.
    #[must_use]
    pub fn start_position(&self) -> &Chess {
        &self.start
    }

    /// The moves of the game as a tree, without positions.
    #[must_use]
    pub fn main_line(&self) -> &MainLine {
        &self.main_line
    }

    /// The result from the marker that ends the move text. The `Result` tag stays in
    /// [`Game::tags`] as written.
    #[must_use]
    pub fn result(&self) -> Outcome {
        self.result
    }

    /// Walks the main line with the position before each move. Empty for a game without
    /// moves.
    #[must_use]
    pub fn steps(&self) -> Steps<'_> {
        match &self.main_line {
            MainLine::Empty { .. } => Steps {
                first: None,
                rest: [].iter(),
                position: self.start.clone(),
            },
            MainLine::Moves(line) => line.steps_from(self.start.clone()),
        }
    }

    /// Counts the moves, variations, and comments of the game, without replaying a move.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        match &self.main_line {
            MainLine::Empty { comments } => counts.comments = comments.len(),
            MainLine::Moves(main_line) => {
                counts.main_line_moves = main_line.nodes().count();
                let mut pending = vec![main_line];
                while let Some(line) = pending.pop() {
                    for node in line.nodes() {
                        counts.comments += node.comments_before.len() + node.comments_after.len();
                        counts.variations += node.variations.len();
                        pending.extend(&node.variations);
                    }
                }
            }
        }
        counts
    }
}

impl Line {
    /// The nodes in order. Never empty.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        iter::once(&self.first).chain(&self.rest)
    }

    pub(super) fn new(first: Node) -> Self {
        Self {
            first,
            rest: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, node: Node) {
        self.rest.push(node);
    }

    pub(super) fn nodes_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        iter::once(&mut self.first).chain(&mut self.rest)
    }

    fn steps_from(&self, position: Chess) -> Steps<'_> {
        Steps {
            first: Some(&self.first),
            rest: self.rest.iter(),
            position,
        }
    }

    pub(super) fn last_mut(&mut self) -> &mut Node {
        self.rest.last_mut().unwrap_or(&mut self.first)
    }
}

impl Node {
    /// The move of this node.
    #[must_use]
    pub fn played(&self) -> Move {
        match self.played {
            Played::Legal(chess_move) => Move::Legal(chess_move),
            Played::Null { .. } => Move::Null,
        }
    }

    /// The annotation symbols after the move, in file order.
    #[must_use]
    pub fn annotation_symbols(&self) -> &[AnnotationSymbol] {
        &self.annotation_symbols
    }

    /// Comments before the move: at the start of a line, or after a variation of the previous
    /// move.
    #[must_use]
    pub fn comments_before(&self) -> &[String] {
        &self.comments_before
    }

    /// Comments after the move, in file order.
    #[must_use]
    pub fn comments_after(&self) -> &[String] {
        &self.comments_after
    }

    /// The variations that replace this move. Each starts from the position before it.
    #[must_use]
    pub fn variations(&self) -> &[Line] {
        &self.variations
    }

    pub(super) fn legal(chess_move: shakmaty::Move, comments_before: Vec<String>) -> Self {
        Self::new(Played::Legal(chess_move), comments_before)
    }

    fn new(played: Played, comments_before: Vec<String>) -> Self {
        Self {
            played,
            annotation_symbols: Vec::new(),
            comments_before,
            comments_after: Vec::new(),
            variations: Vec::new(),
        }
    }

    pub(super) fn replay(&self, position: &mut Chess) {
        match &self.played {
            Played::Legal(chess_move) => position.play_unchecked(*chess_move),
            Played::Null { after } => *position = Chess::clone(after),
        }
    }
}

impl<'game> Iterator for Steps<'game> {
    type Item = Step<'game>;

    fn next(&mut self) -> Option<Step<'game>> {
        let node = self.first.take().or_else(|| self.rest.next())?;
        let before = self.position.clone();
        node.replay(&mut self.position);
        Some(Step { node, before })
    }
}

impl<'game> Step<'game> {
    /// The node: its move, annotation symbols, comments, and variations.
    #[must_use]
    pub fn node(&self) -> &'game Node {
        self.node
    }

    /// The position the move was played in. Its fullmove number and side to move give the move
    /// number, like `3...` for Black's third move.
    #[must_use]
    pub fn position_before(&self) -> &Chess {
        &self.before
    }

    /// The move in standard notation with its check or mate suffix, like `Qxf7#` or `--`.
    #[must_use]
    pub fn notation(&self) -> SanPlus {
        match &self.node.played {
            Played::Legal(chess_move) => SanPlus::from_move(self.before.clone(), *chess_move),
            Played::Null { after } => SanPlus {
                san: San::Null,
                suffix: Suffix::from_position(after.as_ref()),
            },
        }
    }

    /// Walks each variation of this move from the position before it.
    pub fn variations(&self) -> impl Iterator<Item = Steps<'game>> + '_ {
        self.node
            .variations
            .iter()
            .map(|line| line.steps_from(self.before.clone()))
    }
}

impl GameBuilder {
    /// Starts a game with these tags from this position.
    #[must_use]
    pub fn new(tags: Vec<Tag>, start: Chess) -> GameBuilder {
        GameBuilder {
            tags,
            main: OpenLine::new(start.clone()),
            start,
            variations: Vec::new(),
        }
    }

    /// The position where the next move of the innermost open line is played.
    #[must_use]
    pub fn position(&self) -> &Chess {
        &self.variations.last().unwrap_or(&self.main).position
    }

    /// Plays a move at the end of the innermost open line. `San::Null` plays a null move.
    pub fn play(&mut self, san: San) -> Result<(), MoveError> {
        let chess_move = if san == San::Null {
            Move::Null
        } else {
            Move::Legal(san.to_move(self.position()).map_err(|error| match error {
                SanError::IllegalSan => MoveError::IllegalMove,
                SanError::AmbiguousSan => MoveError::AmbiguousMove,
            })?)
        };
        self.append_move(chess_move)
    }

    /// Plays a move by its squares, including castling to the rook's square and a null move.
    pub fn play_uci(&mut self, coordinates: UciMove) -> Result<(), MoveError> {
        let chess_move = if coordinates == UciMove::Null {
            Move::Null
        } else {
            Move::Legal(
                coordinates
                    .to_move(self.position())
                    .map_err(|_| MoveError::IllegalMove)?,
            )
        };
        self.append_move(chess_move)
    }

    /// Adds an annotation symbol to the last move of the innermost open line.
    pub fn annotate(&mut self, symbol: AnnotationSymbol) -> Result<(), NoMove> {
        let last = self.innermost().last_node().ok_or(NoMove)?;
        last.annotation_symbols.push(symbol);
        Ok(())
    }

    /// Adds a comment. It belongs after the last move of the innermost open line, unless that
    /// line has no move yet or a variation closed since that move. Then it waits for the next
    /// move of the line.
    pub fn comment(&mut self, text: String) {
        let line = self.innermost();
        match line.last_node() {
            Some(last) if last.variations.is_empty() => last.comments_after.push(text),
            Some(_) | None => line.waiting_comments.push(text),
        }
    }

    /// Opens a variation that replaces the last move of the innermost open line. Its first move
    /// is played on the position before that move.
    pub fn begin_variation(&mut self) -> Result<(), VariationError> {
        if self.variations.len() >= MAX_VARIATION_DEPTH {
            return Err(VariationError::TooDeep);
        }
        let start = self
            .innermost()
            .moves
            .as_ref()
            .map(|(_, before_last)| before_last.clone())
            .ok_or(VariationError::NoMove)?;
        self.variations.push(OpenLine::new(start));
        Ok(())
    }

    /// Closes the innermost open variation. Does nothing when no variation is open.
    ///
    /// A variation without a move is not kept. Its comments are read as if the parentheses
    /// were absent.
    pub fn end_variation(&mut self) {
        let Some(variation) = self.variations.pop() else {
            return;
        };
        match variation.close() {
            Ok(line) => {
                if let Some(last) = self.innermost().last_node() {
                    last.variations.push(line);
                }
            }
            Err(comments) => {
                for comment in comments {
                    self.comment(comment);
                }
            }
        }
    }

    /// Closes every open variation and returns the game with this result.
    #[must_use]
    pub fn finish(mut self, result: Outcome) -> Game {
        while !self.variations.is_empty() {
            self.end_variation();
        }
        let main_line = match self.main.close() {
            Ok(line) => MainLine::Moves(line),
            Err(comments) => MainLine::Empty { comments },
        };
        Game {
            tags: self.tags,
            start: self.start,
            main_line,
            result,
        }
    }

    fn innermost(&mut self) -> &mut OpenLine {
        self.variations.last_mut().unwrap_or(&mut self.main)
    }

    fn append_move(&mut self, chess_move: Move) -> Result<(), MoveError> {
        let line = self.innermost();
        let played = match chess_move {
            Move::Legal(legal) => Played::Legal(legal),
            Move::Null => Played::Null {
                after: Box::new(play_null(&line.position).ok_or(MoveError::IllegalMove)?),
            },
        };
        let node = Node::new(played, mem::take(&mut line.waiting_comments));
        line.push(node);
        Ok(())
    }
}

impl OpenLine {
    fn new(position: Chess) -> OpenLine {
        OpenLine {
            position,
            waiting_comments: Vec::new(),
            moves: None,
        }
    }

    fn last_node(&mut self) -> Option<&mut Node> {
        self.moves.as_mut().map(|(line, _)| line.last_mut())
    }

    fn push(&mut self, node: Node) {
        let before = self.position.clone();
        node.replay(&mut self.position);
        match &mut self.moves {
            None => self.moves = Some((Line::new(node), before)),
            Some((line, before_last)) => {
                line.push(node);
                *before_last = before;
            }
        }
    }

    fn close(self) -> Result<Line, Vec<String>> {
        match self.moves {
            None => Err(self.waiting_comments),
            Some((mut line, _)) => {
                line.last_mut().comments_after.extend(self.waiting_comments);
                Ok(line)
            }
        }
    }
}

/// `Position::swap_turn` leaves the move counters unchanged, so a report after a null move
/// would name the wrong move number. This advances them like any other move.
fn play_null(position: &Chess) -> Option<Chess> {
    let mut setup = position.to_setup(EnPassantMode::Legal);
    setup.swap_turn();
    setup.halfmoves = setup.halfmoves.saturating_add(1);
    if position.turn() == Color::Black {
        setup.fullmoves = setup.fullmoves.saturating_add(1);
    }
    Chess::from_setup(setup, position.castles().mode()).ok()
}
