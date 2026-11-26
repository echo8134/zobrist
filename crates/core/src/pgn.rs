//! Reads and writes PGN at the boundary of the game model.
//!
//! No game is dropped. A game that cannot be read completely keeps every move before its first
//! problem, and the [`Problem`] travels next to it in an [`Entry::Game`].

use std::cell::RefCell;
use std::fmt;
use std::io::{self, Read, Write};
use std::mem;
use std::num::NonZeroU32;
use std::ops::{ControlFlow, Range};
use std::str;

use pgn_reader::{Nag as RawNag, RawComment, RawTag, Reader, SanPlus, Skip, Visitor};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Outcome, Position};

use crate::game::{
    AnnotationSymbol, Game, GameBuilder, MAX_VARIATION_DEPTH, MainLine, MoveError, Steps, Tag,
    VariationError,
};

/// pgn-reader's default of 255 bytes fails or not depending on where a longer tag line falls in
/// its buffer, and real `Annotator` and `Event` tags can be longer.
const SUPPORTED_TAG_LINE_LENGTH: usize = 1 << 16;

const MAX_COMMENT_LENGTH: usize = 1 << 20;

/// The buffer of pgn-reader is twice the supported tag line length, and the reader keeps twice
/// that buffer.
const KEPT_BYTES: usize = 4 * SUPPORTED_TAG_LINE_LENGTH;

/// Reads every game of a PGN byte source in file order and hands each entry to `each`.
/// Streams, memory stays flat.
///
/// Each entry comes with its range in the source, counted in the bytes of the source as they
/// are on disk. The range starts at the first byte of the entry and ends behind its last byte
/// that is not white space, so text that the reader skips at the end of a game belongs to that
/// game. White space and byte order marks around an entry belong to no entry.
///
/// Returns `Err` when the source fails, after every entry that was finished before the failure
/// has been handed out. Damage inside the PGN text never ends the reading. The reader buffers
/// on its own, so pass a `File` directly.
pub fn read_games<R: Read>(source: R, each: impl FnMut(Entry)) -> io::Result<()> {
    let trail = RefCell::new(Trail::default());
    let mut reader = Reader::build(WithoutByteOrderMarks::new(source, &trail))
        .set_supported_tag_line_length(SUPPORTED_TAG_LINE_LENGTH)
        .finish();
    let mut collector = Collector::new(each);
    // pgn-reader tells its place only between two calls, as the bytes it holds unread.
    let place = |reader: &Reader<_>| trail.borrow().handed() - reader.buffer().len() as u64;
    loop {
        // `read_game` skips a byte order mark and then white space, and another mark can stand
        // behind that white space. The entry starts where asking again no longer moves the place.
        let mut start = place(&reader);
        loop {
            reader.has_more()?;
            let settled = place(&reader);
            if settled == start {
                break;
            }
            start = settled;
        }
        collector.begin(trail.borrow().start_at(start)?);
        let end = |reader| trail.borrow().end_before(place(reader));
        match reader.read_game(&mut collector) {
            Ok(Some(())) => collector.finish(Some(end(&reader)?)),
            Ok(None) => {
                collector.finish_game_left_open(end(&reader)?);
                return Ok(());
            }
            Err(error) if is_damage(&error) => collector.recover(end(&reader)?),
            Err(error) => return Err(error),
        }
    }
}

/// Writes a single game as UTF-8 PGN, including a final newline.
/// The caller separates consecutive games with a blank line. Returns an error if the tags
/// cannot reproduce the starting position, or the reader cannot retain the tags or comments.
pub fn write_game(game: &Game, mut output: impl Write) -> io::Result<()> {
    let start = match start_position(game.tags()) {
        Some(start) => start,
        // The reader retains an invalid FEN with its empty game and skips all move text.
        None if matches!(game.main_line(), MainLine::Empty { comments } if comments.is_empty()) => {
            Chess::default()
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid FEN would discard the move text",
            ));
        }
    };
    let actual = game.start_position();
    if start.to_setup(EnPassantMode::Always) != actual.to_setup(EnPassantMode::Always)
        || start.castles().mode() != actual.castles().mode()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PGN tags do not give the game's starting position",
        ));
    }

    for (index, tag) in game.tags().iter().enumerate() {
        if index > 0 && game.tags()[0].name == tag.name {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "repeated first PGN tag would start a new game",
            ));
        }
        if tag.name.trim_ascii() != tag.name
            || tag.name.contains(['"', ']', '\n'])
            || tag.name.contains(MARKED_TAG_LINE)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PGN tag cannot be read back unchanged",
            ));
        }
        let value = escaped_tag_value(&tag.name, &tag.value)?;
        writeln!(output, "[{} \"{value}\"]", tag.name)?;
    }
    if !game.tags().is_empty() {
        writeln!(output)?;
    }

    match game.main_line() {
        MainLine::Empty { comments } => {
            write_comments(comments, &mut output, "", " ")?;
        }
        MainLine::Moves(_) => {
            write_line(game.steps(), &mut output)?;
            output.write_all(b" ")?;
        }
    }
    writeln!(output, "{}", game.result())
}

