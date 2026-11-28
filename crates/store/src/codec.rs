use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, KnownOutcome, Outcome, Position, PositionError,
    Role, Setup, Square,
};
use zobrist_core::game::{AnnotationSymbol, Game, GameBuilder, Line, MainLine, Move, Tag};

use crate::{PgnGameRow, read_bytes};

pub(super) const GAME_WIDTH: u64 = 18;

const END_GAME: u8 = 0;
const MAIN_MOVE: u8 = 1;
const VARIATION_MOVE: u8 = 2;
const BEGIN_VARIATION: u8 = 3;
const END_VARIATION: u8 = 4;
const COMMENT: u8 = 5;
const SYMBOL: u8 = 6;

#[derive(Default)]
pub(super) struct Encoder {
    names: HashMap<Box<str>, u64>,
    values: HashMap<Box<str>, u64>,
    move_index: u64,
}

impl Encoder {
    pub(super) fn push(&mut self, game: &Game, parts: &mut [impl Write; 7]) -> io::Result<()> {
        let [games, moves, starts, names, values, tags, annotations] = parts;
        tags.write_all(&(game.tags().len() as u64).to_le_bytes())?;
        for tag in game.tags() {
            let name = intern(&mut self.names, names, &tag.name)?;
            let value = intern(&mut self.values, values, &tag.value)?;
            tags.write_all(&name.to_le_bytes())?;
            tags.write_all(&value.to_le_bytes())?;
        }

        let start = game.start_position();
        let mode = start.castles().mode();
        let fen = Fen::from_position(start, EnPassantMode::Always);
        let has_start = mode.is_chess960() || *fen.as_setup() != Setup::initial();
        if has_start {
            starts.write_all(&[u8::from(mode.is_chess960())])?;
            write_string(starts, &fen.to_string())?;
        }

        let first = self.move_index;
        let annotated = match game.main_line() {
            MainLine::Empty { comments } => {
                write_comments(annotations, comments)?;
                !comments.is_empty()
            }
            MainLine::Moves(line) => {
                let mut annotated = false;
                for node in line.nodes() {
                    write_move(moves, node.played())?;
                    self.move_index = self.move_index.checked_add(1).ok_or_else(damaged)?;
                    annotated |= !node.comments_before().is_empty()
                        || !node.comments_after().is_empty()
                        || !node.annotation_symbols().is_empty()
                        || !node.variations().is_empty();
                }
                if annotated {
                    write_line(annotations, line, true)?;
                }
                annotated
            }
        };
        if annotated {
            annotations.write_all(&[END_GAME])?;
        }
        let result = match game.result() {
            Outcome::Unknown => 0,
            Outcome::Known(KnownOutcome::Decisive {
                winner: Color::White,
            }) => 1,
            Outcome::Known(KnownOutcome::Decisive {
                winner: Color::Black,
            }) => 2,
            Outcome::Known(KnownOutcome::Draw) => 3,
        };
        games.write_all(&first.to_le_bytes())?;
        games.write_all(&(self.move_index - first).to_le_bytes())?;
        games.write_all(&[result, u8::from(has_start) | (u8::from(annotated) << 1)])
    }
}

fn intern(
    dictionary: &mut HashMap<Box<str>, u64>,
    part: &mut impl Write,
    text: &str,
) -> io::Result<u64> {
    if let Some(&index) = dictionary.get(text) {
        return Ok(index);
    }
    let index = dictionary.len() as u64;
    write_string(part, text)?;
    dictionary.insert(text.into(), index);
    Ok(index)
}

fn write_line(output: &mut impl Write, line: &Line, main: bool) -> io::Result<()> {
    for node in line.nodes() {
        write_comments(output, node.comments_before())?;
        if main {
            output.write_all(&[MAIN_MOVE])?;
        } else {
            output.write_all(&[VARIATION_MOVE])?;
            write_move(output, node.played())?;
        }
        for symbol in node.annotation_symbols() {
            output.write_all(&[SYMBOL, symbol.0])?;
        }
        write_comments(output, node.comments_after())?;
        for variation in node.variations() {
            output.write_all(&[BEGIN_VARIATION])?;
            write_line(output, variation, false)?;
            output.write_all(&[END_VARIATION])?;
        }
    }
    Ok(())
}

