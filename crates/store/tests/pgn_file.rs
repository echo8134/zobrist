//! Adding games through the public PGN file API.

use std::path::PathBuf;
use std::{env, fs, io, process};

use shakmaty::{Chess, Outcome};
use zobrist_core::game::{GameBuilder, Tag};
use zobrist_store::add_pgn_games;

fn target(test: &str) -> PathBuf {
    env::temp_dir().join(format!("zobrist-store-pgn-{test}-{}.pgn", process::id()))
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
