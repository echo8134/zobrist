//! Adding and replacing games through the public PGN file API.

use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::{env, fs, io, process};

use shakmaty::{Chess, Outcome};
use zobrist_core::game::{Game, GameBuilder, Tag};
use zobrist_core::pgn::{self, Entry};
use zobrist_store::{
    PgnBatch, PgnGameRow, PgnHistory, PgnState, PgnStateDetails, add_pgn_game, add_pgn_games,
    pgn_states, prepare_pgn_restore, read_pgn_game, replace_pgn_game, restore_pgn_state,
};

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
fn batch_preserves_a_variant_and_appends_after_a_save_during_staging() -> io::Result<()> {
    let root = target("batch-save");
    fs::create_dir(&root)?;
    let path = root.join("Study.pgn");
    let original = "[Event \"Original\"]\n\n1. c4 {Keep my note} *";
    let saved = "[Event \"Local save\"]\n\n1. d4 *";
    let variant = "[Event \"Drops\"]\r\n[Variant \"Crazyhouse\"]\r\n[Result \"1-0\"]\r\n\r\n1. N@e4 { [%clk 0:03:05] } 1-0\r\n";
    fs::write(&path, original)?;
    let mut batch = PgnBatch::new()?;
    batch.push(variant)?;
    add_pgn_game(&path, &game(saved.as_bytes())?, None)?;
    batch.push(variant)?;
    assert_eq!(batch.append_to(&path, Some(history(&root)))?, (2, None));
    assert_eq!(
        fs::read_to_string(&path)?,
        format!("{original}\n\n{saved}\n\n{variant}\n{variant}")
    );
    let stamp = prepare_pgn_restore(&path, &root, 1)?;
    restore_pgn_state(&path, history(&root), 1, &stamp)?;
    assert_eq!(
        fs::read_to_string(&path)?,
        format!("{original}\n\n{saved}\n")
    );
    fs::remove_dir_all(root)
}

#[test]
fn a_nonempty_batch_creates_its_destination() -> io::Result<()> {
    let path = target("new-batch");
    let mut batch = PgnBatch::new()?;
    batch.push("1. d4 d5 1/2-1/2")?;
    assert_eq!(batch.append_to(&path, None)?, (1, None));
    assert_eq!(fs::read_to_string(&path)?, "1. d4 d5 1/2-1/2\n");
    fs::remove_file(path)
}