pub(super) fn escaped_tag_value(name: &str, value: &str) -> io::Result<String> {
    if value.contains(MARKED_TAG_LINE) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PGN tag cannot be read back unchanged",
        ));
    }
    if value.contains('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("PGN tag {name} contains a line break"),
        ));
    }
    let value = value.replace('\\', "\\\\").replace('"', "\\\"");
    // The reader needs the bytes from '[' through the closing quote in one buffer.
    if name.len() + value.len() + 4 > SUPPORTED_TAG_LINE_LENGTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("PGN tag {name} exceeds the supported UTF-8 tag length"),
        ));
    }
    Ok(value)
}

fn write_line(steps: Steps<'_>, output: &mut impl Write) -> io::Result<()> {
    let mut number_black = true;
    for (index, step) in steps.enumerate() {
        if index > 0 {
            output.write_all(b" ")?;
        }
        let node = step.node();
        write_comments(node.comments_before(), output, "", " ")?;
        let position = step.position_before();
        if position.turn() == Color::White {
            write!(output, "{}. ", position.fullmoves())?;
        } else if number_black {
            write!(output, "{}... ", position.fullmoves())?;
        }
        write!(output, "{}", step.notation())?;
        for AnnotationSymbol(number) in node.annotation_symbols() {
            write!(output, " ${number}")?;
        }
        write_comments(node.comments_after(), output, " ", "")?;
        for variation in step.variations() {
            output.write_all(b" (")?;
            write_line(variation, output)?;
            output.write_all(b")")?;
        }
        number_black = !node.variations().is_empty()
            || !node.comments_after().is_empty()
            || !node.annotation_symbols().is_empty();
    }
    Ok(())
}

fn write_comments(
    comments: &[String],
    output: &mut impl Write,
    before: &str,
    after: &str,
) -> io::Result<()> {
    for comment in comments {
        validate_comment(comment)
            .map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))?;
        write!(output, "{before}{{{comment}}}{after}")?;
    }
    Ok(())
}

pub(super) fn validate_comment(comment: &str) -> Result<(), &'static str> {
    if comment.contains('}') || comment.trim() != comment || comment.contains(MARKED_TAG_LINE) {
        return Err("PGN comment cannot be read back unchanged");
    }
    if comment.len() > MAX_COMMENT_LENGTH {
        return Err("PGN comment exceeds 1 MiB in UTF-8");
    }
    Ok(())
}

/// One thing found in a PGN source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A game.
    Game {
        /// Every tag, and every move before the first problem.
        game: Box<Game>,
        /// The first problem, when the game could not be read completely.
        problem: Option<Problem>,
        /// Where the game stands in the source, as [`read_games`] counts it. `None` for a game
        /// with only tags that another game follows, and for that other game, because
        /// pgn-reader reads the two as one and gives no place between them.
        range: Option<Range<u64>>,
    },
    /// Text between two games that holds no tag, no move, no comment, and no result marker. It
    /// is not a game.
    StrayText {
        /// The number of games before it.
        after_game: u64,
        /// Where the text stands in the source, as [`read_games`] counts it.
        range: Range<u64>,
    },
}

/// Why a game could not be read completely. Not part of the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The number of the game in the source, counting from 1.
    pub game_number: u64,
    /// What went wrong, and where.
    pub reason: Reason,
}

