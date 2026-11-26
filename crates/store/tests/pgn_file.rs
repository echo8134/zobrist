//! Adding and replacing games through the public PGN file API.

use std::num::NonZeroU64;
use std::path::PathBuf;
use std::{env, fs, io, process};

use shakmaty::{Chess, Outcome};
use zobrist_core::game::{Game, GameBuilder, Tag};
use zobrist_core::pgn::{self, Entry};
use zobrist_store::{add_pgn_games, replace_pgn_game};

const OLD_GAME: &[u8] = b"[White \"Old\"]\n\n1. e4 e5 1-0";
const NEW_GAME: &[u8] = b"[White \"New\"]\n\n1. d4 d5 *";

fn target(test: &str) -> PathBuf {
    env::temp_dir().join(format!("zobrist-store-pgn-{test}-{}.pgn", process::id()))
}

fn game(text: &[u8]) -> io::Result<Game> {
    let mut games = Vec::new();
    pgn::read_games(text, |entry| {
        if let Entry::Game { game, .. } = entry {
            games.push(*game);
        }
    })?;
    games.pop().ok_or_else(|| io::Error::other("no game"))
}

fn number(number: u64) -> io::Result<NonZeroU64> {
    NonZeroU64::new(number).ok_or_else(|| io::Error::other("game numbers start at 1"))
}

#[test]
fn copies_the_old_bytes_and_supplies_only_missing_line_breaks() -> io::Result<()> {
    let path = target("separator");
    let game = GameBuilder::new(Vec::new(), Chess::default()).finish(Outcome::Unknown);
    let cases: &[(Option<&[u8]>, &[u8])] = &[
        (None, b"*\n\n*\n"),
        (Some(b""), b"*\n\n*\n"),
        (Some(b"\xff"), b"\xff\n\n*\n\n*\n"),
        (Some(b"old\n"), b"old\n\n*\n\n*\n"),
        (Some(b"old\n\n"), b"old\n\n*\n\n*\n"),
        (Some(b"old\n\n\n"), b"old\n\n\n*\n\n*\n"),
        (Some(b"old\r\n\r\n"), b"old\r\n\r\n*\n\n*\n"),
    ];
    for &(old, expected) in cases {
        if let Some(bytes) = old {
            fs::write(&path, bytes)?;
        }
        let count = add_pgn_games(&path, |add| {
            add(&game)?;
            add(&game)
        })?;
        assert_eq!(count, 2);
        assert_eq!(fs::read(&path)?, expected);
        fs::remove_file(&path)?;
    }
    Ok(())
}