#[test]
fn empty_or_failed_batches_never_publish() -> io::Result<()> {
    let path = target("empty-batch");
    assert_eq!(PgnBatch::new()?.append_to(&path, None)?, (0, None));
    assert!(!path.exists());
    for bad in [
        "",
        "[Result \"*\"]\n\n1. e4 *",
        "[Event \"Cut\"]\n\n1. e4",
        "1. e4 { the result is 1-0",
        "1. e4 *\n\n1. c4 1-0",
        "[Result \"0-1\"]\n\n1. e4 1-0",
    ] {
        let mut batch = PgnBatch::new()?;
        batch.push("1. e4 e5 1-0")?;
        assert_eq!(
            batch.push(bad).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(batch.append_to(&path, None).is_err());
        assert!(!path.exists());
    }
    Ok(())
}

#[test]
fn batch_refuses_destinations_that_absorb_the_first_record() -> io::Result<()> {
    let path = target("batch-open");
    for old in ["1. d4 {unfinished", "1. d4 *\n\n[Black \"Cut"] {
        fs::write(&path, old)?;
        let mut batch = PgnBatch::new()?;
        batch.push("[Event \"First\"]\n\n1. e4 1-0")?;
        batch.push("[Event \"Second\"]\n\n1. d4 0-1")?;
        assert_eq!(
            batch.append_to(&path, None).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(fs::read_to_string(&path)?, old);
    }
    fs::remove_file(path)
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
        let count = add_pgn_games(&path, None, |add| {
            add(&game)?;
            add(&game)
        })?;
        assert_eq!(count, (2, None));
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
    let error = add_pgn_games(&path, None, |add| {
        add(&game)?;
        add(&invalid)
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&path)?, b"1. e4 *");
    let result = add_pgn_games::<io::Error>(&path, None, |add| {
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
    add_pgn_games(&link, None, |add| add(&game))?;
    assert_eq!(fs::read_to_string(&destination)?, "1. e4 *\n\n*\n");
    assert_eq!(fs::read_link(&link)?, relative);

    fs::remove_file(&destination)?;
    add_pgn_games(&link, None, |add| add(&game))?;
    assert_eq!(fs::read_to_string(&destination)?, "*\n");
    assert_eq!(fs::read_link(&link)?, relative);
    fs::remove_dir_all(directory)
}

#[test]
fn adds_one_game_as_the_last_game_and_gives_its_number() -> io::Result<()> {
    let path = target("add-one");
    let new = game(NEW_GAME)?;
    assert_eq!(add_pgn_game(&path, &new, None)?, (number(1)?, None));
    assert_eq!(fs::read(&path)?, [NEW_GAME, b"\n"].concat());

    fs::write(&path, [OLD_GAME, b"\n\n", OLD_GAME].concat())?;
    assert_eq!(add_pgn_game(&path, &new, None)?, (number(3)?, None));
    assert_eq!(
        fs::read(&path)?,
        [OLD_GAME, b"\n\n", OLD_GAME, b"\n\n", NEW_GAME, b"\n"].concat()
    );
    fs::remove_file(path)
}

#[test]
fn refuses_to_add_a_game_that_the_end_of_the_file_would_take_in() -> io::Result<()> {
    let path = target("add-one-open");
    let new = game(NEW_GAME)?;
    let open_ends: [&[u8]; 2] = [b"1. d4 {unfinished", b"1. d4 *\n\n[Black \"Cut"];
    for old in open_ends {
        fs::write(&path, old)?;
        let error = add_pgn_game(&path, &new, None).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            "the file would not read back with the new game as its last game"
        );
        assert_eq!(fs::read(&path)?, old);
    }
    fs::remove_file(path)
}

#[test]
fn reads_one_game_by_its_number_and_none_behind_the_last_game() -> io::Result<()> {
    let path = target("read-one");
    fs::write(
        &path,
        [OLD_GAME, b"\n\n", NEW_GAME, b"\n\n1. e4 e5 2. Ke3 *\n"].concat(),
    )?;
    assert_eq!(read_pgn_game(&path, number(1)?)?, Some(game(OLD_GAME)?));
    assert_eq!(read_pgn_game(&path, number(2)?)?, Some(game(NEW_GAME)?));
    assert_eq!(
        read_pgn_game(&path, number(3)?)?,
        Some(game(b"1. e4 e5 *")?)
    );
    assert_eq!(read_pgn_game(&path, number(4)?)?, None);
    fs::remove_file(&path)?;
    let error = read_pgn_game(&path, number(1)?).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    Ok(())
}

#[test]
fn a_row_holds_the_number_and_five_values_each_in_one_field() -> io::Result<()> {
    let game = game(
        b"[White \"First\tLast\"]\n[Event \"two\\\nlines\"]\n[Result \"1-0\"]\n\n1. d4 1/2-1/2\n",
    )?;
    assert_eq!(
        PgnGameRow::new(7, &game),
        PgnGameRow {
            number: 7,
            white: "First Last".to_owned(),
            black: "?".to_owned(),
            result: "1/2-1/2".to_owned(),
            date: "?".to_owned(),
            event: "two\\ lines".to_owned(),
        }
    );
    Ok(())
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
        replace_pgn_game(&path, number(game_number)?, &new, None, None)?;
        assert_eq!(fs::read(&path)?, [before, NEW_GAME, after].concat());
    }
    fs::remove_file(path)
}

#[test]
fn refuses_a_number_that_the_target_does_not_have() -> io::Result<()> {
    let path = target("no-such-game");
    let old = [OLD_GAME, b"\n\n", OLD_GAME, b"\n"].concat();
    fs::write(&path, &old)?;
    let error = replace_pgn_game(&path, number(3)?, &game(NEW_GAME)?, None, None).unwrap_err();
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
    let error = replace_pgn_game(&path, number(1)?, &invalid, None, None).unwrap_err();
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
        let error = replace_pgn_game(&path, number(game_number)?, &game(NEW_GAME)?, None, None)
            .unwrap_err();
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
    let error = replace_pgn_game(&path, number(2)?, &without_tags, None, None).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(
        error.to_string(),
        "the file would not read back with the same number of games and the new game as game 2"
    );
    assert_eq!(fs::read(&path)?, old);

    fs::write(&path, [b"1. c4 *\n\n", OLD_GAME, b"\n"].concat())?;
    replace_pgn_game(&path, number(2)?, &without_tags, None, None)?;
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
    replace_pgn_game(&link, number(1)?, &game(NEW_GAME)?, None, None)?;
    assert_eq!(fs::read(&destination)?, [NEW_GAME, b"\n"].concat());
    assert_eq!(fs::read_link(&link)?, relative);
    assert_eq!(
        fs::read_dir(&directory)?.count() + fs::read_dir(&archive)?.count(),
        3
    );
    fs::remove_dir_all(directory)
}

fn history(root: &Path) -> PgnHistory<'_> {
    PgnHistory {
        root,
        captured_at: 1_767_346_200,
    }
}

#[test]
fn refuses_a_stale_restore_without_creating_a_state() -> io::Result<()> {
    let root = target("restore-conflict");
    fs::create_dir(&root)?;
    let file = root.join("Club.pgn");
    let new = game(NEW_GAME)?;
    fs::write(&file, b"1. d4 {unfinished")?;
    assert!(add_pgn_game(&file, &new, Some(history(&root))).is_err());
    assert_eq!(pgn_states(&file, &root)?, []);
    fs::write(&file, OLD_GAME)?;
    add_pgn_game(&file, &new, Some(history(&root)))?;
    let stamp = prepare_pgn_restore(&file, &root, 1)?;
    fs::write(&file, b"1. e4 *")?;
    let error = restore_pgn_state(&file, history(&root), 1, &stamp).unwrap_err();
    assert_eq!(
        error.to_string(),
        "This file changed. Choose the state again."
    );
    assert_eq!(fs::read(&file)?, b"1. e4 *");
    assert_eq!(
        pgn_states(&file, &root)?,
        [PgnState {
            number: 1,
            details: Some(PgnStateDetails {
                captured_at: 1_767_346_200,
                length: 27
            })
        }]
    );
    fs::remove_dir_all(root)
}

#[test]
fn refuses_a_changed_payload_and_a_timestamp_outside_the_displayed_calendar() -> io::Result<()> {
    let root = target("state-checksum");
    fs::create_dir(&root)?;
    let file = root.join("Club.pgn");
    fs::write(&file, b"1. e4 *")?;
    add_pgn_game(&file, &game(NEW_GAME)?, Some(history(&root)))?;
    let folder = fs::read_dir(root.join("pgn-history"))?
        .next()
        .unwrap()?
        .path();
    let state = folder.join("1.state");
    let mut bytes = fs::read(&state)?;
    *bytes.last_mut().unwrap() = b'!';
    fs::write(&state, &bytes)?;
    let expected = format!(
        "Could not read {}. Your PGN files are untouched. Restore this state file from a backup or choose another state.",
        state.display()
    );
    assert_eq!(
        prepare_pgn_restore(&file, &root, 1)
            .unwrap_err()
            .to_string(),
        expected
    );
    assert_eq!(
        fs::read(&file)?,
        b"1. e4 *\n\n[White \"New\"]\n\n1. d4 d5 *\n"
    );
    assert_eq!(fs::read(&state)?, bytes);
    bytes[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    fs::write(&state, &bytes)?;
    fs::write(folder.join("01.state"), b"not a state number")?;
    assert_eq!(
        pgn_states(&file, &root)?,
        [PgnState {
            number: 1,
            details: None
        }]
    );
    fs::remove_dir_all(root)
}

#[test]
fn simultaneous_board_appends_capture_each_actual_predecessor() -> io::Result<()> {
    let root = target("concurrent-history");
    fs::create_dir(&root)?;
    let file = root.join("Club.pgn");
    fs::write(&file, b"")?;
    let new = game(NEW_GAME)?;
    let start = std::sync::Barrier::new(4);
    let mut numbers = std::thread::scope(|scope| -> io::Result<Vec<_>> {
        let writers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    add_pgn_game(&file, &new, Some(history(&root)))
                })
            })
            .collect();
        writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect()
    })?;
    numbers.sort_unstable();
    assert_eq!(
        numbers,
        [
            (number(1)?, None),
            (number(2)?, None),
            (number(3)?, None),
            (number(4)?, None)
        ]
    );
    for (state, games) in [(4, 3), (3, 2), (2, 1), (1, 0)] {
        let stamp = prepare_pgn_restore(&file, &root, state)?;
        restore_pgn_state(&file, history(&root), state, &stamp)?;
        let one = "[White \"New\"]\n\n1. d4 d5 *\n";
        assert_eq!(fs::read_to_string(&file)?, vec![one; games].join("\n"));
    }
    fs::remove_dir_all(root)
}

