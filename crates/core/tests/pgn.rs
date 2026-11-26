//! The PGN reader, called on bytes the way `zobrist pgn check` calls it on a file.

use std::io::{self, Read};
use std::num::NonZeroU32;
use std::ops::Range;

use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, KnownOutcome, Outcome, Position};
use zobrist_core::game::{AnnotationSymbol, Counts, Game, GameBuilder, MainLine, Tag};
use zobrist_core::pgn::{self, Entry, MoveText, Problem, Reason};

const CLEAN: &[u8] = include_bytes!("../../cli/tests/fixtures/clean.pgn");
const DAMAGED: &[u8] = include_bytes!("../../cli/tests/fixtures/damaged.pgn");

fn read_source(source: impl Read) -> (Vec<Entry>, io::Result<()>) {
    let mut entries = Vec::new();
    let result = pgn::read_games(source, |entry| entries.push(entry));
    (entries, result)
}

fn read(bytes: &[u8]) -> io::Result<Vec<Entry>> {
    let (entries, result) = read_source(bytes);
    result.map(|()| entries)
}

fn split(entry: &Entry) -> Option<(&Game, Option<&Problem>)> {
    match entry {
        Entry::Game { game, problem, .. } => Some((game.as_ref(), problem.as_ref())),
        Entry::StrayText { .. } => None,
    }
}

fn ranges(source: &[u8]) -> io::Result<Vec<Option<Range<u64>>>> {
    let range = |entry| match entry {
        Entry::Game { range, .. } => range,
        Entry::StrayText { range, .. } => Some(range),
    };
    Ok(read(source)?.into_iter().map(range).collect())
}

fn notation(game: &Game) -> Vec<String> {
    game.steps()
        .map(|step| step.notation().to_string())
        .collect()
}

fn tag(name: &str, value: &str) -> Tag {
    Tag {
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

struct OneByteAtATime<'bytes>(&'bytes [u8]);

impl Read for OneByteAtATime<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match (self.0.split_first(), buffer.first_mut()) {
            (Some((byte, rest)), Some(slot)) => {
                *slot = *byte;
                self.0 = rest;
                Ok(1)
            }
            (None, _) | (_, None) => Ok(0),
        }
    }
}

struct AlwaysInvalidData;

impl Read for AlwaysInvalidData {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the source is broken",
        ))
    }
}

#[test]
fn returns_an_invalid_data_error_of_the_source() {
    let (entries, result) = read_source(AlwaysInvalidData);

    assert_eq!(entries, []);
    let error = result.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.to_string(), "the source is broken");
}

#[test]
fn reads_the_tags_moves_and_results_of_clean_pgn() {
    let entries = read(CLEAN).unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(
        first.tags(),
        [
            tag("Event", "Example"),
            tag("Site", "?"),
            tag("Date", "2024.01.01"),
            tag("Round", "1"),
            tag("White", "Alpha"),
            tag("Black", "Beta"),
            tag("Result", "1-0"),
        ]
    );
    assert_eq!(
        notation(first),
        ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"]
    );
    assert_eq!(
        first.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::White
        })
    );

    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(
        second.tags(),
        [
            tag("Event", "Example"),
            tag("Site", "?"),
            tag("Date", "2024.01.01"),
            tag("Round", "2"),
            tag("White", "Gamma"),
            tag("Black", "Delta"),
            tag("Result", "0-1"),
        ]
    );
    assert_eq!(notation(second), ["f3", "e5", "g4", "Qh4#"]);
    assert_eq!(
        second.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::Black
        })
    );
}

const JOINED: &[u8] = b"\xEF\xBB\xBF[Event \"First\"]\n\
    [White \"Alpha\"]\n\
    \n\
    1. e4 {A quiet start.\n\
    [Event \"Second\"] is not a tag here.} e5 *\n\
    \n\
    \xEF\xBB\xBF[Event \"Second\"]\n\
    [White \"Gamma\"]\n\
    \n\
    1. d4 d5 1/2-1/2\n";

#[test]
fn reads_files_joined_with_byte_order_marks_and_a_comment_line_that_looks_like_a_tag() {
    let entries = read(JOINED).unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(first.tags(), [tag("Event", "First"), tag("White", "Alpha")]);
    assert_eq!(notation(first), ["e4", "e5"]);
    let e4 = first.steps().next().unwrap();
    assert_eq!(
        e4.node().comments_after(),
        ["A quiet start.\n[Event \"Second\"] is not a tag here."]
    );
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(
        second.tags(),
        [tag("Event", "Second"), tag("White", "Gamma")]
    );
    assert_eq!(notation(second), ["d4", "d5"]);
    assert_eq!(second.result(), Outcome::Known(KnownOutcome::Draw));
}