/// What went wrong in a game. `Display` gives the text that `zobrist pgn check` prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A move is not legal in its position. A null move is illegal in check.
    IllegalMove(MoveText),
    /// A move matches more than one legal move.
    AmbiguousMove(MoveText),
    /// An annotation symbol came before the first move of a line.
    AnnotationWithoutMove(AnnotationSymbol),
    /// A variation came before the first move of a line, so it replaces nothing.
    VariationWithoutMove,
    /// A variation is nested inside more than [`MAX_VARIATION_DEPTH`] others.
    VariationTooDeep,
    /// The `FEN` tag cannot be read or is not a legal position. The game starts from the
    /// standard position and keeps no moves. The tag stays in the tag list as written.
    InvalidFen,
    /// A tag line is longer than the reader accepts, or the source ends inside it. The game
    /// keeps its other tags.
    UnreadableTagLine,
    /// A comment is still open at the end of the source.
    UnclosedComment,
    /// A comment holds more than a mebibyte. It is not kept.
    CommentTooLong,
}

/// A move that failed, with its place in the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveText {
    /// The move as shakmaty writes it, like `Ke3`. `0-0` in a file shows as `O-O`.
    pub text: String,
    /// The fullmove number, like 2 in `2. Ke3`.
    pub number: NonZeroU32,
    /// The side that tried the move.
    pub side: Color,
}

impl fmt::Display for Reason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::IllegalMove(move_text) => write!(formatter, "illegal move {move_text}"),
            Reason::AmbiguousMove(move_text) => write!(formatter, "ambiguous move {move_text}"),
            Reason::AnnotationWithoutMove(AnnotationSymbol(number)) => write!(
                formatter,
                "annotation symbol ${number} before the first move of a line"
            ),
            Reason::VariationWithoutMove => {
                formatter.write_str("variation before the first move of a line")
            }
            Reason::VariationTooDeep => write!(
                formatter,
                "variation is nested more than {MAX_VARIATION_DEPTH} deep"
            ),
            Reason::InvalidFen => formatter.write_str("FEN tag is not a legal position"),
            Reason::UnreadableTagLine => formatter.write_str("tag line is too long or not closed"),
            Reason::UnclosedComment => formatter
                .write_str("comment is not closed, the rest of the file was read as its text"),
            Reason::CommentTooLong => formatter.write_str("comment is too long to read"),
        }
    }
}

impl fmt::Display for MoveText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let side = match self.side {
            Color::White => "White's",
            Color::Black => "Black's",
        };
        write!(
            formatter,
            "\"{}\" at {side} move {}",
            self.text, self.number
        )
    }
}

/// The visitor behind [`read_games`]. All state lives here and not in pgn-reader's per-game
/// values, because pgn-reader drops those when `read_game` returns an error, and the game read
/// so far must survive that error. It hands each entry to `each` the moment the entry is
/// finished, because one `read_game` call can finish many games.
///
/// It never returns `ControlFlow::Break`, because a break makes pgn-reader skip the result
/// marker.
struct Collector<F> {
    each: F,
    games: u64,
    phase: Phase,
    problem: Option<Problem>,
    result: Option<Outcome>,
    comment: Vec<u8>,
    start: Start,
}

/// Where in the source the entry that is being read starts.
enum Start {
    /// No entry is open, so the next call of pgn-reader opens one at its place.
    Unset,
    At(u64),
    /// The entry follows a game with only tags. pgn-reader read both in one call.
    Unknown,
}

enum Phase {
    Tags(Vec<Tag>),
    Moves(Box<GameBuilder>),
}

impl Default for Phase {
    fn default() -> Phase {
        Phase::Tags(Vec::new())
    }
}

impl<F: FnMut(Entry)> Collector<F> {
    fn new(each: F) -> Collector<F> {
        Collector {
            each,
            games: 0,
            phase: Phase::default(),
            problem: None,
            result: None,
            comment: Vec::new(),
            start: Start::Unset,
        }
    }

    fn begin(&mut self, start: u64) {
        if matches!(self.start, Start::Unset) {
            self.start = Start::At(start);
        }
    }

    fn unfrozen_builder(&mut self) -> Option<&mut GameBuilder> {
        match &mut self.phase {
            Phase::Moves(builder) if self.problem.is_none() => Some(builder),
            Phase::Moves(_) | Phase::Tags(_) => None,
        }
    }

    fn record_first_problem(&mut self, reason: Reason) {
        if self.problem.is_none() {
            self.problem = Some(Problem {
                game_number: self.games + 1,
                reason,
            });
        }
    }

    fn start_moves(&mut self, tags: Vec<Tag>) -> GameBuilder {
        let start = start_position(&tags).unwrap_or_else(|| {
            self.record_first_problem(Reason::InvalidFen);
            Chess::default()
        });
        GameBuilder::new(tags, start)
    }