#[test]
fn keeps_distinct_owners_when_two_path_keys_collide() -> io::Result<()> {
    let root = target("colliding-history");
    fs::create_dir(&root)?;
    // These equal-length names have the same CRC32 after any common directory prefix.
    let first = root.join("587fd3befe5c6ffa.pgn");
    let second = root.join("e2f3679cc5968d27.pgn");
    fs::write(&first, b"1. e4 *")?;
    fs::write(&second, b"1. d4 *")?;
    let new = game(NEW_GAME)?;
    for file in [&first, &second] {
        add_pgn_game(file, &new, Some(history(&root)))?;
    }
    for (file, old) in [(&first, b"1. e4 *"), (&second, b"1. d4 *")] {
        let stamp = prepare_pgn_restore(file, &root, 1)?;
        restore_pgn_state(file, history(&root), 1, &stamp)?;
        assert_eq!(fs::read(file)?, old);
    }
    fs::remove_dir_all(root)
}

#[test]
fn refuses_a_save_and_restore_while_another_handle_holds_the_history_lock() -> io::Result<()> {
    let root = target("held-history-lock");
    fs::create_dir(&root)?;
    let file = root.join("Club.pgn");
    fs::write(&file, OLD_GAME)?;
    let new = game(NEW_GAME)?;
    let old = game(OLD_GAME)?;
    replace_pgn_game(&file, number(1)?, &new, Some(&old), Some(history(&root)))?;
    let stamp = prepare_pgn_restore(&file, &root, 1)?;
    let folder = fs::read_dir(root.join("pgn-history"))?
        .next()
        .unwrap()?
        .path();
    let first_state = fs::read(folder.join("1.state"))?;

    let refuse = |result: io::Result<()>| result.map_err(|error| error.to_string());
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let owner = fs::File::open(folder.join("owner"))?;
        owner.lock()?;
        scope.spawn(|| {
            send.send(refuse(
                replace_pgn_game(
                    &file,
                    NonZeroU64::MIN,
                    &old,
                    Some(&new),
                    Some(history(&root)),
                )
                .map(drop),
            ))
            .ok();
        });
        scope.spawn(|| {
            send.send(refuse(restore_pgn_state(&file, history(&root), 1, &stamp)))
                .ok();
        });
        let mut batch = PgnBatch::new()?;
        batch.push("1. e4 1-0")?;
        scope.spawn(|| {
            send.send(refuse(
                batch.append_to(&file, Some(history(&root))).map(drop),
            ))
            .ok();
        });
        // Leave room for worker scheduling around the five-second lock deadline.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        for _ in 0..3 {
            let answer =
                receive.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
            assert_eq!(
                answer,
                Ok(Err(
                    "Another change to this file is still running. Try again.".to_owned()
                ))
            );
        }
        assert_eq!(fs::read(&file)?, NEW_GAME);
        assert_eq!(fs::read(folder.join("1.state"))?, first_state);
        assert!(!folder.join("2.state").try_exists()?);
        Ok::<(), io::Error>(())
    })?;

    assert_eq!(
        replace_pgn_game(&file, number(1)?, &old, Some(&new), Some(history(&root)))?,
        None
    );
    let stamp = prepare_pgn_restore(&file, &root, 2)?;
    restore_pgn_state(&file, history(&root), 2, &stamp)?;
    assert_eq!(fs::read(&file)?, NEW_GAME);
    assert_eq!(
        pgn_states(&file, &root)?
            .iter()
            .map(|state| state.number)
            .collect::<Vec<_>>(),
        [3, 2, 1]
    );
    fs::remove_dir_all(root)
}