#[test]
fn reads_the_same_games_from_a_source_that_hands_out_one_byte_at_a_time() {
    let (entries, result) = read_source(OneByteAtATime(JOINED));
    result.unwrap();

    assert_eq!(entries.len(), 2);
    let (second, _) = split(&entries[1]).unwrap();
    assert_eq!(second.tag("Event"), Some("Second"));
    assert_eq!(entries, read(JOINED).unwrap());
}

const GLUED_WITH_A_MARK: &[u8] =
    b"[White \"A\"]\n\n1. e4 1-0\xEF\xBB\xBF[White \"C\"]\n\n1. d4 *\n";

#[test]
fn reads_a_tag_line_behind_a_byte_order_mark_in_the_middle_of_a_line() {
    let entries = read(GLUED_WITH_A_MARK).unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(first.tags(), [tag("White", "A")]);
    assert_eq!(notation(first), ["e4"]);
    assert_eq!(
        first.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::White
        })
    );
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(second.tags(), [tag("White", "C")]);
    assert_eq!(notation(second), ["d4"]);
    let (one_byte_at_a_time, result) = read_source(OneByteAtATime(GLUED_WITH_A_MARK));
    result.unwrap();
    assert_eq!(one_byte_at_a_time, entries);
}

#[test]
fn reads_the_variation_comment_and_symbol_of_clean_pgn() {
    let entries = read(CLEAN).unwrap();
    let (first, _) = split(&entries[0]).unwrap();
    let knight = first.steps().nth(5).unwrap();

    assert_eq!(knight.notation().to_string(), "Nf6");
    assert_eq!(knight.node().annotation_symbols(), [AnnotationSymbol(4)]);
    assert_eq!(knight.node().comments_after(), ["Overlooks the mate."]);
    let variations: Vec<Vec<String>> = knight
        .variations()
        .map(|steps| steps.map(|step| step.notation().to_string()).collect())
        .collect();
    assert_eq!(variations, [["g6", "Qf3"]]);
    let g6 = knight.variations().next().unwrap().next().unwrap();
    assert_eq!(u32::from(g6.position_before().fullmoves()), 3);
    assert_eq!(g6.position_before().turn(), Color::Black);
    assert_eq!(
        first.counts(),
        Counts {
            main_line_moves: 7,
            variations: 1,
            comments: 1,
        }
    );
}

#[test]
fn reads_null_moves_written_as_dashes_and_as_z0() {
    let entries = read(b"[White \"A\"]\n\n1. e4 -- 2. d4 Z0 3. c4 *\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(problem, None);
    assert_eq!(notation(game), ["e4", "--", "d4", "--", "c4"]);
}

#[test]
fn reads_nested_variations_with_comments_and_symbols() {
    let entries =
        read(b"1. e4 e5 (1... c5 {Sicilian} 2. Nf3 (2. c3 $6) d6 !?) (1... e6) 2. Nf3 $1 *\n")
            .unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(problem, None);
    assert_eq!(notation(game), ["e4", "e5", "Nf3"]);
    let e5 = game.steps().nth(1).unwrap();
    let variations: Vec<Vec<String>> = e5
        .variations()
        .map(|steps| steps.map(|step| step.notation().to_string()).collect())
        .collect();
    assert_eq!(variations, [vec!["c5", "Nf3", "d6"], vec!["e6"]]);
    let sicilian: Vec<_> = e5.variations().next().unwrap().collect();
    assert_eq!(sicilian[0].node().comments_after(), ["Sicilian"]);
    let alapin: Vec<Vec<String>> = sicilian[1]
        .variations()
        .map(|steps| steps.map(|step| step.notation().to_string()).collect())
        .collect();
    assert_eq!(alapin, [["c3"]]);
    assert_eq!(
        sicilian[1].node().variations()[0]
            .nodes()
            .next()
            .unwrap()
            .annotation_symbols(),
        [AnnotationSymbol(6)]
    );
    assert_eq!(
        sicilian[2].node().annotation_symbols(),
        [AnnotationSymbol(5)]
    );
    let nf3 = game.steps().nth(2).unwrap();
    assert_eq!(nf3.node().annotation_symbols(), [AnnotationSymbol(1)]);
    assert_eq!(
        game.counts(),
        Counts {
            main_line_moves: 3,
            variations: 3,
            comments: 1,
        }
    );
}

#[test]
fn puts_a_comment_after_a_variation_before_the_next_move() {
    let entries = read(b"1. e4 (1. d4) {Back to the game.} 1... e5 *\n").unwrap();
    let (game, _) = split(&entries[0]).unwrap();

    let e4 = game.steps().next().unwrap();
    assert_eq!(e4.node().comments_after(), [] as [String; 0]);
    let e5 = game.steps().nth(1).unwrap();
    assert_eq!(e5.node().comments_before(), ["Back to the game."]);
}

#[test]
fn keeps_a_game_without_moves_and_its_comment() {
    let entries =
        read(b"[White \"A\"]\n\n{White forfeits.} 0-1\n\n[White \"B\"]\n\n1. e4 *\n").unwrap();

    assert_eq!(entries.len(), 2);
    let (game, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(
        game.main_line(),
        &MainLine::Empty {
            comments: vec!["White forfeits.".to_owned()]
        }
    );
    assert_eq!(
        game.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::Black
        })
    );
}