    /// Sorts damage by phase. pgn-reader reports a tag line longer than its buffer, which happens
    /// only in the tags, and a comment open at the end of the source, which happens only in the
    /// move text. After a skipped tag line the next `read_game` continues the same game.
    fn recover(&mut self, end: u64) {
        match &mut self.phase {
            Phase::Tags(_) => self.record_first_problem(Reason::UnreadableTagLine),
            Phase::Moves(_) => {
                let text = decode_utf8_or_latin1(&self.comment).trim().to_owned();
                if let Some(builder) = self.unfrozen_builder() {
                    builder.comment(text);
                }
                self.record_first_problem(Reason::UnclosedComment);
                self.finish(Some(end));
            }
        }
    }

    fn collect_comment(&mut self, bytes: &[u8]) {
        if self.unfrozen_builder().is_none() {
            return;
        }
        if self.comment.len() + bytes.len() > MAX_COMMENT_LENGTH {
            self.comment.clear();
            self.record_first_problem(Reason::CommentTooLong);
            return;
        }
        self.comment.extend_from_slice(bytes);
    }

    fn finish_game_left_open(&mut self, end: u64) {
        if self.problem.is_some() {
            self.finish(Some(end));
        }
    }

    fn finish(&mut self, end: Option<u64>) {
        let range = match (mem::replace(&mut self.start, Start::Unset), end) {
            (Start::At(start), Some(end)) => Some(start..end),
            _ => None,
        };
        let builder = match mem::take(&mut self.phase) {
            Phase::Moves(builder) => *builder,
            Phase::Tags(tags) => self.start_moves(tags),
        };
        let marker = self.result.take();
        let game = builder.finish(marker.unwrap_or(Outcome::Unknown));
        let problem = self.problem.take();
        self.comment.clear();
        let holds_nothing = marker.is_none()
            && problem.is_none()
            && game.tags().is_empty()
            && matches!(game.main_line(), MainLine::Empty { comments } if comments.is_empty());
        let entry = match range {
            // An entry without a range has a tag, so it is never stray text.
            Some(range) if holds_nothing => Entry::StrayText {
                after_game: self.games,
                range,
            },
            range => {
                self.games += 1;
                Entry::Game {
                    game: Box::new(game),
                    problem,
                    range,
                }
            }
        };
        (self.each)(entry);
    }
}

impl<F: FnMut(Entry)> Visitor for Collector<F> {
    type Tags = ();
    type Movetext = ();
    type Output = ();

    fn begin_tags(&mut self) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn tag(&mut self, _tags: &mut (), name: &[u8], value: RawTag<'_>) -> ControlFlow<()> {
        let tag = Tag {
            name: decode_utf8_or_latin1(name.trim_ascii()),
            value: decode_utf8_or_latin1(&value.decode()),
        };
        // pgn-reader skips empty lines between tag lines, so after a game without move text the
        // next game arrives in the same tag section. Every game of a file starts with the same
        // tag, so the name of the first tag coming back marks the next game.
        if let Phase::Tags(tags) = &self.phase
            && tags.first().is_some_and(|first| first.name == tag.name)
        {
            self.finish(None);
            self.start = Start::Unknown;
        }
        if let Phase::Tags(tags) = &mut self.phase {
            tags.push(tag);
        }
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, _tags: ()) -> ControlFlow<()> {
        self.phase = match mem::take(&mut self.phase) {
            Phase::Tags(tags) => Phase::Moves(Box::new(self.start_moves(tags))),
            moves @ Phase::Moves(_) => moves,
        };
        ControlFlow::Continue(())
    }

    fn san(&mut self, _movetext: &mut (), san_plus: SanPlus) -> ControlFlow<()> {
        let Some(builder) = self.unfrozen_builder() else {
            return ControlFlow::Continue(());
        };
        if let Err(error) = builder.play(san_plus.san) {
            let position = builder.position();
            let move_text = MoveText {
                text: san_plus.to_string(),
                number: position.fullmoves(),
                side: position.turn(),
            };
            self.record_first_problem(match error {
                MoveError::IllegalMove => Reason::IllegalMove(move_text),
                MoveError::AmbiguousMove => Reason::AmbiguousMove(move_text),
            });
        }
        ControlFlow::Continue(())
    }