#[test]
fn a_game_that_cannot_be_written_leaves_the_target_intact() -> io::Result<()> {
    let path = target("failed-write");
    fs::write(&path, b"1. e4 *")?;
    let game = GameBuilder::new(Vec::new(), Chess::default()).finish(Outcome::Unknown);
    let invalid = GameBuilder::new(
        vec![Tag {
            name: "Event".to_owned(),
            value: "two\nlines".to_owned(),
        }],
        Chess::default(),
    )
    .finish(Outcome::Unknown);
    let error = add_pgn_games(&path, |add| {
        add(&game)?;
        add(&invalid)
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&path)?, b"1. e4 *");
    let result = add_pgn_games::<io::Error>(&path, |add| {
        assert!(add(&invalid).is_err());
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(fs::read(&path)?, b"1. e4 *");
    fs::remove_file(path)
}

#[cfg(unix)]
#[test]
fn adding_through_a_symbolic_link_preserves_it_and_updates_its_destination() -> io::Result<()> {
    let directory = target("symbolic-link");
    let archive = directory.join("archive");
    fs::create_dir(&directory)?;
    fs::create_dir(&archive)?;
    let destination = archive.join("mine.pgn");
    let link = directory.join("favorite.pgn");
    let relative = std::path::Path::new("archive/mine.pgn");
    std::os::unix::fs::symlink(relative, &link)?;
    let game = GameBuilder::new(Vec::new(), Chess::default()).finish(Outcome::Unknown);

    fs::write(&destination, "1. e4 *")?;
    add_pgn_games(&link, |add| add(&game))?;
    assert_eq!(fs::read_to_string(&destination)?, "1. e4 *\n\n*\n");
    assert_eq!(fs::read_link(&link)?, relative);

    fs::remove_file(&destination)?;
    add_pgn_games(&link, |add| add(&game))?;
    assert_eq!(fs::read_to_string(&destination)?, "*\n");
    assert_eq!(fs::read_link(&link)?, relative);
    fs::remove_dir_all(directory)
}

#[test]
fn replaces_one_game_and_keeps_every_byte_around_it() -> io::Result<()> {
    let path = target("replace");
    let new = game(NEW_GAME)?;
    let cases: &[(&[u8], u64, &[u8])] = &[
        (b"", 1, b""),
        (
            b"[White \"A\"]\n\n1. c4 *\n\n",
            2,
            b"\n\n[White \"B\"]\n\n1. Nf3 *\n",
        ),
        (b"1. c4 *\n\n\n\n", 2, b"\n\n\n\n\n1. Nf3 *\n\n\n"),
        (
            b"[White \"A\"]\n\n1. c4 *  \n\n",
            2,
            b"  \n \n[White \"B\"]\n\n1. Nf3 *  \n",
        ),
        (
            b"\xEF\xBB\xBF",
            1,
            b"\n\n\xEF\xBB\xBF[White \"B\"]\n\n1. Nf3 *\n",
        ),
        (
            b"\xEF\xBB\xBF[White \"A\"]\n\n1. c4 *\xEF\xBB\xBF",
            2,
            b"\xEF\xBB\xBF[White \"B\"]\n\n1. Nf3 *",
        ),
        (
            b"[White \"A\"]\r\n\r\n1. c4 *\r\n\r\n",
            2,
            b"\r\n\r\n[White \"B\"]\r\n\r\n1. Nf3 *\r\n",
        ),
        (
            b"[White \"A\"]\n\n1. c4 *\n\nnot a game\n\n",
            2,
            b"\n\nnot a game either\n\n[White \"B\"]\n\n1. Nf3 *\n",
        ),
    ];
    for &(before, game_number, after) in cases {
        fs::write(&path, [before, OLD_GAME, after].concat())?;
        replace_pgn_game(&path, number(game_number)?, &new)?;
        assert_eq!(fs::read(&path)?, [before, NEW_GAME, after].concat());
    }
    fs::remove_file(path)
}

#[test]
fn refuses_a_number_that_the_target_does_not_have() -> io::Result<()> {
    let path = target("no-such-game");
    let old = [OLD_GAME, b"\n\n", OLD_GAME, b"\n"].concat();
    fs::write(&path, &old)?;
    let error = replace_pgn_game(&path, number(3)?, &game(NEW_GAME)?).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(error.to_string(), "there is no game 3, the file holds 2");
    assert_eq!(fs::read(&path)?, old);
    fs::remove_file(path)
}

#[test]
fn a_new_game_that_cannot_be_written_leaves_the_target_as_it_was() -> io::Result<()> {
    let path = target("replace-failed-write");
    fs::write(&path, OLD_GAME)?;
    let invalid = GameBuilder::new(
        vec![Tag {
            name: "Event".to_owned(),
            value: "two\nlines".to_owned(),
        }],
        Chess::default(),
    )
    .finish(Outcome::Unknown);
    let error = replace_pgn_game(&path, number(1)?, &invalid).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&path)?, OLD_GAME);
    fs::remove_file(path)
}

#[test]
fn refuses_a_game_with_only_tags_that_a_game_follows_and_that_game() -> io::Result<()> {
    let path = target("only-tags");
    let old = b"[White \"A\"]\n\n[White \"B\"]\n\n1. e4 *\n";
    fs::write(&path, old)?;
    for game_number in [1, 2] {
        let error = replace_pgn_game(&path, number(game_number)?, &game(NEW_GAME)?).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            format!(
                "game {game_number} has only tags or follows a game with only tags, \
                 so its place in the file is unknown"
            )
        );
        assert_eq!(fs::read(&path)?, old);
    }
    fs::remove_file(path)
}

#[test]
fn refuses_a_new_game_that_would_merge_with_the_game_in_front_of_it() -> io::Result<()> {
    let path = target("merge");
    let without_tags = game(b"1. d4 d5 *")?;
    let old = [b"1. c4 *\n", OLD_GAME, b"\n"].concat();
    fs::write(&path, &old)?;
    let error = replace_pgn_game(&path, number(2)?, &without_tags).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(
        error.to_string(),
        "the file would not read back with the same number of games and the new game as game 2"
    );
    assert_eq!(fs::read(&path)?, old);

    fs::write(&path, [b"1. c4 *\n\n", OLD_GAME, b"\n"].concat())?;
    replace_pgn_game(&path, number(2)?, &without_tags)?;
    assert_eq!(fs::read(&path)?, b"1. c4 *\n\n1. d4 d5 *\n");
    fs::remove_file(path)
}

#[cfg(unix)]
#[test]
fn replacing_through_a_symbolic_link_preserves_it_and_updates_its_destination() -> io::Result<()> {
    let directory = target("replace-symbolic-link");
    let archive = directory.join("archive");
    fs::create_dir(&directory)?;
    fs::create_dir(&archive)?;
    let destination = archive.join("mine.pgn");
    let link = directory.join("favorite.pgn");
    let relative = std::path::Path::new("archive/mine.pgn");
    std::os::unix::fs::symlink(relative, &link)?;

    fs::write(&destination, [OLD_GAME, b"\n"].concat())?;
    replace_pgn_game(&link, number(1)?, &game(NEW_GAME)?)?;
    assert_eq!(fs::read(&destination)?, [NEW_GAME, b"\n"].concat());
    assert_eq!(fs::read_link(&link)?, relative);
    assert_eq!(
        fs::read_dir(&directory)?.count() + fs::read_dir(&archive)?.count(),
        3
    );
    fs::remove_dir_all(directory)
}