#[test]
fn reads_a_comment_longer_than_the_buffer_of_the_reader_as_one_comment() {
    let long = "word ".repeat(40_000);
    assert_eq!(long.len(), 200_000);
    let entries = read(format!("1. e4 {{{long}}} *\n").as_bytes()).unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(problem, None);
    assert_eq!(
        game.steps().next().unwrap().node().comments_after(),
        [long.trim_end()]
    );
}

#[test]
fn reads_a_tag_value_of_40000_bytes() {
    let annotator = "a".repeat(40_000);
    let entries =
        read(format!("[White \"A\"]\n[Annotator \"{annotator}\"]\n\n1. e4 *\n").as_bytes())
            .unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(problem, None);
    assert_eq!(
        game.tags(),
        [tag("White", "A"), tag("Annotator", &annotator)]
    );
    assert_eq!(notation(game), ["e4"]);
}

#[test]
fn keeps_a_comment_of_exactly_the_length_limit_whole() {
    let long = "a".repeat(1 << 20);
    let entries = read(format!("1. e4 {{{long}}} *\n").as_bytes()).unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(problem, None);
    assert_eq!(game.steps().next().unwrap().node().comments_after(), [long]);
}

#[test]
fn reports_a_comment_one_byte_longer_than_the_limit_and_keeps_the_moves_before_it() {
    let long = "a".repeat((1 << 20) + 1);
    let bytes =
        format!("[White \"A\"]\n\n1. e4 e5 {{{long}}} 2. Nf3 1-0\n\n[White \"B\"]\n\n1. d4 *\n");
    let entries = read(bytes.as_bytes()).unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::CommentTooLong
        })
    );
    assert_eq!(
        problem.map(|problem| problem.reason.to_string()),
        Some("comment is too long to read".to_owned())
    );
    assert_eq!(notation(first), ["e4", "e5"]);
    assert_eq!(first.counts().comments, 0);
    assert_eq!(
        first.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::White
        })
    );
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(notation(second), ["d4"]);
}

#[test]
fn reads_a_chess960_game_that_castles_from_a_fen_tag() {
    let entries = read(
        b"[Variant \"Chess960\"]\n\
          [FEN \"rbbqnknr/pppppppp/8/8/8/8/PPPPPPPP/RBBQNKNR w HAha - 0 1\"]\n\
          \n\
          1. Nh3 Nh6 2. O-O e6 *\n",
    )
    .unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(problem, None);
    assert_eq!(
        Fen::from_position(game.start_position(), EnPassantMode::Legal).to_string(),
        "rbbqnknr/pppppppp/8/8/8/8/PPPPPPPP/RBBQNKNR w KQkq - 0 1"
    );
    assert_eq!(notation(game), ["Nh3", "Nh6", "O-O", "e6"]);
    let after_castling = game.steps().nth(3).unwrap();
    assert_eq!(
        Fen::from_position(after_castling.position_before(), EnPassantMode::Legal).to_string(),
        "rbbqnk1r/pppppppp/7n/8/8/7N/PPPPPPPP/RBBQNRK1 b kq - 3 2"
    );
}

#[test]
fn trims_spaces_around_a_tag_name() {
    let entries = read(
        b"[ White \"A\"]\n\
          [FEN  \"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1\"]\n\
          \n\
          1... e5 *\n",
    )
    .unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(
        game.tags(),
        [
            tag("White", "A"),
            tag(
                "FEN",
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
            ),
        ]
    );
    assert_eq!(problem, None);
    assert_eq!(
        Fen::from_position(game.start_position(), EnPassantMode::Legal).to_string(),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
    );
    assert_eq!(notation(game), ["e5"]);
}