    fn nag(&mut self, _movetext: &mut (), nag: RawNag) -> ControlFlow<()> {
        let symbol = AnnotationSymbol(nag.0);
        if let Some(builder) = self.unfrozen_builder()
            && builder.annotate(symbol).is_err()
        {
            self.record_first_problem(Reason::AnnotationWithoutMove(symbol));
        }
        ControlFlow::Continue(())
    }

    fn partial_comment(&mut self, _movetext: &mut (), comment: RawComment<'_>) -> ControlFlow<()> {
        self.collect_comment(comment.as_bytes());
        ControlFlow::Continue(())
    }

    fn comment(&mut self, _movetext: &mut (), comment: RawComment<'_>) -> ControlFlow<()> {
        self.collect_comment(comment.as_bytes());
        let text = decode_utf8_or_latin1(&self.comment).trim().to_owned();
        self.comment.clear();
        if let Some(builder) = self.unfrozen_builder() {
            builder.comment(text);
        }
        ControlFlow::Continue(())
    }

    fn begin_variation(&mut self, _movetext: &mut ()) -> ControlFlow<(), Skip> {
        let Some(builder) = self.unfrozen_builder() else {
            return ControlFlow::Continue(Skip(true));
        };
        let reason = match builder.begin_variation() {
            Ok(()) => return ControlFlow::Continue(Skip(false)),
            Err(VariationError::NoMove) => Reason::VariationWithoutMove,
            Err(VariationError::TooDeep) => Reason::VariationTooDeep,
        };
        // pgn-reader still delivers the closing `)` of a skipped variation. The game is frozen
        // from here, so that `)` cannot close a variation of the builder.
        self.record_first_problem(reason);
        ControlFlow::Continue(Skip(true))
    }

    fn end_variation(&mut self, _movetext: &mut ()) -> ControlFlow<()> {
        if let Some(builder) = self.unfrozen_builder() {
            builder.end_variation();
        }
        ControlFlow::Continue(())
    }

    fn outcome(&mut self, _movetext: &mut (), outcome: Outcome) -> ControlFlow<()> {
        self.result = Some(outcome);
        ControlFlow::Continue(())
    }

    // `read_games` finishes the entry behind this call, where it knows the place of pgn-reader.
    fn end_game(&mut self, _movetext: ()) {}
}

/// Whether a `read_game` error is one of pgn-reader's two errors for damage in the text. Every
/// other error comes from the source, and a source can fail with `InvalidData` as well.
fn is_damage(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::InvalidData
        && matches!(
            error.to_string().as_str(),
            "unterminated tag" | "unterminated comment"
        )
}

fn start_position(tags: &[Tag]) -> Option<Chess> {
    let Some(fen_tag) = tags.iter().find(|tag| tag.name == "FEN") else {
        return Some(Chess::default());
    };
    let fen = Fen::from_ascii(fen_tag.value.as_bytes()).ok()?;
    let mode = CastlingMode::detect(fen.as_setup());
    fen.into_position(mode).ok()
}

fn decode_utf8_or_latin1(bytes: &[u8]) -> String {
    match str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().copied().map(char::from).collect(),
    }
}

/// The newest bytes that pgn-reader was handed. pgn-reader tells only how many bytes it holds
/// unread, so the reader keeps the bytes itself to turn that place into an offset of the source
/// and to step back from it over white space.
#[derive(Default)]
struct Trail {
    kept: Vec<u8>,
    /// How many bytes pgn-reader was handed before `kept`.
    kept_start: u64,
    /// The source offset behind the last byte that left `kept` and is not white space.
    end_before_kept: u64,
    /// Each removed byte order mark puts the source further ahead of the bytes that pgn-reader
    /// was handed: from which of those bytes on, and by how many bytes.
    leads: Vec<(u64, u64)>,
}

impl Trail {
    fn handed(&self) -> u64 {
        self.kept_start + self.kept.len() as u64
    }

    fn hand(&mut self, bytes: &[u8]) {
        if self.kept.len() >= 2 * KEPT_BYTES {
            let dropped = self.kept.len() - KEPT_BYTES;
            if let Some(end) = self.end_in(&self.kept[..dropped]) {
                self.end_before_kept = end;
            }
            self.kept.drain(..dropped);
            self.kept_start += dropped as u64;
            let passed = self.leads_up_to(self.kept_start);
            self.leads.drain(..passed.saturating_sub(1));
        }
        self.kept.extend_from_slice(bytes);
    }