fn write_comments(output: &mut impl Write, comments: &[String]) -> io::Result<()> {
    for comment in comments {
        output.write_all(&[COMMENT])?;
        write_string(output, comment)?;
    }
    Ok(())
}

fn write_string(output: &mut impl Write, text: &str) -> io::Result<()> {
    output.write_all(&(text.len() as u64).to_le_bytes())?;
    output.write_all(text.as_bytes())
}

fn write_move(output: &mut impl Write, played: Move) -> io::Result<()> {
    let word = match played {
        Move::Null => u16::MAX,
        Move::Legal(chess_move) => {
            let from = chess_move.from().ok_or_else(damaged)?;
            let promotion = chess_move.promotion().map_or(0, u16::from);
            u16::from(from) | (u16::from(chess_move.to()) << 6) | (promotion << 12)
        }
    };
    output.write_all(&word.to_le_bytes())
}

pub(super) struct Decoder<R> {
    games: R,
    moves: R,
    starts: R,
    tags: R,
    annotations: R,
    names: Vec<Box<str>>,
    values: Vec<Box<str>>,
    remaining: u64,
    move_index: u64,
}

impl<R: BufRead> Decoder<R> {
    pub(super) fn new(parts: [R; 7], game_count: u64) -> io::Result<Self> {
        let [games, moves, starts, names, values, tags, annotations] = parts;
        Ok(Self {
            games,
            moves,
            starts,
            tags,
            annotations,
            names: read_dictionary(names)?,
            values: read_dictionary(values)?,
            remaining: game_count,
            move_index: 0,
        })
    }

    pub(super) fn next_row(&mut self, number: u64) -> io::Result<Option<PgnGameRow>> {
        if self.remaining == 0 {
            if !self.games.fill_buf()?.is_empty() || !self.tags.fill_buf()?.is_empty() {
                return Err(damaged());
            }
            return Ok(None);
        }
        let (_, result, _) = self.header()?;
        let tags = self.read_tags()?;
        self.remaining -= 1;
        Ok(Some(PgnGameRow::from_tags(number, &tags, result)))
    }

    fn header(&mut self) -> io::Result<(u64, Outcome, u8)> {
        if u64::from_le_bytes(read_bytes(&mut self.games)?) != self.move_index {
            return Err(damaged());
        }
        let count = u64::from_le_bytes(read_bytes(&mut self.games)?);
        self.move_index = self.move_index.checked_add(count).ok_or_else(damaged)?;
        let [result, flags] = read_bytes(&mut self.games)?;
        let result = match result {
            0 => Outcome::Unknown,
            1 => KnownOutcome::Decisive {
                winner: Color::White,
            }
            .into(),
            2 => KnownOutcome::Decisive {
                winner: Color::Black,
            }
            .into(),
            3 => KnownOutcome::Draw.into(),
            _ => return Err(damaged()),
        };
        if flags > 3 {
            return Err(damaged());
        }
        Ok((count, result, flags))
    }

    fn read_tags(&mut self) -> io::Result<Vec<Tag>> {
        let mut tags = Vec::new();
        for _ in 0..u64::from_le_bytes(read_bytes(&mut self.tags)?) {
            tags.push(Tag {
                name: read_reference(&mut self.tags, &self.names)?,
                value: read_reference(&mut self.tags, &self.values)?,
            });
        }
        Ok(tags)
    }

    pub(super) fn next_game(&mut self) -> io::Result<Option<Game>> {
        if self.remaining == 0 {
            self.check_end()?;
            return Ok(None);
        }
        let (count, result, flags) = self.header()?;
        let start = if flags & 1 == 0 {
            Chess::new()
        } else {
            let mode = match read_bytes(&mut self.starts)? {
                [0] => CastlingMode::Standard,
                [1] => CastlingMode::Chess960,
                _ => return Err(damaged()),
            };
            let fen: Fen = read_string(&mut self.starts)?
                .parse()
                .map_err(|_| damaged())?;
            // These two conditions can be present in a Chess that a caller supplies.
            fen.into_position(mode)
                .or_else(PositionError::ignore_too_much_material)
                .or_else(PositionError::ignore_impossible_check)
                .map_err(|_| damaged())?
        };

        let tags = self.read_tags()?;
        let mut builder = GameBuilder::new(tags, start);
        if flags & 2 == 0 {
            for _ in 0..count {
                read_move(&mut self.moves, &mut builder)?;
            }
        } else {
            read_annotations(&mut self.annotations, &mut self.moves, &mut builder, count)?;
        }
        self.remaining -= 1;
        if self.remaining == 0 {
            self.check_end()?;
        }
        Ok(Some(builder.finish(result)))
    }