#[test]
fn reports_a_fen_tag_that_is_not_a_legal_position_and_keeps_the_tags() {
    let entries =
        read(b"[White \"A\"]\n[FEN \"8/8/8/8/8/8/8/8 w - - 0 1\"]\n\n1. e4 1/2-1/2\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::InvalidFen
        })
    );
    assert_eq!(game.steps().count(), 0);
    assert_eq!(game.tag("FEN"), Some("8/8/8/8/8/8/8/8 w - - 0 1"));
    assert_eq!(game.tag("White"), Some("A"));
    assert_eq!(
        Fen::from_position(game.start_position(), EnPassantMode::Legal).to_string(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
    assert_eq!(game.result(), Outcome::Known(KnownOutcome::Draw));
}

#[test]
fn reports_a_fen_tag_that_cannot_be_read() {
    let entries = read(b"[FEN \"not a position\"]\n\n1. e4 *\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(
        problem.map(|problem| &problem.reason),
        Some(&Reason::InvalidFen)
    );
    assert_eq!(game.steps().count(), 0);
}

#[test]
fn known_limit_a_word_that_is_not_a_move_is_skipped_without_a_report() {
    let entries = read(b"1. e4 e5 2. Dh5 Nc6 *\n\n1. e4 e5 2. Dh5 *\n").unwrap();

    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(notation(first), ["e4", "e5"]);
    assert_eq!(
        problem.map(|problem| problem.reason.to_string()),
        Some("illegal move \"Nc6\" at White's move 2".to_owned())
    );
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(notation(second), ["e4", "e5"]);
    assert_eq!(problem, None);
}

#[test]
fn reports_the_illegal_move_of_damaged_pgn_and_keeps_every_game() {
    let entries = read(DAMAGED).unwrap();

    assert_eq!(entries.len(), 3);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(
        notation(first),
        ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"]
    );
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 2,
            reason: Reason::IllegalMove(MoveText {
                text: "Ke3".to_owned(),
                number: NonZeroU32::new(2).unwrap(),
                side: Color::White,
            }),
        })
    );
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "illegal move \"Ke3\" at White's move 2"
    );
    assert_eq!(second.tag("White"), Some("Gamma"));
    assert_eq!(notation(second), ["e4", "e5"]);
    assert_eq!(second.result(), Outcome::Unknown);
    let (third, problem) = split(&entries[2]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(notation(third), ["f3", "e5", "g4", "Qh4#"]);
    assert_eq!(
        third.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::Black
        })
    );
}

#[test]
fn reports_an_ambiguous_move_by_black() {
    let entries = read(b"1. e4 Nf6 2. d4 d6 3. Nf3 Nd7 *\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(game.counts().main_line_moves, 5);
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "ambiguous move \"Nd7\" at Black's move 3"
    );
}

#[test]
fn ignores_everything_after_the_first_problem_except_the_result_marker() {
    let entries =
        read(b"1. e4 e5 2. Ke3 $2 Nc6 (2... Nf6 3. Nc3) {late comment} 3. Kd7 1-0\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(notation(game), ["e4", "e5"]);
    assert_eq!(
        game.counts(),
        Counts {
            main_line_moves: 2,
            variations: 0,
            comments: 0,
        }
    );
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "illegal move \"Ke3\" at White's move 2"
    );
    assert_eq!(
        game.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::White
        })
    );
}

#[test]
fn reports_a_variation_before_the_first_move_of_a_line() {
    let entries = read(b"1. e4 ((1. d4) 1. c4) e5 *\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::VariationWithoutMove
        })
    );
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "variation before the first move of a line"
    );
    assert_eq!(notation(game), ["e4"]);
    assert_eq!(game.counts().variations, 0);
}

#[test]
fn reports_a_variation_nested_deeper_than_255_and_keeps_the_game() {
    let bytes = format!(
        "1. e4 {}{} 1... e5 *\n\n1. d4 *\n",
        "(1. d4 ".repeat(256),
        ")".repeat(256)
    );
    let entries = read(bytes.as_bytes()).unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::VariationTooDeep
        })
    );
    assert_eq!(
        problem.map(|problem| problem.reason.to_string()),
        Some("variation is nested more than 255 deep".to_owned())
    );
    assert_eq!(notation(first), ["e4"]);
    assert_eq!(first.counts().variations, 255);
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(notation(second), ["d4"]);
}

#[test]
fn reports_an_annotation_symbol_before_the_first_move() {
    let entries = read(b"$1 1. e4 *\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(
        problem.map(|problem| &problem.reason),
        Some(&Reason::AnnotationWithoutMove(AnnotationSymbol(1)))
    );
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "annotation symbol $1 before the first move of a line"
    );
    assert_eq!(game.steps().count(), 0);
}

#[test]
fn reads_a_latin1_player_name_and_comment() {
    let entries = read(b"[White \"M\xFCller\"]\n\n1. e4 {Stra\xDFe} *\n").unwrap();
    let (game, problem) = split(&entries[0]).unwrap();

    assert_eq!(problem, None);
    assert_eq!(game.tag("White"), Some("Müller"));
    assert_eq!(
        game.steps().next().unwrap().node().comments_after(),
        ["Straße"]
    );
}

#[test]
fn reads_a_utf8_player_name() {
    let entries = read("[White \"Müller\"]\n\n1. e4 *\n".as_bytes()).unwrap();
    let (game, _) = split(&entries[0]).unwrap();

    assert_eq!(game.tag("White"), Some("Müller"));
}