    fn leads_up_to(&self, handed: u64) -> usize {
        self.leads.partition_point(|(from, _)| *from <= handed)
    }

    /// The source holds this many bytes here that pgn-reader does not get.
    fn skip(&mut self, source_bytes: u64) {
        let lead = self.leads.last().map_or(0, |(_, lead)| *lead);
        self.leads.push((self.handed(), lead + source_bytes));
    }

    fn in_source(&self, handed: u64) -> u64 {
        let lead = self.leads[..self.leads_up_to(handed)].last();
        handed + lead.map_or(0, |(_, lead)| *lead)
    }

    /// The source offset behind the last of the first kept bytes that is not white space.
    fn end_in(&self, kept: &[u8]) -> Option<u64> {
        let last = kept
            .iter()
            .rposition(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))?;
        Some(self.in_source(self.kept_start + last as u64) + 1)
    }

    /// pgn-reader does not say how many bytes it buffers. If it ever holds more unread bytes
    /// than `KEPT_BYTES`, fail here and do not report a wrong range.
    fn kept_before(&self, place: u64) -> io::Result<&[u8]> {
        place
            .checked_sub(self.kept_start)
            .and_then(|length| self.kept.get(..usize::try_from(length).ok()?))
            .ok_or_else(|| io::Error::other("pgn-reader fell behind the bytes kept for it"))
    }

    fn start_at(&self, place: u64) -> io::Result<u64> {
        self.kept_before(place)?;
        Ok(self.in_source(place))
    }

    fn end_before(&self, place: u64) -> io::Result<u64> {
        let kept = self.kept_before(place)?;
        Ok(self.end_in(kept).unwrap_or(self.end_before_kept))
    }
}

const MARKED_TAG_LINE: &str = "\u{feff}[";

/// Removes a UTF-8 byte order mark directly in front of `[`. At the start of a line, or of the
/// source, the mark is dropped. Anywhere else it becomes a line break.
///
/// Files joined from several files carry such marks. pgn-reader skips a mark only at the very
/// start of a game, before it skips whitespace, so a mark after an empty line would hide the
/// tag line behind it. A file joined without a final line break puts the mark at the end of the
/// last line of move text, and the line break makes the tag line behind it start a line.
struct WithoutByteOrderMarks<'trail, R> {
    source: R,
    trail: &'trail RefCell<Trail>,
    buffer: Box<[u8]>,
    start: usize,
    end: usize,
    at_line_start: bool,
    source_done: bool,
}

impl<'trail, R: Read> WithoutByteOrderMarks<'trail, R> {
    fn new(source: R, trail: &'trail RefCell<Trail>) -> WithoutByteOrderMarks<'trail, R> {
        WithoutByteOrderMarks {
            source,
            trail,
            buffer: vec![0; 1 << 16].into_boxed_slice(),
            start: 0,
            end: 0,
            at_line_start: true,
            source_done: false,
        }
    }

    fn refill(&mut self) -> io::Result<()> {
        self.buffer.copy_within(self.start..self.end, 0);
        self.end -= self.start;
        self.start = 0;
        let read = self.source.read(&mut self.buffer[self.end..])?;
        if read == 0 {
            self.source_done = true;
        }
        self.end += read;
        Ok(())
    }
}

impl<R: Read> Read for WithoutByteOrderMarks<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            let mark = MARKED_TAG_LINE.as_bytes();
            let window = &self.buffer[self.start..self.end];
            let undecided =
                window.is_empty() || (window[0] == mark[0] && window.len() < mark.len());
            if undecided && !self.source_done {
                self.refill()?;
                continue;
            }
            if window.is_empty() {
                return Ok(0);
            }
            if window.starts_with(mark) {
                self.start += 3;
                let mut trail = self.trail.borrow_mut();
                if self.at_line_start {
                    trail.skip(3);
                    continue;
                }
                out[0] = b'\n';
                trail.hand(b"\n");
                trail.skip(2);
                self.at_line_start = true;
                return Ok(1);
            }
            let limit = window.len().min(out.len());
            let next_possible_mark = (1..limit).find(|&index| window[index] == mark[0]);
            let stop = next_possible_mark.unwrap_or(limit);
            out[..stop].copy_from_slice(&window[..stop]);
            self.trail.borrow_mut().hand(&window[..stop]);
            self.at_line_start = window[stop - 1] == b'\n';
            self.start += stop;
            return Ok(stop);
        }
    }
}
