//! The database format as seen by a writer and a new reader.

use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::process;
use std::{fs, io};

use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, EnPassantMode, Outcome, Position};
use zobrist_core::game::{AnnotationSymbol, Game, GameBuilder, Tag};
use zobrist_core::pgn::{self, Entry};
use zobrist_store::{
    Database, DatabaseWriter, GameColumn, GameQuery, ImportError, OpenError, import_pgn,
};

struct Directory(PathBuf);

impl Directory {
    fn new(test: &str) -> io::Result<Self> {
        let path = std::env::temp_dir().join(format!("zobrist-store-{test}-{}", process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn tag(name: &str, value: &str) -> Tag {
    Tag {
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

fn play(builder: &mut GameBuilder, moves: &[&str]) -> Result<(), String> {
    for text in moves {
        let san = text.parse().map_err(|error| format!("{text}: {error}"))?;
        builder
            .play(san)
            .map_err(|error| format!("{text}: {error:?}"))?;
    }
    Ok(())
}

fn store(path: &Path, games: &[Game]) -> io::Result<()> {
    let mut writer = DatabaseWriter::create(path)?;
    for game in games {
        writer.push(game)?;
    }
    writer.finish()
}

fn read(path: &Path) -> Result<Vec<Game>, OpenError> {
    let mut database = Database::open(path)?;
    let games: Vec<_> = database.games()?.collect::<Result<_, _>>()?;
    assert_eq!(database.game_count(), games.len() as u64);
    Ok(games)
}

#[test]
fn version_one_fixture_reads_as_two_games_and_rebuilds_byte_for_byte() {
    let start = |round, white, black, result| {
        GameBuilder::new(
            vec![
                tag("Event", "Example"),
                tag("Site", "?"),
                tag("Date", "2024.01.01"),
                tag("Round", round),
                tag("White", white),
                tag("Black", black),
                tag("Result", result),
            ],
            Chess::default(),
        )
    };
    let mut first = start("1", "Alpha", "Beta", "1-0");
    play(&mut first, &["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6"]).unwrap();
    first.annotate(AnnotationSymbol(4)).unwrap();
    first.comment("Overlooks the mate.".to_owned());
    first.begin_variation().unwrap();
    play(&mut first, &["g6", "Qf3"]).unwrap();
    first.end_variation();
    play(&mut first, &["Qxf7#"]).unwrap();

    let mut second = start("2", "Gamma", "Delta", "0-1");
    play(&mut second, &["f3", "e5", "g4", "Qh4#"]).unwrap();
    let expected = [
        first.finish("1-0".parse().unwrap()),
        second.finish("0-1".parse().unwrap()),
    ];
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clean.zobrist");
    assert_eq!(read(&fixture).unwrap(), expected);
    let directory = Directory::new("fixture").unwrap();
    let path = directory.0.join("games");
    store(&path, &expected).unwrap();
    assert_eq!(fs::read(path).unwrap(), fs::read(fixture).unwrap());
}

#[test]
fn a_diagnostic_that_cannot_be_finished_leaves_no_database() -> io::Result<()> {
    let directory = Directory::new("diagnostic-error")?;
    let pgn = directory.0.join("input.pgn");
    let target = directory.0.join("study.zobrist");
    fs::write(&pgn, "1. e4 Ke7 *")?;
    let mut lines = Vec::new();
    let result = import_pgn(&pgn, &target, |line| {
        if let Some(line) = line {
            lines.push(line);
            Ok(())
        } else {
            Err(io::Error::other("cannot finish the report"))
        }
    });
    assert_eq!(
        lines,
        ["game 1 (? - ?): illegal move \"Ke7\" at Black's move 1, 1 move kept"]
    );
    assert!(
        matches!(result, Err(ImportError::Create(error)) if error.to_string() == "cannot finish the report")
    );
    assert!(!target.exists());
    assert_eq!(fs::read_dir(&directory.0)?.count(), 1);
    Ok(())
}

#[test]
fn an_empty_database_appears_only_after_finish() {
    let directory = Directory::new("empty").unwrap();
    let path = directory.0.join("games");
    let writer = DatabaseWriter::create(&path).unwrap();
    assert!(!path.exists());
    drop(writer);
    assert_eq!(directory.0.read_dir().unwrap().count(), 0);
    let writer = DatabaseWriter::create(&path).unwrap();
    assert!(!path.exists());
    writer.finish().unwrap();
    assert_eq!(read(&path).unwrap(), []);
    assert_eq!(directory.0.read_dir().unwrap().count(), 1);
}

#[test]
fn another_writer_cannot_replace_a_file_even_if_it_won_the_race_to_start() {
    let directory = Directory::new("existing").unwrap();
    let path = directory.0.join("games");
    let mut first = DatabaseWriter::create(&path).unwrap();
    first
        .push(&GameBuilder::new(Vec::new(), Chess::default()).finish(Outcome::Unknown))
        .unwrap();
    store(&path, &[]).unwrap();
    assert_eq!(
        first.finish().unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(read(&path).unwrap(), []);
    assert_eq!(directory.0.read_dir().unwrap().count(), 1);
}

#[test]
fn open_distinguishes_read_failure_file_type_and_format_version() {
    let directory = Directory::new("errors").unwrap();
    let path = directory.0.join("games");
    assert!(
        matches!(Database::open(&path), Err(OpenError::Read(error)) if error.kind() == std::io::ErrorKind::NotFound)
    );
    fs::write(&path, b"[Event \"Example\"]").unwrap();
    assert!(matches!(Database::open(&path), Err(OpenError::NotDatabase)));
    let mut bytes = include_bytes!("fixtures/clean.zobrist").to_vec();
    bytes[8..12].copy_from_slice(&27_u32.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        Database::open(&path),
        Err(OpenError::Version {
            found: 27,
            expected: 1
        })
    ));
}

#[test]
fn every_single_byte_change_and_every_truncation_is_refused_at_open() {
    let directory = Directory::new("damage").unwrap();
    let path = directory.0.join("games");
    let mut bytes = include_bytes!("fixtures/clean.zobrist").to_vec();
    for offset in 0..bytes.len() {
        bytes[offset] ^= 1;
        fs::write(&path, &bytes).unwrap();
        assert!(Database::open(&path).is_err(), "changed byte {offset}");
        bytes[offset] ^= 1;
        fs::write(&path, &bytes[..offset]).unwrap();
        assert!(Database::open(&path).is_err(), "cut at byte {offset}");
    }
}

#[test]
fn correct_checksums_do_not_turn_an_illegal_move_into_a_game() {
    let directory = Directory::new("illegal").unwrap();
    let path = directory.0.join("games");
    let mut bytes = include_bytes!("fixtures/clean.zobrist").to_vec();
    let table = bytes.len() - 172;
    let moves_entry = table + 24;
    let offset = u64::from_le_bytes(bytes[moves_entry + 4..moves_entry + 12].try_into().unwrap());
    let start = usize::try_from(offset).unwrap();
    let length = 2 * 11;
    // e2 to e5, with both checksums updated so the move reaches the builder.
    bytes[start..start + 2].copy_from_slice(&0x090c_u16.to_le_bytes());
    let checksum = crc32fast::hash(&bytes[start..start + length]);
    bytes[moves_entry + 20..moves_entry + 24].copy_from_slice(&checksum.to_le_bytes());
    let checksum = crc32fast::hash(&bytes[table..table + 168]);
    bytes[table + 168..].copy_from_slice(&checksum.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    let mut database = Database::open(&path).unwrap();
    assert!(matches!(
        database.game(NonZeroU64::new(2).unwrap()),
        Err(OpenError::Damaged)
    ));
    let mut games = database.games().unwrap();
    assert!(matches!(games.next(), Some(Err(OpenError::Damaged))));
    assert!(games.next().is_none());
}

#[test]
fn keeps_start_positions_independent_of_tags_and_uses_them_to_rebuild_moves() {
    let directory = Directory::new("starts").unwrap();
    let path = directory.0.join("games");
    let positions = [
        (
            "4k3/8/8/8/3pP3/8/8/4K3 b - e3 0 47",
            CastlingMode::Standard,
            "dxe3",
        ),
        (
            "k7/8/8/8/8/8/8/5K1R w H - 16 99",
            CastlingMode::Chess960,
            "O-O",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 7 6",
            CastlingMode::Chess960,
            "e4",
        ),
    ];
    let mut games = Vec::new();
    for (fen, mode, first_move) in positions {
        let start = fen.parse::<Fen>().unwrap().into_position(mode).unwrap();
        let mut game = GameBuilder::new(
            vec![
                tag("Other", "one"),
                tag("Other", "Renée"),
                tag("Result", "1-0"),
            ],
            start,
        );
        play(&mut game, &[first_move, "--"]).unwrap();
        games.push(game.finish(Outcome::Unknown));
    }
    games.push(GameBuilder::new(Vec::new(), Chess::default()).finish(Outcome::Unknown));
    store(&path, &games).unwrap();
    let actual = read(&path).unwrap();
    for (game, expected) in actual.iter().zip(&games) {
        let start = game.start_position();
        assert_eq!(
            start.to_setup(EnPassantMode::Always),
            expected.start_position().to_setup(EnPassantMode::Always)
        );
        assert_eq!(
            start.castles().mode(),
            expected.start_position().castles().mode()
        );
    }
    assert_eq!(actual, games);
}

#[test]
fn restores_nested_and_sibling_variations_comments_symbols_and_damaged_games() {
    let directory = Directory::new("annotations").unwrap();
    let path = directory.0.join("games");
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    builder.comment("before".to_owned());
    play(&mut builder, &["e4"]).unwrap();
    builder.annotate(AnnotationSymbol(3)).unwrap();
    builder.annotate(AnnotationSymbol(255)).unwrap();
    builder.comment("after".to_owned());
    builder.begin_variation().unwrap();
    builder.comment("variation".to_owned());
    play(&mut builder, &["d4"]).unwrap();
    builder.begin_variation().unwrap();
    play(&mut builder, &["--", "c5"]).unwrap();
    builder.end_variation();
    builder.end_variation();
    builder.begin_variation().unwrap();
    play(&mut builder, &["c4"]).unwrap();
    builder.end_variation();
    builder.comment("before the reply".to_owned());
    play(&mut builder, &["e5"]).unwrap();
    let mut empty = GameBuilder::new(Vec::new(), Chess::default());
    empty.comment(" {with braces} ".to_owned());
    empty.comment(String::new());
    let mut games = vec![
        builder.finish(Outcome::Unknown),
        empty.finish(Outcome::Unknown),
    ];
    pgn::read_games(&b"1. e4 e5 2. Ke3 *"[..], |entry| {
        let Entry::Game {
            game,
            problem: Some(_),
            ..
        } = entry
        else {
            unreachable!("expected the kept part of a damaged game");
        };
        games.push(*game);
    })
    .unwrap();

    let mut kept = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut kept, &["e4", "e5"]).unwrap();
    assert_eq!(games[2], kept.finish(Outcome::Unknown));
    store(&path, &games).unwrap();
    assert_eq!(read(&path).unwrap(), games);
}

#[test]
fn stores_every_variation_at_the_depth_limit() {
    let directory = Directory::new("depth").unwrap();
    let path = directory.0.join("games");
    let mut builder = GameBuilder::new(Vec::new(), Chess::default());
    play(&mut builder, &["e4"]).unwrap();
    for _ in 0..255 {
        builder.begin_variation().unwrap();
        play(&mut builder, &["d4"]).unwrap();
    }
    let games = [builder.finish(Outcome::Unknown)];
    store(&path, &games).unwrap();
    assert_eq!(read(&path).unwrap(), games);
}

#[test]
fn pages_sorted_headers_across_the_collection_and_reports_the_full_total() {
    let directory = Directory::new("sorted-pages").unwrap();
    let path = directory.0.join("games");
    let mut writer = DatabaseWriter::create(&path).unwrap();
    for number in 1..=401 {
        let white = if number % 2 == 0 { "Zoë" } else { "Ada" };
        let game =
            GameBuilder::new(vec![tag("White", white)], Chess::default()).finish(Outcome::Unknown);
        writer.push(&game).unwrap();
    }
    writer.finish().unwrap();
    let mut database = Database::open(&path).unwrap();
    let query = GameQuery {
        sort: GameColumn::White,
        descending: true,
        ..GameQuery::default()
    };
    for (page, expected) in [
        (1, (2..=200).step_by(2).collect::<Vec<_>>()),
        (2, (202..=400).step_by(2).collect()),
        (5, vec![401]),
        (6, vec![]),
    ] {
        let result = database.query(&query, NonZeroUsize::new(page)).unwrap();
        assert_eq!(result.total, 401);
        assert_eq!(
            result.rows.iter().map(|row| row.number).collect::<Vec<_>>(),
            expected
        );
    }
    let query = GameQuery {
        player: Some("OË".into()),
        ..query
    };
    let result = database.query(&query, NonZeroUsize::new(3)).unwrap();
    assert_eq!(result.total, 200);
    assert_eq!(result.rows, []);
}

#[test]
fn reads_the_selected_complete_game_by_its_original_number() {
    let path = Path::new("tests/fixtures/clean.zobrist");
    let mut database = Database::open(path).unwrap();
    let first = database.game(NonZeroU64::MIN).unwrap().unwrap();
    assert_eq!(first.tag("White"), Some("Alpha"));
    let mut written = Vec::new();
    pgn::write_game(&first, &mut written).unwrap();
    let text = String::from_utf8(written).unwrap();
    assert!(text.contains("{Overlooks the mate.}"));
    assert!(text.contains("(3... g6 4. Qf3)"));
    let second = database.game(NonZeroU64::new(2).unwrap()).unwrap().unwrap();
    assert_eq!(second.tag("White"), Some("Gamma"));
    assert!(
        database
            .game(NonZeroU64::new(3).unwrap())
            .unwrap()
            .is_none()
    );
}