#[test]
fn reports_stray_text_between_games_and_after_the_last_game() {
    let entries = read(
        b"[White \"A\"]\n\n1. e4 *\n\n\
          Exported with care\n\n\
          [White \"B\"]\n\n1. d4 d5 2. Ke3 *\n\n\
          trailing words\n",
    )
    .unwrap();

    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries[1],
        Entry::StrayText {
            after_game: 1,
            range: 22..40
        }
    );
    let (second, problem) = split(&entries[2]).unwrap();
    assert_eq!(second.tag("White"), Some("B"));
    assert_eq!(problem.map(|problem| problem.game_number), Some(2));
    assert_eq!(
        entries[3],
        Entry::StrayText {
            after_game: 2,
            range: 74..88
        }
    );
}

#[test]
fn keeps_text_without_tags_whose_first_move_is_illegal_as_a_game_with_its_problem() {
    let entries = read(b"[White \"A\"]\n\n1. e4 *\n\n1. Ke3\n").unwrap();

    assert_eq!(entries.len(), 2);
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(second.tags(), &[] as &[Tag]);
    assert_eq!(notation(second), Vec::<String>::new());
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 2,
            reason: Reason::IllegalMove(MoveText {
                text: "Ke3".to_owned(),
                number: NonZeroU32::new(1).unwrap(),
                side: Color::White,
            })
        })
    );
}

#[test]
fn keeps_a_lone_tag_line_cut_off_at_the_end_of_the_source_as_a_game_with_its_problem() {
    let entries = read(b"[White \"A\"]\n\n1. e4 *\n\n[Event \"cut off").unwrap();

    assert_eq!(entries.len(), 2);
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(second.tags(), &[] as &[Tag]);
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 2,
            reason: Reason::UnreadableTagLine
        })
    );
}

#[test]
fn starts_a_new_game_at_the_name_of_the_first_tag_of_the_open_tag_section() {
    let entries = read(b"[White \"Alpha\"]\n\n[White \"Gamma\"]\n\n1. e4 e5 2. Ke3 1-0\n").unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(first.tags(), [tag("White", "Alpha")]);
    assert_eq!(first.main_line(), &MainLine::Empty { comments: vec![] });
    assert_eq!(first.result(), Outcome::Unknown);
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(second.tags(), [tag("White", "Gamma")]);
    assert_eq!(notation(second), ["e4", "e5"]);
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 2,
            reason: Reason::IllegalMove(MoveText {
                text: "Ke3".to_owned(),
                number: NonZeroU32::new(2).unwrap(),
                side: Color::White,
            }),
        })
    );
    assert_eq!(
        second.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::White
        })
    );
}

#[test]
fn gives_the_problem_of_a_game_split_off_by_a_repeated_first_tag_to_that_game() {
    let entries =
        read(b"[White \"A\"]\n[FEN \"8/8/8/8/8/8/8/8 w - - 0 1\"]\n\n[White \"B\"]\n\n1. e4 *\n")
            .unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::InvalidFen
        })
    );
    assert_eq!(
        first.tags(),
        [tag("White", "A"), tag("FEN", "8/8/8/8/8/8/8/8 w - - 0 1")]
    );
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(second.tags(), [tag("White", "B")]);
    assert_eq!(notation(second), ["e4"]);
}

#[test]
fn keeps_the_problem_of_a_failed_tag_line_with_its_game_when_the_first_tag_comes_back() {
    let mut bytes = b"[Event \"A\"]\n[Annotator \"".to_vec();
    bytes.extend_from_slice("a".repeat(200_000).as_bytes());
    bytes.extend_from_slice(b"\"]\n\n[Event \"B\"]\n\n1. e4 *\n");
    let entries = read(&bytes).unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::UnreadableTagLine
        })
    );
    assert_eq!(first.tags(), [tag("Event", "A")]);
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(second.tags(), [tag("Event", "B")]);
    assert_eq!(notation(second), ["e4"]);
}

#[test]
fn takes_the_start_position_of_a_game_without_move_text_from_its_fen_tag() {
    let entries = read(
        b"[White \"A\"]\n\
          [FEN \"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1\"]\n\
          \n\
          [White \"B\"]\n\n1. e4 *\n",
    )
    .unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(first.main_line(), &MainLine::Empty { comments: vec![] });
    assert_eq!(
        Fen::from_position(first.start_position(), EnPassantMode::Legal).to_string(),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
    );
}

#[test]
fn keeps_a_repeated_tag_name_in_the_same_game_when_it_is_not_the_first_tag() {
    let entries =
        read(b"[White \"A\"]\n[Annotator \"x\"]\n[Annotator \"y\"]\n\n1. e4 1-0\n").unwrap();

    assert_eq!(entries.len(), 1);
    let (game, problem) = split(&entries[0]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(
        game.tags(),
        [
            tag("White", "A"),
            tag("Annotator", "x"),
            tag("Annotator", "y")
        ]
    );
    assert_eq!(notation(game), ["e4"]);
    assert_eq!(
        game.result(),
        Outcome::Known(KnownOutcome::Decisive {
            winner: Color::White
        })
    );
}

struct FailsAfter<'bytes>(&'bytes [u8]);

impl Read for FailsAfter<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.0.is_empty() {
            return Err(io::Error::other("the disk is gone"));
        }
        self.0.read(buffer)
    }
}