    fn check_end(&mut self) -> io::Result<()> {
        for part in [
            &mut self.games,
            &mut self.moves,
            &mut self.starts,
            &mut self.tags,
            &mut self.annotations,
        ] {
            if !part.fill_buf()?.is_empty() {
                return Err(damaged());
            }
        }
        Ok(())
    }
}

fn read_annotations(
    annotations: &mut impl BufRead,
    moves: &mut impl Read,
    builder: &mut GameBuilder,
    mut main_moves: u64,
) -> io::Result<()> {
    let mut depth = 0_u32;
    let mut has_move = false;
    loop {
        match read_bytes(annotations)? {
            [END_GAME] if depth == 0 && main_moves == 0 => return Ok(()),
            [MAIN_MOVE] if depth == 0 && main_moves > 0 => {
                read_move(moves, builder)?;
                main_moves -= 1;
                has_move = true;
            }
            [VARIATION_MOVE] if depth > 0 => {
                read_move(annotations, builder)?;
                has_move = true;
            }
            [BEGIN_VARIATION] => {
                builder.begin_variation().map_err(|_| damaged())?;
                depth += 1;
                has_move = false;
            }
            [END_VARIATION] if depth > 0 && has_move => {
                builder.end_variation();
                depth -= 1;
            }
            [COMMENT] => builder.comment(read_string(annotations)?),
            [SYMBOL] => builder
                .annotate(AnnotationSymbol(read_bytes::<1>(annotations)?[0]))
                .map_err(|_| damaged())?,
            _ => return Err(damaged()),
        }
    }
}

fn read_move(input: &mut impl Read, builder: &mut GameBuilder) -> io::Result<()> {
    let word = u16::from_le_bytes(read_bytes(input)?);
    let played = if word == u16::MAX {
        UciMove::Null
    } else {
        let promotion = match word >> 12 {
            0 => None,
            2 => Some(Role::Knight),
            3 => Some(Role::Bishop),
            4 => Some(Role::Rook),
            5 => Some(Role::Queen),
            _ => return Err(damaged()),
        };
        UciMove::Normal {
            from: Square::new(u32::from(word & 63)),
            to: Square::new(u32::from((word >> 6) & 63)),
            promotion,
        }
    };
    builder.play_uci(played).map_err(|_| damaged())
}

fn read_dictionary(mut part: impl BufRead) -> io::Result<Vec<Box<str>>> {
    let mut entries = Vec::new();
    while !part.fill_buf()?.is_empty() {
        entries.push(read_string(&mut part)?.into_boxed_str());
    }
    Ok(entries)
}

fn read_reference(part: &mut impl Read, dictionary: &[Box<str>]) -> io::Result<String> {
    let index = usize::try_from(u64::from_le_bytes(read_bytes(part)?)).map_err(|_| damaged())?;
    dictionary
        .get(index)
        .map(ToString::to_string)
        .ok_or_else(damaged)
}

fn read_string(input: &mut impl BufRead) -> io::Result<String> {
    let mut remaining =
        usize::try_from(u64::from_le_bytes(read_bytes(input)?)).map_err(|_| damaged())?;
    let mut bytes = Vec::new();
    while remaining > 0 {
        let buffer = input.fill_buf()?;
        let count = buffer.len().min(remaining);
        if count == 0 {
            return Err(damaged());
        }
        bytes.extend_from_slice(&buffer[..count]);
        input.consume(count);
        remaining -= count;
    }
    String::from_utf8(bytes).map_err(|_| damaged())
}

fn damaged() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "damaged game data")
}