#[test]
fn hands_out_a_game_split_off_by_a_repeated_first_tag_before_a_later_source_error() {
    let mut bytes = b"[White \"A\"]\n[White \"B\"]\n".to_vec();
    for number in 0..7000 {
        bytes.extend_from_slice(format!("[Tag{number} \"v\"]\n").as_bytes());
    }
    let (entries, result) = read_source(FailsAfter(&bytes));

    assert_eq!(entries.len(), 1);
    let (game, problem) = split(&entries[0]).unwrap();
    assert_eq!(game.tags(), [tag("White", "A")]);
    assert_eq!(problem, None);
    assert_eq!(result.unwrap_err().to_string(), "the disk is gone");
}

#[test]
fn reports_a_tag_line_that_is_too_long_and_keeps_the_game() {
    let annotator = "a".repeat(200_000);
    let bytes = format!(
        "[White \"A\"]\n[Annotator \"{annotator}\"]\n[Black \"B\"]\n\n1. e4 *\n\n\
         [White \"C\"]\n\n1. d4 *\n"
    );
    let entries = read(bytes.as_bytes()).unwrap();

    assert_eq!(entries.len(), 2);
    let (first, problem) = split(&entries[0]).unwrap();
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::UnreadableTagLine
        })
    );
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "tag line is too long or not closed"
    );
    assert_eq!(first.tags(), [tag("White", "A"), tag("Black", "B")]);
    assert_eq!(first.steps().count(), 0);
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(second.tag("White"), Some("C"));
    assert_eq!(notation(second), ["d4"]);
}

#[test]
fn hands_out_a_game_whose_tag_line_failed_at_the_end_of_the_source() {
    let annotator = "a".repeat(200_000);
    let bytes = format!("[White \"A\"]\n[Annotator \"{annotator}\"]\n");
    let entries = read(bytes.as_bytes()).unwrap();

    assert_eq!(entries.len(), 1);
    let (game, problem) = split(&entries[0]).unwrap();
    assert_eq!(game.tags(), [tag("White", "A")]);
    assert_eq!(
        problem.map(|problem| &problem.reason),
        Some(&Reason::UnreadableTagLine)
    );
}

#[test]
fn reports_a_tag_line_cut_off_at_the_end_of_the_source() {
    let entries = read(b"[White \"A\"]\n[Black \"B").unwrap();

    assert_eq!(entries.len(), 1);
    let (game, problem) = split(&entries[0]).unwrap();
    assert_eq!(game.tags(), [tag("White", "A")]);
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::UnreadableTagLine
        })
    );
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "tag line is too long or not closed"
    );
}

#[test]
fn reports_a_comment_open_at_the_end_of_the_source_and_keeps_its_text() {
    let entries = read(b"[White \"A\"]\n\n1. e4 e5 {never closed\n").unwrap();

    assert_eq!(entries.len(), 1);
    let (game, problem) = split(&entries[0]).unwrap();
    assert_eq!(
        problem,
        Some(&Problem {
            game_number: 1,
            reason: Reason::UnclosedComment
        })
    );
    assert_eq!(
        problem.unwrap().reason.to_string(),
        "comment is not closed, the rest of the file was read as its text"
    );
    assert_eq!(notation(game), ["e4", "e5"]);
    assert_eq!(
        game.steps().nth(1).unwrap().node().comments_after(),
        ["never closed"]
    );
}

#[test]
fn gives_each_entry_of_the_fixtures_the_range_from_its_first_token_to_its_last() {
    assert_eq!(ranges(CLEAN).unwrap(), [Some(0..192), Some(194..326)]);
    assert_eq!(
        ranges(DAMAGED).unwrap(),
        [Some(0..108), Some(110..195), Some(197..287)]
    );
    let texts: [(&[u8], &str); 5] = [
        (&CLEAN[0..192], "Qxf7# 1-0"),
        (&CLEAN[194..326], "Qh4# 0-1"),
        (&DAMAGED[0..108], "Qxf7# 1-0"),
        (&DAMAGED[110..195], "Nc6 *"),
        (&DAMAGED[197..287], "Qh4# 0-1"),
    ];
    for (text, last_tokens) in texts {
        assert!(text.starts_with(b"[Event \"Example\"]\n"));
        assert!(text.ends_with(last_tokens.as_bytes()));
    }
}

const MARKS_BLANK_LINES_AND_STRAY_TEXT: &[u8] =
    b"\xEF\xBB\xBF[White \"A\"]\n\n1. e4 *\n\n\n\nnot a game\n\n\
      \xEF\xBB\xBF[White \"B\"]\r\n\r\n1. d4 d5 1/2-1/2  \r\n\r\n\
      1. c4 *\xEF\xBB\xBF[White \"C\"]\n\n1. Nf3 *\n";

#[test]
fn counts_ranges_in_the_bytes_of_the_source_around_marks_blank_lines_and_stray_text() {
    let entries = read(MARKS_BLANK_LINES_AND_STRAY_TEXT).unwrap();

    assert_eq!(
        ranges(MARKS_BLANK_LINES_AND_STRAY_TEXT).unwrap(),
        [
            Some(3..23),
            Some(27..37),
            Some(42..73),
            Some(79..86),
            Some(89..110)
        ]
    );
    assert_eq!(
        entries[1],
        Entry::StrayText {
            after_game: 1,
            range: 27..37
        }
    );
    assert_eq!(&MARKS_BLANK_LINES_AND_STRAY_TEXT[27..37], b"not a game");
    assert_eq!(
        &MARKS_BLANK_LINES_AND_STRAY_TEXT[42..73],
        b"[White \"B\"]\r\n\r\n1. d4 d5 1/2-1/2"
    );
    assert_eq!(&MARKS_BLANK_LINES_AND_STRAY_TEXT[79..86], b"1. c4 *");
    let (one_byte_at_a_time, result) =
        read_source(OneByteAtATime(MARKS_BLANK_LINES_AND_STRAY_TEXT));
    result.unwrap();
    assert_eq!(one_byte_at_a_time, entries);
}

#[test]
fn keeps_a_byte_order_mark_behind_white_space_out_of_the_entries_around_it() {
    let source = b"1. e4 *\n\n\xEF\xBB\xBFe4 e5 *\n\n\xEF\xBB\xBF\n\n1. d4 *\n";
    let entries = read(source).unwrap();

    assert_eq!(entries.len(), 3);
    let (second, problem) = split(&entries[1]).unwrap();
    assert_eq!(problem, None);
    assert_eq!(notation(second), ["e4", "e5"]);
    assert_eq!(
        ranges(source).unwrap(),
        [Some(0..7), Some(12..19), Some(26..33)]
    );
}

#[test]
fn ends_an_entry_at_its_last_byte_that_is_not_white_space() {
    let source = b"1. e4 1-0 {time}  \n\n1. d4 d5 unreadable ; rest\r\n\r\n1. c4 c5 $1\n";

    assert_eq!(
        ranges(source).unwrap(),
        [Some(0..16), Some(20..46), Some(50..61)]
    );
    assert_eq!(&source[20..46], b"1. d4 d5 unreadable ; rest");
}

#[test]
fn ends_a_game_with_an_open_comment_at_the_last_byte_of_the_source_that_is_not_white_space() {
    assert_eq!(
        ranges(b"[White \"A\"]\n\n1. e4 {never closed  \n\n").unwrap(),
        [Some(0..32)]
    );
}

#[test]
fn gives_no_range_to_a_game_with_only_tags_that_a_game_follows_nor_to_that_game() {
    let source = b"[White \"A\"]\n\n1. e4 *\n\n\
                   [White \"B\"]\n\n[White \"C\"]\n\n1. d4 *\n\n\
                   [White \"D\"]\n\n1. c4 *\n";

    assert_eq!(
        ranges(source).unwrap(),
        [Some(0..20), None, None, Some(57..77)]
    );
}

#[test]
fn gives_a_game_with_only_tags_at_the_end_of_the_source_its_range() {
    let source = b"[White \"D\"]\n\n1. c4 *\n\n[White \"E\"]\n";

    assert_eq!(ranges(source).unwrap(), [Some(0..20), Some(22..33)]);
    assert_eq!(&source[22..33], b"[White \"E\"]");
}

#[test]
fn keeps_the_range_of_a_game_with_a_tag_line_that_is_too_long_in_one_piece() {
    let annotator = "a".repeat(200_000);
    let bytes = format!(
        "[White \"A\"]\n[Annotator \"{annotator}\"]\n[Black \"B\"]\n\n1. e4 *\n\n\
         [White \"C\"]\n\n1. d4 *\n"
    );

    assert_eq!(
        ranges(bytes.as_bytes()).unwrap(),
        [Some(0..200_047), Some(200_049..200_069)]
    );
}

#[test]
fn keeps_ranges_exact_in_a_source_much_longer_than_the_buffers_of_the_reader() {
    let source = "\u{feff}[White \"A\"]\n\n1. e4 e5 1-0\n\n".repeat(40_000);
    let found = ranges(source.as_bytes()).unwrap();

    assert_eq!(found.len(), 40_000);
    for (index, range) in found.into_iter().enumerate() {
        let start = u64::try_from(index).unwrap() * 30 + 3;
        assert_eq!(range, Some(start..start + 25));
    }
}

fn assert_written(input: &[u8], expected: &str) -> io::Result<()> {
    let entries = read(input)?;
    assert_eq!(entries.len(), 1);
    let (game, _) = split(&entries[0]).ok_or_else(|| io::Error::other("expected a game"))?;
    let mut bytes = Vec::new();
    pgn::write_game(game, &mut bytes)?;
    assert_eq!(bytes, expected.as_bytes());

    let written = read(&bytes)?;
    assert_eq!(written.len(), 1);
    assert_eq!(split(&written[0]), Some((game, None)));
    Ok(())
}

#[test]
fn writes_ordered_tags_and_uses_the_position_and_movetext_result() {
    let tags = "[FEN \"4k3/8/8/8/8/8/8/4K3 b - - 0 37\"]\n\
                [Label \"a\\\"b\\\\c\"]\n[Label \"é\"]\n[Result \"*\"]\n\n";
    assert_written(
        format!("{tags}Kd7 Kd2 1/2-1/2").as_bytes(),
        &format!("{tags}37... Kd7 38. Kd2 1/2-1/2\n"),
    )
    .unwrap();
}

#[test]
fn writes_comments_variations_and_glyphs_in_their_lines() {
    assert_written(
        b"{start} e4!? $42 {after} (d4 {side} (c4) d5) (Nf3) {before} Z0 *",
        "{start} 1. e4 $5 $42 {after} (1. d4 {side} (1. c4) 1... d5) (1. Nf3) {before} 1... -- *\n",
    )
    .unwrap();
}

#[test]
fn writes_games_without_moves_and_the_part_before_an_illegal_move() {
    assert_written(b"*", "*\n").unwrap();
    assert_written(b"{A} {B} 1-0", "{A} {B} 1-0\n").unwrap();
    assert_written(b"e4 e5 Ke3 Nc6 0-1", "1. e4 e5 0-1\n").unwrap();
    assert_written(b"e4 {Stra\xDFe} *", "1. e4 {Straße} *\n").unwrap();
}

#[test]
fn writes_chess960_castling() {
    let game = "[Variant \"Chess960\"]\n\
                [FEN \"rbbqnknr/pppppppp/8/8/8/8/PPPPPPPP/RBBQNKNR w HAha - 0 1\"]\n\n\
                1. Nh3 Nh6 2. O-O e6 *\n";
    assert_written(game.as_bytes(), game).unwrap();
}

#[test]
fn writes_every_variation_at_the_depth_limit() {
    let game = format!(
        "1. e4 {}{} *\n",
        "(1. d4 ".repeat(254) + "(1. d4",
        ")".repeat(255)
    );
    assert_written(game.as_bytes(), &game).unwrap();
}

#[test]
fn writes_the_largest_supported_tag_and_comment_in_utf8() {
    // The name and tag punctuation through the closing quote take five bytes. The
    // escaped quote in the value takes two more.
    let value = "a".repeat((1 << 16) - 7);
    let text = "ü".repeat(1 << 19);
    let game = format!("[X \"{value}\\\"\"]\n\n{{{text}}} *\n");
    assert_written(game.as_bytes(), &game).unwrap();
}

#[test]
fn refuses_start_positions_the_tags_cannot_reproduce() {
    for (tags, fen) in [
        (vec![], "4k3/8/8/8/8/8/8/4K3 b - - 0 37"),
        (
            vec![tag("FEN", "4k3/8/8/8/8/8/8/4K3 b - - 0 1")],
            "4k3/8/8/8/8/8/8/4K3 b - - 0 37",
        ),
    ] {
        let start = fen
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        let game = GameBuilder::new(tags, start).finish(Outcome::Unknown);
        assert_eq!(
            pgn::write_game(&game, io::sink()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn keeps_a_damaged_empty_game_but_refuses_moves_a_bad_fen_would_hide() {
    let tags = vec![tag("FEN", "broken")];
    let mut builder = GameBuilder::new(tags, Chess::default());
    let mut output = Vec::new();
    pgn::write_game(&builder.clone().finish(Outcome::Unknown), &mut output).unwrap();
    assert_eq!(output, b"[FEN \"broken\"]\n\n*\n");
    builder.play("e4".parse().unwrap()).unwrap();
    let game = builder.finish(Outcome::Unknown);
    assert_eq!(
        pgn::write_game(&game, io::sink()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn refuses_comment_text_that_cannot_roundtrip() {
    for text in [
        "note} 0-1 {tail",
        " leading space",
        "trailing space ",
        "mark\u{feff}[here",
    ] {
        let mut builder = GameBuilder::new(vec![], Chess::default());
        builder.comment(text.to_owned());
        let game = builder.finish(Outcome::Unknown);
        assert_eq!(
            pgn::write_game(&game, io::sink()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn refuses_tags_that_would_change_or_split_a_built_game() {
    for tags in [
        vec![tag("Event", "One"), tag("Event", "Two")],
        vec![tag(" White ", "Alpha")],
        vec![tag("Event]", "One")],
        vec![tag("Event", "mark\u{feff}[here")],
    ] {
        let game = GameBuilder::new(tags, Chess::default()).finish(Outcome::Unknown);
        assert_eq!(
            pgn::write_game(&game, io::sink()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
